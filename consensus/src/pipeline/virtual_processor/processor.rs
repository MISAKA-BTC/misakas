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
use rocksdb::WriteBatch;
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
    pub(super) max_block_parents: u8,
    pub(super) mergeset_size_limit: u64,
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
    /// ADR-0095 §4.11 as corrected: the membership's own fence.
    pub(super) palw_model_benefits: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0114: `Params::palw_model_leg_v2_fence` — the five-percent owner leg's height.
    pub(super) palw_model_leg_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0120: `Params::palw_model_seed_v2_fence` — the one-million-MSK floor's height.
    pub(super) palw_model_seed_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
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
    /// `Params::palw_settled_anchor_depth` — the second clock's depth, read only past the fence
    /// above through [`Self::palw_settled_anchor_depth_at`].
    pub(super) palw_settled_anchor_depth: Option<u64>,
    /// `Params::palw_admission_audit_period_daa` — how often a `Candidate` meets its ADR-0147 jury.
    pub(super) palw_admission_audit_period_daa: Option<u64>,
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

    /// **ADR-0018 §E's payout bounds** (mainnet audit 2026-09-06 — H-2/H-3/M-1), mode folded in.
    /// `None` on testnet-11, devnet and simnet; `always()` on a card. Resolved at the BLOCK's DAA
    /// on both the coinbase construction and the validation path — they must agree, or every node
    /// builds a different coinbase.
    pub(super) palw_validator_payout_bounds: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0066: the heartbeat lane's fence, mode folded in.
    pub(super) palw_heartbeat_lane: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0138: `Params::palw_anchor_clock` and ADR-0083's receipt fence — the heartbeat miner's
    /// hint reads them for the same reason the slot rule does (`heartbeat_yield_hint_v2`).
    pub(super) palw_anchor_clock: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0142: `Params::palw_clock_cursor`. Past it the heartbeat lane's admissibility is the
    /// rooted cursor, and only a heartbeat writes it.
    pub(super) palw_clock_cursor: Option<kaspa_consensus_core::config::params::ForkActivation>,
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

    /// ADR-0033 (B14): the PALW credit gate's fence — `None` (every shipped network) keeps
    /// the whole gate dormant; `Some` makes crossing commitments mintable in the coinbase
    /// and validated identically. Cloned from `Params::palw_credit` at construction.
    pub(super) palw_credit_params: Option<kaspa_consensus_core::palw_credit::PalwCreditParamsV1>,

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
    /// ADR-0109 Decision 3: the PALW locked-bond set the mempool refuses spends of, memoised per
    /// (registry tip, DAA) so a burst of admissions does not rebuild it per transaction.
    pub(super) palw_mempool_locked_cache:
        parking_lot::Mutex<Option<(BlockHash, u64, Arc<std::collections::HashSet<TransactionOutpoint>>)>>,
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

/// One object a block's acceptance produced, with what its carrier paid (ADR-0075 SA-1/SA-2).
///
/// The pairing exists because `AcceptanceData` cannot express it: it records a transaction id and
/// an index and nothing about value, so before this the object walk had no way to read the fee
/// that Decision 1 calls "the rent". The number is resolved in `palw_v2_objects_of_block` from
/// the map `calculate_utxo_state` filled, and spent in `palw_v2_accepted_objects`.
pub(super) struct PalwCarriedObjectV1 {
    pub(super) object: kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    pub(super) carrier_fee: u64,
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
    ),
    Spend(BlockHash, kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3),
}

impl VirtualStateProcessor {
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
            max_block_parents: params.max_block_parents(),
            mergeset_size_limit: params.mergeset_size_limit(),
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
            // The MODE-folded fence: capability declarations, the claim lane and the panel draw
            // exist only under ConsensusV2, so the mode condition is folded once in `Params`
            // rather than remembered at each of the three sites below.
            palw_capability_bound: params.palw_capability_bound_fence(),
            palw_certification_rent: params.palw_certification_rent,
            palw_kary_court: params.palw_kary_court_fence(),
            palw_prompt_ids_merkle: params.palw_prompt_ids_merkle_fence(),
            palw_panel_da: params.palw_panel_da_fence(),
            palw_court_ladder: params.palw_court_ladder_fence(),
            palw_court_responder_coverage: params.palw_court_responder_coverage_fence(),
            palw_fp_da_pins: params.palw_fp_da_pins_fence(),
            palw_share_growth_final: params.palw_share_growth_final_fence(),
            palw_model_market: params.palw_model_market_fence(),
            palw_model_lines: params.palw_model_lines_fence(),
            palw_model_benefits: params.palw_model_benefits_fence(),
            palw_model_leg_v2: params.palw_model_leg_v2_fence(),
            palw_model_seed_v2: params.palw_model_seed_v2_fence(),
            palw_panel_economy: params.palw_panel_economy_fence(),
            palw_work_priced_reward: params.palw_work_priced_reward_fence(),
            palw_execution_lane: params.palw_execution_lane_fence(),
            palw_overlay_carve: params.palw_overlay_carve_fence(),
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
            palw_shard_licensing: params.palw_shard_licensing_fence(),
            palw_token_lift: params.palw_token_lift_fence(),
            palw_kimi_k3: params.palw_kimi_k3_fence(),
            palw_fused_dissectable: params.palw_fused_dissectable_fence(),
            palw_attn_anchored_root: params.palw_attn_anchored_root_fence(),
            palw_held_context: params.palw_held_context_fence(),
            palw_audit_2026_09_11: params.palw_audit_2026_09_11_fence(),
            palw_audit_2026_09_11_deep: params.palw_audit_2026_09_11_deep_fence(),
            palw_audit_2026_09_23: params.palw_audit_2026_09_23_fence(),
            palw_settled_anchor_depth: params.palw_settled_anchor_depth,
            palw_admission_audit_period_daa: params.palw_admission_audit_period_daa,
            palw_frontier_provenance: params.palw_frontier_provenance,
            palw_validator_payout_bounds: params.palw_validator_payout_bounds_fence(),
            finality_depth: params.blockrate.finality_depth,
            palw_credit_params: params.palw_credit.clone(),
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

    /// The EVM template's producer policy (never block validity): is DNS finality confirmed and
    /// keeping up with `sink`? Measured in blue score beyond the anchor's healthy distance — see
    /// [`dns_finality_fresh_for_bridge`] — and read the same way the deposit-claim RPC reads it,
    /// so the RPC never accepts a claim the template would then leave out, or the reverse.
    fn bridge_finality_is_fresh(&self, sink: BlockHash) -> bool {
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

    /// ADR-0109 Decision 3: the PALW bonds the registry holds locked at the virtual tip — the set the
    /// acceptance path skips spends of — memoised per (registry tip, DAA). `None` when the network
    /// has no V2 registry or the registry has no tip yet.
    fn palw_mempool_locked_bonds(&self, now_daa: u64) -> Option<Arc<std::collections::HashSet<TransactionOutpoint>>> {
        let params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_state_v2_store.read().load_tip_cached(params).ok().flatten()?;
        let mut cache = self.palw_mempool_locked_cache.lock();
        if let Some((cached_tip, cached_daa, set)) = cache.as_ref()
            && *cached_tip == tip
            && *cached_daa == now_daa
        {
            return Some(set.clone());
        }
        let set = Arc::new(self.palw_v2_locked_bond_outpoints(&state, now_daa));
        *cache = Some((tip, now_daa, set.clone()));
        Some(set)
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

            self.resolve_virtual();

            let statuses_read = self.statuses_store.read();
            for msg in messages {
                match msg {
                    VirtualStateProcessingMessage::Exit => break 'outer,
                    VirtualStateProcessingMessage::Process(task, virtual_state_result_transmitter) => {
                        // We don't care if receivers were dropped
                        let _ = virtual_state_result_transmitter.send(Ok(statuses_read.get(task.block().hash()).unwrap()));
                    }
                };
            }
        }

        // Pass the exit signal on to the following processor
        self.pruning_sender.send(PruningProcessingMessage::Exit).unwrap();
    }

    fn resolve_virtual(self: &Arc<Self>) {
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
        );
        let (virtual_parents, virtual_ghostdag_data) =
            self.pick_virtual_parents(new_sink, virtual_parent_candidates, pruning_point, round_tips);
        assert_eq!(virtual_ghostdag_data.selected_parent, new_sink);

        let sink_multiset = self.utxo_multisets_store.get(new_sink).unwrap();
        let chain_path = self.dag_traversal_manager.calculate_chain_path(prev_sink, new_sink, None);
        let sink_ghostdag_data = Lazy::new(|| self.ghostdag_store.get_data(new_sink).unwrap());
        // Cache the DAA and Median time windows of the sink for future use, as well as prepare for virtual's window calculations
        self.cache_sink_windows(new_sink, prev_sink, &sink_ghostdag_data);

        let new_virtual_state = self
            .calculate_and_commit_virtual_state(
                virtual_read,
                virtual_parents,
                virtual_ghostdag_data,
                sink_multiset,
                &mut accumulated_diff,
                // After `sink_search_algorithm` the walked view equals the bond
                // set as-of the new sink (= the virtual block's selected parent).
                &accumulated_bond_view,
                &chain_path,
            )
            .expect("all possible rule errors are unexpected here");

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
    /// `to` itself (with the exception of returning `from` if `to` is already known to be UTXO disqualified).
    /// When returning it is guaranteed that `diff` holds the diff of the returned block from virtual
    pub(super) fn calculate_utxo_state_relatively(
        &self,
        stores: &VirtualStores,
        diff: &mut UtxoDiff,
        bond_view: &mut ActiveBondView,
        from: BlockHash,
        to: BlockHash,
    ) -> BlockHash {
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
                let loaded = self.palw_state_v2_store.read().load_tip(params).unwrap_or_else(|e| {
                    panic!(
                        "the stored PALW V2 tip does not load under this build ({e}). A state written by another \
                         PALW_STATE_V2_VERSION or ruleset is not migrated in place: this network re-genesises on a \
                         ruleset change (ADR-0042 Decision 11, ADR-0075 §7), so wipe the datadir and resync from peers \
                         announcing this build's fingerprint"
                    )
                });
                match loaded {
                    None => None,
                    Some((tip_block, tip_state)) if tip_block == from => Some(tip_state),
                    Some((tip_block, tip_state)) => {
                        let path = self.dag_traversal_manager.calculate_chain_path(tip_block, from, None);
                        let removed: Vec<BlockHash> = path.removed.to_vec();
                        let added: Vec<BlockHash> = path.added.to_vec();
                        let store = self.palw_state_v2_store.read();
                        match crate::processes::palw_state_walk::walk_chain_path(&store, params, tip_state, &removed, &added) {
                            Ok(state) => {
                                warn!(
                                    "PALW V2 state tip stood at {tip_block} while the UTXO walk starts at {from} (an unclean shutdown between the two commits); re-derived the state at {from} over {} reverted and {} applied deltas",
                                    removed.len(),
                                    added.len()
                                );
                                Some(state)
                            }
                            Err(e) => {
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
                                match store.load_pruning_snapshot(params) {
                                    Ok(Some((snap_block, snap_state))) => {
                                        let snap_path = self.dag_traversal_manager.calculate_chain_path(snap_block, from, None);
                                        let snap_removed: Vec<BlockHash> = snap_path.removed.to_vec();
                                        let snap_added: Vec<BlockHash> = snap_path.added.to_vec();
                                        match crate::processes::palw_state_walk::walk_chain_path(
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
                                                error!(
                                                    "PALW V2 state cannot be established at {from}: the tip at {tip_block} does not walk here ({e}) and neither does the \
                                                     pruning snapshot at {snap_block} ({snap_err}). This node cannot evaluate ANY candidate until its state store is repaired \
                                                     — resync this data directory from peers announcing this build's fingerprint."
                                                );
                                                return from;
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        error!(
                                            "PALW V2 state cannot be established at {from}: the tip at {tip_block} does not walk here ({e}) and this store holds no pruning \
                                             snapshot to rebuild from. This node cannot evaluate ANY candidate until its state store is repaired — resync this data directory."
                                        );
                                        return from;
                                    }
                                    Err(snap_err) => {
                                        error!(
                                            "PALW V2 state cannot be established at {from}: the tip at {tip_block} does not walk here ({e}) and the pruning snapshot cannot be \
                                             read ({snap_err}). This node cannot evaluate ANY candidate until its state store is repaired — resync this data directory."
                                        );
                                        return from;
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
            return from;
        }

        let mut split_point: Option<BlockHash> = None;

        // Walk down to the reorg split point
        for current in self.reachability_service.default_backward_chain_iterator(from) {
            if self.reachability_service.is_chain_ancestor_of(current, to) {
                split_point = Some(current);
                break;
            }

            let mergeset_diff = self.utxo_diffs_store.get(current).unwrap();
            // Apply the diff in reverse
            diff.with_diff_in_place(&mergeset_diff.as_reversed()).unwrap();
            if track_bonds {
                // Mirror the reverse on the bond view. `current` is leaving the
                // selected chain, so its acceptance data is committed.
                bond_view.revert(&self.dns_bond_mutations_for_chain_block(current, bond_view));
            }
            // ADR-0042 Unit C: the PALW state walks in lockstep with `diff`, for the same reason
            // the bond view does — a candidate's V2 standing must be a fold over THAT candidate's
            // chain and never a read of the node's sink (P0-4, the partition this layout exists to
            // make unrepresentable). `revert_delta_v2` verifies every value it replaces, so a
            // delta applied to the wrong parent is an error rather than a quiet divergence.
            if let Some(state) = palw_state.as_mut() {
                // The backward twin of the forward leg's rule: a row this node does not have is a
                // gap in this node. Stopping returns the last point actually established, which is
                // what every other unfinished walk in this function returns.
                let params = self.palw_state_params_v2.as_ref().expect("palw_state is Some only when the params are");
                let reverted = self.palw_state_v2_store.read().delta_of(current).map_err(|e| e.to_string()).and_then(|(_, delta)| {
                    kaspa_consensus_core::palw_state_v2::revert_delta_v2(state, &delta, params).map_err(|e| e.to_string())
                });
                match reverted {
                    Ok(previous) => *state = previous,
                    Err(why) => {
                        error!(
                            "PALW V2 state cannot be reverted through chain block {current} ({why}); the UTXO walk stops at {from} \
                             and virtual will hold there. This is a gap in THIS node's delta store, not a fault of the chain — \
                             resync this data directory if it persists."
                        );
                        return from;
                    }
                }
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
                    diff.with_diff_in_place(mergeset_diff.deref()).unwrap();
                    diff_point = current;
                    if track_bonds {
                        // `current` is an already-validated chain block joining
                        // the diff; its acceptance data is committed.
                        bond_view.apply(&self.dns_bond_mutations_for_chain_block(current, bond_view));
                    }
                    // Unit C, forward leg: this block was validated before, so its delta is on
                    // disk and re-applying it reproduces the transition bit-for-bit. Re-running
                    // the transition here instead would be a second computation of one fact, and
                    // a second chance to disagree with what the chain already committed to.
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
                        let applied =
                            self.palw_state_v2_store.read().delta_of(current).map_err(|e| e.to_string()).and_then(|(_, delta)| {
                                kaspa_consensus_core::palw_state_v2::apply_delta_v2(state, &delta, params).map_err(|e| e.to_string())
                            });
                        match applied {
                            Ok(next) => *state = next,
                            Err(why) => {
                                error!(
                                    "PALW V2 state cannot advance through chain block {current} ({why}); the UTXO walk stops at {diff_point} \
                                     and virtual will hold there. This is a gap in THIS node's delta store, not a fault of the chain — \
                                     resync this data directory if it persists."
                                );
                                return diff_point;
                            }
                        }
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
                        palw_state.as_ref().map(|s| self.palw_v2_escrow_withheld_at(s, selected_parent)).unwrap_or(0);
                    // Launch blockers §8: and which of the OTHER merged blues this block may not
                    // pay at all, from the same parent state and for the same reason.
                    ctx.palw_v2_unentitled_blues = palw_state
                        .as_ref()
                        .map(|s| {
                            // The same non-DAA set `verify_expected_utxo_state` reads a few frames
                            // down, from the same store — the two must agree or the coinbase this
                            // computes is not the coinbase that is checked.
                            use crate::model::stores::daa::DaaStoreReader;
                            let non_daa = self
                                .daa_excluded_store
                                .get_mergeset_non_daa(current)
                                .expect("the DAA window is written before the UTXO walk reaches this block");
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
                    // B-1 (deep fence): and how much of each OTHER merged block's carve is withheld
                    // and escrowed rather than paid, from the same parent state and the same
                    // evaluation point. Empty below `palw_audit_2026_09_11_deep` (paid in full, as
                    // before). Needs no `mergeset_rewards` — it derives each block's subsidy from its
                    // own header — so it is safe to compute here, before the reward map is filled.
                    ctx.palw_v2_merged_escrow_withheld = palw_state
                        .as_ref()
                        .map(|s| {
                            use crate::model::stores::daa::DaaStoreReader;
                            let non_daa = self
                                .daa_excluded_store
                                .get_mergeset_non_daa(current)
                                .expect("the DAA window is written before the UTXO walk reaches this block");
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
                    // ADR-0125: which merged round blocks hold their permit, from the same parent state.
                    ctx.palw_round_verdicts =
                        palw_state.as_ref().and_then(|s| self.palw_round_verdicts_v1(s, &ctx.ghostdag_data, pov_daa_score));
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
                        palw_state.as_ref(),
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
                                let parent_root = state.state_root();
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
                                    &ctx.palw_v2_accepted_tx_fees,
                                );
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
                                let (objects, folded) = self.palw_v2_accepted_objects(state, state_params, &point, objects, current);
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
                                    self.palw_bootstrap_activation.filter(|fence| fence.is_active(point.daa_score)).map(|_| &folded);
                                let attempt =
                                    match self.palw_v2_check_attempt_admission(&header, state, state_params, &point, bootstrap) {
                                        Ok(attempt) => attempt,
                                        Err(adm_error) => {
                                            info!(
                                                "Block {} is disqualified from virtual chain (PALW admission): {}",
                                                current, adm_error
                                            );
                                            self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                                            chain_disqualified_counter += 1;
                                            continue;
                                        }
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
                                    self.daa_excluded_store.get_mergeset_non_daa(current).unwrap_or_default()
                                };
                                let (merged_owned, merged_preskips) =
                                    self.palw_v2_merged_works(&ctx.ghostdag_data, state, state_params, &merged_non_daa, &point);
                                let merged_refs: Vec<kaspa_consensus_core::palw_state_v2::PalwMergedWorkV1<'_>> = merged_owned
                                    .iter()
                                    .map(|owned| match owned {
                                        PalwMergedOwnedWorkV1::Attempt(blue, envelope, subsidy, carve, bits) => {
                                            kaspa_consensus_core::palw_state_v2::PalwMergedWorkV1 {
                                                carrying_block: *blue,
                                                work: kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::Attempt(envelope),
                                                // B-4: the execution key from the merged blue's OWN header (pre_pow +
                                                // nonce). A header the store cannot serve degrades to the attempt id — a
                                                // unique value that never false-dedups — but a mergeset blue always has one.
                                                execution_key: self
                                                    .headers_store
                                                    .get_header(*blue)
                                                    .ok()
                                                    .map(|h| self.palw_execution_key_v1(&h, &envelope.attempt))
                                                    .unwrap_or_else(|| {
                                                        kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&envelope.attempt)
                                                    }),
                                                // B-1: the merged block's OWN subsidy — the pool its escrow is carved
                                                // from past the deep fence, the SAME value `palw_v2_merged_escrow_withheld`
                                                // hands the coinbase to withhold (both read it from this one field).
                                                subsidy: *subsidy,
                                                // ADR-0126: the carve resolved at the merged block's DAA, read from the
                                                // same record for the same reason.
                                                escrow_carve: *carve,
                                                bits: *bits,
                                            }
                                        }
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
                                            }
                                        }
                                    })
                                    .collect();
                                // B-4: this block's OWN attempt's execution key, from its own header
                                // (pre_pow + nonce). `default` when the block carries no attempt.
                                let own_execution_key = match attempt.as_ref() {
                                    Some(envelope) => self
                                        .headers_store
                                        .get_header(current)
                                        .ok()
                                        .map(|h| self.palw_execution_key_v1(&h, &envelope.attempt))
                                        .unwrap_or_else(|| kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&envelope.attempt)),
                                    None => kaspa_hashes::Hash64::default(),
                                };
                                match kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
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
                                        let mut extras = self.palw_transition_extras_for(&point);
                                        if let Some(staged) = evm_staged.as_ref() {
                                            extras.evm_actions = staged.result.market_actions.clone();
                                        }
                                        // ADR-0125: the permits this block accepted, as decided above.
                                        if let Some(verdicts) = ctx.palw_round_verdicts.as_ref() {
                                            extras.round_permit_uses = verdicts.uses.clone();
                                        }
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
                                        Some((state.state_root(), delta))
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
                            palw_v2_staged,
                        );
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
            store.set_tip_batch(&mut batch, diff_point, state).unwrap();
            drop(store);
            self.db.write(batch).unwrap();
        }

        // Report counters
        self.counters.chain_block_counts.fetch_add(chain_block_counter, Ordering::Relaxed);
        if chain_disqualified_counter > 0 {
            self.counters.chain_disqualified_counts.fetch_add(chain_disqualified_counter, Ordering::Relaxed);
        }

        diff_point
    }

    /// kaspa-pq EVM Lane v0.4 (§2.3): the lazy chain-context EVM step for one
    /// selected-chain candidate. Gated on `evm_activation_daa_score` (a single
    /// u64 compare on every current network); no-replay and the commitment
    /// check live in `processes::evm::evm_validate`. `Err` = the block is
    /// disqualified from the chain (commitment fault), mirroring a UTXO fault.
    #[cfg(feature = "evm")]
    fn evm_chain_context_step<V: UtxoView>(
        &self,
        current: BlockHash,
        selected_parent: BlockHash,
        header: &Header,
        ctx: &mut UtxoProcessingContext<'_>,
        selected_parent_utxo_view: &V,
        pipeline: Option<&crate::processes::evm::EvmPipeline>,
        palw_state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
    ) -> Result<Option<crate::processes::evm::EvmStaged>, String> {
        use crate::model::stores::evm::EvmPayloadStoreReader; // EvmHeaderStoreReader is in module scope
        use crate::processes::evm::{
            EvmValidateError, apply_evm_bridge_effects, evm_validate, evm_validate_chained, validate_evm_deposit_claims,
        };
        if header.daa_score < self.evm_activation_daa_score {
            return Ok(None);
        }
        // The §4.3 version rule admits only v2+ headers at/after activation.
        debug_assert!(header.version >= kaspa_consensus_core::constants::EVM_HEADER_VERSION);
        // B's own payload (system_ops + the accepting coinbase); absent ⇒ empty
        // (only non-empty payloads are persisted at body commit).
        let own_payload = match self.evm_payload_store.get(current) {
            Ok(p) => p,
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => Default::default(),
            Err(e) => return Err(format!("evm payload store: {e}")),
        };
        // §9.2: deposit claims are validated against the CLAIM VIEW = the
        // selected-parent UTXO set composed with the mergeset diff so far (a
        // lock spent by a mergeset tx is not claimable; a same-block lock is
        // not visible). Any violation is an accepting-producer fault.
        let consumed_locks = {
            let claim_view = selected_parent_utxo_view.compose(&ctx.mergeset_diff);
            validate_evm_deposit_claims(&own_payload, &claim_view, header.daa_score)?
        };
        // ADR-0089 Decisions 2, 6 and 9: the window (the selected parent's fold rows), the
        // settlements this block must carry (the ones that fold decided), and the fences at
        // this block's DAA. Below the fence the list is empty and no door is registered — and a
        // settlement op carried below the fence is refused by the same equality.
        let market_fences = self.palw_evm_market_fences_at(header.daa_score);
        let expected_settlements: Vec<kaspa_consensus_core::evm::model_market::PalwEvmSettlementV1> =
            if market_fences.evm_active { palw_state.map(|s| s.evm_settlements()).unwrap_or_default() } else { Vec::new() };
        crate::processes::evm::validate_evm_market_settlements(&own_payload, &expected_settlements)?;
        let market_view = match (market_fences.evm_active, palw_state, self.palw_state_params_v2.as_ref()) {
            (true, Some(state), Some(params)) => {
                Some(std::sync::Arc::new(state.evm_view_v1(kaspa_consensus_core::evm::EVM_CHAIN_ID, params.base_class_id())))
            }
            _ => None,
        };
        let market = kaspa_evm::EvmMarketInput {
            palw_view: market_view,
            fences: market_fences,
            expected_settlements: &expected_settlements,
            chain_id: kaspa_consensus_core::evm::EVM_CHAIN_ID,
        };
        // C-01 S9 cutover: when flat-authoritative (and the shadow backend that maintains the flat
        // store is on), seed the executor from the flat/reconstruct parent state instead of 206 —
        // but ONLY after asserting it byte-identical to 206 (inside `validated_flat_parent_seed`,
        // which HALTs on divergence BEFORE the seed is used, so a backend bug can never falsely
        // disqualify a valid block). A pre-activation / Unavailable parent ⇒ `None` ⇒ the 206 path.
        // 206 is still written, so this is reversible; the result is identical (validated == 206).
        let flat_auth = self.evm_flat_authoritative && self.evm_shadow_state_backend;
        // Whether the inline path pre-validated the flat seed (so the post-execution S6 check below
        // is not run twice). The pipeline path (206-seeded) leaves this false and is checked below.
        let mut seed_prevalidated = false;
        // O12: a pipelined run pre-executed this block's acceptance on the
        // worker (same pure function, same inputs — see EvmPipeline). Consume
        // its result; fall back to inline execution when the pipeline ended.
        // ADR-0089: the pipeline pre-executed without the window; past the fence its result is
        // consumed and discarded, and the block is executed inline with the market inputs.
        let pipelined = pipeline.and_then(|p| p.recv(current)).filter(|_| !market_fences.evm_active);
        let staged = match pipelined {
            Some(Ok(staged)) => Some(staged),
            Some(Err(msg)) => return Err(msg),
            None => {
                // AcceptedEvmTxs(B) source: the consensus-ordered mergeset (selected
                // parent first, then ascending blue work — §3.1 canonical order).
                let sorted_mergeset: Vec<BlockHash> =
                    ctx.ghostdag_data.consensus_ordered_mergeset(self.ghostdag_store.as_ref()).collect();
                // ADR-0139: this chain block's accepted-user-gas cap — one round budget per DISTINCT
                // permitted round among the round blocks it merges, from the round indices their
                // envelopes carry; the base where the execution lane is not in force.
                let user_gas_cap =
                    kaspa_consensus_core::evm::evm_user_gas_cap_v1(self.palw_execution_lane_at(header.daa_score).map(|_| {
                        ctx.palw_round_verdicts
                            .as_ref()
                            .map(|v| kaspa_consensus_core::evm::evm_distinct_permitted_rounds_v1(&v.uses))
                            .unwrap_or(0)
                    }));
                let map_err = |e| match e {
                    EvmValidateError::CommitmentMismatch { .. } => {
                        "evm_commitment_root mismatch (mergeset acceptance re-execution)".to_string()
                    }
                    EvmValidateError::Exec(e) => format!("evm execution: {e}"),
                    EvmValidateError::Store(e) => format!("evm store: {e}"),
                };
                // The validated flat/reconstruct seed (S9), or None ⇒ seed from 206 (the default,
                // and the fallback for pre-activation / Unavailable parents).
                match flat_auth.then(|| self.validated_flat_parent_seed(selected_parent)).flatten() {
                    Some(seed) => {
                        seed_prevalidated = true;
                        evm_validate_chained(
                            &self.evm_header_store,
                            &self.evm_state_store,
                            &self.evm_payload_store,
                            current,
                            selected_parent,
                            &sorted_mergeset,
                            header,
                            &own_payload,
                            Some(seed),
                            self.evm_gas_pool_v2_activation_daa_score,
                            self.evm_f002_withdraw_cap_activation_daa_score,
                            self.evm_f003_mldsa_verify_activation_daa_score,
                            self.evm_typed_receipt_root_activation_daa_score,
                            user_gas_cap,
                            market.clone(),
                        )
                        .map_err(map_err)?
                    }
                    None => {
                        // C-01 S9b: with 206 retired there is NO 206 fallback for an EVM-ACTIVE
                        // parent — the `evm_validate` (206) path below would read an absent snapshot
                        // and disqualify a VALID block (a fork). A flat backend that cannot yield an
                        // EVM-active parent's seed is a NODE fault, not a chain fault: HALT (design §7),
                        // never disqualify. A header-store read error is treated the same way (we cannot
                        // prove the parent is pre-activation, so we must not risk the 206 path) — a
                        // swallowed error here (`unwrap_or(false)`) would let an EVM-active parent fall
                        // through and false-disqualify. A PRE-ACTIVATION parent (no EVM header) needs no
                        // 206 — `evm_validate` seeds the empty genesis parent — so it stays correct.
                        // (The Unavailable-seed case for an EVM-active parent — e.g. a non-head parent
                        // whose §12 history is unreconstructable — also HALTs here; that is the safe
                        // fail-stop, never a fork. It should not arise in recent/archive mode, where
                        // §12 is retained for every unpruned block; if it recurs, retention is
                        // insufficient for the reorg depth — use archive — or the flat backend is faulty.)
                        if self.evm_retire_206 {
                            match self.evm_header_store.has(selected_parent) {
                                Ok(false) => {} // pre-activation: the 206 path seeds the empty parent (no 206 read)
                                Ok(true) => panic!(
                                    "C-01 S9b: --evm-retire-206 is on but no flat/reconstruct seed could be obtained for EVM-active \
                                     selected parent {selected_parent} (the 206 snapshot is retired). HALTING this node — chain integrity \
                                     is intact; restore the flat backend (or use --evm-history-mode=archive), or disable --evm-retire-206."
                                ),
                                Err(e) => panic!(
                                    "C-01 S9b: --evm-retire-206 is on and the EVM header store could not be read for selected parent \
                                     {selected_parent} ({e}); cannot prove it is pre-activation, and there is no 206 fallback. HALTING \
                                     this node (chain integrity intact) rather than risk false-disqualifying a valid block."
                                ),
                            }
                        }
                        evm_validate(
                            &self.evm_header_store,
                            &self.evm_state_store,
                            &self.evm_payload_store,
                            current,
                            selected_parent,
                            &sorted_mergeset,
                            header,
                            &own_payload,
                            self.evm_gas_pool_v2_activation_daa_score,
                            self.evm_f002_withdraw_cap_activation_daa_score,
                            self.evm_f003_mldsa_verify_activation_daa_score,
                            self.evm_typed_receipt_root_activation_daa_score,
                            user_gas_cap,
                            market.clone(),
                        )
                        .map_err(map_err)?
                    }
                }
            }
        };
        let Some(staged) = staged else {
            // The EVM rows commit in the SAME batch as the UTXO diff, so a
            // present result with an absent diff (this KeyNotFound arm) is
            // store corruption — never a reachable consensus state.
            panic!("EVM result for {current} exists but its UTXO diff does not — corrupt store");
        };
        // §9: fold the bridge's UTXO side-effects into THIS block's diff +
        // multiset (before verify_expected_utxo_state reads them).
        apply_evm_bridge_effects(
            &mut ctx.mergeset_diff,
            &mut ctx.multiset_hash,
            header.daa_score,
            &consumed_locks,
            &staged.result.withdrawals,
        )?;
        // ADR-0089 Decision 6: the settlements' sink outputs, into THIS block's diff, keyed by the
        // block whose fold decided them (the selected parent).
        crate::processes::evm::apply_evm_market_effects(
            &mut ctx.mergeset_diff,
            &mut ctx.multiset_hash,
            header.daa_score,
            selected_parent,
            &expected_settlements,
        )?;
        // kaspa-pq EVM bridge observability (P0-4): a deposit lock that reaches
        // this point is being APPLIED into this accepted chain block's committed
        // UTXO diff (consumed). Log each so a successful claim is directly visible
        // — the accepted-gas KPI rounds to 0.00% even for several real claims.
        for (outpoint, entry) in &consumed_locks {
            info!(
                "[evm-claim-applied] accepting_block={current} deposit_outpoint={outpoint} amount_sompi={} pov_daa={}",
                entry.amount, header.daa_score
            );
        }
        // O9: chain-rate / mergeset / gas-utilization observability + applied-claim count.
        self.evm_lane_kpi.record(ctx.ghostdag_data.mergeset_size(), staged.result.header.gas_used, consumed_locks.len());
        // C-01 (slice S6/S9) shadow seed validation: confirm the flat/reconstruct PARENT seed source
        // reproduces the committed 206 parent snapshot byte-for-byte (HALT on divergence; never
        // disqualifies — 206 is still written). Skipped when the flat-authoritative inline path
        // already validated the seed BEFORE executing from it (`seed_prevalidated`), so the check
        // runs exactly once: here for 206-seeded blocks (non-flat-auth inline, or the O12 pipeline),
        // pre-execution for flat-authoritative blocks. Node-local, off by default.
        if self.evm_shadow_state_backend && !seed_prevalidated {
            self.shadow_validate_parent_seed(selected_parent);
        }
        Ok(Some(staged))
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

    /// Non-`evm` builds cannot validate the lane. On every default network the
    /// lane is `u64::MAX`-inert so this is unreachable; on an evm-ACTIVE net a
    /// non-evm binary must refuse to follow a chain it cannot validate rather
    /// than silently fork.
    #[cfg(not(feature = "evm"))]
    fn evm_chain_context_step<V: UtxoView>(
        &self,
        _current: BlockHash,
        _selected_parent: BlockHash,
        header: &Header,
        _ctx: &mut UtxoProcessingContext<'_>,
        _selected_parent_utxo_view: &V,
        _pipeline: Option<&crate::processes::evm::EvmPipeline>,
        _palw_state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
    ) -> Result<Option<crate::processes::evm::EvmStaged>, String> {
        if header.daa_score >= self.evm_activation_daa_score {
            panic!(
                "the EVM lane is active at DAA {} but this kaspad was built without the `evm` feature — refusing to follow a chain it cannot validate (rebuild with --features evm)",
                header.daa_score
            );
        }
        Ok(None)
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
            self.evm_f003_mldsa_verify_activation_daa_score,
            self.evm_typed_receipt_root_activation_daa_score,
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

    /// kaspa-pq EVM Lane v0.4 (§15): producer-side EVM fields for a template
    /// built from the current virtual state. Runs the SAME acceptance-execution
    /// core the verifier uses, so a block mined from this template reproduces
    /// `evm_commitment_root` byte-for-byte. The own payload is empty until the
    /// EVM mempool lands (§16). NOTE: the commitment derives from the header's
    /// timestamp — a miner must not mutate the template timestamp (refreshing
    /// the template re-derives the commitment).
    #[cfg(feature = "evm")]
    fn evm_template_fields(
        &self,
        header: Header,
        virtual_state: &VirtualState,
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
        // kaspa-pq narrow P0-1: deposit claims already validated + their lock
        // entries materialized against the template's virtual generation (no
        // re-read of a possibly-advanced view here).
        prepared_claims: crate::processes::evm::PreparedDepositClaims,
    ) -> Result<
        (
            Header,
            kaspa_consensus_core::evm::EvmExecutionPayload,
            Vec<(kaspa_consensus_core::tx::TransactionOutpoint, EvmClaimStaleKind)>,
        ),
        RuleError,
    > {
        use crate::processes::evm::{evm_execute_acceptance, evm_execute_acceptance_with_parent}; // EvmHeaderStoreReader in module scope
        if header.daa_score < self.evm_activation_daa_score {
            return Ok((header, Default::default(), vec![]));
        }
        // narrow P0-1: split the deposit-claim snapshot prepared against the
        // template's virtual generation — `accepted` claims go into the payload,
        // their `consumed_locks` fold into the commitment, the `stale` set flows
        // back to the mining manager.
        let crate::processes::evm::PreparedDepositClaims { accepted: accepted_claims, consumed_locks, stale: stale_claims } =
            prepared_claims;
        // ADR-0089 Decisions 6 and 9 on the PRODUCER's side: the selected parent's fold decides
        // the settlements this template must carry, and the executor is given the same window and
        // fences validation will use — the mirror of `evm_chain_context_step`, generation caveat
        // and all (the PALW tip is the template's selected parent for a fresh template).
        let template_selected_parent = virtual_state.ghostdag_data.selected_parent;
        let market_fences = self.palw_evm_market_fences_at(header.daa_score);
        let template_palw_state = self
            .palw_state_params_v2
            .as_ref()
            .filter(|_| market_fences.evm_active)
            .and_then(|params| self.palw_state_v2_store.read().load_tip(params).ok().flatten())
            .filter(|(block, _)| *block == template_selected_parent)
            .map(|(_, state)| state);
        // ADR-0139: the template's accepted-user-gas cap is what validation will compute for it —
        // one round budget per DISTINCT permitted round among the round blocks the template merges.
        let user_gas_cap = kaspa_consensus_core::evm::evm_user_gas_cap_v1(self.palw_execution_lane_at(header.daa_score).map(|_| {
            template_palw_state
                .as_ref()
                .and_then(|state| self.palw_round_verdicts_v1(state, &virtual_state.ghostdag_data, header.daa_score))
                .map(|v| kaspa_consensus_core::evm::evm_distinct_permitted_rounds_v1(&v.uses))
                .unwrap_or(0)
        }));

        let expected_settlements: Vec<kaspa_consensus_core::evm::model_market::PalwEvmSettlementV1> =
            template_palw_state.as_ref().map(|s| s.evm_settlements()).unwrap_or_default();
        let market_view = match (&template_palw_state, self.palw_state_params_v2.as_ref()) {
            (Some(state), Some(params)) => {
                Some(std::sync::Arc::new(state.evm_view_v1(kaspa_consensus_core::evm::EVM_CHAIN_ID, params.base_class_id())))
            }
            _ => None,
        };
        let market = kaspa_evm::EvmMarketInput {
            palw_view: market_view,
            fences: market_fences,
            expected_settlements: &expected_settlements,
            chain_id: kaspa_consensus_core::evm::EVM_CHAIN_ID,
        };
        // §15 step 6: assemble the own payload from the mempool candidates.
        // Defense-in-depth re-admission (the body class-1 rule): an inadmissible
        // tx here would make our OWN block payload-block-invalid, so hard-filter
        // rather than trust the pool; independently re-enforce the byte cap.
        // The candidates execute in a LATER accepting chain block, never here.
        let own_payload = {
            use kaspa_consensus_core::evm::{EvmExecutionPayload, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK};
            let mut payload = EvmExecutionPayload::default();
            let base = payload.payload_bytes().len();
            let mut budget = MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK.saturating_sub(base);
            for raw in evm_template_data.transactions {
                if 4 + raw.len() > budget {
                    continue;
                }
                match crate::processes::evm::admit_evm_payload_txs(&EvmExecutionPayload {
                    transactions: vec![raw.clone()],
                    ..Default::default()
                }) {
                    Ok(()) => {
                        budget -= 4 + raw.len();
                        payload.transactions.push(raw);
                    }
                    Err((_, reason)) => {
                        warn!("EVM template: dropping inadmissible mempool candidate ({reason})");
                    }
                }
            }
            // §9.2 (narrow P0-1): own-payload deposit claims. These EXECUTE in the
            // accepting chain block, so an invalid claim would make our block invalid.
            // The claims were ALREADY validated, and their consumed lock entries
            // materialized, by `prepare_deposit_claims` against the SAME virtual
            // generation this template's selected parent is taken from — NOT a
            // re-read of a possibly-advanced view here (that second read was the
            // mixed-generation TOCTOU that could self-disqualify the block or wrongly
            // drop a still-valid claim). The claim view for a block B extending the
            // virtual tip is `selected_parent(B)_view ∘ B.mergeset_diff`, which for a
            // fresh template IS the captured virtual UTXO set — exactly what the
            // acceptance path re-checks. Emit the accepted claims; the consumed locks
            // fold into the commitment below; the tagged stale set flows back to the
            // mining manager (`Absent` ⇒ retain + retry, `Invalid` ⇒ evict).
            for claim in accepted_claims {
                payload.system_ops.push(kaspa_consensus_core::evm::EvmSystemOp::DepositClaim(claim));
            }
            // ADR-0089 Decision 6: the settlements the selected parent's fold decided, in order.
            for settlement in &expected_settlements {
                payload.system_ops.push(kaspa_consensus_core::evm::EvmSystemOp::MarketSettle(*settlement));
            }
            // audit #3: the tx loop above budgets ONLY the txs against the byte
            // cap; the deposit-claim system ops are appended afterwards and each
            // is ~105 bytes, so a near-full tx payload + ≥1 claim can exceed
            // MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK — which body validation rejects,
            // making the node's OWN template invalid. Claims must execute (they
            // are this block's bridge credits), so keep every selected claim and
            // drop trailing (lowest-priority) txs until the WHOLE payload fits.
            while !payload.transactions.is_empty() && payload.payload_bytes().len() > MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK {
                payload.transactions.pop();
            }
            // §8.2: the declared coinbase claims this payload's priority fees —
            // meaningful only when the payload actually carries content (and
            // keeping it zero otherwise preserves the empty payload / empty
            // store-row form). A claim-only payload also declares the coinbase
            // (the claim tip routes to it, §9.2).
            if !payload.transactions.is_empty() || !payload.system_ops.is_empty() {
                payload.evm_coinbase = evm_template_data.evm_coinbase;
            }
            payload
        };
        let sorted_mergeset: Vec<BlockHash> =
            virtual_state.ghostdag_data.consensus_ordered_mergeset(self.ghostdag_store.as_ref()).collect();
        let selected_parent = virtual_state.ghostdag_data.selected_parent;
        // C-01 S9/S9b: the producer must seed the SAME parent state the verifier later seeds from
        // (so the mined block reproduces evm_commitment_root). When flat-authoritative, seed from the
        // validated flat/reconstruct parent (HALT on divergence, inside `validated_flat_parent_seed`),
        // exactly like the inline verifier — otherwise the 206 store path. With 206 retired there is no
        // 206 to read for an EVM-active parent, so a missing flat seed fails the template build (a
        // transient producer failure — never a panic / never a wrong commitment), not a 206 read error.
        let parent_override = (self.evm_flat_authoritative && self.evm_shadow_state_backend)
            .then(|| self.validated_flat_parent_seed(selected_parent))
            .flatten();
        let mapper = |e| RuleError::EvmTemplateExecutionFailed(format!("{e:?}"));
        let result = match parent_override {
            Some(seed) => {
                evm_execute_acceptance_with_parent(
                    &self.evm_header_store,
                    &self.evm_state_store,
                    &self.evm_payload_store,
                    selected_parent,
                    &sorted_mergeset,
                    &header,
                    &own_payload,
                    Some(seed),
                    self.evm_gas_pool_v2_activation_daa_score,
                    self.evm_f002_withdraw_cap_activation_daa_score,
                    self.evm_f003_mldsa_verify_activation_daa_score,
                    self.evm_typed_receipt_root_activation_daa_score,
                    user_gas_cap,
                    market.clone(),
                )
                .map_err(mapper)?
                .0
            }
            None => {
                // C-01 S9b: with 206 retired there is no 206 seed for an EVM-active parent. Unlike the
                // verifier (which HALTs to avoid a fork), a PRODUCER failure must never crash the node —
                // fail THIS template build and let the miner retry. A header-store read error is treated
                // the same (we cannot prove pre-activation, and `unwrap_or(false)` would wrongly let an
                // EVM-active parent fall through to the absent-206 path). Pre-activation (Ok(false)) needs
                // no 206 and proceeds via `evm_execute_acceptance` (empty parent).
                if self.evm_retire_206 {
                    match self.evm_header_store.has(selected_parent) {
                        Ok(false) => {} // pre-activation: empty parent, no 206 read
                        Ok(true) => {
                            return Err(RuleError::EvmTemplateExecutionFailed(format!(
                                "--evm-retire-206: no flat/reconstruct seed for EVM-active selected parent {selected_parent} (206 retired); \
                                 cannot build a template this round — retrying"
                            )));
                        }
                        Err(e) => {
                            return Err(RuleError::EvmTemplateExecutionFailed(format!(
                                "--evm-retire-206: EVM header store read failed for selected parent {selected_parent} ({e}); cannot build a template this round"
                            )));
                        }
                    }
                }
                // audit R2-#4: a producer-side acceptance failure (e.g. a local EVM
                // store-integrity error) is a template-build failure, not a panic.
                evm_execute_acceptance(
                    &self.evm_header_store,
                    &self.evm_state_store,
                    &self.evm_payload_store,
                    selected_parent,
                    &sorted_mergeset,
                    &header,
                    &own_payload,
                    self.evm_gas_pool_v2_activation_daa_score,
                    self.evm_f002_withdraw_cap_activation_daa_score,
                    self.evm_f003_mldsa_verify_activation_daa_score,
                    self.evm_typed_receipt_root_activation_daa_score,
                    user_gas_cap,
                    market.clone(),
                )
                .map_err(mapper)?
                .0
            }
        };
        let mut header = header.with_evm_payload_hash(own_payload.payload_hash()).with_evm_commitment(result.header.commitment_root());
        // §9: the validator folds the bridge's UTXO side-effects (consumed
        // deposit locks + materialized withdrawals) into THIS block's diff and
        // checks them against `header.utxo_commitment` — so the PRODUCER must
        // fold the identical effects into the template's commitment (the
        // template inherited the virtual multiset, which has none of them).
        // Found live: the first claim-bearing template self-disqualified.
        if !consumed_locks.is_empty() || !result.withdrawals.is_empty() || !expected_settlements.is_empty() {
            let mut multiset = virtual_state.multiset.clone();
            let mut scratch_diff = kaspa_consensus_core::utxo::utxo_diff::UtxoDiff::default();
            crate::processes::evm::apply_evm_bridge_effects(
                &mut scratch_diff,
                &mut multiset,
                header.daa_score,
                &consumed_locks,
                &result.withdrawals,
            )
            .expect("template bridge effects mirror validation on already-validated inputs");
            crate::processes::evm::apply_evm_market_effects(
                &mut scratch_diff,
                &mut multiset,
                header.daa_score,
                template_selected_parent,
                &expected_settlements,
            )
            .expect("template market effects mirror validation on already-validated inputs");
            header.utxo_commitment = multiset.finalize();
            header.finalize();
        }
        Ok((header, own_payload, stale_claims))
    }

    /// Non-`evm` builds cannot produce evm-active templates (same refusal as
    /// the validation seam); unreachable on every default network.
    #[cfg(not(feature = "evm"))]
    fn evm_template_fields(
        &self,
        header: Header,
        _virtual_state: &VirtualState,
        _evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
        _prepared_claims: crate::processes::evm::PreparedDepositClaims,
    ) -> Result<
        (
            Header,
            kaspa_consensus_core::evm::EvmExecutionPayload,
            Vec<(kaspa_consensus_core::tx::TransactionOutpoint, EvmClaimStaleKind)>,
        ),
        RuleError,
    > {
        if header.daa_score >= self.evm_activation_daa_score {
            panic!(
                "the EVM lane is active at DAA {} but this kaspad was built without the `evm` feature — cannot build a valid template (rebuild with --features evm)",
                header.daa_score
            );
        }
        Ok((header, Default::default(), vec![]))
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
        palw_v2_staged: Option<(kaspa_hashes::Hash64, kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2)>,
    ) {
        let mut batch = WriteBatch::default();
        if let Some((state_root, delta)) = palw_v2_staged {
            let mut store = self.palw_state_v2_store.write();
            store.insert_delta_batch(&mut batch, current, state_root, &delta).unwrap();
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
        if !rewarded_keys.is_empty() {
            self.rewarded_epochs_store.insert_batch(&mut batch, current, Arc::new(rewarded_keys)).unwrap();
        }
        if quality_subpool > 0 {
            self.block_quality_pool_store.insert_batch(&mut batch, current, quality_subpool).unwrap();
        }
        if reserve_balance > 0 {
            self.reserve_balance_store.insert_batch(&mut batch, current, reserve_balance).unwrap();
        }
        // Note we call idempotent since this field can be populated during IBD with headers proof
        self.pruning_samples_store.insert_batch(&mut batch, current, pruning_sample_from_pov).idempotent().unwrap();
        let write_guard = self.statuses_store.set_batch(&mut batch, current, StatusUTXOValid).unwrap();
        self.db.write(batch).unwrap();
        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(write_guard);
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
        let mut ctx = UtxoProcessingContext::new((&virtual_ghostdag_data).into(), selected_parent_multiset);

        // Calc virtual DAA score, difficulty bits and past median time
        let virtual_daa_window = self.window_manager.block_daa_window(&virtual_ghostdag_data)?;
        let virtual_bits = self.window_manager.calculate_difficulty_bits(&virtual_ghostdag_data, &virtual_daa_window);
        let virtual_past_median_time = self.window_manager.calc_past_median_time(&virtual_ghostdag_data)?.0;

        // Audit C-08: virtual acceptance runs the same bond-spend filter the chain walk runs, or
        // the template would build on a UTXO view that includes a spend every validating node
        // skips. Read from the store tip, which IS virtual's selected parent — and only when the
        // tip really names it, because a tip on another branch is not this candidate's registry.
        let virtual_palw_state = self
            .palw_state_params_v2
            .as_ref()
            .and_then(|params| self.palw_state_v2_store.read().load_tip(params).ok().flatten())
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
            virtual_daa_window.mergeset_non_daa,
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

        // Flush the batch changes
        self.db.write(batch).unwrap();

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

    /// Re-derives the [`BondMutation`]s a chain block contributed, from its
    /// retained acceptance data (ADR-0009 Addendum A.4). Deterministic, so it
    /// serves both apply (added) and revert (removed).
    fn dns_bond_mutations_for_chain_block(&self, chain_block: BlockHash, bond_view: &ActiveBondView) -> Vec<BondMutation> {
        let accepted_daa_score = self.headers_store.get_header(chain_block).unwrap().daa_score;
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        let txs = self.accepted_txs_of_chain_block(chain_block);
        self.dns_bond_mutations_from_txs(&txs, bond_view, accepted_daa_score, min_bond, unbonding_floor)
    }

    /// Shared tail of the two bond-mutation derivations: map accepted txs to mutations, then —
    /// at and above `unbond_authz_mergeset_activation_daa_score` — drop `Unbond` mutations whose
    /// ML-DSA-87 signature does not verify (incident 2026-08-07).
    ///
    /// The own-body `unbond_request_authorized` block gate never sees a request that arrives via
    /// the MERGESET, so authorization has to be re-established at acceptance. This half checks the
    /// signature; the owner-to-record binding is enforced in `ActiveBondView::apply`/`revert` and
    /// in `stage_dns_bond_mutations`, which have the record. Both halves are symmetric between
    /// apply and revert: this one reads only the block's own accepted txs (no chain view), so it
    /// returns the same set every time it is re-derived.
    fn dns_bond_mutations_from_txs(
        &self,
        txs: &[Transaction],
        bond_view: &ActiveBondView,
        accepted_daa_score: u64,
        min_bond: u64,
        unbonding_floor: u64,
    ) -> Vec<BondMutation> {
        let enforce = self.dns_params.as_ref().is_some_and(|p| accepted_daa_score >= p.unbond_authz_mergeset_activation_daa_score);
        let mut muts = bond_mutations_from_accepted_txs(txs, accepted_daa_score, min_bond, unbonding_floor, enforce);

        // 2026-08-11 audit P0: evidence that arrives by MERGE was never signature-checked — the
        // three genuineness rules are block-validity gates over the block's OWN body, while these
        // mutations come from everything it ACCEPTS. Drop any `Slash` whose evidence is not
        // proved, exactly as the H-05 half above drops an unauthorized `Unbond`. See
        // `proved_slash_targets` for why this is symmetric between apply and revert.
        if let Some(params) = self.dns_params.as_ref()
            && accepted_daa_score >= params.dns_activation_daa_score
        {
            let proved = super::utxo_validation::proved_slash_targets(
                txs,
                bond_view,
                self.genesis.hash,
                accepted_daa_score,
                params.evidence_window_blocks,
            );
            muts.retain(|m| match m {
                BondMutation::Slash(outpoint, _) => proved.contains(outpoint),
                _ => true,
            });
        }
        if enforce {
            let net_id = self.genesis.hash;
            let signed: std::collections::HashSet<(TransactionOutpoint, Hash64)> = unbond_requests_from_accepted_txs(txs)
                .into_iter()
                .map(|(_, req)| req)
                .filter(|req| {
                    let digest = unbond_request_message(net_id.as_byte_slice(), req.bond_outpoint);
                    matches!(
                        verify_mldsa87_with_context(&req.owner_pubkey, &digest.as_bytes(), &req.signature, UNBOND_REQUEST_CONTEXT),
                        Ok(true)
                    )
                })
                .map(|req| (req.bond_outpoint, validator_id_from_pubkey(&req.owner_pubkey)))
                .collect();
            muts.retain(|m| match m {
                BondMutation::Unbond(outpoint, _, Some(claimed)) => signed.contains(&(*outpoint, *claimed)),
                _ => true,
            });
        }
        muts
    }

    /// The per-bond acceptance floors (min stake amount, min unbonding window) from the network's
    /// `DnsParams`, or `(0, 0)` where the overlay is off — so the bond-acceptance filter is a no-op
    /// on networks without `dns_params`.
    pub(super) fn dns_bond_floors(&self) -> (u64, u64) {
        self.dns_params.as_ref().map(|p| (p.min_bond_amount_sompi, p.unbonding_period_blocks)).unwrap_or((0, 0))
    }

    /// Resolves a chain block's accepted transactions from its acceptance data
    /// (`acceptance_data_store` → `block_transactions_store[index_within_block]`).
    /// Shared by the bond-population (A.4) and StakeScore-aggregation (A.5) passes,
    /// AND (with `--features evm`) the EVM lane.
    ///
    /// Tolerates missing acceptance data → no accepted transactions. A chain block has no committed
    /// acceptance data only when it is the imported pruning point (UTXO-set IBD writes the multiset
    /// but never acceptance data) or a pruned ancestor that a bounded backward overlay walk reaches.
    /// Every overlay reader funnels through here, so guarding the shared helper covers them all (the
    /// per-caller sink guard in `update_dns_state` was not enough: a NORMAL recompute walk legitimately
    /// reaches the pruning point). Returning empty is semantically correct — a block with no
    /// accountable acceptance data contributes no txs; a genuine inconsistency on a non-pruned block
    /// surfaces in the trace log instead of crashing the virtual processor.
    /// **Unit D: the ONE fork-choice authority, as this node reads it for a candidate.**
    ///
    /// Every V2 selection site — virtual tip, IBD commit, the pruning ceiling, the deep-reorg gate
    /// — orders candidates by `PalwCandidateOrderV1` and nothing else. The point of the unit is
    /// that they order by the SAME thing: P0-5 is two canonical-chain views inside one node, and
    /// wiring one site would create exactly that. So there is one constructor, here, and the four
    /// consumers below take its output.
    ///
    /// `None` on a network with no V2 bundle, and every consumer falls through to blue work there
    /// — which is every shipped preset. A candidate this node cannot weigh is also `None`, and
    /// that is deliberately NOT "zero": a chain nobody could weigh must not outrank one that was
    /// weighed and found small.
    pub(crate) fn palw_candidate_order_v2(
        &self,
        candidate: BlockHash,
    ) -> Option<kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1> {
        let state = self.palw_candidate_state_v2(candidate)?;
        let (frontier_blue_score, _) = state.safe_frontier();
        Some(kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1::new(
            frontier_blue_score,
            state.safe_weight(),
            state.bounded_immature(),
            candidate,
        ))
    }

    /// **ADR-0065 D2 — a deep reorg may not rest on bonds the fork minted for itself.**
    ///
    /// `palw_fork_choice` orders matured work ahead of everything else on a stated invariant: *a
    /// fork nobody could see collects no receipts, so it has no frontier however much unproven work
    /// it piles up.* That was false. A holder of one cheap bond could fork, fold sybil
    /// `BondRegistered` objects into its own blocks, seat panels from them, self-license, and grow
    /// `safe_frontier` in private — the frontier is a per-branch fold, and the branch supplies both
    /// the bonds and the receipts.
    ///
    /// D2 was first written as a rule about the frontier ADVANCE, which is unimplementable: the
    /// advance happens inside a pure single-chain fold whose result is hashed into `state_root`, so
    /// a value that depended on a fork point would depend on which competing branch a node holds,
    /// and two nodes would compute different roots for one block. It belongs at the COMPARISON,
    /// which is this function — the one site holding both tips inside one consensus instance.
    ///
    /// **Provenance needs no new state, because the registry is append-only.** `write_bond`'s
    /// `None` branch has no callers (ADR-0065 D5 makes that a decision), so bonds only ever
    /// accumulate along a chain, and therefore
    ///
    /// > *registered after the fork point* ⟺ *in the challenger's registry and not in the
    /// > ancestor's*
    ///
    /// — a set difference between two states this node materializes from its own stores. No
    /// `registered_daa` is read, so the blue-score-versus-DAA unit mismatch that blocked the
    /// earlier design never arises.
    ///
    /// **Everything here is node-local.** Both states come from this node's root-verified tip and
    /// its own reachability walk; the peer proposing the reorg supplies none of it.
    pub(crate) fn palw_frontier_provenance_outcome(&self, candidate: BlockHash, prev_sink: BlockHash) -> DnsReorgOutcome {
        use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;

        // Read at the INCUMBENT's DAA: a candidate's own score is attacker-chosen, and this is the
        // same one-sided clock the confirmed-anchor TTL uses a few hundred lines below.
        let incumbent_daa = self.headers_store.get_daa_score(prev_sink).unwrap_or(0);
        if !self.palw_frontier_provenance.is_some_and(|fence| fence.is_active(incumbent_daa)) {
            return DnsReorgOutcome::GateInactive;
        }
        let Some(panel_params) = self.palw_panel_params_v2.as_ref() else { return DnsReorgOutcome::GateInactive };

        // **A veto that cannot name a fork point abstains.** The horizon is `finality_depth`, which
        // is exactly the deepest reorg the sink search will ever offer, and it is canonical-side —
        // it bounds how far THIS node would rewind, not how long the challenger's branch is, which
        // is the right metric for an attack whose private branch may be thousands of blocks while
        // the canonical-side fork depth is one. Refusing on a missing ancestor would be the
        // permanent-partition shape this file already learned to escape.
        let horizon = self.finality_depth;
        let Some(ancestor) = self.chain_common_ancestor_within(candidate, prev_sink, horizon) else {
            warn!("PALW frontier provenance abstains: no common ancestor for {candidate} and {prev_sink} within {horizon}");
            return DnsReorgOutcome::GateInactive;
        };
        // **An absence abstains; a FAULT refuses.** Both used to arrive as `None` and both
        // abstained, so a tip snapshot this node could not read turned D2's veto off — the gate
        // failing open on exactly the state that says it cannot judge. A missing ancestor or a
        // pruned delta is genuinely an absence and still abstains, for the reason the horizon
        // paragraph above gives; a store that will not read is this node's own fault and the
        // honest answer to "may this reorg proceed" is no.
        let (challenger_state, ancestor_state) =
            match (self.palw_candidate_state_v2_checked(candidate), self.palw_candidate_state_v2_checked(ancestor)) {
                (Ok(challenger), Ok(ancestor_state)) => (challenger, ancestor_state),
                (Err(PalwWeighFaultV2::StoreUnreadable), _) | (_, Err(PalwWeighFaultV2::StoreUnreadable)) => {
                    warn!(
                        "deep reorg refused: this node's own PALW snapshot is unreadable, so ADR-0065 D2 cannot clear \
                         candidate {candidate} — a veto that cannot read its evidence does not wave the reorg through"
                    );
                    return DnsReorgOutcome::FrontierProvenanceViolation;
                }
                _ => {
                    warn!("PALW frontier provenance abstains: this node cannot materialize both sides of the fork at {ancestor}");
                    return DnsReorgOutcome::GateInactive;
                }
            };

        let minted: std::collections::BTreeSet<_> =
            challenger_state.bonds_iter().map(|(k, _)| *k).filter(|k| ancestor_state.bond(k).is_none()).collect();
        if minted.is_empty() {
            return DnsReorgOutcome::GateInactive;
        }

        // **The fast path is exact, not an approximation.** `PalwPanelParamsV2::new` enforces
        // `2*quorum > seat_count`, so a panel cannot reach quorum out of newly-minted seats unless
        // more than `seat_count - quorum` of them exist. Below that threshold no scan can find a
        // violation, so not scanning is a proof rather than a shortcut. On the shipped (5, 3) that
        // is 2, so a static registry never pays for the walk.
        if !kaspa_consensus_core::palw_panel_v2::palw_minted_seats_can_reach_quorum_v1(minted.len(), panel_params) {
            return DnsReorgOutcome::GateInactive;
        }

        // Past it, the panels themselves. Read off the DELTAS rather than the challenger's tip
        // state: `retire_claim` deletes a claim and its panel while `safe_frontier` never retreats,
        // so the frontier can stand on a panel the tip no longer holds. The deltas record every
        // panel this branch ever bound, verbatim.
        let path = self.dag_traversal_manager.calculate_chain_path(ancestor, candidate, None);
        let store = self.palw_state_v2_store.read();
        for block in path.added.iter() {
            let Ok((_, delta)) = store.delta_of(*block) else {
                drop(store);
                warn!("PALW frontier provenance abstains: delta for {block} is unavailable");
                return DnsReorgOutcome::GateInactive;
            };
            for entry in delta.entries.iter() {
                let PalwDeltaEntryV2::Panel { new: Some(panel), .. } = entry else { continue };
                let seated_new = panel.seats.iter().filter(|seat| minted.contains(&seat.bond)).count();
                if seated_new >= panel_params.quorum() as usize {
                    drop(store);
                    warn!(
                        "deep reorg refused (ADR-0065 D2): candidate {candidate} seated {seated_new} of {} panel seats \
                         from bonds registered after the fork point {ancestor} — its matured work is its own",
                        panel.seats.len()
                    );
                    return DnsReorgOutcome::FrontierProvenanceViolation;
                }
            }
        }
        DnsReorgOutcome::GateInactive
    }

    /// **The PALW state AT a chain block**, materialized from this node's own stores.
    ///
    /// Split out of [`Self::palw_candidate_order_v2`], which built a whole `PalwChainStateV2` and
    /// then kept three numbers out of it. ADR-0065 D2 needs the rest — specifically the bond
    /// registry — and materializing it twice would be two walks where the walk is the cost.
    ///
    /// Node-local throughout: the tip is this node's root-verified store row and the path is its
    /// own reachability, so nothing here is supplied by the peer proposing the reorg.
    pub(crate) fn palw_candidate_state_v2(
        &self,
        candidate: BlockHash,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwChainStateV2> {
        self.palw_candidate_state_v2_checked(candidate).ok()
    }

    /// [`Self::palw_candidate_state_v2`] with the two reasons it can answer nothing kept APART.
    ///
    /// The `Option` twin above spelled them the same way — one `.ok().flatten()` over `load_tip`,
    /// so a snapshot a disk corrupted or a row written by another schema became "no V2 tip yet".
    /// That is the identical conflation the pruning ceiling beside it was fixed for, and it is not
    /// harmless here either: [`Self::palw_frontier_provenance_outcome`] reads `None` as
    /// `GateInactive`, which is ALLOW, so ADR-0065 D2's veto abstained on a deep reorg exactly
    /// when it had lost the ability to weigh one — a fork-choice gate failing open on its own
    /// state fault. The gates that can refuse now get the distinction; the ones for which both
    /// answers really are a refusal keep taking the `Option`.
    pub(crate) fn palw_candidate_state_v2_checked(
        &self,
        candidate: BlockHash,
    ) -> Result<kaspa_consensus_core::palw_state_v2::PalwChainStateV2, PalwWeighFaultV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Err(PalwWeighFaultV2::NoOpinion) };
        let store = self.palw_state_v2_store.read();
        let (tip_block, tip_state) = match store.load_tip(state_params) {
            Ok(Some(loaded)) => loaded,
            Ok(None) => return Err(PalwWeighFaultV2::NoOpinion),
            Err(e) => {
                warn!("PALW state fault: this node's own V2 tip snapshot could not be read ({e}); it can weigh no candidate");
                return Err(PalwWeighFaultV2::StoreUnreadable);
            }
        };
        // **Both ends of the walk must be blocks this consensus can reach, or the walk is a
        // panic.** `calculate_chain_path` asks `is_chain_ancestor_of`, which `unwrap`s the
        // reachability lookup — `KeyNotFound` there is `process::exit(1)`, not an error.
        //
        // A STAGING consensus is built `skip_adding_genesis`, so it has no virtual state row and
        // `get_sink()` answers the all-zero hash, which is not a block. It has a PALW tip, because
        // the headers-proof IBD imports the peer's pruning-point carriage immediately before asking
        // this question — so the tip load succeeds and the candidate is the zero hash. Every node
        // taking that path past its own genesis died on the next line; a peer can steer a victim
        // into it by advertising a chain whose pruning point the victim lacks.
        //
        // No order is the honest answer for a candidate this consensus does not HOLD. But `None`
        // is not "no opinion" to a caller — the IBD gate and the deep-reorg gate both refuse on it
        // — so answering it for a consensus that merely has no virtual sink turned the crash into
        // a permanent IBD refusal. That case is not this one: see [`Self::palw_weighing_point_v2`],
        // which never offers a candidate this function would have to answer nothing about.
        let unknown = |block: BlockHash| !self.reachability_service.has_reachability_data(block);
        if unknown(candidate) || unknown(tip_block) {
            return Err(PalwWeighFaultV2::NoOpinion);
        }
        // The order must be the CANDIDATE's, not the sink's, or every candidate would compare
        // equal and the authority would be a constant (P0-4 in fork-choice clothing). The walk
        // from the materialized tip to the candidate is what makes it candidate-scoped.
        let path = self.dag_traversal_manager.calculate_chain_path(tip_block, candidate, None);
        let removed: Vec<BlockHash> = path.removed.to_vec();
        let added: Vec<BlockHash> = path.added.to_vec();
        drop(store);
        let store = self.palw_state_v2_store.read();
        // **The same two-way split, one level down.** Removing the `.ok().flatten()` from
        // `load_tip` above and leaving `map_err(|_| NoOpinion)` here would have moved the
        // conflation rather than closed it: `load_delta` names an undecodable row
        // `StoreError::DataInconsistency` precisely so it is never reported absent (the store's
        // own words: "the same refusal by another name"), and collapsing it into an abstention
        // hands `palw_frontier_provenance_outcome` a `GateInactive` — ALLOW — for a node whose tip
        // reads back perfectly and whose delta rows are corrupt. A pruned or absent delta really
        // is an absence and still abstains; anything the STORE refused, and any delta of this
        // node's own that will not compose with this node's own state, is a fault.
        crate::processes::palw_state_walk::walk_chain_path(&store, state_params, tip_state, &removed, &added).map_err(|e| {
            use crate::processes::palw_state_walk::PalwStateWalkError as W;
            match e {
                W::MissingDelta(_) | W::NoAnchor => PalwWeighFaultV2::NoOpinion,
                W::Store(_) | W::State(_) => {
                    warn!("PALW state fault: this node's own V2 delta rows would not walk ({e}); it can weigh no candidate");
                    PalwWeighFaultV2::StoreUnreadable
                }
            }
        })
    }

    /// **The block whose PALW standing represents this consensus** — its virtual sink, normally.
    ///
    /// A STAGING consensus has no sink to be weighed at. It is built `skip_adding_genesis`, so it
    /// holds no virtual state row and `get_sink()` answers the all-zero hash, which is not a block
    /// any consensus can reach. Weighing it there is weighing it at nothing, and nothing is not an
    /// abstention at either consumer: `validate_staging_palw_order` answers `(Some(_), None)` with
    /// a hard `ProtocolError` and the deep-reorg gate answers `(None, _)` with
    /// `DominanceViolation`. So every headers-proof sync on a ConsensusV2 network was refused
    /// against every peer, permanently, and the pruning-point PALW import placed immediately
    /// before that gate "so the challenger becomes a real value" was dead work. Not crashing was
    /// necessary and not sufficient.
    ///
    /// What a staged chain CAN be weighed at is the pruning point whose carriage this node has
    /// just root-verified against the witness child's header (`import_pruning_point_palw_state`)
    /// — the deepest block of that chain whose PALW state this node has actually checked, and the
    /// block the tip row names. Weighing it further forward is not available: the deltas the walk
    /// needs are written by virtual processing, which staging has not run.
    ///
    /// Deliberately NOT the header download hint. For a consensus that HAS a sink the sink is the
    /// answer, because the hint is a download heuristic that says so in its own name and a
    /// fork-choice decision reading it is P0-5. The fallback fires only where the sink is not a
    /// block at all.
    pub(crate) fn palw_weighing_point_v2(&self, sink: BlockHash) -> Option<BlockHash> {
        if self.reachability_service.has_reachability_data(sink) {
            return Some(sink);
        }
        let state_params = self.palw_state_params_v2.as_ref()?;
        // **`load_tip`, not `tip_record`** — fail CLOSED on a snapshot that cannot be read, and
        // the raw row cannot tell you that. `set_tip_batch` derives the root from the state it is
        // handed, so a tampered carriage is detected only by rebuilding it and demanding the
        // recorded root, which is what `load_tip` does. Reading just the row would hand a chain
        // whose state this node cannot reproduce back as its own standing — a state fault
        // promoted to the only opinion this consensus has.
        let tip = match self.palw_state_v2_store.read().load_tip(state_params) {
            Ok(Some((block, _))) => block,
            Ok(None) => return None,
            Err(e) => {
                warn!("PALW state fault: the V2 tip snapshot is unreadable ({e}); this consensus offers no weighing point");
                return None;
            }
        };
        self.reachability_service.has_reachability_data(tip).then_some(tip)
    }

    /// Unit D, site 3: whether a proposed pruning point respects the safe frontier.
    ///
    /// History under trial is not prunable — an unresolved claim's history and an open court's
    /// committed roots are evidence, and pruning them would make a dispute unadjudicable by
    /// deletion. `true` on a network with no V2 bundle: there is no frontier to respect.
    pub(crate) fn palw_pruning_point_allowed_v2(&self, candidate_point: BlockHash) -> bool {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return true };
        let store = self.palw_state_v2_store.read();
        // **Fail CLOSED on what cannot be read, and OPEN only on what genuinely is not there yet.**
        //
        // This was one `.ok().flatten()`, which spells both answers the same way: a `load_tip` that
        // returns `Err` — a corrupted snapshot, or one written by another schema — became `None`
        // became "every pruning point allowed", so the gate deleted the history under trial exactly
        // when it had lost the ability to weigh it. The deep-reorg gate beside it was already fixed
        // for this shape, and records it: the gate failed open on exactly the candidate it could not
        // weigh. `Ok(None)` is different and stays open — a V2 chain before its first tip has no
        // frontier to respect, and refusing there would mean a fresh node could never prune.
        let state = match store.load_tip(state_params) {
            Ok(Some((_, state))) => state,
            Ok(None) => return true,
            Err(_) => return false,
        };
        let Ok(header) = self.headers_store.get_header(candidate_point) else { return false };
        let (frontier, _) = state.safe_frontier();
        kaspa_consensus_core::palw_fork_authority_v2::pruning_point_allowed_v2(header.blue_score, frontier)
    }

    /// **ADR-0042 Decision 10's consumer: what this block's PALW carve is, and whether anyone may
    /// spend it.**
    ///
    /// The decision is an ESCROW, not a split: `block accepted → escrow → Final → spendable`, with
    /// `Voided → forfeit` (burned). So the question a coinbase must ask is never "what is the
    /// carve" alone — it is "which claims BECAME spendable on this chain", and the answer is a
    /// fold over candidate state, not a property of the block being built.
    ///
    /// Returns `(claim, worker_carve_sompi)` for every claim this candidate has matured. A block's
    /// own claim is `Provisional` at the moment its coinbase is built, so it is never in its own
    /// list — which is Decision 10 stated as arithmetic rather than as a rule to remember.
    ///
    /// **What this deliberately does not do is pay — and the reason is no longer "there is no
    /// payee".** This block used to say a `PalwBondStateV2` carries a pubkey and collateral but no
    /// payout script; it carries `payout_payload`, registration refuses an empty one
    /// (`palw_state_v2.rs`, the `BondRegistered` arm), `finalize_claim` writes
    /// `PalwPayoutV2 { payload: bond.payout_payload, amount: claim.escrowed_reward }`, and
    /// `palw_v2_payout_outputs` below renders that queue into real coinbase outputs. The
    /// registration-object change this paragraph was waiting for LANDED, and leaving the sentence
    /// standing cost something concrete: the ADR-0062 SA-7 review cited this line as proof that no
    /// party can be paid, and deferred the data-availability accuser's reward on it.
    ///
    /// What remains true is narrower and worth keeping: this function is the ELIGIBILITY and the
    /// AMOUNT, and the outputs are built one function down from a queue the transition wrote —
    /// nothing here invents a payee. The queue itself is generic: neither `pending_payouts_iter`
    /// nor the drain asks which phase enqueued an entry, so a payout to some party other than the
    /// producer (an accuser that voided a claim, say) needs a transition that writes one, not a
    /// new mechanism on the paying side.
    /// **The escrows a block's coinbase must pay, from its parent's committed queue.**
    ///
    /// `state` is the SELECTED PARENT's state, not this block's: a claim that reaches `Final` is
    /// enqueued by the transition that finalized it and paid by the NEXT block. That one-block
    /// lag is what makes the payout computable at all — a coinbase is fixed before its own
    /// block's PALW transition runs, so a block cannot pay what it is about to decide.
    ///
    /// Everything the amount depends on was decided when the claim was accepted (its escrow is a
    /// snapshot of THAT block's subsidy) and everything the payee depends on was decided when it
    /// finalized (the payload is copied out of the bond at release). So this function has no
    /// policy left in it: it renders a committed list, in `BTreeMap` key order, and two nodes
    /// holding the same parent state cannot produce different bytes.
    ///
    /// Scripts are DERIVED from the payload, never carried — see `PalwBondStateV2::payout_payload`
    /// for why a registrant may not write a coinbase script.
    /// **Audit C-08: the bond collateral outpoints this block may not let anyone spend.**
    ///
    /// A `PalwBondKeyV2` is the outpoint holding the collateral, and until this existed nothing
    /// kept the money there: the genesis gate could check that an outpoint held what a bond
    /// declared, and the owner could spend it in block 1. Every exposure ceiling and every slash
    /// was then denominated in a balance the bond no longer had — which is the whole of C-08.
    ///
    /// `state` is the SELECTED PARENT's, exactly like `palw_v2_payout_outputs` beside it: the
    /// registry it reads is a committed fact of the block's own past, so two nodes validating the
    /// same block resolve the same set however their virtual tips are placed. (That placement is
    /// what made the audit-call gate a partition risk before it was moved onto the block's chain;
    /// this one is on the block's chain by construction.)
    ///
    /// Built once per block rather than once per spending input.
    /// The producer contract's chain half (ADR-0042): the facts an attempt is refused for
    /// getting wrong, read at the point a block template builds on.
    ///
    /// **The chain point is virtual's selected parent and the DAA score is the CANDIDATE's.** The
    /// store tip is the selected parent's state, so the epoch a producer checks its budget against
    /// must be the one the candidate will be admitted in — reading the tip's would put a producer
    /// one epoch behind at every boundary, mining into a refusal it could not see the reason for.
    /// The seat duties this node holds at the state store's tip (launch blockers §2).
    /// Claims this node could still dispute — licensed, not its own, and not already under a
    /// session of its own.
    // ---------------------------------------------------------------------------------------
    // **Every read-side `palw_*_impl` below takes `load_tip_cached`, and that is the rule, not a
    // property of the sites that happen to** (audit M-7, re-opened by mainnet audit H-1). These
    // are the answers `ConsensusApi` hands the RPC service and the p2p serve flows, so an
    // uncached read here is a borsh decode of the whole carriage, `rebuild_deadline_free_indices`,
    // `rebuild_deadline_index_v2`, two consistency walks and a full `state_root()` per
    // unauthenticated ~200-byte request. `load_tip` stays for the fold and restart paths only —
    // `calculate_utxo_state_relatively`, `calculate_virtual_state`,
    // `palw_candidate_state_v2_checked`, `palw_weighing_point_v2`, `palw_pruning_point_allowed_v2`
    // and `capture_pruning_point_palw_state` — which materialize in order to WALK or to WRITE and
    // need an owned state. A new read path added on `load_tip` is caught by
    // `palw_v2_no_read_side_impl_takes_an_uncached_tip_materialization`, which measures decoded
    // carriage bytes rather than trusting this comment.
    // ---------------------------------------------------------------------------------------
    pub fn palw_disputable_claims_v2_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten() else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_producer_v2::palw_disputable_claims_v2(&state, mine)
    }

    /// **What verdict would this proof produce, at this node's tip?**
    ///
    /// A `CourtClosed` must ANNOUNCE the verdict the evidence supports — the pipeline derives it
    /// and refuses an object that names a different one — so a party assembling a close has to
    /// know the answer before it spends a fee on it. Asking the node is also the honest ordering:
    /// the party that assembled the evidence does not get to be the party that decides what it
    /// means.
    pub fn palw_court_close_verdict_v2_impl(
        &self,
        session_id: &kaspa_consensus_core::Hash64,
        proof: &kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let court = self.palw_court_params_v2.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        // The tip's DAA: this is the verdict the close WOULD get if it rode the next block.
        let daa_score = self.virtual_stores.read().state.get().ok()?.daa_score;
        let step_ladder = self.palw_court_step_ladder_at(daa_score, court);
        let form = self.palw_prompt_ids_form_at(daa_score);
        kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(
            &state,
            session_id,
            proof,
            court,
            step_ladder,
            form,
            self.palw_held_context_at(daa_score),
        )
        .ok()
    }

    /// The court's half of [`Self::palw_seat_duties_v2_impl`]: the open sessions this node is a
    /// party to. Read at the same tip, for the same reason — a duty derived at a point the node is
    /// not standing on is a duty about a chain it is not on.
    pub fn palw_court_duties_v2_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten() else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_producer_v2::palw_court_duties_v2(&state, mine)
    }

    /// **The data-availability duties this node holds** (ADR-0062 D3): every claim under an open
    /// accusation whose producing bond is in `mine`, with the event it must open.
    pub fn palw_da_duties_v2_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwDaDutyV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten() else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_producer_v2::palw_da_duties_v2(&state, state_params, mine)
    }

    /// **Who may be served a claim's private material**, at the tip (ADR-0077 Decision 16's
    /// transport half) — see `PalwChainStateV2::claim_readers_v2`.
    pub fn palw_claim_readers_v2_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten() else {
            return Vec::new();
        };
        state.claim_readers_v2(&claim)
    }

    /// A claim's committed roots and price at the tip (ADR-0111 Decision 2) — see the trait doc.
    pub fn palw_claim_roots_v2_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Option<(kaspa_consensus_core::Hash64, kaspa_consensus_core::Hash64, u64)> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        state.claim(&claim).map(|c| (c.execution_root, c.trace_root, c.work_leaves))
    }

    /// The payout payload the chain has registered for `bond`, if it is registered at all.
    pub fn palw_bond_payout_payload_v2_impl(
        &self,
        bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::Hash64> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        state.bond(bond).map(|b| b.payout_payload)
    }

    /// The bond this key already registered, if any. See the trait doc: this is what keeps a
    /// left-in `--palw-register-bond` from locking collateral again on every restart.
    /// The class table as an operator needs it. See the trait doc: share and budget are read
    /// together because "budget 0" alone cannot say whether the class was never granted share.
    pub fn palw_v2_class_table_impl(&self) -> Vec<kaspa_consensus_core::palw_state_v2::PalwClassRowV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Ok(Some((_, state))) = self.palw_state_v2_store.read().load_tip_cached(state_params) else { return Vec::new() };
        // The PALW state's own last point, not the virtual store's: the two can disagree while a
        // node is coming up, and reading the store's default of 0 answers about genesis while
        // looking like an answer about now.
        let daa = state.last_point().map(|p| p.daa_score).unwrap_or(0);
        let epoch_index = daa / state_params.epoch_length();
        let budgets = state.epoch_budgets().filter(|b| b.epoch_index == epoch_index);
        state
            .classes_iter()
            .map(|(id, record)| kaspa_consensus_core::palw_state_v2::PalwClassRowV2 {
                class_id: *id,
                status: format!("{:?}", record.status),
                share_permille: state.class_share_permille(id),
                budget_blocks: budgets.and_then(|b| b.budget_blocks.get(id).copied()).unwrap_or(0),
                canonical_leaves: record.pwu_rule.canonical_leaves_v1(),
                is_base_class: *id == state_params.base_class_id(),
                artifact_root: record.artifact_root,
                // The producer facts' own derivation, so the two reads cannot disagree.
                fp_certified: state_params.fp_certified_classes().is_none_or(|set| set.contains(id))
                    || state.fp_lane_certification(id).is_some(),
                held: state.class_is_held_v1(id),
                registered_daa: record.registered_daa,
            })
            .collect()
    }

    /// ADR-0131 Decision 1: the class census at the PALW state's own tip.
    pub fn palw_class_census_v1_impl(&self) -> Option<kaspa_consensus_core::palw_economic_compute_v1::PalwClassCensusReadV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        // ADR-0132: the tip's `bits` prices the network draw every class win faces at this height.
        let network_bits = self.headers_store.get_header(tip).map(|h| h.bits).unwrap_or(0);
        Some(kaspa_consensus_core::palw_economic_compute_v1::palw_class_census_v1(&state, state_params, network_bits))
    }

    /// ADR-0135: the model registry as the tip state holds it — every class's lifecycle row, the
    /// seats ready for it now, the claims in flight, and every seat's last possession proof.
    pub fn palw_model_registry_v1_impl(&self) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryReadV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let tip_daa = self.headers_store.get_header(tip).map(|h| h.daa_score).unwrap_or(0);
        let fold = self.palw_model_registry_fold_at(tip_daa);
        let tip_point =
            kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 { block: tip, daa_score: tip_daa, blue_score: 0, subsidy: 0 };
        let work = self.palw_work_target_fold_for(&tip_point);
        Some(kaspa_consensus_core::palw_model_registry_v1::palw_model_registry_read_v1(
            &state,
            state_params,
            tip_daa,
            self.palw_model_registry.map(|f| f.daa_score()),
            fold.as_ref(),
            work.as_ref(),
        ))
    }

    /// Class panel status, bonded seats, and per-claim assignments at the tip.
    pub fn palw_panel_network_view_v1_impl(&self) -> Option<kaspa_consensus_core::palw_panel_view_v1::PalwPanelNetworkViewV1> {
        let registry = self.palw_model_registry_v1_impl()?;
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let tip_daa = self.headers_store.get_header(tip).map(|h| h.daa_score).unwrap_or(0);
        let schedule = kaspa_consensus_core::palw_panel_view_v1::PalwVerificationScheduleV1 {
            v2: self.palw_verification_v2,
            s3: self.palw_verification_s3,
            s2: self.palw_verification_s2,
        };
        Some(kaspa_consensus_core::palw_panel_view_v1::palw_panel_network_view_v1(
            &state,
            state_params,
            tip_daa,
            &registry,
            schedule,
        ))
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

    /// ADR-0132: the attempt-lane claims of the tip state as the end-to-end ledger observes them.
    pub fn palw_claim_ledger_observations_v1_impl(
        &self,
    ) -> Option<Vec<kaspa_consensus_core::palw_economics_ledger_v1::PalwClaimLedgerObservationV1>> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        Some(kaspa_consensus_core::palw_economics_ledger_v1::palw_claim_ledger_observations_v1(&state))
    }

    pub fn palw_bond_of_pubkey_v2_impl(
        &self,
        pubkey: &[u8],
    ) -> Option<(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2)> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        // One spelling, in the state that owns the registry. `PalwChainStateV2::bond_of_pubkey_v2`
        // carries the reason this must NOT filter on status — the chain's own `DuplicateBondKey`
        // rule does not, so a lookup that did would promise a registration the transition refuses.
        state.bond_of_pubkey_v2(pubkey)
    }

    /// **The terms a class entrant must take rather than choose** (ADR-0049 Decision H).
    ///
    /// The share and the panel floor are the ruleset's. The economic terms are the BASE CLASS's,
    /// read off the chain rather than restated: an entrant priced by its own registrant would be
    /// an entrant whose slash value and starting difficulty are whatever it liked, and "the same
    /// work costs the same everywhere" is the property the share table conserves.
    pub fn palw_certified_families_v1_impl(
        &self,
    ) -> Vec<(
        kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1,
        kaspa_hashes::Hash64,
        kaspa_consensus_core::palw_state_v2::PalwCertifiedFamilyStateV2,
    )> {
        use kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1 as Lane;
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten() else {
            return Vec::new();
        };
        [Lane::Attempt, Lane::FreePrompt]
            .into_iter()
            .flat_map(|lane| state.certified_family_entries(lane).into_iter().map(move |(digest, record)| (lane, digest, record)))
            .collect()
    }

    pub fn palw_v2_registration_terms_impl(&self) -> Option<kaspa_consensus_core::palw_state_v2::PalwRegistrationTermsV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let bundle = self.palw_v2_bundle.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let base = state.class(&bundle.base_class_id)?;
        // The target lives beside the class, not inside it — retargeting moves one and not the
        // other, and an entrant seeded from a stale copy would start at a difficulty the chain
        // stopped using.
        let base_target = state.class_target(&bundle.base_class_id)?;
        // **The terms an operator is told are the terms the chain will take** (ADR-0122 §6: this
        // exists so a registration is not assembled from two numbers that can disagree). Past
        // `palw_admission_independence` every entrant joins at 0‰ — registration buys existence,
        // cadence is earned — so the offer has to say 0, or the CLI builds an object the gate
        // refuses. Resolved at the tip, which is the height the registration would land at or just
        // below; the gate resolves it at the carrying block, and a registration assembled in the
        // last block before the fence is refused by the gate exactly as one assembled after it.
        //
        // **At the VIRTUAL's DAA, not the sink's.** A carrier sent now is accepted by a block at or
        // past the virtual; the sink is one step behind it, and on the fence's own block that step
        // is the difference between 1‰ and 0‰ (the Studio economy drill: the offer said 1‰ at DAA
        // 29, the carrier landed at 31 and was dropped). The node's registration also waits out
        // the landing margin below the fence (`palw_registration_waits_for_fences_v2`).
        let tip_daa = state.last_point().map(|point| point.daa_score).unwrap_or(0);
        let virtual_daa = self.virtual_stores.read().state.get().map(|state| state.daa_score).unwrap_or(tip_daa).max(tip_daa);
        let entrant_share =
            if self.palw_admission_independence_at(virtual_daa) { 0 } else { state_params.min_grantable_share_permille() };
        Some(kaspa_consensus_core::palw_state_v2::PalwRegistrationTermsV2 {
            min_grantable_share_permille: entrant_share,
            slash_value_per_pwu: base.slash_value_per_pwu,
            initial_target: base_target.target,
            registered_class_ids: state.class_ids(),
            registered_artifact_roots: state.class_artifact_roots(),
            chain_certified_families: state
                .chain_certified_families(kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt),
        })
    }

    /// **ADR-0067: the declaration the chain registered under `class_id`** — the profile and
    /// canonical job the accepted registration carried — existence-gated against CURRENT state,
    /// so a row left by a reorged-out registration answers nothing.
    pub fn palw_registered_class_carriage_v1_impl(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, kaspa_consensus_core::palw_v2::PalwJobContextV2)> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let class = state.class(&class_id)?;
        // **Frozen means frozen for serving too.** The store's doc promised this read was gated on
        // the class existing "and not being frozen", and only existence was in the code — so a
        // node would have gone on producing and judging under a class the chain had STOPPED,
        // which is the one state an emergency stop exists to prevent.
        if matches!(class.status, kaspa_consensus_core::palw_state_v2::PalwClassStatusV2::Frozen { .. }) {
            return None;
        }
        let registered_root = class.artifact_root;
        let record = self.palw_class_carriage_store.read().get(class_id)?;
        let carriage: kaspa_consensus_core::palw_state_v2::PalwClassAdmissionCarriageV2 = borsh::from_slice(&record.carriage).ok()?;
        // The id IS the profile's hash; a row that fails this was corrupted, and absent (None)
        // fails closed at every consumer.
        if carriage.profile.shape_profile_id() != class_id {
            return None;
        }
        // **The canonical job is NOT covered by the class id, so the row is re-tied to the class
        // the chain currently holds.** `class_id` hashes the profile alone; a registration that
        // lost a reorg and the one that won can share an id while differing in the artifact root
        // (which weights) and the canonical job (which prices the class). Existence alone let a
        // losing branch's canonical through — this pins the root, so a row describing another
        // registration of the same graph is refused rather than served.
        if record.artifact_root != registered_root {
            return None;
        }
        Some((carriage.profile, carriage.canonical))
    }

    /// **ADR-0067 Decision 6, the serving half: every declaration this node can hand a syncing
    /// peer.** One entry per class in current state that this node has a row for — the rows a
    /// pruned-syncing peer cannot obtain any other way, since it never walks the blocks whose
    /// acceptance wrote them. A class this node lacks a row for is simply absent: the peer's
    /// adoption is checked against its OWN state, so a short list costs it nothing but a fallback
    /// to `--palw-class-carriage`.
    pub fn palw_class_carriages_for_sync_v1_impl(&self) -> Vec<(kaspa_hashes::Hash64, Vec<u8>)> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        // Audit M-7's shared materialization, on the one caller that is a P2P serve path
        // (mainnet audit H-1): this answers `RequestPruningPointPalwState`, so an uncached read
        // here is a full PALW-state re-rooting per forty-byte request from any handshaked peer.
        let Some((_, state)) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten() else {
            return Vec::new();
        };
        let store = self.palw_class_carriage_store.read();
        state
            .class_ids()
            .into_iter()
            .filter_map(|class_id| store.get(class_id).map(|record| (class_id, record.carriage.clone())))
            .collect()
    }

    /// **ADR-0067 Decision 6: adopt a class declaration this node did not watch arrive.**
    ///
    /// The index's one incompleteness is a pruned sync: `import_pruning_point_palw_state` brings
    /// the class table over wholesale and no carriage row with it, so a node that joined that way
    /// holds classes whose graphs it does not have and refuses to serve them. This is the way in,
    /// and it needs no trust in whoever supplied the bytes — **a carriage is self-authenticating
    /// against chain state**:
    ///
    /// * the class must be one this chain currently holds (and not frozen — a stopped class is
    ///   not one to start serving);
    /// * the profile must hash to that class id, which is what `class_id` MEANS, so a wrong graph
    ///   cannot be adopted under a right name;
    /// * the artifact root must be the one the chain registered, which pins the half the id does
    ///   not cover — the weights, and with them the canonical job that prices the class.
    ///
    /// A supplier who satisfies all three has handed over exactly the bytes the accept path would
    /// have written. Anything else is refused, so the worst a hostile source achieves is wasting
    /// its own bandwidth.
    pub fn palw_adopt_class_carriage_v1_impl(&self, class_id: kaspa_hashes::Hash64, carriage_bytes: &[u8]) -> Result<(), String> {
        let state_params = self.palw_state_params_v2.as_ref().ok_or("this network has no V2 state params")?;
        let (_, state) = self
            .palw_state_v2_store
            .read()
            .load_tip_cached(state_params)
            .ok()
            .flatten()
            .ok_or("this node holds no V2 state to check a declaration against")?;
        let class = state.class(&class_id).ok_or_else(|| format!("this chain does not hold class {class_id}"))?;
        if matches!(class.status, kaspa_consensus_core::palw_state_v2::PalwClassStatusV2::Frozen { .. }) {
            return Err(format!("class {class_id} is frozen; a stopped class is not one to start serving"));
        }
        let carriage: kaspa_consensus_core::palw_state_v2::PalwClassAdmissionCarriageV2 =
            borsh::from_slice(carriage_bytes).map_err(|e| format!("the supplied carriage does not decode: {e}"))?;
        let derived = carriage.profile.shape_profile_id();
        if derived != class_id {
            return Err(format!("the supplied profile hashes to {derived}, not to {class_id} — it is another class's graph"));
        }
        if carriage.canonical.shape_profile_id != class_id {
            return Err("the supplied canonical job names another class, and it is what prices this one".to_string());
        }
        let record = crate::model::stores::palw_class_carriage::PalwClassCarriageRecord {
            registered_daa: class.registered_daa,
            artifact_root: class.artifact_root,
            carriage: carriage_bytes.to_vec(),
        };
        self.palw_class_carriage_store.write().insert(class_id, record).map_err(|e| format!("cannot store the declaration: {e}"))
    }

    /// A bond's claims at the tip (ADR-0122 §6.5). See the trait doc.
    pub fn palw_claim_rows_v1_impl(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        role: kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1,
        include_terminal: bool,
        limit: usize,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwBondClaimsV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let tip_daa = state.last_point().map(|p| p.daa_score).unwrap_or(0);
        let (rows, truncated) =
            kaspa_consensus_core::palw_producer_v2::palw_claim_rows_v1(&state, state_params, &bond, role, include_terminal, limit);
        let bond = kaspa_consensus_core::palw_producer_v2::palw_bond_summary_v1(&state, &bond);
        Some(kaspa_consensus_core::palw_producer_v2::PalwBondClaimsV1 { tip_daa, rows, truncated, bond })
    }

    pub fn palw_seat_duties_v2_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten() else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&state, state_params, mine)
    }

    /// **ADR-0148: the free-prompt lane's price for one job, at the virtual** — the fold's own
    /// function over the tip state, at the DAA a commitment sent now would be accepted at, with the
    /// bond's room by the fold's two terms. A gateway reads this AFTER its job ran and BEFORE the
    /// commitment is written, so what it checks and what the ledger reserves are one expression.
    pub fn palw_fp_commitment_price_impl(
        &self,
        class_id: kaspa_hashes::Hash64,
        prompt_token_ids: &[u32],
        prompt_tokens: u32,
        decode_tokens_executed: u32,
        work_leaves: u64,
        bond: Option<kaspa_consensus_core::tx::TransactionOutpoint>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwFpPriceAnswerV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_chain_point, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let virtual_read = self.virtual_stores.read();
        let daa_score = virtual_read.state.get().ok()?.daa_score;
        drop(virtual_read);
        let inputs = kaspa_consensus_core::palw_state_v2::PalwFpPriceInputsV1 {
            fp_derived_work_daa: self
                .palw_fp_derived_work
                .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
                .map(|fence| fence.daa_score()),
            canonical_work_daa: self.palw_canonical_work_daa,
            daa_score,
            // The audit's #5: the rights the fold will reserve beside the weight, priced from the same
            // carve and `W₀` the fold reads at this DAA — so the answer is the ledger's reservation.
            receipt_rights: self.palw_fp_receipt_rights_inputs_at(daa_score),
        };
        // The claim id only names a refusal; no claim exists until the rail signs one.
        let price = kaspa_consensus_core::palw_state_v2::palw_fp_commitment_price_v1(
            &state,
            state_params,
            inputs,
            &kaspa_hashes::Hash64::default(),
            &class_id,
            prompt_token_ids,
            prompt_tokens,
            decode_tokens_executed,
            work_leaves,
        );
        let bond_room = bond.and_then(|outpoint| {
            kaspa_consensus_core::palw_state_v2::palw_fp_bond_room_v1(
                &state,
                state_params,
                &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint),
                self.palw_capability_bound_at(daa_score),
            )
        });
        Some(kaspa_consensus_core::palw_state_v2::PalwFpPriceAnswerV1 { daa_score, price, bond_room })
    }

    pub fn palw_producer_facts_v2_impl(
        &self,
        class_id: kaspa_hashes::Hash64,
        bond: Option<kaspa_consensus_core::tx::TransactionOutpoint>,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let admission = self.palw_admission_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let virtual_read = self.virtual_stores.read();
        let candidate_daa = virtual_read.state.get().ok()?.daa_score;
        drop(virtual_read);
        let budget_fences = self.palw_epoch_budget_fences_at(candidate_daa);
        let mut facts = kaspa_consensus_core::palw_producer_v2::palw_producer_facts_v3(
            &state,
            state_params,
            admission,
            chain_point,
            candidate_daa,
            class_id,
            bond.map(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2).as_ref(),
            kaspa_consensus_core::palw_admission_v2::palw_work_lottery_floor_v1(
                &state,
                budget_fences.work_target_floor,
                budget_fences.single_lottery,
            ),
            // ADR-0149: the height the admission compares the candidate against, and the floor's
            // draw it prices the floor on before the registry has written its row.
            budget_fences.canonical_work_daa,
            budget_fences.base_known_draw,
            budget_fences.audit_2026_09_23_active,
            // Option A: the escrow the candidate block's own claim would carry — this block's subsidy
            // under the carve resolved at its DAA, the pair the fold and the ceiling price it from.
            kaspa_consensus_core::palw_state_v2::palw_claim_escrow_v1(
                state_params,
                self.coinbase_manager.calc_block_subsidy(candidate_daa),
                budget_fences.escrow_carve,
            ),
        )?;
        // The producer reads the same parent snapshot as admission. At a crossing block the
        // snapshot still carries the closed epoch's table, so the boundary fence must derive the
        // candidate epoch's budget here too; otherwise the producer would hold a non-floor block
        // that admission accepts. Recompute the release after installing that derived budget,
        // because the class's own share is part of the release formula.
        if budget_fences.boundary_budget_active && facts.epoch_budget_blocks == 0 && !facts.is_base_class {
            let epoch_index = candidate_daa / state_params.epoch_length();
            if let Some(budgets) = kaspa_consensus_core::palw_state_v2::palw_epoch_budgets_for_v2(&state, state_params, epoch_index)
                && let Some(budget) = budgets.budget_blocks.get(&class_id).copied()
            {
                facts.epoch_budget_blocks = budget;
                facts.epoch_budget_released = kaspa_consensus_core::palw_state_v2::palw_epoch_budget_release_v1(
                    &state,
                    state_params.epoch_length(),
                    candidate_daa,
                    &class_id,
                    budget,
                );
            }
        }
        // ADR-0123: the builder computes the release but cannot know whether it counts — it holds
        // no `Params`. This is the one path every producer and the RPC ask through, and it resolves
        // the fence at the SAME candidate score admission will judge the block at, so a producer
        // holds exactly when the chain would refuse and draws exactly when it would accept.
        facts.epoch_budget_release_armed = budget_fences.budget_release_active;
        // **The route-matrix audit's #7: the fold's own class gate, asked before an inference is
        // spent** — at the candidate's DAA under the fences the fold will read there (the block is
        // not built yet, so its header facts are the tip's; the gate reads none of them).
        let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score: candidate_daa,
            blue_score: 0,
            subsidy: 0,
        });
        facts.class_admission_refusal =
            kaspa_consensus_core::palw_state_v2::palw_class_admits_claim_v1(&state, state_params, &extras, &class_id, candidate_daa)
                .err()
                .map(|refusal| refusal.to_string());
        Some(facts)
    }

    /// **The certified free-prompt quanta `bond` may spend into receipt blocks (FP-R5).**
    ///
    /// Read at virtual, which is where a producer's next template builds. Each returned row has
    /// its whole story checked here — claim `Final` and free-prompt, quantum unspent, beacon fact
    /// derived from THIS chain, ticket compared against the class's receipt target — because the
    /// producer's only move with a row is to build a block, and a row the admission would refuse
    /// is a template wasted at best and a false "I am mining" at worst.
    ///
    /// The lottery needs no nonce: `fp_quantum_ticket_v3` is a function of (domain, beacon, claim,
    /// quantum), so a quantum either wins at its beacon or it never does. `wins: false` rows are
    /// returned too — an operator asking "why am I not producing receipt blocks" deserves to see
    /// the tickets that lost rather than an empty list that also means "no claims".
    pub fn palw_fp_spendable_v3_impl(
        &self,
        bond: kaspa_consensus_core::tx::TransactionOutpoint,
    ) -> Vec<kaspa_consensus_core::palw_freeprompt_v3::PalwFpSpendableQuantumV3> {
        use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpSpendableQuantumV3, fp_draw_slot_v3, fp_quantum_ticket_v3};
        use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwClaimSourceV2};

        let Some(freeprompt) = self.palw_freeprompt_params_v3.as_ref() else {
            return Vec::new();
        };
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return Vec::new();
        };
        let Ok(Some((chain_point, state))) = self.palw_state_v2_store.read().load_tip_cached(state_params) else {
            return Vec::new();
        };
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(bond);
        let pricing = self.palw_fp_pricing();

        let mut out = Vec::new();
        for (claim_id, claim) in state.claims_iter() {
            if claim.bond != key {
                continue;
            }
            let PalwClaimPhaseV2::Final { final_daa } = claim.phase else { continue };
            let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source else { continue };
            // ADR-0148: the admission's own target expression for the claim's era — the pooled
            // target scaled by the compute a quantum carries, or the class's target below the fence.
            let target = match pricing.as_ref() {
                Some(pricing) => {
                    kaspa_consensus_core::palw_state_v2::palw_fp_quantum_receipt_target_v1(&state, claim, *quanta, pricing)
                }
                None => state.receipt_target(&claim.class_id).map(|target| target.target),
            };
            let Some(target) = target else { continue };
            // A compute-era claim may carry up to 2^16 quanta; listing every losing ticket of every
            // such claim would make this RPC's answer the size of the lottery. Its winners are what a
            // producer can spend, and they are what it returns.
            let winners_only = pricing.as_ref().is_some_and(|pricing| pricing.prices_in_compute(claim));
            let Some(slot) = fp_draw_slot_v3(final_daa, freeprompt.receipt_maturity_daa()) else { continue };
            // The beacon is a chain fact; a slot the chain has not reached yet has no beacon and
            // therefore no rows — "not yet drawable" and "lost" must not look alike.
            let Ok(beacon) = self.palw_beacon_fact_of_candidate(chain_point, slot) else { continue };
            for quantum_index in 0..*quanta {
                if spent.contains(&quantum_index) {
                    continue;
                }
                let ticket = fp_quantum_ticket_v3(network_domain, beacon.beacon_block, *claim_id, quantum_index);
                let wins = kaspa_consensus_core::palw_pwu::palw_ticket_admits_v1(ticket, target);
                if winners_only && !wins {
                    continue;
                }
                out.push(PalwFpSpendableQuantumV3 {
                    claim_id: *claim_id,
                    class_id: claim.class_id,
                    quantum_index,
                    beacon,
                    receipt_target: target,
                    ticket,
                    wins,
                    spend_deadline_daa: beacon.beacon_daa.saturating_add(freeprompt.receipt_use_window_daa()),
                });
            }
        }
        out
    }

    /// **The bonds this block's own mergeset declares** — the half the parent state cannot hold.
    ///
    /// A safe superset by construction, and by the same argument the DNS `bond_gate_view` makes:
    /// a declaration that turns out to be UTXO-invalid names an output that does not exist, so
    /// forbidding its spend forbids nothing. This set is only ever UNIONED into the locked set,
    /// which only ever refuses, so no superset can admit a spend that the parent-state half would
    /// have refused.
    ///
    /// Reads the RAW mergeset transactions — the same set the acceptance loop iterates — rather
    /// than acceptance data, because acceptance data does not exist yet at this point in the walk,
    /// and that is precisely why the collateral was spendable in its own registering block.
    pub(super) fn palw_v2_bonds_declared_in_mergeset(
        &self,
        ctx: &UtxoProcessingContext,
    ) -> std::collections::HashSet<TransactionOutpoint> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
        let mergeset_txs: Vec<kaspa_consensus_core::tx::Transaction> = std::iter::once(ctx.selected_parent())
            .chain(ctx.ghostdag_data.consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.deref()))
            .flat_map(|b| (*self.block_transactions_store.get(b).unwrap()).clone())
            .collect();
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(&mergeset_txs)
            .objects
            .iter()
            .filter_map(|carried| match &carried.object {
                PalwConsensusObjectV2::BondRegistered { bond, .. } => Some(bond.0),
                _ => None,
            })
            .collect()
    }

    /// The locked-collateral set at the node's own virtual DAA, for the wallet's input selector
    /// (audit3 H3). Reads the materialized tip, so it answers the same question
    /// `palw_v2_locked_bond_outpoints` answers on the block path — one predicate, two callers.
    pub fn palw_locked_bond_outpoints_v2_impl(&self) -> Vec<TransactionOutpoint> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return Vec::new();
        };
        let Ok(Some((_, state))) = self.palw_state_v2_store.read().load_tip_cached(state_params) else {
            return Vec::new();
        };
        let now_daa = self.lkg_virtual_state.load().daa_score;
        let mut out: Vec<TransactionOutpoint> = self.palw_v2_locked_bond_outpoints(&state, now_daa).into_iter().collect();
        // Deterministic order, so two nodes answering the same question give the same answer and a
        // paging caller cannot be handed a shuffled set.
        out.sort_by(|a, b| (a.transaction_id, a.index).cmp(&(b.transaction_id, b.index)));
        out
    }

    pub(super) fn palw_v2_locked_bond_outpoints(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        now_daa: u64,
    ) -> std::collections::HashSet<TransactionOutpoint> {
        let Some(_bond_params) = self.palw_bond_params_v2.as_ref() else {
            return Default::default();
        };
        state
            .bonds_iter()
            .filter(|(_, record)| {
                kaspa_consensus_core::palw_state_v2::palw_bond_collateral_is_locked_v3(
                    record,
                    now_daa,
                    self.palw_bond_withdrawal_delay_at(now_daa),
                    // The second clock: a retiring bond's collateral also waits for settled anchors.
                    state.settled_attempt_finals(),
                    self.palw_settled_anchor_depth_at(now_daa),
                )
            })
            .map(|(key, _)| key.0)
            .collect()
    }

    /// **ADR-0042 Decision 10, the funding side: what `block`'s own worker reward owes its claim.**
    ///
    /// The Decision says the PALW reward is "a carve of the fixed subsidy ... never an addition to
    /// it — the schedule is never exceeded (I6/I15)". Only the release half existed: escrows were
    /// appended to the coinbase and nothing was taken out to fund them, so every finalized claim
    /// minted its whole carve above the emission schedule. This is the deduction.
    ///
    /// Summed over the claims `block` itself accepted rather than derived from the subsidy, so the
    /// number withheld is the number that will be paid — by construction, from the same record.
    /// Today that is at most one claim (an attempt IS its block, and a free-prompt claim escrows
    /// nothing), but summing makes the identity hold whatever a later lane does.
    ///
    /// **"By construction, from the same record" is true of CLAIM rows and false of MARKET rows,
    /// and that is deliberate** (mainnet audit 2026-09-06, M-12). ADR-0087's market also writes
    /// `PalwPayoutV2` rows into `pending_payouts`, and `palw_v2_payout_outputs` appends the queue's
    /// prefix verbatim, so a market payout IS minted into the coinbase with nothing withheld here.
    /// ADR-0087 Decision 3 says so in terms — the reserve is "an accounting entry funded by sinks
    /// and drained by coinbase payouts" — so for the market lane ADR-0042 Decision 10's "a carve,
    /// never an addition" does not hold and is not meant to.
    ///
    /// What holds instead, and what makes the mint safe, is ADR-0087 M2: every sompi a market
    /// payout mints was previously paid into a sink output that is dead by script
    /// (`palw_model_sink_spk_v1` — an `OP_RETURN`), and `palw_model_sell_quote_v1` caps the gross
    /// leg by `msk_reserve`, which only sinks credit. So Σ market payouts ≤ Σ MSK sunk, and
    /// SPENDABLE supply never exceeds premine + emission — ADR-0059's 10 B cap is not breached.
    /// The consequence a reader must carry away: **a supply figure obtained by summing the UTXO set
    /// over-reports by the total ever sunk**, because a sink output is in the set and can never be
    /// spent. `indexes/utxoindex` excludes them for exactly this reason.
    ///
    /// Nothing here needs to change for that to be safe. It needs to be WRITTEN DOWN, because the
    /// sentence above ("the number withheld is the number that will be paid — by construction")
    /// reads as a claim about the whole queue and is one about half of it.
    ///
    /// **Past `palw_audit_2026_09_23`, an own attempt the fold SKIPPED still has its carve withheld**
    /// (the 2026-09-23 re-audit of finding 17). Past that fence step 4 of the transition skips a chain
    /// block's own attempt that the live exposure ceiling refuses (`AttemptExposureCeiling`): the
    /// block stays valid and records no claim. Read from claims alone, this then withheld nothing and
    /// the child's coinbase paid the selected parent's whole worker share — a payment with no claim,
    /// no reservation, no panel and no court behind it, the defect S-04/M2-3 closed for merged blues.
    /// And the skip is steerable: step 3 lands this block's own accepted objects (its free-prompt
    /// commitments, whose receipt rights reserve ~500× their weight) on the bond first, so a producer
    /// could fill its own ceiling to the sompi and be paid for an attempt nobody will ever examine. So
    /// when the header carries an attempt whose claim the state does not hold as accepted here, the
    /// carve it WOULD have escrowed — `worker_carve_at` of the block's own subsidy at the carve
    /// resolved at its own DAA, the expression `apply_attempt` stores — is withheld anyway and never
    /// released: burned, as a skipped merged blue's is. An admitted attempt's claim is found and
    /// withheld from its record exactly as before, so the figure moves only for a skipped one.
    pub(super) fn palw_v2_escrow_withheld_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
    ) -> u64 {
        let recorded = state
            .claims_iter()
            .filter(|(_, claim)| claim.accepted_block == block)
            .fold(0u64, |acc, (_, claim)| acc.saturating_add(claim.escrowed_reward));
        recorded.saturating_add(self.palw_v2_skipped_own_attempt_carve(state, block))
    }

    /// The carve of `block`'s own attempt where the fold skipped it — see
    /// [`Self::palw_v2_escrow_withheld_at`]. Zero below `palw_audit_2026_09_23` (at the block's own
    /// DAA, the height its fold resolved the fence at), for a block that carries no attempt, and for
    /// one whose attempt the state holds as a claim accepted in it.
    fn palw_v2_skipped_own_attempt_carve(&self, state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2, block: BlockHash) -> u64 {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return 0;
        };
        let Ok(header) = self.headers_store.get_header(block) else {
            return 0;
        };
        if !self.palw_audit_2026_09_23_at(header.daa_score)
            || !kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id)
        {
            return 0;
        }
        // The block is on the selected chain, so its attempt decoded and was admitted at the header
        // and the chain walk; an envelope that does not decode here cannot have made a claim either.
        let Ok(envelope) = kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment) else {
            return 0;
        };
        let own = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&envelope.attempt);
        if state.claim(&own).is_some_and(|claim| claim.accepted_block == block) {
            return 0;
        }
        state_params.worker_carve_at(
            self.coinbase_manager.calc_block_subsidy(header.daa_score),
            self.palw_escrow_carve_at(header.daa_score, header.daa_score),
        )
    }

    /// **B-1 (mainnet audit 2026-09-11, deep fence): the worker carve withheld from each MERGED
    /// block's coinbase output** — `{ merged block → worker_carve(its own subsidy) }`.
    ///
    /// `palw_v2_escrow_withheld_at` above handles the ONE chain block a mergeset has (the selected
    /// parent), whose claim a prior transition already committed and can be read from state. This
    /// handles every OTHER merged blue and every entitled in-window red, whose claim THIS accepting
    /// block's transition creates — not yet in state when the coinbase is built — so the amount is
    /// SIZED rather than read.
    ///
    /// **Alignment is by construction, three ways.** The keys are exactly the Attempt works
    /// `palw_v2_merged_works` returns — the same function, same parent `state`, same `point` the fold
    /// folds — so the coinbase withholds for precisely the set the fold will try to escrow. The fold
    /// may still refuse one in its LIVE state (a budget race, a B-4 execution duplicate); then the
    /// withheld carve is simply not minted (burned), never released, so the withhold set is a safe
    /// SUPERSET of the escrow set. The amount is `PalwStateParamsV2::worker_carve_at` of the merged
    /// block's own subsidy at the carve resolved at the lower of the merged block's DAA and this
    /// block's (ADR-0126) — both read from the SAME `PalwMergedOwnedWorkV1::Attempt` fields the fold
    /// escrows from — so the carve withheld and the carve recorded are one number on both the build
    /// and validate paths. That subsidy is `calc_block_subsidy(the block's DAA)`, which for an attempt
    /// block equals the coinbase-declared subsidy the block is actually paid from (body validation
    /// pins it; an attempt block is never a heartbeat), so the withheld carve never exceeds the worker
    /// share the coinbase would otherwise pay: `validate_palw_v2` fits the bundle's carve into the
    /// network's split and ADR-0126's into the split it lowers, and the lower score puts a lowered
    /// carve only under a paying block whose own split is lowered, whatever order the DAG gives the
    /// two scores.
    ///
    /// **Empty below `palw_audit_2026_09_11_deep`** and on every network without a V2 bundle, where
    /// merged carves are paid in full at acceptance (ADR-0058 Decision 5) — byte-identical to before.
    /// Past the fence it re-runs `palw_v2_merged_works` (the fold runs it again at fold time); the
    /// redundant pass is deterministic in the same inputs and paid only past a provisional flag day.
    pub(super) fn palw_v2_merged_escrow_withheld(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        ghostdag_data: &GhostdagData,
        mergeset_non_daa: &BlockHashSet,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> BlockHashMap<u64> {
        let mut withheld = BlockHashMap::default();
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return withheld;
        };
        if !self.palw_audit_2026_09_11_deep_at(point.daa_score) {
            return withheld;
        }
        let (works, _skips) = self.palw_v2_merged_works(ghostdag_data, state, state_params, mergeset_non_daa, point);
        for work in &works {
            if let PalwMergedOwnedWorkV1::Attempt(blue, _, subsidy, carve, _) = work {
                withheld.insert(*blue, state_params.worker_carve_at(*subsidy, *carve));
            }
        }
        withheld
    }

    /// **Launch blockers §8: which merged blues this block's coinbase may not pay.**
    ///
    /// The subsidy is what PALW work is paid with, so the chain has to be able to say the producer
    /// did PALW work under a bond. For the selected parent it can — the transition admitted it in
    /// full. For every other merged blue it never asked: the stateful half of admission runs on
    /// the selected chain only, and the coinbase paid the rest of the mergeset anyway. A miner
    /// with no bond at all could therefore collect the worker share on nothing but a solved hash,
    /// which is the opposite of ADR-0038.
    ///
    /// Only the ENTITLEMENT items (bond present and not retiring, its key and operator, the class
    /// registered and unfrozen) — see `check_palw_producer_entitlement_v2` for why the resource
    /// items are excluded. The stateless half is already true of every relayed block, so between
    /// the two a paid blue has a verified signature from a bonded key of a live class.
    ///
    /// Evaluated against `state`, which is the state AT THE SELECTED PARENT — the same state the
    /// escrow and the payouts are read from, so a node building a template and a node validating
    /// it ask the identical question. Empty on every network without a V2 bundle.
    /// **The evaluation point a TEMPLATE is built at.** The block does not exist yet, so it stands
    /// in as virtual's own selected parent and virtual's scores — which is the point the block this
    /// template becomes will actually be validated at. One definition, because the construction
    /// side and the validation side computing this differently is how a node mines coinbases its
    /// own validator then refuses (audit3 S-04).
    fn palw_v2_template_point(
        &self,
        virtual_state: &crate::model::stores::virtual_state::VirtualState,
    ) -> kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: virtual_state.ghostdag_data.selected_parent,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: self.coinbase_manager.calc_block_subsidy(virtual_state.daa_score),
        }
    }

    /// PALW V2 state as-of `at` — the same reconstruction `calculate_utxo_state_relatively` uses,
    /// so a template cannot commit a root the virtual walk will then refuse.
    ///
    /// The stored tip and virtual's selected parent agree between rounds. They disagree after an
    /// unclean shutdown (tip written, virtual not) and whenever this template's selected parent is
    /// not the store tip. Walking is a no-op when they agree. testnet-11 2026-09-21 `df80394b`
    /// committed the store tip `a9a629…` while validators compared the selected-parent root
    /// `7d36f3…` and disqualified a UTXO-valid chain block on that mismatch alone.
    fn palw_v2_state_at(&self, at: BlockHash) -> Option<(BlockHash, Arc<kaspa_consensus_core::palw_state_v2::PalwChainStateV2>)> {
        let params = self.palw_state_params_v2.as_ref()?;
        let store = self.palw_state_v2_store.read();
        let (tip_block, tip_state) = store.load_tip_cached(params).ok().flatten()?;
        if tip_block == at {
            return Some((tip_block, tip_state));
        }
        let path = self.dag_traversal_manager.calculate_chain_path(tip_block, at, None);
        let removed: Vec<BlockHash> = path.removed.to_vec();
        let added: Vec<BlockHash> = path.added.to_vec();
        match crate::processes::palw_state_walk::walk_chain_path(&store, params, (*tip_state).clone(), &removed, &added) {
            Ok(state) => {
                warn!(
                    "PALW V2 state tip stood at {tip_block} while the template selected parent is {at}; re-derived the parent state over {} reverted and {} applied deltas so the committed root matches the validating walk",
                    removed.len(),
                    added.len()
                );
                Some((at, Arc::new(state)))
            }
            Err(e) => match store.load_pruning_snapshot(params) {
                Ok(Some((snap_block, snap_state))) => {
                    let snap_path = self.dag_traversal_manager.calculate_chain_path(snap_block, at, None);
                    let snap_removed: Vec<BlockHash> = snap_path.removed.to_vec();
                    let snap_added: Vec<BlockHash> = snap_path.added.to_vec();
                    match crate::processes::palw_state_walk::walk_chain_path(&store, params, snap_state, &snap_removed, &snap_added) {
                        Ok(state) => {
                            warn!(
                                "PALW V2 state tip stands at {tip_block}, template selected parent is {at}, and the path between them cannot be walked ({e}); rebuilt the parent state from the pruning snapshot at {snap_block}"
                            );
                            Some((at, Arc::new(state)))
                        }
                        Err(snap_err) => {
                            warn!(
                                "PALW V2 template cannot establish the state at selected parent {at}: tip {tip_block} does not walk here ({e}) and neither does the pruning snapshot at {snap_block} ({snap_err}); leaving palw_state_root unset rather than committing the store tip"
                            );
                            None
                        }
                    }
                }
                Ok(None) => {
                    warn!(
                        "PALW V2 template cannot establish the state at selected parent {at}: tip {tip_block} does not walk here ({e}) and this store holds no pruning snapshot; leaving palw_state_root unset rather than committing the store tip"
                    );
                    None
                }
                Err(snap_err) => {
                    warn!(
                        "PALW V2 template cannot establish the state at selected parent {at}: tip {tip_block} does not walk here ({e}) and the pruning snapshot cannot be read ({snap_err}); leaving palw_state_root unset rather than committing the store tip"
                    );
                    None
                }
            },
        }
    }

    pub(super) fn palw_v2_unentitled_blues(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        ghostdag_data: &GhostdagData,
        mergeset_non_daa: &BlockHashSet,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> BlockHashSet {
        use kaspa_consensus_core::palw_attempt_v2::attempt_id_v2;
        let mut unentitled = BlockHashSet::default();
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return unentitled;
        };
        // Identities already paid or claimed in THIS mergeset. The state answers for identities
        // the chain has already seen; nothing but this answers for two siblings carrying one.
        let mut seen_here: std::collections::HashSet<kaspa_consensus_core::Hash64> = Default::default();
        // **B-5: the receipt lane's version of `seen_here`.** A certified free-prompt quantum is
        // spent by at most one block — the fold applies the first spend and returns
        // `QuantumAlreadySpent` for the rest — but two conflicting receipt siblings in one mergeset
        // both validate against the parent state (where the quantum is unspent), so both were
        // entitled and the coinbase paid a full worker share for each. Weighed once, paid N times.
        // Dedup the (claim, quantum) here exactly as the attempt arm dedups the attempt identity.
        // **Gated to the audit flag day (`palw_audit_2026_09_11`):** below the fence the pay set is
        // byte-identical to before the fix (both siblings paid), so a fenced build and an unfenced
        // one agree on every block's coinbase before the height.
        let audit_active = self.palw_audit_2026_09_11_at(point.daa_score);
        let mut seen_here_quanta: std::collections::HashSet<(kaspa_consensus_core::Hash64, u32)> = Default::default();
        // **B-4: the attempt lane's execution dedup, matching the fold's `seen_exec`.** Two merged
        // siblings that re-announce one inference under different within-bucket nonces have distinct
        // attempt ids (so `seen_here` above lets both through) but one pre_pow-inclusive
        // `execution_commitment_v3`; the fold claims one and refuses the rest, so the coinbase must
        // pay one, not each.
        //
        // **This set is seeded EMPTY, and that is exactly the fold's behaviour on the merged loop.**
        // The fold applies the block's own work first (step 4) and inserts its key too, so its
        // `seen_exec` also carries `own_execution_key` when the mergeset loop runs — but that seed can
        // never refuse a merged blue, because `palw_execution_key_v1` binds the CARRYING block's
        // `pre_pow_hash` and two blocks share a key iff they share pre_pow (are nonce-siblings at one
        // DAG position). A merged blue is in B's past, so it cannot be a nonce-sibling of B; hence
        // `own_execution_key` differs from every merged key, the fold's own seed is inert against the
        // merged loop, and this set omitting it agrees with the fold by construction rather than by
        // reachability. Gated on the deep flag day (`palw_audit_2026_09_11_deep`); below it the pay
        // set is byte-identical to before.
        let audit_deep_active = self.palw_audit_2026_09_11_deep_at(point.daa_score);
        let mut seen_here_exec: std::collections::HashSet<kaspa_consensus_core::Hash64> = Default::default();
        // **Blues AND reds.** This iterated `mergeset_blues` alone, so the set could never contain a
        // red — and the coinbase's reds loop had no skip to apply one anyway. At the frozen 120 s
        // cadence `ghostdag_k = 1` against a `mergeset_size_limit` of 180, so the blues this
        // filtered were at most ONE block per mergeset and the reds it did not filter were
        // everything else: every red was paid its full worker share to the merging miner, with no
        // bond, class, lottery, budget or exposure behind it. The door does not compensate — the
        // header gate checks shape, the challenge equation and a signature under the CARRIED key,
        // and leaves trace, output and execution roots unchecked.
        //
        // A red is not a chain block, so the selected-parent exemption below cannot apply to one,
        // and the same entitlement question is the right question for both colours: did this chain
        // accept work from that block.
        let mergeset_daa =
            ghostdag_data.mergeset_blues.iter().chain(ghostdag_data.mergeset_reds.iter()).filter(|h| !mergeset_non_daa.contains(h));
        for blue in mergeset_daa {
            // The selected parent went through the full admission on its way to becoming a chain
            // block, and its reward is escrowed rather than paid. Re-deciding it here could only
            // ever disagree with the transition that already accepted it.
            if *blue == ghostdag_data.selected_parent {
                continue;
            }
            let Ok(header) = self.headers_store.get_header(*blue) else {
                // A blue whose header this node cannot read is one it cannot vouch for. It never
                // happens — the block is in the mergeset — and if it did, not paying is the side
                // that does not mint.
                unentitled.insert(*blue);
                continue;
            };
            // **Payment asks the acceptance question, it does not re-derive an answer to it**
            // (audit3 S-04).
            //
            // This site used to run a hand-picked subset — entitlement items 1-5, the class
            // lottery, one-identity-one-claim, and (after M2-3) the pwu equality — and said so:
            // "The epoch budget and the exposure ceiling are sequential ... and belong with the
            // wider fix noted in the audit, not here." M2-3 was then recorded `fixed` on the pwu
            // leg alone, and the two it named stayed open. A merged block that the transition
            // refuses with `EpochBudgetExceeded` or `ExposureCeilingExceeded` was paid its whole
            // worker carve while creating no claim, reserving no exposure, incurring no panel duty
            // and entering no epoch counter — so a class at the 1‰ grant floor spends its one
            // budgeted block honestly and then mints unboundedly many unbudgeted ones, none of
            // which anybody can ever examine because no claim exists to dispute.
            //
            // Two parallel lists of predicates is the defect class itself, so this now asks the
            // SAME function `palw_v2_merged_works` asks — the full admission, against the same
            // parent state and the same block context. Entitlement and acceptance agree by
            // construction rather than by two people keeping two lists in step. It also repairs
            // the receipt lane in passing: an algo-7 free-prompt receipt spend was denied its
            // entire coinbase because this site only ever recognised the attempt lane, while the
            // chain folded its work and minted weight for it.
            //
            // **NOT YET COVERED BY A TEST, and said here rather than in a report.** The honest
            // half is: `palw_v2_an_unbonded_merged_blue_is_not_paid` proves this set stays empty on
            // a clean V2 chain, so the tightening does not deny honest miners. The refusing half —
            // a chain that keeps going while one merged block is refused for budget or exposure —
            // has no fixture, because the harness mines every block under ONE bond: exhausting
            // that bond's exposure disqualifies the chain blocks too, and the chain stops instead
            // of carrying a refused sibling. A two-bond harness is what this needs, and until it
            // exists this leg is verified by reading. That is exactly the standing M2-3 was
            // recorded `fixed` at, so it is written down instead of claimed.
            //
            // **The residual, stated rather than hidden.** Step 4b of `apply_palw_transition_v4`
            // re-runs admission against the live fold state, so it can still refuse a second block
            // that this parent-state view accepts — two merged siblings racing for one class's
            // last budgeted block of the epoch. That is bounded by one mergeset and needs the
            // attacker to land several blocks in one, where the open version needed only one
            // earlier block of its own; closing it needs the payment set to come from the
            // transition's `merged_skips`, which is computed after this set is consumed.
            match self.palw_v2_check_attempt_admission(&header, state, state_params, point, None) {
                Ok(Some(envelope)) => {
                    // The one question the parent state cannot answer: two siblings in THIS
                    // mergeset carrying one identity. The state answers for identities the chain
                    // has already seen; nothing but this answers for a pair arriving together.
                    if !seen_here.insert(attempt_id_v2(&envelope.attempt)) {
                        debug!("merged block {blue} carries an attempt identity already paid in this mergeset");
                        unentitled.insert(*blue);
                    } else if audit_deep_active && !seen_here_exec.insert(self.palw_execution_key_v1(&header, &envelope.attempt)) {
                        // **B-4:** a distinct attempt id but the SAME execution (a nonce-sibling in one
                        // bucket) — the fold folds one claim's weight and refuses the rest, so paying
                        // this second one would mint a worker share the chain never counted.
                        debug!("merged block {blue} re-announces an execution already paid in this mergeset (B-4)");
                        unentitled.insert(*blue);
                    }
                }
                Ok(None) => match self.palw_v2_check_receipt_spend(&header, state, state_params, point) {
                    Ok(Some(envelope)) => {
                        // B-5: at most one sibling per (claim, quantum) is entitled; the fold folds
                        // the quantum's weight once, so paying a second spend of it mints reward the
                        // chain never counted. Only past the audit fence (below it, unchanged).
                        if audit_active && !seen_here_quanta.insert((envelope.spend.claim_id, envelope.spend.quantum_index)) {
                            debug!(
                                "merged block {blue} spends a certified quantum ({}, {}) already paid in this mergeset",
                                envelope.spend.claim_id, envelope.spend.quantum_index
                            );
                            unentitled.insert(*blue);
                        }
                    }
                    Ok(None) => {
                        debug!("merged block {blue} carries no work this chain accepted");
                        unentitled.insert(*blue);
                    }
                    Err(why) => {
                        debug!("merged block {blue} carries a receipt spend this chain refuses: {why}");
                        unentitled.insert(*blue);
                    }
                },
                Err(why) => {
                    debug!("merged block {blue} is not paid a worker share: {why}");
                    unentitled.insert(*blue);
                }
            }
        }
        unentitled
    }

    /// **Audit C-08's third part: what a RELEASED bond's collateral still owes the burn.**
    ///
    /// The lock keeps a live bond's outpoint unspendable; this is what happens at the other end.
    /// A bond that was slashed and then retired would otherwise walk away with its whole outpoint,
    /// and `PalwBondStateV2::slashed`'s own documentation — "it leaves `collateral` and enters
    /// circulation nowhere" — would be false at the only moment it mattered.
    ///
    /// Only bonds whose collateral is spendable appear: a locked one cannot be spent at all, so it
    /// owes nothing yet. Same parent state, same walk, same reason as the lock beside it.
    pub(super) fn palw_v2_bond_burn_obligations(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        now_daa: u64,
    ) -> std::collections::HashMap<TransactionOutpoint, u64> {
        let Some(_bond_params) = self.palw_bond_params_v2.as_ref() else {
            return Default::default();
        };
        state
            .bonds_iter()
            .filter(|(_, record)| {
                !kaspa_consensus_core::palw_state_v2::palw_bond_collateral_is_locked_v3(
                    record,
                    now_daa,
                    self.palw_bond_withdrawal_delay_at(now_daa),
                    // The second clock: a retiring bond's collateral also waits for settled anchors.
                    state.settled_attempt_finals(),
                    self.palw_settled_anchor_depth_at(now_daa),
                )
            })
            .map(|(key, record)| (key.0, kaspa_consensus_core::palw_state_v2::palw_bond_burn_obligation_v2(record)))
            .filter(|(_, owed)| *owed > 0)
            .collect()
    }

    fn palw_v2_payout_outputs(&self, state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2) -> Vec<TransactionOutput> {
        // The SAME prefix the transition drains — see `PALW_V2_MAX_PAYOUTS_PER_BLOCK`. Both sides
        // read the selected parent's queue in `BTreeMap` key order, so "the first N" names one set
        // on every node. Paying more than the transition clears would pay a claim twice; clearing
        // more than the coinbase pays would destroy the reward the escrow exists to deliver.
        state
            .pending_payouts_iter()
            .take(kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK)
            .map(|(_, payout)| {
                TransactionOutput::new(
                    payout.amount,
                    kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payout.payload.as_bytes()),
                )
            })
            .collect()
    }

    /// **ADR-0042 Decisions 7 and 8's consumers: every lifecycle object is ADJUDICATED before it
    /// is folded.**
    ///
    /// `apply_palw_transition_v3` moves a claim's phase on the object's say-so — deliberately, its
    /// module boundary says so: signature verification and sortition are `palw_panel_v2`'s job,
    /// the court's verdict is `palw_court_v2`'s. Folding an object nobody validated would make the
    /// state machine a transcription service for whatever a block asserted, which is the fail-open
    /// shape the consumer-layer audit found ten of.
    ///
    /// So each object meets its own validator here, BEFORE the fold:
    ///
    /// * **`PanelBound`** — `validate_panel_bound_v2` re-derives the panel by sortition from the
    ///   candidate's own bond registry and compares seats EXACTLY. The anchor is not taken from
    ///   the object either: it is derived from this candidate's chain, the same rule the beacon
    ///   follows, because a producer that supplies its own anchor picks its own jury.
    /// * **`CourtOpened`** — `validate_court_opened_v2`: the claim is challengeable at this point
    ///   and the session id is the one its own parts derive.
    /// * **`CourtClosed`** — `adjudicate_court_close_v2` returns the ONLY verdict the proof
    ///   supports, and the object's announced verdict must equal it. An object that merely NAMES
    ///   a verdict is an accusation, not an adjudication.
    ///
    /// A network with no V2 bundle has no objects to check (extraction yields none), so this is
    /// `Ok(())` everywhere today.
    /// [`Self::palw_v2_validate_objects`] as a FILTER: the objects that passed, in order, with the
    /// rejected ones logged by their own reason. See the call site for why a rejection drops the
    /// object instead of the block.
    /// [`Self::palw_v2_accepted_objects`], reachable from the sibling test module. The filter's
    /// contract — "what it returns, the transition applies" — is only checkable by calling both.
    #[cfg(test)]
    pub(super) fn palw_v2_accepted_objects_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
        block: BlockHash,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.palw_v2_accepted_objects(state, state_params, point, Self::unpriced_for_tests(objects), block).0
    }

    /// Objects a test hands the filter directly, carried by nobody: `PALW_RENT_UNPRICED` so the
    /// ADR-0075 rent rules read as absent rather than as "every carrier paid zero", which is what
    /// every pre-SA test means and what an unarmed network does.
    #[cfg(test)]
    pub(super) fn unpriced_for_tests(
        objects: Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
    ) -> Vec<PalwCarriedObjectV1> {
        objects.into_iter().map(|object| PalwCarriedObjectV1 { object, carrier_fee: PALW_RENT_UNPRICED }).collect()
    }

    /// The same filter, with the state it folded to — ADR-0064's half. The bootstrap lookup reads
    /// bonds out of THIS, so a test that only saw the accepted objects could not tell the fixed
    /// behaviour ("the registry the transition will have") from the one it replaced ("the first
    /// `BondRegistered` in the list"), which differ exactly when a block registers and then
    /// touches the same bond.
    #[cfg(test)]
    pub(super) fn palw_v2_accepted_objects_and_state_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
        block: BlockHash,
    ) -> (Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>, kaspa_consensus_core::palw_state_v2::PalwChainStateV2) {
        self.palw_v2_accepted_objects(state, state_params, point, Self::unpriced_for_tests(objects), block)
    }

    /// [`Self::palw_v2_accepted_objects`] with each object's carrier fee spelled out, so the
    /// ADR-0075 rent rules can be exercised from the sibling test module.
    #[cfg(test)]
    pub(super) fn palw_v2_accepted_priced_objects_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, u64)>,
        block: BlockHash,
    ) -> (Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>, kaspa_consensus_core::palw_state_v2::PalwChainStateV2) {
        let objects = objects.into_iter().map(|(object, carrier_fee)| PalwCarriedObjectV1 { object, carrier_fee }).collect();
        self.palw_v2_accepted_objects(state, state_params, point, objects, block)
    }

    /// **The REAL one-shot block fold, over the whole accepted list, using the same fences the
    /// pipeline resolves** — the transition the acceptance rehearsal exists to predict (A-1).
    ///
    /// Byte-for-byte the fold the virtual processor runs on a chain candidate's PALW content, for
    /// an object-only block (no own work, empty mergeset). It resolves the ADR fences at
    /// `point.daa_score` through the SAME accessors the filter uses, so a divergence this exposes
    /// is a divergence between the filter and the transition and nothing else. The sibling test
    /// module calls it to check the filter's one contract: what `palw_v2_accepted_objects` returns,
    /// this applies on the parent state without error.
    #[cfg(test)]
    pub(super) fn palw_v2_block_fold_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
    ) -> Result<kaspa_consensus_core::palw_state_v2::PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v2_with_extras(
            state,
            state_params,
            point,
            objects,
            None,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for(point),
        )
        .map(|(state, _delta)| state)
    }

    /// **How many court re-executions this object is about to buy** (ADR-0075 SA-2), or `None`
    /// when it buys none.
    ///
    /// `Some` for a directly carried `FamilyCertified` — the count is a field of its evidence —
    /// and for the `ObjectChunk` that COMPLETES a group, because that chunk's arm assembles the
    /// object and applies it in the same block. The assembly is repeated here rather than
    /// threaded out of the transition: it is a borsh decode of at most
    /// `PALW_OBJECT_CHUNK_MAX_BYTES × PALW_OBJECT_CHUNK_MAX_COUNT` bytes, which is the cost this
    /// rule is protecting against paying thirty-two times over, and it runs only while the rent
    /// fence is armed.
    ///
    /// `None` — no rent owed — for anything else, INCLUDING a group whose bytes do not decode: an
    /// object nobody can decode is never graded, so it buys no court time, and the transition
    /// refuses it on its own (`ChunkedObjectUndecodable`).
    fn palw_v2_graded_vector_count(
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<usize> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
            Obj::FamilyCertified { evidence } => Some(evidence.vector_count()),
            Obj::ObjectChunk { group, index, count, bytes } => {
                let mut parts: std::collections::BTreeMap<u8, &[u8]> = Default::default();
                if let Some(pending) = state.pending_chunk_group(group) {
                    if pending.count != *count || pending.parts.contains_key(index) {
                        return None;
                    }
                    parts.extend(pending.parts.iter().map(|(i, part)| (*i, part.as_slice())));
                }
                parts.insert(*index, bytes.as_slice());
                if parts.len() != *count as usize {
                    return None;
                }
                let mut whole = Vec::with_capacity(parts.values().map(|p| p.len()).sum());
                for i in 0..*count {
                    whole.extend_from_slice(parts.get(&i)?);
                }
                match borsh::from_slice::<Obj>(&whole) {
                    Ok(Obj::FamilyCertified { evidence }) => Some(evidence.vector_count()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn palw_v2_accepted_objects(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<PalwCarriedObjectV1>,
        block: BlockHash,
    ) -> (Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>, kaspa_consensus_core::palw_state_v2::PalwChainStateV2) {
        // **Filtered SEQUENTIALLY, against the state each accepted object leaves behind.**
        //
        // Validating every object against the parent state alone is wrong in exactly the way the
        // transition is right: the transition applies them in order, so the second object naming a
        // claim meets the phase the first one moved it to. Two nodes independently assembling the
        // same quorum — which is the design, since one funded submitter per network suffices and
        // several may be funded — put two `ReceiptLicensed` for one claim in one block. Both passed
        // this filter (both were valid against the parent), the transition applied the first, and
        // the second was refused as `wrong phase for ReceiptLicensed` — killing an honest block for
        // carrying a duplicate of its own valid object.
        //
        // Measured on testnet-12: 175 blocks produced, 23 accepted, 74 disqualified, the chain's
        // DAA frozen at 103 while three hosts submitted correctly.
        //
        // Folding the state forward here makes the filter ask the question the transition will ask.
        // A duplicate is then dropped as an object — which is what `Ⅱ.5` decided a failing object
        // should be — and the block stands.
        // The fold is a REHEARSAL, and its chain point has to advance for the transition to accept
        // it at all: `apply_palw_transition_v3` demands a strictly increasing blue score, so
        // re-applying at the block's own point succeeds exactly once and then refuses everything
        // with `blue_score must strictly increase` — which silently dropped every object after the
        // first, licensed nothing, and left the chain producing blocks whose weight never moved.
        // (Measured before this line existed: 356 blocks, 72 submissions, zero licensed.)
        //
        // Each rehearsal step therefore gets its own synthetic point one blue score along. The
        // fold's job is to answer "what phase will this claim be in when the transition reaches
        // the next object", and that answer does not depend on the point's exact value — only on
        // the objects already applied.
        // **A-1 fix (mainnet audit 2026-09-11): rehearse the fold's step 3, not a whole-block
        // transition per object.** The real fold applies every object in step 3, then runs
        // activation, budgets and block/merged work ONCE. Rehearsing each object through a full
        // transition ran activation between objects, so a `ClassLaneCertified` for a class that
        // becomes `Active` in this very block was accepted here and refused by the real fold —
        // disqualifying the block on every node and halting the chain. `folded` is now the fold's
        // pre-object base (payout/settlement drain, sweeps, retarget/growth/reclamation, once), and
        // each accepted object advances it by one `apply_object` and nothing else.
        // **A-1/AC-SLOT are gated to the audit flag day (`palw_audit_2026_09_11`).** Below the
        // fence this filter reproduces the pre-audit behavior EXACTLY — it rehearses each object
        // through a whole-block transition at a synthetic point, and charges the court slot on the
        // acceptance check — so a build with the fence and one without fold every block before the
        // height identically. Past the fence it folds the transition's step 3 (A-1) and charges the
        // slot only after a move applies (AC-SLOT).
        let audit_active = self.palw_audit_2026_09_11_at(point.daa_score);
        // The old path's synthetic chain point; unused past the fence.
        let mut rehearsal = *point;
        let mut folded = if audit_active {
            match kaspa_consensus_core::palw_state_v2::palw_v2_pre_object_base_v1(
                state,
                state_params,
                point,
                self.palw_unavailable_abstains_at(point.daa_score),
                self.palw_capability_bound_at(point.daa_score),
                self.palw_uncertified_weightless_at(point.daa_score),
                self.palw_da_court_at(point.daa_score),
                &self.palw_transition_extras_for(point),
            ) {
                Ok(base) => base,
                // The pre-object steps are what the real fold runs before any object; if they error
                // the block is disqualified whatever it carries, so the filter accepts nothing.
                Err(why) => {
                    info!(
                        "Block {block}: no PALW object is accepted; the pre-object fold fails and the block will be disqualified: {why}"
                    );
                    return (Vec::new(), state.clone());
                }
            }
        } else {
            state.clone()
        };
        let mut accepted = Vec::with_capacity(objects.len());
        let mut certifications_graded = 0usize;
        // ADR-0080 design A, W9: how many declared closes this block has already completed.
        let mut court_closes_completed = 0usize;
        // **ADR-0042 Decision 10 / ADR-0087 Decision 3 (mainnet audit 2026-09-06, M-10): the payout
        // rows this block's CARRIER-borne market moves have already promised.**
        //
        // Counted here, in transaction order, against the state each accepted object leaves behind —
        // the same rehearsal `PALW_CERTIFICATION_MAX_PER_BLOCK` and `PALW_COURT_CLOSE_MAX_PER_BLOCK`
        // are counted against — and the excess move is DROPPED with the block standing, exactly as
        // those two are. Invalidating instead would be audit M-01's shape: admission on the
        // lifecycle band is stateless, so a second market carrier relays and mines freely and honest
        // miners would be the ones producing the invalid blocks.
        //
        // Without it the fold would refuse the move with `ModelPayoutQueueFull` and take the whole
        // block with it. The rehearsal drops it first, so the fold's guard is the belt to this
        // brace and is unreachable on the carrier lane.
        let mut model_payout_rows_promised = 0usize;
        // **ADR-0075 SA-1/SA-2's fence, asked HERE and not only where the fees were resolved.**
        // `false` on every shipped preset, and then neither rent below can fire whatever a caller
        // passed for a carrier fee — so the filter's behaviour is byte-identical to before the
        // amendment rather than identical-if-the-caller-remembers.
        let rent_armed = self.palw_certification_rent_at(point.daa_score);
        // Armed only (see the accounting below): the fault vectors this block has already asked
        // the grader to walk, refused or accepted. Dormant it is never read and never written.
        let mut vectors_graded = 0usize;
        for carried in objects {
            let PalwCarriedObjectV1 { object, carrier_fee } = carried;
            // **ADR-0075 SA-1: a chunk group's opener pays for the SLOT it takes.**
            //
            // A group holds one of `PALW_OBJECT_CHUNK_MAX_GROUPS` rows in the state root for up to
            // `PALW_OBJECT_CHUNK_TTL_DAA` — about five and a half days at a 120 s cadence — and
            // that row is denied to every honest drill just as completely by a group declaring two
            // parts as by one declaring eight. Pricing the DECLARED `count` therefore priced the
            // wrong resource, and priced it in the griefer's favour: his optimum was `count = 2`,
            // eight of which held the whole table for a quarter of what the rule was reasoned
            // about and for LESS per row than ADR-0075 D14's own 3- and 4-part drills pay. The
            // rent is the slot, flat, whatever the opener says it will send; chunks that merely
            // EXTEND an open group pay nothing, because the row is already paid for and charging
            // them would fall on the carrier that completes.
            //
            // Refused BEFORE the group is opened, so the junk never reaches the state root.
            if rent_armed
                && let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectChunk { group, .. } = &object
                && folded.pending_chunk_group(group).is_none()
            {
                let owed = kaspa_consensus_core::palw_state_v2::palw_object_chunk_group_rent_v1();
                if carrier_fee < owed {
                    info!(
                        "Block {block}: an ObjectChunk opening group {group} was dropped, and the block stands: its carrier paid \
                         {carrier_fee} sompi and one of the {} pending-chunk slots rents for {owed} (ADR-0075 SA-1)",
                        kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_GROUPS
                    );
                    continue;
                }
            }
            // **ADR-0080 design A: a declared close buys one adjudication, and pays for it before
            // the group opens.**
            //
            // The court lane's `palw_certification_min_fee_v1`, on the object that carries the
            // count the work is a function of — see `palw_court_close_min_fee_v1` for why it is the
            // DECLARATION and not the completing chunk: chunks arrive in any order, so "the last
            // one" is the declarer's choice of `index` and pricing on it is a discount of up to
            // `max_close_chunks`. Refused BEFORE the row is written, exactly as the slot rent above
            // is, so an underpaid declaration never reaches the state root and never commits any
            // validator to the assembly it would have bought.
            //
            // Dropped rather than block-invalidating, for SA-2's own reason: admission on the
            // lifecycle band is stateless, so an underpaid carrier relays and mines freely and it
            // would be honest miners producing the invalid blocks. Behind the same
            // `palw_certification_rent` fence as the two rents it copies — `None` on every shipped
            // preset, and dormant this whole block is skipped.
            // **ADR-0088 Decision 11's rent, actually collected** (audit 2026-09-19).
            //
            // `palw_object_rent_ceiling_v1` prices a line founding, a proposal and an evaluation at
            // `PALW_MODEL_OBJECT_RENT_SOMPI_V1`, and the coinbase burns that much — but it burns
            // `min(rent, fee)` (utxo_validation.rs), which is a CEILING on the burn and never a
            // floor on the fee. A carrier paying one sompi burned one sompi, so the anti-spam price
            // of the permissionless registry was not collected from anyone. The two objects above
            // are refused by name when their carrier underpays; these three were not, and they are
            // the three that fill a bounded table.
            //
            // Refused BEFORE the row is written, exactly as the two above are, and dropped with the
            // block standing for the reason audit M-01 gives: admission on the lifecycle band is
            // stateless, so invalidating would put honest miners on the losing side of somebody
            // else's underpayment.
            //
            // On the same dormant fence as the other two: `palw_certification_rent` is `None` on
            // every shipped preset, so this changes no block until an operator arms it.
            if rent_armed
                && matches!(
                    &object,
                    kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ModelLineFounded { .. }
                        | kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ModelProposalPosted { .. }
                        | kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ModelEvaluationPosted { .. }
                )
            {
                let owed = kaspa_consensus_core::palw_state_v2::palw_object_rent_ceiling_v1(&object);
                if carrier_fee < owed {
                    info!(
                        "Block {block}: a model-registry object was dropped, and the block stands: its carrier paid \
                         {carrier_fee} sompi and a row in a bounded registry table rents for {owed} (ADR-0088 Decision 11)"
                    );
                    continue;
                }
            }
            if rent_armed
                && let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::CourtCloseDeclared { session_id, count, .. } =
                    &object
            {
                let owed = kaspa_consensus_core::palw_state_v2::palw_court_close_min_fee_v1(*count as u64);
                if carrier_fee < owed {
                    info!(
                        "Block {block}: a CourtCloseDeclared for session {session_id} was dropped, and the block stands: its \
                         carrier paid {carrier_fee} sompi and adjudicating a close of {count} chunks rents for {owed} \
                         (ADR-0080 design A)"
                    );
                    continue;
                }
            }
            // ADR-0075 Decision 9: a block grades at most `PALW_CERTIFICATION_MAX_PER_BLOCK`
            // family drills. Counted before grading, in transaction order, so every node drops the
            // same object and the grader is never run for a drill the block may not carry.
            // A chunk that completes its group applies the FamilyCertified it carried, so it
            // counts against the same cap (ADR-0075 Decision 14).
            //
            // **A chunk the transition will refuse on its face completes nothing, and must not
            // spend the cap.** This asked only `count == 1` for a group that does not exist yet,
            // so `ObjectChunk { group: anything, index: 5, count: 1 }` — sixty bytes, refused by
            // the transition as `ChunkIndexOutOfRange` — was charged as a grading. Two of them per
            // block exhaust `PALW_CERTIFICATION_MAX_PER_BLOCK` and every genuine `FamilyCertified`
            // in that block is dropped, for two ordinary fees.
            //
            // **The index rule alone bought nothing, and that was the whole defect.** `index: 0,
            // count: 1` was always available at the identical sixty bytes, is refused by the
            // transition just as fast (`ChunkGroupHashMismatch`), and starves the identical cap —
            // so a version of this that asked only `index < count` deleted a strictly dominated
            // variant and left the attack. What the cap is spent on is the `FamilyCertified` the
            // completing chunk carries, so "completes" has to mean what the TRANSITION means by
            // it: every part present, the assembled bytes hashing to the declared group id, and
            // the object they decode to being a `FamilyCertified`. That is
            // [`Self::palw_chunk_completes_a_certification_v1`], and asking it here is the same
            // agreement-with-the-transition this whole rehearsal exists to keep.
            //
            // **Behind ADR-0075 D14's fence, `None` on every shipped preset.** It decides which
            // objects a block accepts and therefore its state root, so an upgraded node and an
            // un-upgraded one must not disagree about it in silence — see
            // [`Self::palw_chunk_cap_charge_at`]. Dormant, every added conjunct is `true` and the
            // predicate is byte-identical to the one before the fence existed.
            let cap_charged_on_grading = self.palw_chunk_cap_charge_at(point.daa_score);
            // Armed: the fault-vector count of the certification this chunk would assemble, read
            // ONCE here rather than decoded again for the work charge below. `None` — and dormant,
            // always `None` — means the payload was never touched.
            let mut chunk_vectors: Option<usize> = None;
            let completes_a_group = match &object {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectChunk { group, index, count, bytes } => {
                    let pending = folded.pending_chunk_group(group);
                    // The cheap structural test decides almost every chunk, and dormant it is the
                    // WHOLE test — byte-identical to the accounting before the fence existed.
                    let structurally_completes = match pending {
                        Some(p) => p.count == *count && !p.parts.contains_key(index) && p.parts.len() + 1 == *count as usize,
                        None => *count == 1,
                    };
                    if !structurally_completes {
                        false
                    } else if !cap_charged_on_grading {
                        true
                    } else if *index >= *count {
                        // `ChunkIndexOutOfRange`: the transition refuses it on its face.
                        false
                    } else {
                        // Armed, and only here does anything touch the payload: the transition's
                        // own completion test, and the grading work the object it assembles to
                        // will cost.
                        chunk_vectors =
                            Self::palw_chunk_completes_a_certification_v1(pending.map(|p| &p.parts), *index, *count, bytes, group);
                        chunk_vectors.is_some()
                    }
                }
                _ => false,
            };
            let is_certification =
                matches!(object, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::FamilyCertified { .. })
                    || completes_a_group;
            // **What the block's cap counts is what the court will actually re-execute.**
            //
            // The predicate above answers "does this chunk finish its group", which is not the same
            // question. `ObjectChunk { group: <fresh>, index: 5, count: 1 }` finishes a group by
            // that reading and the transition then refuses it as `ChunkIndexOutOfRange`; so does
            // `{ index: 0, count: 1, bytes: [1 byte] }`, which assembles into bytes no object
            // decodes from. Sixty bytes either way, and two of them per block exhausted
            // `PALW_CERTIFICATION_MAX_PER_BLOCK` and dropped every genuine `FamilyCertified` the
            // block carried — a block-cheap way to keep an honest class weightless.
            //
            // `palw_v2_graded_vector_count` is exactly the "will the court grade this" predicate:
            // it assembles, decodes, and answers `None` for everything the transition will refuse
            // before `apply_object(FamilyCertified)`. Counting on IT rather than on "completes a
            // group" charges the cap for court work and for nothing else — and it subsumes the
            // narrower `index < count` reading of the same hole, so the two compose: with both
            // rules in force this one already refuses everything that one does.
            //
            // Behind the SAME fence as the two rents (`palw_certification_rent`), because it
            // decides which objects a block accepts and therefore its state root — an upgraded and
            // an un-upgraded node must never answer that differently in silence. Dormant, the
            // count is skipped entirely and the predicate below is the one that shipped.
            let graded_vectors = rent_armed.then(|| Self::palw_v2_graded_vector_count(&folded, &object)).flatten();
            // Which of the two fences answers depends on which is armed, and they compose in one
            // direction: `palw_chunk_completes_a_certification_v1` (D14) also demands the assembled
            // bytes hash to the declared group id, so anything it calls a completion
            // `palw_v2_graded_vector_count` calls one too. The slot gate is therefore never tighter
            // than the work gate, and no object is charged a grading it was not also priced for.
            let charges_a_grading_slot = if rent_armed { graded_vectors.is_some() } else { is_certification };
            if charges_a_grading_slot {
                // **ADR-0075 SA-2: grading is priced before it is performed.**
                //
                // The court re-executes one recorded fault vector per vector the object carries,
                // up to `PALW_CERTIFICATION_MAX_VECTORS`, on every node, whether the drill passes
                // or is refused — and the vector count is a field the submitter writes, tied to
                // nothing about how much the object cost to carry. So the count is priced here,
                // from the object alone, and an underpaid one is dropped BEFORE it is graded and
                // before it consumes a grading slot: charging it a slot would let a griefer starve
                // the honest drills for free, which is the same denial the cap exists to stop.
                //
                // **DROPPED, where the amendment says "a block that grades one paying less is
                // invalid".** The security property is the same — nothing underpaid is ever
                // graded, so the CPU a block costs every validator is bounded by fees its carriers
                // paid — and the difference is who dies for a stranger's transaction. Admission on
                // the 0x4b band is stateless (decode, version, may-ride), so an underpaid carrier
                // relays and mines freely; making the accepting block invalid is audit M-01's
                // shape exactly, where "one ~100-byte transaction, one ordinary fee, and the chain
                // stops" — and a template builder selects transactions by fee without ever
                // consulting this filter, so honest miners would be the ones producing the invalid
                // blocks. Dropping needs no mempool rule to be safe; invalidity would.
                if let Some(vectors) = graded_vectors {
                    let owed = kaspa_consensus_core::palw_state_v2::palw_certification_min_fee_v1(vectors);
                    if carrier_fee < owed {
                        info!(
                            "Block {block}: a FamilyCertified object was dropped ungraded, and the block stands: its carrier paid \
                             {carrier_fee} sompi and grading {vectors} fault vectors rents for {owed} (ADR-0075 SA-2)"
                        );
                        continue;
                    }
                }
                // **And the SLOT itself is ADR-0075 D14's question, under D14's own fence.**
                // Lane C's rule above decides what may be charged; this decides WHEN. The two are
                // separate fences because they were found separately and each is sound alone: the
                // rent bounds what a block's grading may cost its carriers, and D14 bounds what a
                // slot may be spent on. Dormant, this is the accounting that shipped.
                if !cap_charged_on_grading {
                    // **Dormant — every shipped preset — this is the pre-fence accounting verbatim.**
                    if certifications_graded >= kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK {
                        info!(
                            "Block {block}: a FamilyCertified object was dropped, and the block stands: the block already carries \
                             {} (PALW_CERTIFICATION_MAX_PER_BLOCK)",
                            kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK
                        );
                        continue;
                    }
                    certifications_graded += 1;
                } else {
                    // **Armed: a SLOT is what a certification the grader accepted has spent, and the
                    // grading WORK is charged in fault vectors before the grader runs.**
                    //
                    // Charging the slot up front was the defect, and the chunk rule closed only the
                    // side door: `matches!(object, FamilyCertified{..})` charged it too, before
                    // `palw_v2_validate_objects` or the transition ever ran. So an object the court
                    // refuses at its first line — `NoVectors`, decided before a single fault vector is
                    // graded — took a slot from a family that had earned one, and two of them per
                    // block dropped every genuine certification the block carried. Proven by
                    // experiment twice, with the fence armed, in both the direct and the chunked
                    // shape.
                    //
                    // Slots are therefore charged in the `Ok` arm of the apply below, to objects that
                    // WERE graded and accepted, which preserves Decision 9 exactly: at most
                    // `PALW_CERTIFICATION_MAX_PER_BLOCK` families enter the state per block. What an
                    // object costs before it is graded is the vectors it asks the grader to walk —
                    // `Vec::len`, free to read — against the block's fixed work budget, so a
                    // certification the court will refuse for free costs the block nothing.
                    //
                    // **Residual, stated rather than hidden.** The work budget is still first-come:
                    // an attacker who builds `PALW_CERTIFICATION_GRADING_VECTORS_PER_BLOCK` vectors of
                    // well-formed-but-wrong evidence exhausts it and starves the block. That costs it
                    // two `PalwShapeProfileV3`s per vector — the first thing the grader compares — so
                    // the price of the attack rises from two sixty-byte carriers to tens of kilobytes
                    // of structurally plausible drill, and it is bounded by the block's own byte
                    // limit. Pricing it outright is a deposit, which is ADR-0075's chunk-deposit work
                    // and not this fence.
                    if certifications_graded >= kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK {
                        info!(
                            "Block {block}: a FamilyCertified object was dropped, and the block stands: the block already certified {} \
                             families (PALW_CERTIFICATION_MAX_PER_BLOCK)",
                            kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK
                        );
                        continue;
                    }
                    let vectors = match &object {
                        kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::FamilyCertified { evidence } => {
                            evidence.vector_count()
                        }
                        _ => chunk_vectors.unwrap_or(0),
                    };
                    if vectors_graded.saturating_add(vectors) > PALW_CERTIFICATION_GRADING_VECTORS_PER_BLOCK {
                        info!(
                            "Block {block}: a FamilyCertified object was dropped, and the block stands: it would ask for {vectors} more \
                             fault vectors than the block's remaining grading budget ({} of {PALW_CERTIFICATION_GRADING_VECTORS_PER_BLOCK} \
                             already spent)",
                            vectors_graded
                        );
                        continue;
                    }
                    vectors_graded += vectors;
                }
            }
            // **ADR-0080 design A: a block completes at most one declared close.**
            //
            // Counted here, in transaction order, against the state each accepted object leaves
            // behind — the same rehearsal every other per-block cap is counted against — and the
            // extra one is DROPPED with the block standing, mirroring
            // `PALW_CERTIFICATION_MAX_PER_BLOCK` exactly. Invalidating instead would be audit
            // M-01's shape: admission on the lifecycle band is stateless, so a second completing
            // carrier relays and mines freely, and honest miners would be the ones producing the
            // invalid blocks.
            //
            // Not fenced, because the objects it counts do not exist before this state version:
            // a chain that can carry a `CourtCloseChunk` at all is one that already re-minted for
            // `PALW_STATE_V2_VERSION` 18, so there is no un-upgraded node to disagree with.
            //
            // **ADR-0082 Decision 2 counts here too**, on the same slot: a dissection move is
            // court CPU the carrying block must spend (a fold checked, an artifact proven, a
            // binding verified), and giving it a second per-block budget would be two answers to
            // "how much court may one block ask for". Same drop-not-invalidate shape, same
            // constant, one counter.
            //
            // **Audit C-1: the slot is SPENT below, once acceptance has said the object is real.**
            // Both halves of the disjunction are state-dependent predicates over the folded state
            // — `palw_court_move_spends_the_slot_v1` replaced a bare variant match that let an
            // unauthenticated `CourtAttnRootClaimed` burn the network's only slot for the price of
            // one transaction fee — but a predicate cannot see a signature or the k-ary fence, and
            // `palw_v2_validate_objects` can. So the CAP is read here, before any work is asked
            // for, and the COUNTER moves after acceptance returns `Ok`: an object acceptance drops
            // did no court work (the drop path does none), and must not deny the block's real
            // close its slot. A close denied inside its assembly window is not a delay, it is the
            // conviction of the party that filed it.
            // **ADR-0042 Decision 10's queue, on the carrier lane** (audit M-10). A market move
            // writes fee legs into `pending_payouts`, which drains at
            // `PALW_V2_MAX_PAYOUTS_PER_BLOCK` and is capped at `PALW_V2_MAX_PENDING_PAYOUTS`. The
            // fold refuses the move that would breach the cap; the rehearsal drops it here so the
            // block stands. Read against `folded`, which is the state this block's earlier objects
            // have already moved — the same state the fold will see.
            {
                use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as MObj;
                let rows = match &object {
                    MObj::ModelBuy { .. } => 2usize,
                    MObj::ModelSell { .. } => 3usize,
                    MObj::ModelSeed { .. } => 0usize,
                    _ => 0usize,
                };
                if rows > 0 {
                    let held = folded.pending_payouts_iter().count();
                    let cap = kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PENDING_PAYOUTS;
                    if held.saturating_add(model_payout_rows_promised).saturating_add(rows) > cap {
                        info!(
                            "Block {block}: a model-market move was dropped, and the block stands: the payout queue holds {held} \
                             rows and this block has already promised {model_payout_rows_promised} more, against a cap of {cap} \
                             (PALW_V2_MAX_PENDING_PAYOUTS)"
                        );
                        continue;
                    }
                    model_payout_rows_promised += rows;
                }
            }
            let spends_the_court_slot = kaspa_consensus_core::palw_state_v2::palw_court_close_completes_a_group_v1(&folded, &object)
                || kaspa_consensus_core::palw_state_v2::palw_court_move_spends_the_slot_v1(&folded, &object);
            if spends_the_court_slot && court_closes_completed >= kaspa_consensus_core::palw_state_v2::PALW_COURT_CLOSE_MAX_PER_BLOCK {
                info!(
                    "Block {block}: a court move that spends the block's adjudication slot was dropped, and the block \
                     stands: the block already spent {} (PALW_COURT_CLOSE_MAX_PER_BLOCK)",
                    kaspa_consensus_core::palw_state_v2::PALW_COURT_CLOSE_MAX_PER_BLOCK
                );
                continue;
            }
            match self.palw_v2_validate_objects(&folded, state_params, point, std::slice::from_ref(&object)) {
                Ok(()) => {
                    // **AC-SLOT below the fence: the slot is charged on the acceptance check** (the
                    // pre-audit behavior). Past the fence it is charged only after the move applies.
                    if !audit_active && spends_the_court_slot {
                        court_closes_completed += 1;
                    }
                    let applied = if audit_active {
                        kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_v1(
                            &folded,
                            state_params,
                            point,
                            &object,
                            self.palw_unavailable_abstains_at(point.daa_score),
                            self.palw_capability_bound_at(point.daa_score),
                            self.palw_uncertified_weightless_at(point.daa_score),
                            self.palw_da_court_at(point.daa_score),
                            &self.palw_transition_extras_for(point),
                        )
                    } else {
                        // The pre-audit path: rehearse the object through a whole-block transition
                        // at a synthetic point one blue score along. Byte-identical to before the
                        // audit fence existed.
                        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v2_with_extras(
                            &folded,
                            state_params,
                            &rehearsal,
                            std::slice::from_ref(&object),
                            None,
                            self.palw_unavailable_abstains_at(point.daa_score),
                            self.palw_capability_bound_at(point.daa_score),
                            self.palw_uncertified_weightless_at(point.daa_score),
                            self.palw_da_court_at(point.daa_score),
                            &self.palw_transition_extras_for(point),
                        )
                        .map(|(next, _)| next)
                    };
                    match applied {
                        Ok(next) => {
                            // **AC-SLOT past the fence: the slot is spent only by a move that
                            // ACTUALLY APPLIES.** A move the fold refuses is dropped below and
                            // leaves the slot for the next.
                            if audit_active && spends_the_court_slot {
                                court_closes_completed += 1;
                            }
                            if !audit_active {
                                rehearsal.blue_score = rehearsal.blue_score.saturating_add(1);
                            }
                            if completes_a_group {
                                // The certification a chunk group carried is applied inside the
                                // chunk's own arm, so the kinds tally below never names it; say so
                                // here, in the same words a directly carried one gets, so an
                                // operator reading every validator's log for the same verdict
                                // (ADR-0075 §7) sees the family land.
                                info!(
                                    "Block {block}: PALW lifecycle carried 1× FamilyCertified (assembled from its chunks; {} attempt-lane and {} free-prompt-lane families are chain-certified now)",
                                    next.chain_certified_families(kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt)
                                        .len(),
                                    next.chain_certified_families(
                                        kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt
                                    )
                                    .len()
                                );
                            }
                            // **Armed: the slot is spent HERE, by a certification the grader
                            // accepted.** Dormant it was already charged before the grader ran,
                            // which is the accounting this fence replaces.
                            if cap_charged_on_grading && is_certification {
                                certifications_graded += 1;
                            }
                            folded = next;
                            // **ADR-0067: keep the declaration the chain just accepted.** The
                            // state retains the class's economic facts and drops the carriage;
                            // a node that will SERVE the class needs the graph, so it is indexed
                            // here — the accept path IBD replays, which is what makes a syncing
                            // node's index arrive with its chain.
                            if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                                class_id,
                                artifact_root,
                                admission: Some(carriage),
                                ..
                            } = &object
                            {
                                match borsh::to_vec(carriage.as_ref()) {
                                    Ok(bytes) => {
                                        let record = crate::model::stores::palw_class_carriage::PalwClassCarriageRecord {
                                            registered_daa: point.daa_score,
                                            artifact_root: *artifact_root,
                                            carriage: bytes,
                                        };
                                        if let Err(err) = self.palw_class_carriage_store.write().insert(*class_id, record) {
                                            warn!("[palw-class-carriage] could not index class {class_id}: {err}");
                                        }
                                    }
                                    Err(err) => {
                                        warn!("[palw-class-carriage] class {class_id}'s carriage does not re-serialize: {err}")
                                    }
                                }
                            }
                            accepted.push(object);
                        }
                        Err(why) => {
                            info!("Block {block}: a PALW lifecycle object was dropped, and the block stands: {why}");
                        }
                    }
                }
                Err(why) => {
                    info!("Block {block}: a PALW lifecycle object was dropped, and the block stands: {why}");
                }
            }
        }
        // **What this block actually carried.** A dropped object says so; an ACCEPTED one said
        // nothing at all, so "the chain is not carrying courts" and "the chain is carrying courts
        // and something later discards them" looked identical from every log on every node. On the
        // testnet-11 drill that ambiguity cost a full investigation: 176 `CourtOpened` submitted
        // with zero mempool refusals and zero drops, and no way to tell whether they were reaching
        // blocks. One line per block that carried anything, kinds only.
        if !accepted.is_empty() {
            let mut kinds: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
            for object in &accepted {
                *kinds.entry(palw_object_kind_name(object)).or_default() += 1;
            }
            info!(
                "Block {block}: PALW lifecycle carried {}",
                kinds.iter().map(|(k, n)| format!("{n}× {k}")).collect::<Vec<_>>().join(", ")
            );
        }
        // **The rehearsal's end state, handed back rather than recomputed.** ADR-0064 has to ask
        // "what does the bond registry look like once this block's own objects are applied", and
        // the only honest answer is the one this loop just built. Recomputing it beside this
        // function would be a second fold obliged to agree with the first, which is the shape of
        // defect ADR-0064 exists to remove. Only bond records are read out of it: every field of
        // one is a function of the objects and `daa_score`, never of the synthetic blue score the
        // rehearsal advances.
        (accepted, folded)
    }

    /// **Assemble the largest lifecycle object this node's receipt pool supports** (launch
    /// blockers: "what is still missing", piece 3's consensus half).
    ///
    /// Takes whatever receipts gossip delivered — unverified, possibly garbage, possibly for the
    /// wrong claim — and answers with the object a block would ACCEPT, or `None` if no quorum is
    /// assemblable yet. It reuses `validate_receipt_quorum_v2` itself, growing the set one receipt
    /// at a time and dropping any candidate the validator refuses, so the submitter and the
    /// acceptance layer cannot disagree about what a quorum is: the object this returns is checked
    /// by the same function that will check it on-chain, at the same state.
    ///
    /// Greedy-incremental rather than power-set: the validator refuses a SET for the first bad
    /// member, so a single poisoned receipt among honest ones must not sink the assembly. Order
    /// within the pool is the caller's arrival order; a duplicate seat's second receipt is dropped
    /// by the same rule the chain would drop it with.
    ///
    /// The evaluation point is the current sink — where the carrying transaction would be
    /// accepted. A quorum that assembles here can still lapse before inclusion (the receipt
    /// window is checked against the ACCEPTING block's DAA); that is the submitter's race to
    /// lose, not a soundness gap.
    /// **ADR-0124 Decision 2: a seat's own supplementary receipt set, as the door will take it.**
    /// The door is shut — `None` — while the panel-economy fence is dormant at virtual's DAA, while
    /// the claim is not `ReceiptLicensed`, when every receipt in `mine` is already credited or is
    /// not `Valid`, and once the receipt window has closed; otherwise the object carries exactly
    /// the uncredited `Valid` receipts, validated by the same function the acceptance layer runs.
    pub fn palw_v2_supplementary_receipt_assemble_impl(
        &self,
        claim: kaspa_hashes::Hash64,
        mine: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        if !self.palw_panel_economy_active_at(point.daa_score) {
            return None;
        }
        let record = state.claim(&claim)?;
        if !matches!(record.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
            return None;
        }
        let duties = state.panel_duties_of(&claim)?;
        let receipts: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2> = mine
            .iter()
            .filter(|receipt| {
                receipt.claim == claim
                    && matches!(receipt.verdict, kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2::Valid)
                    && duties.get(&receipt.seat_bond).is_some_and(|at| *at == 0)
            })
            .cloned()
            .collect();
        if receipts.is_empty() {
            return None;
        }
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        kaspa_consensus_core::palw_panel_v2::validate_supplementary_receipts_v1(
            &state,
            state_params,
            &point,
            network_domain,
            &claim,
            &receipts,
            Self::verify_mldsa87_with_context_bool,
        )
        .ok()?;
        Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensed { claim, receipts })
    }

    pub fn palw_v2_receipt_quorum_assemble_impl(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2 as Q;
        let state_params = self.palw_state_params_v2.as_ref()?;
        let panel_params = self.palw_panel_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        // The evaluation point is VIRTUAL's — where the carrying transaction will actually be
        // accepted — not the sink's own. The difference is one DAA, and it bites: a receipt signed
        // "now" carries virtual's daa, and a point at the sink's daa refuses it as "signed after
        // the block carrying it". Found by this function returning None on a set the validator
        // itself called `Licensed`.
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let verify = |key: &[u8], message: &[u8], sig: &[u8], context: &[u8]| {
            Self::verify_mldsa87_with_context_bool(key, message, sig, context)
        };

        // **ADR-0100 Decision 4: a claim drawn per shard gets its next ready PART**, one shard a
        // call, by the same greedy rule as the whole licence below and checked by the same
        // function the chain will check it with, at the same point.
        if self.palw_shard_licensing_at(point.daa_score)
            && let Some(plan) = kaspa_consensus_core::palw_shard_licensing_v1::palw_claim_licenses_by_parts_v1(
                &state,
                &claim,
                panel_params.seat_count(),
            )
        {
            let seats = state.panel(&claim)?.seats.clone();
            let licensed = state.shard_licensing_of(&claim).cloned();
            for shard in 0..plan.shard_count {
                if licensed.as_ref().is_some_and(|progress| progress.is_licensed(shard)) {
                    continue;
                }
                let Some(slice) = kaspa_consensus_core::palw_shard_licensing_v1::palw_panel_shard_slice_v1(
                    &seats,
                    plan.shard_count,
                    panel_params.seat_count(),
                    shard,
                ) else {
                    continue;
                };
                let mut kept: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2> = Vec::new();
                let mut ready = false;
                for candidate in candidates.iter().filter(|candidate| slice.iter().any(|seat| seat.bond == candidate.seat_bond)) {
                    if kept.iter().any(|receipt| receipt.seat_bond == candidate.seat_bond) {
                        continue;
                    }
                    let mut attempt = kept.clone();
                    attempt.push(candidate.clone());
                    let part = kaspa_consensus_core::palw_shard_licensing_v1::PalwShardReceiptPartV1 {
                        claim,
                        shard_count: plan.shard_count,
                        shard_index: shard,
                        receipts: attempt.clone(),
                    };
                    match kaspa_consensus_core::palw_panel_v2::validate_shard_receipt_part_v1(
                        &state,
                        panel_params,
                        state_params,
                        &point,
                        network_domain,
                        &part,
                        verify,
                        self.palw_unavailable_abstains_at(point.daa_score),
                    ) {
                        Ok(_) => {
                            kept = attempt;
                            ready = true;
                        }
                        Err(kaspa_consensus_core::palw_panel_v2::PalwPanelV2Error::NoQuorum { .. }) => kept = attempt,
                        Err(_) => {}
                    }
                }
                if ready {
                    return Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ShardReceiptLicensed {
                        part: kaspa_consensus_core::palw_shard_licensing_v1::PalwShardReceiptPartV1 {
                            claim,
                            shard_count: plan.shard_count,
                            shard_index: shard,
                            receipts: kept,
                        },
                    });
                }
            }
            return None;
        }

        let mut kept: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2> = Vec::new();
        let mut verdict: Option<Q> = None;
        for candidate in candidates {
            let mut attempt = kept.clone();
            attempt.push(candidate.clone());
            match kaspa_consensus_core::palw_panel_v2::validate_receipt_quorum_v2_with_policy(
                &state,
                panel_params,
                state_params,
                &point,
                network_domain,
                &claim,
                &attempt,
                verify,
                self.palw_unavailable_abstains_at(point.daa_score),
                // ADR-0147: the same height the fold and the acceptance layer read.
                self.palw_admission_independence_daa(),
            ) {
                Ok(q) => {
                    kept = attempt;
                    verdict = Some(q);
                }
                Err(kaspa_consensus_core::palw_panel_v2::PalwPanelV2Error::NoQuorum { .. })
                | Err(kaspa_consensus_core::palw_panel_v2::PalwPanelV2Error::OutsiderHasNotAnswered { .. }) => {
                    // The set is clean but not yet a licence — no quorum, or (ADR-0147) a quorum
                    // still waiting on its outsider's `Valid`. Keep the receipt, keep collecting.
                    kept.push(candidate.clone());
                }
                Err(_) => {
                    // This candidate poisons the set (bad signature, not a seat, duplicate,
                    // outside a window, wrong claim) — drop IT, keep what already stood.
                }
            }
        }
        match verdict? {
            Q::Licensed { .. } => {
                Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: kept })
            }
            Q::ProducerUnavailable { .. } => {
                Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ProducerDefaulted { claim, receipts: kept })
            }
            // Unreachable from this assembler (it validates without the supplementary door), and
            // harmless if it were not: the object is the same kind the door accepts.
            Q::Supplementary { .. } => {
                Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: kept })
            }
        }
    }

    /// ADR-0133 S1: assemble a `ReceiptLicensedV2` from V3 receipts by coverage, past the fence.
    pub fn palw_v2_receipt_coverage_assemble_impl(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2 as Q;
        if !self.palw_verification_v2_at(self.lkg_virtual_state.load().daa_score) {
            return None;
        }
        let state_params = self.palw_state_params_v2.as_ref()?;
        let panel_params = self.palw_panel_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let verify = |key: &[u8], message: &[u8], sig: &[u8], context: &[u8]| {
            Self::verify_mldsa87_with_context_bool(key, message, sig, context)
        };
        let mut kept: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3> = Vec::new();
        let mut verdict: Option<Q> = None;
        for candidate in candidates {
            let mut attempt = kept.clone();
            attempt.push(candidate.clone());
            match kaspa_consensus_core::palw_panel_v2::validate_receipt_coverage_v2(
                &state,
                panel_params,
                state_params,
                &point,
                network_domain,
                &claim,
                &attempt,
                verify,
                self.palw_unavailable_abstains_at(point.daa_score),
                self.palw_admission_independence_daa(),
            ) {
                Ok(q) => {
                    kept = attempt;
                    verdict = Some(q);
                }
                Err(kaspa_consensus_core::palw_panel_v2::PalwPanelV2Error::NoQuorum { .. })
                | Err(kaspa_consensus_core::palw_panel_v2::PalwPanelV2Error::OutsiderHasNotAnswered { .. }) => {
                    kept.push(candidate.clone());
                }
                Err(_) => {}
            }
        }
        match verdict? {
            Q::Licensed { .. } => {
                Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensedV2 { claim, receipts: kept })
            }
            _ => None,
        }
    }

    /// ADR-0133 S2: assemble an `OptimisticLicensed` from V3 receipts when the full-replay seat's
    /// `Valid` is present, past the fence.
    pub fn palw_v2_optimistic_assemble_impl(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        if !self.palw_verification_s2_at(self.lkg_virtual_state.load().daa_score) {
            return None;
        }
        let state_params = self.palw_state_params_v2.as_ref()?;
        let panel_params = self.palw_panel_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let verify = |key: &[u8], message: &[u8], sig: &[u8], context: &[u8]| {
            Self::verify_mldsa87_with_context_bool(key, message, sig, context)
        };
        let mut kept: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3> = Vec::new();
        for candidate in candidates {
            let mut attempt = kept.clone();
            attempt.push(candidate.clone());
            match kaspa_consensus_core::palw_panel_v2::validate_receipt_coverage_v2(
                &state,
                panel_params,
                state_params,
                &point,
                network_domain,
                &claim,
                &attempt,
                verify,
                self.palw_unavailable_abstains_at(point.daa_score),
                self.palw_admission_independence_daa(),
            ) {
                Ok(_) | Err(kaspa_consensus_core::palw_panel_v2::PalwPanelV2Error::NoQuorum { .. }) => kept = attempt,
                Err(_) => {}
            }
        }
        let panel = state.panel(&claim)?;
        let seats: Vec<_> = panel.seats.iter().map(|s| s.bond).collect();
        kaspa_consensus_core::palw_optimistic_licence_v2::palw_optimistic_receipts_license_v2(
            panel.anchor,
            claim,
            &seats,
            &kept,
        )
        .ok()?;
        Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::OptimisticLicensed { claim, receipts: kept })
    }

    /// **The ladder this network froze, ADR-0084 U-08's one accessor.** `max_step_leaf_count` is a
    /// bundle field — inside `palw_ruleset_id_v2`, so it cannot move on a running chain — and it is
    /// DAA-free (`palw_court_params_at_v2` only ever changes the dissection arity). A node with no
    /// bundle has no ladder to apply and falls back to the executor's structural top, which is
    /// exactly as permissive as this path already was.
    fn palw_max_step_leaf_count_v1(&self) -> u64 {
        self.palw_v2_bundle
            .as_ref()
            .map(|bundle| bundle.court.max_step_leaf_count())
            .unwrap_or(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES)
    }

    pub(crate) fn palw_v2_validate_objects(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        let Some(panel_params) = self.palw_panel_params_v2.as_ref() else {
            return if objects.is_empty() { Ok(()) } else { Err("PALW objects on a network with no V2 bundle".to_string()) };
        };
        for object in objects {
            match object {
                Obj::PanelBound { claim, anchor, seats } => {
                    let claim_record = state.claim(claim).ok_or_else(|| format!("panel names unknown claim {claim}"))?;
                    let anchor_fact = self
                        // The REDRAW's base, or the second panel's anchor is derived for a slot
                        // `validate_panel_bound_v2` no longer expects (it moved to
                        // `bind_base_daa()`), and every revived claim fails `AnchorMismatch`.
                        .palw_v2_anchor_fact_of_candidate(point.block, claim_record.bind_base_daa(), panel_params)
                        .ok_or_else(|| format!("no anchor exists yet for claim {claim} on this chain"))?;
                    // ADR-0065 D1. The fence is read at the ANCHOR's DAA, not at this block's,
                    // and the sibling assembler does the same: the panel is a pure function of the
                    // claim, so the rule that decides it has to be one too. Resolving at
                    // `point.daa_score` would make the derived panel change from block to block
                    // around the fence height, and a `PanelBound` that missed the block it was
                    // built for would be refused as a mismatch rather than accepted late.
                    kaspa_consensus_core::palw_panel_v2::validate_panel_bound_v2_with_policy(
                        state,
                        panel_params,
                        state_params,
                        point,
                        claim,
                        &anchor_fact,
                        *anchor,
                        seats,
                        self.palw_bond_maturity_window_at(state, anchor_fact.anchor_daa),
                        // ADR-0071 SA-3, at the ANCHOR for the D1 reason one line up: the panel is
                        // a pure function of the claim, so the rule that narrows it must be one
                        // too — and the assembler below resolves it at the same point.
                        self.palw_capability_bound_at(anchor_fact.anchor_daa),
                        // C-02 (deep fence) and ADR-0124 (the panel economy): the whole draw policy,
                        // resolved at the ANCHOR for the same purity reason — the assembler resolves
                        // it at the same point, so build and validate recompute one identical panel.
                        // The Valid-lock question (route-matrix #3) is the BINDING block's, as the
                        // assembler asks it.
                        kaspa_consensus_core::palw_panel_v2::PalwPanelDrawPolicyV1 {
                            valid_lock: Self::palw_panel_valid_lock_of_v1(
                                state,
                                state_params,
                                &self.palw_transition_extras_for(point),
                                claim_record,
                                point.daa_score,
                            ),
                            ..self.palw_panel_draw_policy_at(anchor_fact.anchor_daa)
                        },
                        // ADR-0100 Decision 4: the same one-place decision the binding made.
                        self.palw_stratified_shard_count(state, &claim_record.class_id, anchor_fact.anchor_daa),
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::CourtOpened { session_id, claim, challenger_bond, space, space_size, signature } => {
                    // **ADR-0103 Decision 1: the held regime plays no bisection.** Refused here and
                    // in the fold, for the DA court's doubled-fence reason.
                    if self.palw_held_context_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim}: the held regime plays no bisection — accuse the leaf in one move (ADR-0103 Decision 1)"
                        ));
                    }
                    // The object now DECLARES the space, because the transition opens a ladder
                    // from it — but the ruleset still DECIDES it. H2's rule is unchanged: a space
                    // the accuser chose is a ladder depth the accuser chose, so a declaration that
                    // is not the catalog's own step-leaf count is refused here, before the
                    // transition ever sees it.
                    let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                    if *space != kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves
                        || *space_size != court.max_step_leaf_count()
                    {
                        return Err(format!(
                            "court {session_id} declares a {space_size}-wide {space:?} space; the ruleset's is {} step leaves",
                            court.max_step_leaf_count()
                        ));
                    }
                    kaspa_consensus_core::palw_court_v2::validate_court_opened_v2(
                        state,
                        state_params,
                        point,
                        session_id,
                        claim,
                        challenger_bond,
                        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
                        court.max_step_leaf_count(),
                        // The challenger's own signature over the session id (audit M-01). Without
                        // it every check above is a fact ABOUT the bond and none about who spoke
                        // for it, so anyone could prosecute under a stranger's identity — and the
                        // transition disarms the claim's final deadline while a session is open,
                        // which turns that into a freeze of an honest producer's claim.
                        signature,
                        Self::verify_mldsa87_with_context_bool,
                    )
                    .map_err(|e| e.to_string())?;
                }
                // **ADR-0080 design A, W6: the declaration is a court move, and it is signed.**
                //
                // This arm refused every `CourtCloseDeclared` outright until W6 landed, and the
                // refusal was right for as long as it stood: a declaration pins every byte of a
                // close and asserts a verdict on behalf of one of the two bonds the session id
                // binds, the transition acts on it, and nothing proved the SIDE authorised it —
                // P0-9's hole in its original words, where either party could write the other's
                // move. Unsigned it was worse than that: a declaration is singular per
                // `(session, side)` for the life of the session and its failure to assemble is the
                // declarer's conviction, so one forged object would have voided an honest
                // producer's claim.
                //
                // `check_court_close_declaration_acceptance_v2` is the same split
                // `CourtDisclosed` and `CourtVerdictPosted` use: the transition reads WHICH bond
                // may declare, this layer reads whether that bond spoke — its registered key is in
                // the candidate state, and neither party names its own.
                Obj::CourtCloseDeclared { session_id, side, count, chunk_digests, close_digest, verdict, signature } => {
                    // The ruleset's own carriage count, which the transition cannot ask for: it has
                    // no `PalwCourtParamsV2` and enforces only the bitmap's structural bound. On
                    // devnet this is 1, so the split path is refused there rather than engaged.
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a court close declaration on a network with no V2 court parameters".to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_close_declared_chunk_count_v2(session_id, *count, court)
                        .map_err(|e| e.to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_court_close_declaration_acceptance_v2(
                        state,
                        session_id,
                        *side,
                        *count,
                        chunk_digests,
                        close_digest,
                        *verdict,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                // **ADR-0080 design A, W7: the chunk that COMPLETES a group is adjudicated here.**
                //
                // A chunk carries no signature and needs none — the declaration already pinned
                // these bytes at this index, and the transition refuses anything else — so for
                // every chunk but the last there is nothing for this layer to add.
                //
                // The last one is a court close. The split exists so that a close too wide for one
                // carrier can be filed at all, and the whole point of design A is that it reaches
                // the SAME verdict by the SAME function: `adjudicate_court_close_v2` over the
                // assembled proof, compared against the verdict the declaration announced, refused
                // in either direction — "an asserted conviction and an asserted acquittal are the
                // same lie", exactly as the one-carrier `CourtClosed` arm below says it.
                //
                // **What this arm does NOT refuse, and why that is not an omission.** Bytes that do
                // not hash to the declaration's own `close_digest`, or that do not decode to this
                // session's `CourtClosed`, are ADMITTED here — because a refused lifecycle object
                // is dropped with the block standing, which would leave the group intact and the
                // declarer sitting on its reserve until the backstop. Those two facts are
                // establishable from rooted state alone and are entirely the declarer's doing, so
                // the transition convicts on them in the block that carries the last chunk (W5's
                // "a failed declaration loses on its OWN side"). This layer only ever refuses what
                // it alone can see: whether the proof inside adjudicates.
                Obj::CourtCloseChunk { session_id, side, index, bytes } => {
                    if kaspa_consensus_core::palw_state_v2::palw_court_close_completes_a_group_v1(state, object)
                        && let Some(group) = state.court_close_group(session_id, *side)
                    {
                        let mut assembled = Vec::new();
                        for i in 0..group.count {
                            match (i == *index).then_some(bytes).or_else(|| group.chunks.get(&i)) {
                                Some(part) => assembled.extend_from_slice(part),
                                // Unreachable behind the predicate above, which is exactly the
                                // "every part present" test — asserted rather than assumed, because
                                // a hole here would adjudicate a truncated proof.
                                None => return Err(format!("court {session_id} completes with chunk {i} missing")),
                            }
                        }
                        let assembles =
                            kaspa_consensus_core::palw_state_v2::palw_court_close_chunk_digest_v1(&assembled) == group.close_digest;
                        let decoded = assembles
                            .then(|| borsh::from_slice::<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>(&assembled).ok())
                            .flatten();
                        // Only when the bytes ARE this session's close does the adjudication run;
                        // anything else is the transition's conviction, not this layer's refusal.
                        if let Some(Obj::CourtClosed { session_id: decoded_session, verdict, proof }) = &decoded
                            && decoded_session == session_id
                            && *verdict == group.verdict
                        {
                            let court = self
                                .palw_court_params_v2
                                .as_ref()
                                .ok_or_else(|| "a court close on a network with no V2 court parameters".to_string())?;
                            let derived = kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(
                                state,
                                session_id,
                                proof,
                                court,
                                self.palw_court_step_ladder_at(point.daa_score, court),
                                self.palw_prompt_ids_form_at(point.daa_score),
                                // ADR-0119 Decision 4: a fused site's rows open at the claim's
                                // ladder under the held regime.
                                self.palw_held_context_at(point.daa_score),
                            )
                            .map_err(|e| e.to_string())?;
                            if derived != *verdict {
                                return Err(format!(
                                    "court {session_id}: the {} side declared {verdict:?} and the close it assembled adjudicates \
                                     {derived:?}",
                                    side.name()
                                ));
                            }
                        }
                    }
                }
                Obj::CourtDisclosed { session_id, disclosure, signature } => {
                    // The responder's rung, under the responder's key. Unsigned, a challenger
                    // could write the answers it wants to convict.
                    kaspa_consensus_core::palw_court_v2::check_court_disclosure_acceptance_v2(
                        state,
                        session_id,
                        disclosure,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                // ADR-0087 Decision 6: the market's two moves exist only past the fence, refused by
                // name before it (the drop-not-invalidate shape). A sell is signed by the key whose
                // payload is the holder (M8), checked here where the verifier lives.
                Obj::ModelBuy { line_id, .. } => {
                    if !self.palw_model_market_active_at(point.daa_score) {
                        return Err(format!("a model buy of line {line_id} on a chain where the model market is not in force"));
                    }
                }
                // ADR-0090: the seed is the market's third move, under the same fence; its sink
                // binding is checked with the buy's (`palw_model_buy_binds_its_carrier_v1`).
                Obj::ModelSeed { line_id, .. } => {
                    if !self.palw_model_market_active_at(point.daa_score) {
                        return Err(format!("a model seed of line {line_id} on a chain where the model market is not in force"));
                    }
                }
                Obj::ModelSell { line_id, holder, units_in, min_msk_out, held_units, not_after_daa, pubkey, signature } => {
                    if !self.palw_model_market_active_at(point.daa_score) {
                        return Err(format!("a model sell of line {line_id} on a chain where the model market is not in force"));
                    }
                    if kaspa_consensus_core::palw_model_market_v1::palw_model_holder_of_pubkey_v1(pubkey) != *holder {
                        return Err(format!("a model sell of line {line_id} is signed by a key that is not the holder's"));
                    }
                    // **ADR-0087 M8 (mainnet audit 2026-09-06, M-11): an authority with an end.**
                    // Past its own score the sell is refused; and a window longer than
                    // `PALW_MODEL_SELL_MAX_WINDOW_DAA_V1` is refused at any score, so "forever" is
                    // not expressible. Both compared against the ACCEPTING block's DAA, which is
                    // the point the holder's key was asked about.
                    match kaspa_consensus_core::palw_model_market_v1::palw_model_sell_window_v1(point.daa_score, *not_after_daa) {
                        kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::Live => {}
                        kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::Expired => {
                            return Err(format!(
                                "a model sell of line {line_id} expired at daa {not_after_daa}; this block is at {}",
                                point.daa_score
                            ));
                        }
                        kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::TooLong { daa } => {
                            return Err(format!(
                                "a model sell of line {line_id} claims authority for {daa} daa, over the {} the chain allows",
                                kaspa_consensus_core::palw_model_market_v1::PALW_MODEL_SELL_MAX_WINDOW_DAA_V1
                            ));
                        }
                    }
                    let message = kaspa_consensus_core::palw_model_market_v1::palw_model_sell_message_v1(
                        // **The network domain every other market/registry message already carries**
                        // (audit M-11). ADR-0088's ten objects all sign under it; this one did not,
                        // so one signature was good on any chain sharing the line id.
                        self.palw_network_domain_v2(),
                        line_id,
                        holder,
                        *units_in,
                        *min_msk_out,
                        *held_units,
                        *not_after_daa,
                    );
                    if !kaspa_txscript::verify_mldsa87_with_context(
                        pubkey,
                        &message,
                        signature,
                        kaspa_consensus_core::palw_model_market_v1::PALW_MODEL_SELL_MLDSA87_CONTEXT,
                    )
                    .unwrap_or(false)
                    {
                        return Err(format!("a model sell of line {line_id} carries a signature the holder's key does not verify"));
                    }
                }
                // ADR-0088 Decision 11: the registry's ten objects exist only past the fence, refused
                // by name before it. Each is attributed to a bond the fold names — the founder, the
                // line's developer, its owner, the proposer, the evaluator — and its signature is
                // checked HERE, against that bond's stored key, over the message the object's fields
                // spell; the fold then enforces referential integrity and the bounds.
                Obj::ModelLineFounded { class_id, name, founder, root, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!(
                            "a line founding on class {class_id} on a chain where the model registry is not in force"
                        ));
                    }
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_line_founded_message_v1(
                        self.palw_network_domain_v2(),
                        class_id,
                        name,
                        founder,
                        root,
                    );
                    self.palw_model_check_bond_signature(state, founder, &message, signature, "a line founding")?;
                }
                Obj::ModelVersionPublished {
                    line_id,
                    version,
                    root,
                    parent,
                    adopted_from,
                    runtime_hash,
                    dataset_commitment,
                    training_config_hash,
                    notes_hash,
                    preview,
                    signature,
                } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a version of line {line_id} on a chain where the model registry is not in force"));
                    }
                    let developer = self.palw_model_line_role(state, line_id, "developer")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        *version,
                        root,
                        *parent,
                        adopted_from.as_ref(),
                        runtime_hash.as_ref(),
                        dataset_commitment.as_ref(),
                        training_config_hash.as_ref(),
                        notes_hash.as_ref(),
                        *preview,
                    );
                    self.palw_model_check_bond_signature(state, &developer, &message, signature, "a version")?;
                }
                Obj::ModelVersionPromoted { line_id, version, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a promotion on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let developer = self.palw_model_line_role(state, line_id, "developer")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_move_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        *version,
                        b"promote",
                    );
                    self.palw_model_check_bond_signature(state, &developer, &message, signature, "a promotion")?;
                }
                Obj::ModelVersionWithdrawn { line_id, version, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a withdrawal on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let developer = self.palw_model_line_role(state, line_id, "developer")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_move_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        *version,
                        b"withdraw",
                    );
                    self.palw_model_check_bond_signature(state, &developer, &message, signature, "a withdrawal")?;
                }
                Obj::ModelLineBenefitsDeclared { line_id, tiers, cadence_daa, expires_daa, signature } => {
                    // ADR-0095 §4.11 as corrected: its own fence, checked here as well as in the
                    // fold, so an object that cannot apply never rides in the first place.
                    if !self.palw_model_benefits_active_at(point.daa_score) {
                        return Err(format!(
                            "a benefits declaration for line {line_id} on a chain where the membership is not in force"
                        ));
                    }
                    // §4.1: the OWNER signs what the line promises, not the developer who ships it.
                    let owner = self.palw_model_line_role(state, line_id, "owner")?;
                    let message = kaspa_consensus_core::palw_model_benefits_v1::palw_model_benefits_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        tiers,
                        *cadence_daa,
                        *expires_daa,
                    );
                    self.palw_model_check_bond_signature(state, &owner, &message, signature, "a benefits declaration")?;
                }
                Obj::ModelLineRolesSet { line_id, developer, maintainer, contributor_permille_of_leg, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a roles change on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let owner = self.palw_model_line_role(state, line_id, "owner")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_roles_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        developer.as_ref(),
                        maintainer.as_ref(),
                        *contributor_permille_of_leg,
                    );
                    self.palw_model_check_bond_signature(state, &owner, &message, signature, "a roles change")?;
                }
                Obj::ModelLineOwnerTransferred { line_id, new_owner, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a transfer of line {line_id} on a chain where the model registry is not in force"));
                    }
                    let owner = self.palw_model_line_role(state, line_id, "owner")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_transfer_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        new_owner,
                    );
                    self.palw_model_check_bond_signature(state, &owner, &message, signature, "a transfer")?;
                }
                Obj::ModelLineRetired { line_id, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a retirement of line {line_id} on a chain where the model registry is not in force"));
                    }
                    let owner = self.palw_model_line_role(state, line_id, "owner")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_retire_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                    );
                    self.palw_model_check_bond_signature(state, &owner, &message, signature, "a retirement")?;
                }
                Obj::ModelProposalPosted { line_id, root, note_hash, by, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a proposal on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_proposal_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        root,
                        note_hash,
                        by,
                    );
                    self.palw_model_check_bond_signature(state, by, &message, signature, "a proposal")?;
                }
                Obj::ModelProposalClosed { line_id, proposal_id, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a proposal close on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let developer = self.palw_model_line_role(state, line_id, "developer")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_proposal_close_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        proposal_id,
                    );
                    self.palw_model_check_bond_signature(state, &developer, &message, signature, "a proposal close")?;
                }
                Obj::ModelEvaluationPosted { line_id, version, evaluator_id, score_permille, report_hash, by, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("an evaluation on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_evaluation_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        *version,
                        evaluator_id,
                        *score_permille,
                        report_hash,
                        by,
                    );
                    self.palw_model_check_bond_signature(state, by, &message, signature, "an evaluation")?;
                }
                Obj::CourtVerdictPosted { session_id, verdict, signature } => {
                    kaspa_consensus_core::palw_court_v2::check_court_verdict_acceptance_v2(
                        state,
                        session_id,
                        verdict,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::CourtClosed { session_id, verdict, proof } => {
                    // The close carries its proof now, so there is something to adjudicate. The
                    // node re-derives the verdict from the proof and compares: a declared verdict
                    // that its own proof does not produce is refused, in EITHER direction — an
                    // asserted conviction and an asserted acquittal are the same lie.
                    //
                    // `adjudicate_court_close_v2` refuses outright when the proof does not
                    // adjudicate (an out-of-catalog kernel, a non-canonical operand set, a
                    // binding naming another execution), so an unadjudicable object convicts
                    // nobody AND acquits nobody — P0-8's rule, on both sides.
                    //
                    // ADR-0049 Decision C, audit H-03: the ruleset's cost ceilings are applied to
                    // the OBJECT, before a single Merkle path is walked. They were checked only
                    // against a CLASS's geometry at admission — a bound the ruleset id commits to
                    // and that nothing enforced where it is actually spendable.
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a court close on a network with no V2 court parameters".to_string())?;
                    let derived = kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(
                        state,
                        session_id,
                        proof,
                        court,
                        self.palw_court_step_ladder_at(point.daa_score, court),
                        self.palw_prompt_ids_form_at(point.daa_score),
                        // ADR-0119 Decision 4.
                        self.palw_held_context_at(point.daa_score),
                    )
                    .map_err(|e| e.to_string())?;
                    if derived != *verdict {
                        return Err(format!("court {session_id} declares {verdict:?}; its own proof adjudicates {derived:?}"));
                    }
                }
                // -------------------------------------------------------------------------
                // ADR-0082 Decision 2/3 — the dissection's three moves.
                //
                // Two gates, in this order: the FENCE (a move on a network that never armed the
                // k-ary court is refused by name — the arm exists in the binary but the rule does
                // not exist on this chain), then the party's signature, the same split every
                // other court move uses.
                // -------------------------------------------------------------------------
                Obj::CourtAttnRootClaimed { session_id, root, arity, signature, .. }
                | Obj::CourtAttnRootClaimedAnchored { session_id, root, arity, signature, .. } => {
                    // ADR-0093 Decision 8: the anchored form exists only where its fence does. (The
                    // plain form's refusal past the fence needs the site, so the fold makes it.)
                    if matches!(object, Obj::CourtAttnRootClaimedAnchored { .. }) && !self.palw_attn_anchored_root_at(point.daa_score)
                    {
                        return Err(format!(
                            "session {session_id}: an anchored root claim before palw_attn_anchored_root is armed (ADR-0093 Decision 8)"
                        ));
                    }
                    kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    // **The declared arity must be the ruleset's own** (patch note 7). The object
                    // states it because the fold cannot derive it — the derivation reads the
                    // bundle — and this is the layer that holds the bundle, so this is the layer
                    // that refuses any other value. Without the comparison the responder would
                    // choose how many children it discloses per round, which is the move budget
                    // Z4 sizes the window against.
                    let derived = self
                        .palw_court_params_at(point.daa_score)
                        .ok_or_else(|| "a dissection move on a network with no V2 bundle".to_string())?
                        .map_err(|e| format!("session {session_id}: {e}"))?;
                    if *arity != derived.dissection_arity() {
                        return Err(format!(
                            "session {session_id}: the root claim declares dissection arity {arity}; this ruleset derives {}",
                            derived.dissection_arity()
                        ));
                    }
                    kaspa_consensus_core::palw_court_v2::check_court_attn_root_claim_acceptance_v2(
                        state,
                        session_id,
                        root,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::CourtAttnDissected { session_id, round, signature } => {
                    kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    kaspa_consensus_core::palw_court_v2::check_court_attn_round_acceptance_v2(
                        state,
                        session_id,
                        round,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::CourtAttnChildChosen { session_id, choice, signature } => {
                    kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    kaspa_consensus_core::palw_court_v2::check_court_attn_choice_acceptance_v2(
                        state,
                        session_id,
                        choice,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::ClassRegistered {
                    class_id,
                    share_permille,
                    admission,
                    activation_daa,
                    artifact_root,
                    slash_value_per_pwu,
                    initial_target,
                    pwu_rule,
                } => {
                    // **ADR-0049 Decision H: the gate, where the refusal used to be.**
                    //
                    // A class registration is the one object whose validity is an arithmetic fact
                    // about a GRAPH — that every kernel it reaches is adjudicable (ADR-0038 A4),
                    // that its longest job fits the ruleset's ladder, that prosecuting it costs
                    // what the ruleset allows (Decision C), and that its declared
                    // `pwu_per_inference` is the COUNTED one rather than a number that multiplies
                    // its own fork-choice weight. `verify_class_admission_v2` performs all four,
                    // and it needs the shape profile and the canonical job to do it.
                    //
                    // Neither used to ride the object, so the honest answer was no. Both ride it
                    // now, so the honest answer is "let the gate decide" — and the refusal that
                    // stood here was never a policy, it was the absence of a check. Genesis
                    // registrations do not reach this path at all; they are checked by
                    // `verify_palw_genesis_v2` against the catalog the ruleset id commits to.
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err(format!("class {class_id} registered on a network with no V2 bundle"));
                    };
                    let Some(carriage) = admission.as_ref() else {
                        return Err(format!(
                            "class {class_id} carries no shape profile, so coverage, ladder depth, court cost and pwu                              cannot be checked (ADR-0049 Decision H)"
                        ));
                    };
                    // Decision H's share rule: an entrant joins at the MINIMUM grantable share and
                    // no more. A registrant that could name its own permille would be donating
                    // itself an arbitrary slice of every incumbent's cadence, which is the share
                    // table's own conservation rule read backwards.
                    // **The entrant's difficulty is the chain's, not the registrant's** (audit
                    // M2-12). `initial_target` seeds the class's own retarget, so a registrant
                    // naming a huge one mines its class for free until the first retarget catches
                    // up — and naming a tiny one makes the class unminable, which is a way to park
                    // a share nobody can use. The base class's live target is what a registration
                    // is offered (`palw_v2_registration_terms`), and it is what must arrive.
                    if let Some(base_target) = state.class_target(&bundle.base_class_id)
                        && *initial_target != base_target.target
                    {
                        return Err(format!(
                            "class {class_id} registers at target {initial_target}; a post-genesis entrant starts at the                              chain's own ({}) — difficulty is not a registrant's to choose",
                            base_target.target
                        ));
                    }
                    // **An entrant joins at the minimum grantable share — or at NOTHING, when no
                    // certified family can prosecute it** (ADR-0069 Decisions 5 and 6).
                    //
                    // Decision H's rule was `share == min_grantable`, full stop, and
                    // `min_grantable` is never zero (`⌈10⁶/(tol·E)⌉.max(1)`). ADR-0069 then refused
                    // a nonzero grant to a family no drill has certified. Together those two closed
                    // the door completely: measured on this build, the adjudicator catalogs 44
                    // kernels and the certified families cover 37, so a class reaching one of the
                    // other 7 was statically adjudicable, refused weight by the gate, and refused
                    // registration by this line — it could not join at ALL.
                    //
                    // That is the opposite of what ADR-0069 decided. Weight is what certification
                    // buys; EXISTENCE is not, and Decision 5 made a zero grant expressible in
                    // `granted_share_table_v2` precisely so an uncertified family could register,
                    // produce, gossip and count for liveness while its adjudication was built. This
                    // path never let a zero through, so that state was reachable only at genesis —
                    // which is how the model tiers sat at 0‰ while nobody else could.
                    //
                    // The share is still exactly one value, so a registrant cannot choose it — the
                    // thing Decision H protects. What changed is that the value is a function of
                    // the class's own graph rather than a constant.
                    //
                    // **The certified set is the NETWORK's, not this process's.** The drilled
                    // registry is filled by a crate consensus links only as a dev dependency
                    // (ADR-0042 Decision 4: "a node's consensus never links a model runtime"), so
                    // reading it here would make this rule correct on a producing node and wrong on
                    // a validating one — two nodes disagreeing about a block, which is the failure
                    // `court_e2e_root` exists to prevent. `palw_rc_certified_families_v1` derives
                    // the same set from profiles every node already has, and hashes to that root.
                    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
                    // ADR-0075 Decision 4: the chain's own certified families count exactly as
                    // the genesis ones — an entrant whose family a `FamilyCertified` object
                    // certified joins at the floor, not weightless.
                    let chain_certified =
                        state.chain_certified_families(kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt);
                    let prosecutable = kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
                        bundle.court_e2e_root,
                        &certified,
                        &chain_certified,
                        &kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&carriage.profile),
                    )
                    .map_err(|e| format!("class {class_id}: {e}"))?
                    .is_some();
                    let floor = state_params.min_grantable_share_permille();
                    // **ADR-0145 §7 and I4, past `palw_admission_independence`: registration buys
                    // existence, and cadence is earned.** The minimum grantable share is not a
                    // small number — it is a permille taken from every incumbent by donation, and
                    // it moves their weight, their budget and (through
                    // `attempt_target_seed_v1(share, pwu)`) their difficulty, for a class nobody
                    // has yet verified. Past the fence every entrant joins weightless, exactly as
                    // an uncertified one already does, and earns its share from the registry once
                    // it has passed an admission its own registrant cannot grant it.
                    //
                    // This gate and the transition must agree or no prosecutable class could
                    // register at all: the fold refuses a bought registration carrying any share
                    // (`RegistrationTakesNoShare`), so a `required` of `floor` here would be a rule
                    // demanding exactly what the next layer rejects.
                    let independence = self.palw_admission_independence_at(point.daa_score);
                    let required = if prosecutable && !independence { floor } else { 0 };
                    if *share_permille != required {
                        return Err(if prosecutable && !independence {
                            format!(
                                "class {class_id} registers at {share_permille}‰; a post-genesis entrant joins at the \
                                 minimum grantable share ({floor}‰) — ADR-0049 Decision H"
                            )
                        } else if independence {
                            format!(
                                "class {class_id} registers at {share_permille}‰; past palw_admission_independence a \
                                 registration buys existence and not cadence, so every entrant joins at 0‰ and earns its \
                                 share from an admission it has passed — ADR-0145 §7"
                            )
                        } else {
                            format!(
                                "class {class_id} registers at {share_permille}‰; no end-to-end certified family covers \
                                 the kernels it reaches, so it joins WEIGHTLESS (0‰) and earns cadence once some build \
                                 certifies a backend for it — ADR-0069 Decision 6"
                            )
                        });
                    }
                    // **And WHO is registering it** (launch blockers §3). Everything above is a
                    // fact about the graph and the share; none of it is a fact about the sender.
                    // A registration takes a permille from EVERY incumbent through
                    // largest-remainder donation, and the share's own doc says "whoever may
                    // register a class may fund it, and nobody else may move a permille" — there
                    // was no `whoever`. Any stranger could move the cadence table for a fee.
                    //
                    // The registrant must hold an ACTIVE bond and have signed the class it is
                    // registering together with the share it is taking. Not a permission system:
                    // the smallest answer to "who", denominated in the collateral every other
                    // authority on this chain is denominated in.
                    let registrant = state
                        .bond(&carriage.registrant_bond)
                        .ok_or_else(|| format!("class {class_id} is registered under a bond this chain does not have"))?;
                    if !matches!(registrant.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                        return Err(format!("class {class_id} is registered under a bond that is not Active"));
                    }
                    let message = kaspa_consensus_core::palw_state_v2::palw_class_registration_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        *class_id,
                        *share_permille,
                        *activation_daa,
                        &carriage.registrant_bond,
                        // The four fields the signature used to leave open, and the canonical job
                        // the pwu rule is derived from (audit M2-6).
                        *artifact_root,
                        *slash_value_per_pwu,
                        *initial_target,
                        pwu_rule,
                        &carriage.canonical,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &registrant.pubkey,
                        message.as_byte_slice(),
                        &carriage.signature,
                        kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("class {class_id}'s registration is not signed by the bond it names"));
                    }
                    // **The certified family set, from this build** (ADR-0069 Decision 5). The
                    // gate reads it only for a registration asking a nonzero share, and it refuses
                    // a set that does not hash to `bundle.court_e2e_root` — so a node whose court
                    // can play a different set of families than the network agreed to is stopped
                    // here rather than quietly granting or refusing weight on its own authority.
                    // The set is filled by the node's boot drill; the boot gate is what makes sure
                    // it agrees with the network's pin before a block is ever validated.
                    // **The gate is called with the fences this block runs under** (audit D H-3).
                    //
                    // This called `verify_class_admission_v3` — six arguments, `ladder: None` and
                    // `court: None` — which is the gate as it stood before ADR-0077 Phase B and
                    // ADR-0082 existed. Two consequences, on the ONLY permissionless registration
                    // path there is: every graph-v5 profile was refused
                    // `FusedAttentionNeedsTheKaryCourt` even on a chain whose `palw_kary_court` is
                    // armed (ADR-0082's own U-08 route, closed), and the close-chunk, ladder and
                    // `CourtWindowTooShort` bounds were never evaluated against the armed court at
                    // all. Every one of the three reads a fence, and a fence resolved anywhere but
                    // at the ONE accessor is a rule two nodes can answer differently — so all
                    // three come from the accessors above, at this block's own DAA score.
                    //
                    // `decode_rules` is `false` and not a fence read: ADR-0082 Decision 10's
                    // numerator is not in the transition, and `Params::validate_palw_v2` refuses to
                    // start a node that armed `palw_fp_decode_rules` (audit D M-1), so there is no
                    // configuration in which this may be anything else.
                    let court = match self.palw_kary_court_active_at(point.daa_score) {
                        false => None,
                        true => {
                            let derived = self
                                .palw_court_params_at(point.daa_score)
                                .ok_or_else(|| format!("class {class_id} is registered on a chain with no V2 ruleset"))?
                                .map_err(|e| format!("class {class_id} is judged under a court with no shape: {e}"))?;
                            Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                                dissection_arity: derived.dissection_arity(),
                                prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                                window_court_daa: bundle.state.window_court(),
                            })
                        }
                    };
                    let ladder = self.palw_context_ladder_at(point.daa_score).then(|| {
                        kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
                            &carriage.profile,
                            court,
                            bundle.court.max_step_leaf_count(),
                        )
                    });
                    kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v9(
                        bundle,
                        &carriage.profile,
                        &carriage.canonical,
                        object,
                        // The same committed set the share rule above read — see its note on why
                        // consensus must not read the drilled registry — and the chain's own.
                        &certified,
                        &chain_certified,
                        ladder.flatten(),
                        court,
                        false,
                        // ADR-0102: the per-token lift kernel is admitted by its fence alone.
                        self.palw_token_lift_at(point.daa_score),
                        // ADR-0093 Decision 6: past its fence, a fused tile must be one head's.
                        self.palw_fused_dissectable_at(point.daa_score),
                        // **F4, on the economic bundle's fence.** Past it the ladder is compared
                        // against the deepest job the class can LEGALLY run rather than against a
                        // count of one decode call — so a class is refused at REGISTRATION instead
                        // of being admitted and then refused, silently, when it runs its own jobs.
                        // Resolved at the block, unlike the claim-borne half of the same fence: a
                        // registration is decided once and never re-derived.
                        self.palw_canonical_work_daa.is_some_and(|height| point.daa_score >= height),
                        // ADR-0103: the held map is admitted by its fence alone, and a held class's
                        // walls are read under the regime — the PanelDa fence says whether its
                        // widest job commits with no ids.
                        kaspa_consensus_core::palw_class_admission_v2::PalwHeldAdmissionV1 {
                            armed: self.palw_held_context_at(point.daa_score),
                            panel_da: self.palw_panel_da_at(point.daa_score),
                        },
                        self.palw_kimi_k3_at(point.daa_score),
                        // 2026-09-23 audit C-4: past its fence a non-fused class's priced geometry
                        // must fit the query row its graph reads.
                        self.palw_audit_2026_09_23_at(point.daa_score),
                    )
                    .map_err(|e| format!("class {class_id} is not admissible: {e}"))?;
                }
                // **The receipt quorum, verified where the design always said it was** (audit
                // M-01). `PalwConsensusObjectV2::ReceiptLicensed`'s own doc said it carried "the
                // receipt set the acceptance layer validated" — and this match had no arm for it,
                // so nothing ever did. `ProducerDefaulted { receipts: [] }` from any stranger
                // reached `slash_silent_seats` (every seat charged `claim.reserved`) and
                // `void_and_slash` (the producer's bond debited), on a transaction that carried no
                // signature at all. `validate_receipt_quorum_v2` was written, tested, and had no
                // caller anywhere in the tree.
                //
                // The quorum's DIRECTION is checked against the object, not just its existence: a
                // `Licensed` quorum cannot default a producer and an `Unavailable` quorum cannot
                // license one. The majority invariant (`2·quorum > seat_count`) is what makes the
                // two provably disjoint, so exactly one of these objects can ever be acceptable
                // for a given receipt set.
                Obj::ReceiptLicensed { claim, receipts } | Obj::ProducerDefaulted { claim, receipts } => {
                    let quorum = kaspa_consensus_core::palw_panel_v2::validate_receipt_quorum_v2_with_economy(
                        state,
                        panel_params,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        receipts,
                        Self::verify_mldsa87_with_context_bool,
                        self.palw_unavailable_abstains_at(point.daa_score),
                        // ADR-0124 Decision 2: the supplementary door, at the carrying block's DAA.
                        self.palw_panel_economy_active_at(point.daa_score),
                        // ADR-0147: a bought class's licence carries its outsider's `Valid`.
                        self.palw_admission_independence_daa(),
                    )
                    .map_err(|e| format!("claim {claim}'s receipt set does not carry a quorum: {e}"))?;
                    use kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2 as Q;
                    match (object, quorum) {
                        (Obj::ReceiptLicensed { .. }, Q::Licensed { .. })
                        | (Obj::ProducerDefaulted { .. }, Q::ProducerUnavailable { .. }) => {}
                        // ADR-0124 Decision 2: a supplementary set rides the licensing object's kind
                        // and credits seats on a claim already licensed; it can default nobody.
                        (Obj::ReceiptLicensed { .. }, Q::Supplementary { .. }) => {}
                        (Obj::ProducerDefaulted { .. }, Q::Supplementary { .. }) => {
                            return Err(format!("claim {claim} is already licensed; a supplementary receipt set defaults nobody"));
                        }
                        (Obj::ReceiptLicensed { .. }, Q::ProducerUnavailable { .. }) => {
                            return Err(format!("claim {claim} is licensed by a quorum that says the producer withheld"));
                        }
                        (Obj::ProducerDefaulted { .. }, Q::Licensed { .. }) => {
                            return Err(format!("claim {claim} is defaulted by a quorum that verified its trace"));
                        }
                        _ => unreachable!("the outer arm matched exactly these two object kinds"),
                    }
                }
                // **Everything below carries no authorization this layer can check, and is
                // therefore refused on the transaction path** (audit M-01).
                //
                // Each of these folds into consensus state and each was reachable by any stranger
                // for one ordinary transaction fee:
                //
                // * `BondRetireRequested { bond }` names a PUBLIC premine outpoint and flips that
                //   bond to `Retiring`. There is no inverse and no owner binding, so one
                //   transaction permanently stops any producer — including, on the RC, the only
                //   one. Re-admitting it needs an owner signature over the bond key.
                // * `ClassFrozen` carries a contradiction certificate whose signatures
                //   `check_class_contradiction_shape_v2` explicitly defers to "the acceptance
                //   layer" — which had no arm for it. `adjudicate_class_contradiction_v1`, the
                //   version that takes a verifier, is wired only into the other band. A forged
                //   certificate freezes a class permanently (there is deliberately no
                //   `ClassUnfrozen`); inert today only because the liveness floor is exempt, and
                //   armed the moment a second class exists.
                // * `PanelBound` is refused here, for the reason the ride list states.
                //   `BondRegistered` is NOT: the ride list admits it once its carrier locks the
                //   collateral it declares, and the arm below checks the one thing the carrier
                //   cannot — that the registrant holds the key it is registering.
                //
                // Refusing is not the final answer for either — a bond must eventually be able to
                // retire and an emergency freeze must eventually be pullable — but a door that
                // cannot be authenticated is better shut than open, and `palw_lifecycle_objects_v2`
                // refuses them at admission for the same reason. This arm is the second lock.
                // **A retirement is authorised by the bond's own key, and by nothing else.**
                //
                // This used to be a blanket refusal, which was correct about the danger and wrong
                // as a resting state: retirement is the only writer of `Retiring`, an `Active`
                // bond's collateral is unconditionally locked, and the C-08 burn is collected only
                // from a bond the lock has released. Refusing it forever means the collateral can
                // never be withdrawn and the slashed sompi never actually burns.
                //
                // So the door opens with the lock the refusal described: the same registrant-bond
                // signature check `ClassRegistered` performs a few arms above, over
                // `palw_bond_retirement_message_v2`, verified against the pubkey the bond itself
                // registered. A bond key is a public premine outpoint; the signature is what makes
                // naming one different from owning it.
                Obj::BondRetireRequested { bond, signature } => {
                    let record =
                        state.bond(bond).ok_or_else(|| format!("a retirement names bond {bond:?} this chain does not have"))?;
                    if matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { .. }) {
                        return Err(format!("bond {bond:?} is already retiring"));
                    }
                    let message = kaspa_consensus_core::palw_state_v2::palw_bond_retirement_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        bond,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s retirement is not signed by the key it registered"));
                    }
                }
                Obj::ClassFrozen { .. } => {
                    return Err("a class freeze carries no verified contradiction; the object is refused".to_string());
                }
                // Objects the ride list already refuses, and objects with nothing to check here.
                // **This match is EXHAUSTIVE on purpose**: the `_ => {}` it replaces is what let
                // four money-moving object kinds through in silence, and an exhaustive match makes
                // adding a fifth a decision somebody has to write down.
                // **A registration is authorised by the key it declares.**
                //
                // This arm was a silent pass-through, and the only thing refusing an
                // unauthenticated registration was the ride list — one lock where the comment
                // three arms up says there should be two. The carrier proves the collateral output
                // exists, holds what is claimed and pays to the declared payee
                // (`palw_bond_registration_binds_its_carrier_v2`, in the extractor, where the
                // transaction is). What it cannot prove is that the registrant holds the key it
                // names, because anyone can pay to somebody else's script — and a registry is a
                // list of who may be SEATED, so padding it with keys nobody controls is not a
                // harmless gift.
                Obj::BondRegistered { bond, pubkey, operator_pubkey, collateral, payout_payload, capable_classes, signature } => {
                    // **The form the registrant signed, not the one the chain now holds.** A
                    // carried registration names its output by index with a zero transaction id,
                    // because the carrier's id is a function of the payload the signature goes
                    // into; the extractor substituted the real id on the way in. Verifying against
                    // the substituted key would reject every honest registration.
                    let signed_bond = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_bond_registration_signed_key_v2(bond);
                    let message = kaspa_consensus_core::palw_state_v2::palw_bond_registration_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &signed_bond,
                        pubkey,
                        operator_pubkey,
                        *collateral,
                        payout_payload,
                        capable_classes,
                    );
                    // **Past the fence the registration carries TWO signatures, and the second
                    // proves the operator identity is held rather than declared** (audit
                    // 2026-09-19).
                    //
                    // `operator_id` is the unit of panel dedup and of the executor exclusion, and
                    // it is derived from `operator_pubkey` — which this message signs under the
                    // BOND key. That proves the registrant chose those bytes; it proves nothing
                    // about holding them. Undeclared, a registrant could name a victim's identity
                    // and thereby take that victim out of the jury of every claim he produces.
                    //
                    // An ML-DSA-87 signature is a fixed `MLDSA87_SIG_LEN`, so a registration past
                    // the fence is exactly two of them and the split is unambiguous. That is why
                    // this needs no new object and no new state: the field already exists, and the
                    // fence is what says how to read it.
                    let operator_proof = if self.palw_operator_id_unique_at(point.daa_score) {
                        let sig_len = kaspa_txscript::MLDSA87_SIG_LEN;
                        if signature.len() != 2 * sig_len {
                            return Err(format!(
                                "bond {bond:?}'s registration carries {} signature bytes; past the operator-possession fence it \
                                 carries two signatures ({} bytes): the bond's, then the operator key's",
                                signature.len(),
                                2 * sig_len
                            ));
                        }
                        Some(signature.split_at(sig_len))
                    } else {
                        None
                    };
                    let bond_sig = operator_proof.map(|(first, _)| first).unwrap_or(signature.as_slice());
                    if !Self::verify_mldsa87_with_context_bool(
                        pubkey,
                        message.as_byte_slice(),
                        bond_sig,
                        kaspa_consensus_core::palw_state_v2::PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s registration is not signed by the key it declares"));
                    }
                    if let Some((_, operator_sig)) = operator_proof {
                        let proof = kaspa_consensus_core::palw_state_v2::palw_operator_possession_message_v1(
                            kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                                self.network_id_bytes.as_slice(),
                                Some(self.genesis.hash),
                            ),
                            &signed_bond,
                            pubkey,
                            operator_pubkey,
                        );
                        if !Self::verify_mldsa87_with_context_bool(
                            operator_pubkey,
                            proof.as_byte_slice(),
                            operator_sig,
                            kaspa_consensus_core::palw_state_v2::PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT,
                        ) {
                            return Err(format!(
                                "bond {bond:?} declares an operator identity it does not prove it holds: the second signature does \
                                 not verify under the operator key"
                            ));
                        }
                    }
                }
                // **A capability declaration is authorised by the bond's own key, and by nothing
                // else** (ADR-0071 Decision 3) — the same lock retirement carries, for the same
                // reason: a bond key is a public outpoint, so naming one must differ from owning
                // it. Volunteering somebody else's collateral for duty is not a harmless gift when
                // the duty accounting convicts the seats the draw names.
                Obj::BondCapabilityDeclared { bond, capable_classes, signature } => {
                    let record = state
                        .bond(bond)
                        .ok_or_else(|| format!("a capability declaration names bond {bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_state_v2::palw_bond_capability_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        bond,
                        capable_classes,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_BOND_CAPABILITY_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s capability declaration is not signed by the key it registered"));
                    }
                }
                // **ADR-0062 SA-1: an accusation is authorised by the accuser's own bond key, and
                // by nothing else** — the same lock a retirement carries, for the same reason. A
                // bond key is a public outpoint, so naming one must differ from owning it:
                // unsigned, anyone could put an honest producer's claim under a session that
                // pauses its path to `Final`, under a stranger's identity.
                //
                // The FENCE is checked here as well as in the fold. The fold refusing it is what
                // makes it a rule; this refusing it is what stops a block from being folded at all
                // on a network where the rule is dormant.
                Obj::SeatReadinessProvedV2 { bond, class_id, span, proof, signature } => {
                    if !self.palw_model_registry_at(point.daa_score) {
                        return Err(format!("a possession proof for class {class_id} below the model registry's fence"));
                    }
                    if !self.palw_readiness_v2_at(point.daa_score) {
                        return Err(format!("a V2 possession proof for class {class_id} below readiness V2's fence (ADR-0133 §11.2)"));
                    }
                    let record =
                        state.bond(bond).ok_or_else(|| format!("a possession proof names bond {bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_model_registry_v1::palw_seat_readiness_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &borsh::to_vec(bond).expect("a bond key is borsh-serializable"),
                        class_id,
                        *span,
                        proof,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_model_registry_v1::PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s possession proof is not signed by the key it registered"));
                    }
                }
                Obj::SeatReadinessProved { bond, class_id, span, opening, signature } => {
                    // ADR-0135: below the fence the object does not exist; above it the seat's own
                    // key must sign the proof, or a relayer could volunteer another bond's collateral.
                    if !self.palw_model_registry_at(point.daa_score) {
                        return Err(format!("a readiness proof for class {class_id} below the model registry's fence"));
                    }
                    if self.palw_readiness_v2_at(point.daa_score) {
                        return Err(format!(
                            "class {class_id}: the one-leaf possession proof is superseded at this height (ADR-0133 §11.2)"
                        ));
                    }
                    let record =
                        state.bond(bond).ok_or_else(|| format!("a readiness proof names bond {bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_model_registry_v1::palw_seat_readiness_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &borsh::to_vec(bond).expect("a bond key is borsh-serializable"),
                        class_id,
                        *span,
                        opening.leaf_index,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_model_registry_v1::PALW_SEAT_READINESS_V1_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s readiness proof is not signed by the key it registered"));
                    }
                }
                Obj::ClassManifestV2 { class_id, artifact_bytes, registrant_bond, signature } => {
                    if !self.palw_model_registry_at(point.daa_score) {
                        return Err(format!("a manifest for class {class_id} below the model registry's fence"));
                    }
                    let record = state
                        .bond(registrant_bond)
                        .ok_or_else(|| format!("a manifest names bond {registrant_bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_model_registry_v1::palw_class_manifest_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &borsh::to_vec(registrant_bond).expect("a bond key is borsh-serializable"),
                        class_id,
                        *artifact_bytes,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_model_registry_v1::PALW_CLASS_MANIFEST_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {registrant_bond:?}'s manifest is not signed by the key it registered"));
                    }
                }
                Obj::ObjectiveOffence { kind, accused, evidence_id, evidence } => {
                    if !self.palw_objective_offence_at(point.daa_score) {
                        return Err("an objective offence is not armed on this network (ADR-0144 §9)".into());
                    }
                    let record = state
                        .bond(accused)
                        .ok_or_else(|| "an objective offence names a PALW bond this chain does not have".to_string())?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    kaspa_consensus_core::palw_offence_v1::palw_verify_objective_offence_v1(
                        *kind,
                        &accused.0,
                        evidence_id,
                        evidence,
                        &record.pubkey,
                        matches!(
                            record.status,
                            kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active
                                | kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { .. }
                        ),
                        domain.as_byte_slice(),
                        // **ADR-0084 U-08: the RULESET's step ladder, not the executor's.** A
                        // `PanelFalseValid` whose contradiction is a step refutation is adjudicated
                        // inside that call, and walking a shallower ladder than the class was
                        // admitted at refuses honest evidence — on a network carrying the held 2M row
                        // (step space ~2^37.6) against the default 2^22 that is EVERY structural
                        // refutation of that class, so a seat that voted Valid on a visible lie could
                        // not be convicted through this route at all.
                        self.palw_max_step_leaf_count_v1(),
                        |pk, msg, sig, ctx| Self::verify_mldsa87_with_context_bool(pk, msg, sig, ctx),
                    )
                    .map_err(|e| e.to_string())?;
                    if let kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValid = kind {
                        let payload: kaspa_consensus_core::palw_offence_v1::PalwPanelFalseValidEvidenceV1 =
                            borsh::from_slice(evidence).map_err(|_| "PanelFalseValid evidence does not decode".to_string())?;
                        let (execution_root, artifact_root, class_id) = if let Some(claim) = state.claim(&payload.claim_id) {
                            let artifact = state.class(&claim.class_id).map(|c| c.artifact_root).unwrap_or_default();
                            (claim.execution_root, artifact, claim.class_id)
                        } else if let Some(row) = state.panel_liability(&payload.claim_id) {
                            let artifact = state.class(&row.class_id).map(|c| c.artifact_root).unwrap_or_default();
                            (row.execution_root, artifact, row.class_id)
                        } else {
                            return Err("PanelFalseValid names neither a live claim nor a liability row".into());
                        };
                        let ladder = state.class_step_ladder_v1(&class_id, 64);
                        kaspa_consensus_core::palw_offence_v1::palw_panel_contradiction_convicts_execution_v1(
                            &payload.contradiction,
                            execution_root,
                            artifact_root,
                            ladder,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                Obj::ReceiptLicensedV2 { claim, receipts } => {
                    if !self.palw_verification_v2_at(point.daa_score) {
                        return Err(format!("claim {claim}: a segment-scoped receipt set below Verification V2's fence (ADR-0133)"));
                    }
                    let quorum = kaspa_consensus_core::palw_panel_v2::validate_receipt_coverage_v2(
                        state,
                        panel_params,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        receipts,
                        Self::verify_mldsa87_with_context_bool,
                        self.palw_unavailable_abstains_at(point.daa_score),
                        // ADR-0147: coverage by the class's own seats is still the class's own seats.
                        self.palw_admission_independence_daa(),
                    )
                    .map_err(|e| format!("claim {claim}'s segment receipts do not license: {e}"))?;
                    match quorum {
                        kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::Licensed { .. } => {}
                        kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::ProducerUnavailable { .. } => {
                            return Err(format!("claim {claim} is licensed by a quorum that says the producer withheld"));
                        }
                        kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::Supplementary { .. } => {
                            return Err(format!("claim {claim}: supplementary receipts ride the V1 object"));
                        }
                    }
                }
                Obj::OptimisticLicensed { claim, receipts } => {
                    if !self.palw_verification_s2_at(point.daa_score) {
                        return Err(format!("claim {claim}: an optimistic licence below Verification S2's fence (ADR-0133)"));
                    }
                    let Some(panel) = state.panel(claim) else {
                        return Err(format!("claim {claim}: an optimistic licence names a claim with no panel"));
                    };
                    let seats: Vec<_> = panel.seats.iter().map(|s| s.bond).collect();
                    kaspa_consensus_core::palw_optimistic_licence_v2::palw_optimistic_receipts_license_v2(
                        panel.anchor,
                        *claim,
                        &seats,
                        receipts,
                    )
                    .map_err(|e| format!("claim {claim}'s optimistic receipts do not license: {e}"))?;
                    let coverage = kaspa_consensus_core::palw_panel_v2::validate_receipt_coverage_v2(
                        state,
                        panel_params,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        receipts,
                        Self::verify_mldsa87_with_context_bool,
                        self.palw_unavailable_abstains_at(point.daa_score),
                        self.palw_admission_independence_daa(),
                    );
                    // Signatures and seats must still be real; the door does not require coverage.
                    match coverage {
                        Ok(_) | Err(kaspa_consensus_core::palw_panel_v2::PalwPanelV2Error::NoQuorum { .. }) => {}
                        Err(e) => return Err(format!("claim {claim}'s optimistic receipts do not verify: {e}")),
                    }
                }
                Obj::DefaultAccused { claim, missing_event_index, accuser, signature } => {
                    if !self.palw_da_court_at(point.daa_score) {
                        return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                    }
                    let record =
                        state.bond(accuser).ok_or_else(|| format!("an accusation names bond {accuser:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_state_v2::palw_da_accusation_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        *missing_event_index,
                        accuser,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("claim {claim}'s accusation is not signed by the bond it names"));
                    }
                }
                // **ADR-0099 Decision 5, built by ADR-0100: the one-move court's accusation.** Two
                // gates in the order every court move uses — the FENCE, then the accuser's bond
                // key over the session id — then the object's shape, the ruleset's own close
                // ceiling, the claim it names, and the verdict DERIVED here at the ladder the fold
                // will derive it at too (the extras carry that ladder to the fold). A fused site is
                // refused, because its verdict convicts nobody; an accusation whose refutation does
                // not adjudicate is refused too — P0-8's rule on both sides. The FENCE is checked
                // here as well as in the fold, for the DA court's reason.
                Obj::ShardCourtAccused { accusation } => {
                    let claim_id = accusation.claim;
                    if !self.palw_shard_court_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: the one-move court is not armed on this network (ADR-0099 Decision 5)"
                        ));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a one-move accusation on a network with no V2 court parameters".to_string())?;
                    let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                    let record = state
                        .bond(&accusation.accuser_bond)
                        .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let session_id =
                        kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_session_id_v1(domain.as_byte_slice(), accusation);
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        session_id.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_shard_court_v1::PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT,
                    ) {
                        return Err(format!("claim {claim_id}'s accusation is not signed by the bond it names"));
                    }
                    // ADR-0119 Decision 4: the shape is bounded at the CLAIM's ladder, so the claim
                    // is found first — a held class's recorded ladder, every other the network's.
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                    let ladder = state.class_step_ladder_v1(&claim.class_id, ladder);
                    accusation.validate_shape(ladder).map_err(|e| format!("claim {claim_id}: {e}"))?;
                    let bytes = kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_accusation_bytes_v1(accusation);
                    if bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim_id}: the accusation carries {bytes} bytes and this ruleset prices a close at {}",
                            court.max_close_bytes()
                        ));
                    }
                    if claim.bond != accusation.executor_bond
                        || claim.execution_root != accusation.execution_root
                        || claim.trace_root != accusation.trace_root
                    {
                        return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                    }
                    let class = state
                        .class(&claim.class_id)
                        .ok_or_else(|| format!("claim {claim_id} names class {} this chain does not have", claim.class_id))?;
                    match kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_verdict_v1(
                        accusation,
                        claim.class_id,
                        class.artifact_root,
                        ladder,
                    ) {
                        // ADR-0103 Decision 5: under the held regime the accusation at a fused leaf IS
                        // the challenge — the fold opens the dissection there. Dormant, refused.
                        Ok(kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection)
                            if !self.palw_held_context_at(point.daa_score) =>
                        {
                            return Err(format!(
                                "claim {claim_id}: leaf {} is a fused-attention site; its terminal is the dissection, not one move",
                                accusation.leaf_index
                            ));
                        }
                        Ok(_) => {}
                        Err(e) => return Err(format!("claim {claim_id}: the accusation does not adjudicate: {e}")),
                    }
                }
                // **ADR-0103 Decision 1: the checkpoint court.** The shard court's gates in its order —
                // the fence, the accuser's key over the session id, the ceiling, the claim — and the
                // verdict DERIVED here at the ladder the fold derives it at.
                Obj::CheckpointAccused { accusation } => {
                    let claim_id = accusation.claim;
                    if !self.palw_held_context_at(point.daa_score) {
                        return Err(format!("claim {claim_id}: the held regime is not armed on this network (ADR-0103)"));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a checkpoint accusation on a network with no V2 court parameters".to_string())?;
                    let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                    let record = state
                        .bond(&accusation.accuser_bond)
                        .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let session_id = kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_session_id_v1(
                        domain.as_byte_slice(),
                        accusation,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        session_id.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_checkpoint_court_v1::PALW_CHECKPOINT_COURT_MLDSA87_ACCUSE_CONTEXT,
                    ) {
                        return Err(format!("claim {claim_id}'s checkpoint accusation is not signed by the bond it names"));
                    }
                    let bytes = kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_accusation_bytes_v1(accusation);
                    if bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim_id}: the checkpoint accusation carries {bytes} bytes and this ruleset prices a close at {}",
                            court.max_close_bytes()
                        ));
                    }
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                    if claim.bond != accusation.executor_bond
                        || claim.execution_root != accusation.execution_root
                        || claim.trace_root != accusation.trace_root
                    {
                        return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                    }
                    kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_verdict_v1(
                        accusation,
                        claim.class_id,
                        // ADR-0119 Decision 4: the CLAIM's ladder.
                        state.class_step_ladder_v1(&claim.class_id, ladder),
                    )
                    .map_err(|e| format!("claim {claim_id}: the checkpoint accusation does not adjudicate: {e}"))?;
                }
                // **ADR-0103 Decision 4: the held DA court's two moves.** The fence, the DA court's
                // own fence, the signer (the accuser's key; the claim's producer for an answer), the
                // ceiling, and the unit bounded — or answered — against the claim's own roots.
                Obj::DefaultAccusedHeld { accusation } => {
                    let claim_id = accusation.claim;
                    if !self.palw_held_context_at(point.daa_score) || !self.palw_da_court_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: a held accusation needs the held regime and the data-availability court (ADR-0103)"
                        ));
                    }
                    let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                    let bytes = kaspa_consensus_core::palw_held_da_v1::palw_held_da_bytes_v1(accusation.as_ref());
                    if bytes > court.max_close_bytes() {
                        return Err(format!("claim {claim_id}: the held accusation carries {bytes} bytes, above the close ceiling"));
                    }
                    let record = state
                        .bond(&accusation.accuser)
                        .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let message =
                        kaspa_consensus_core::palw_held_da_v1::palw_held_da_accusation_message_v1(domain.as_byte_slice(), accusation);
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_held_da_v1::PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT,
                    ) {
                        return Err(format!("claim {claim_id}'s held accusation is not signed by the bond it names"));
                    }
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                    kaspa_consensus_core::palw_held_da_v1::palw_held_da_check_accusation_v1(
                        &claim.execution_root,
                        &accusation.missing,
                        &accusation.binding,
                        self.palw_prompt_ids_form_at(point.daa_score),
                    )
                    .map_err(|e| format!("claim {claim_id}: {e}"))?;
                    // **ADR-0111 Decision 3: a leaf is demanded only where the chain assigned it.**
                    // The demanding seat's own draw — keyed by the network domain, which is held here
                    // and not in the fold — must have put the leaf's interval in that seat's sample,
                    // so no seat can pick the leaf an executor must put on chain.
                    if let kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf } = accusation.missing {
                        let panel =
                            state.panel(&claim_id).ok_or_else(|| format!("claim {claim_id} has no bound panel to demand from"))?;
                        let seat_index = panel
                            .seats
                            .iter()
                            .position(|seat| seat.bond == accusation.accuser)
                            .ok_or_else(|| format!("claim {claim_id}: a leaf's evidence is demanded by a seat of its panel"))?;
                        kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_demand_is_the_seats_v1(
                            &domain,
                            &panel.anchor,
                            &claim_id,
                            seat_index as u8,
                            &accusation.binding,
                            leaf,
                        )
                        .map_err(|e| format!("claim {claim_id}: {e}"))?;
                    }
                }
                Obj::MaterialDisclosedHeld { disclosure } => {
                    let claim_id = disclosure.claim;
                    if !self.palw_held_context_at(point.daa_score) || !self.palw_da_court_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: a held disclosure needs the held regime and the data-availability court (ADR-0103)"
                        ));
                    }
                    let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                    let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                    let bytes = kaspa_consensus_core::palw_held_da_v1::palw_held_da_bytes_v1(disclosure.as_ref());
                    if bytes > court.max_close_bytes() {
                        return Err(format!("claim {claim_id}: the held disclosure carries {bytes} bytes, above the close ceiling"));
                    }
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("a disclosure names claim {claim_id} this chain does not have"))?;
                    let producer = state
                        .bond(&claim.bond)
                        .ok_or_else(|| format!("claim {claim_id}'s producing bond is not in this chain's registry"))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let message =
                        kaspa_consensus_core::palw_held_da_v1::palw_held_da_disclosure_message_v1(domain.as_byte_slice(), disclosure);
                    if !Self::verify_mldsa87_with_context_bool(
                        &producer.pubkey,
                        message.as_byte_slice(),
                        &disclosure.signature,
                        kaspa_consensus_core::palw_held_da_v1::PALW_HELD_DA_MLDSA87_DISCLOSE_CONTEXT,
                    ) {
                        return Err(format!("claim {claim_id}'s held disclosure is not signed by the bond that produced it"));
                    }
                    kaspa_consensus_core::palw_held_da_v1::palw_held_da_check_disclosure_v1(
                        &claim.execution_root,
                        &disclosure.missing,
                        &disclosure.binding,
                        &disclosure.disclosure,
                        // ADR-0119 Decision 4: the answer opens at the CLAIM's ladder, as the fold's.
                        state.class_step_ladder_v1(&claim.class_id, ladder),
                        self.palw_prompt_ids_form_at(point.daa_score),
                    )
                    .map_err(|e| format!("claim {claim_id}: {e}"))?;
                }
                // **ADR-0125 §7.3: a permit signed twice.** The lane open at this block, the named span
                // one the chain still keeps, its schedule granting the permit to the bond at the
                // span's width, both signatures under the bond's REGISTERED key (not a key the
                // evidence carries), and the permit not burned already.
                Obj::RoundPermitEquivocated { evidence } => {
                    use kaspa_consensus_core::palw_execution_lane_v1::{palw_execution_permit_of_v2, palw_execution_span_v1};
                    let lane = self
                        .palw_execution_lane_at(point.daa_score)
                        .ok_or_else(|| "round equivocation evidence where the execution lane is not open (ADR-0125)".to_string())?;
                    let span_daa = lane.schedule_span_daa_at(point.daa_score);
                    let span_now = palw_execution_span_v1(point.daa_score, span_daa);
                    if evidence.span > span_now || evidence.span + 1 < span_now {
                        return Err(format!(
                            "round equivocation names span {}, which the chain does not keep at span {span_now}",
                            evidence.span
                        ));
                    }
                    let schedule = state
                        .round_schedule(evidence.span)
                        .ok_or_else(|| format!("round equivocation names span {}, which has no schedule", evidence.span))?;
                    palw_execution_permit_of_v2(
                        schedule,
                        evidence.round,
                        lane.width_of_span_len(evidence.span, span_daa),
                        evidence.permit_index,
                        &evidence.bond,
                        self.palw_round_permits_are_tickets_at(point.daa_score),
                    )
                    .ok_or_else(|| {
                        format!(
                            "span {}'s schedule does not grant round {} permit {} to the bond the evidence names",
                            evidence.span, evidence.round, evidence.permit_index
                        )
                    })?;
                    let record = state
                        .bond(&evidence.bond)
                        .ok_or_else(|| "round equivocation names a bond this chain does not have".to_string())?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    evidence
                        .verify(domain, self.genesis.timestamp, &record.pubkey, Self::verify_mldsa87_with_context_bool)
                        .map_err(|e| format!("round {} permit {}: {e}", evidence.round, evidence.permit_index))?;
                    for span in [evidence.span.saturating_sub(1), evidence.span, evidence.span + 1] {
                        if state.round_equivocated(span, evidence.round, evidence.permit_index) {
                            return Err(format!("round {} permit {} was burned already", evidence.round, evidence.permit_index));
                        }
                    }
                }
                // **ADR-0100 Decision 4: per-shard licensing's three objects.** The fence first, the
                // same doubled-fence doctrine as every court move; then each object's authority:
                // the class's registrant over a plan, the bond over its shard list, and over a part
                // the seats' own receipts, checked exactly as a whole licence checks them, over
                // that shard's seats.
                Obj::ClassShardPlanDeclared { class_id, shard_count, signature } => {
                    if !self.palw_shard_licensing_at(point.daa_score) {
                        return Err(format!("class {class_id}: per-shard licensing is not armed on this network (ADR-0100)"));
                    }
                    let class = state
                        .class(class_id)
                        .ok_or_else(|| format!("a shard plan names class {class_id} this chain does not have"))?;
                    let registrant = class
                        .registrant_bond
                        .ok_or_else(|| format!("class {class_id} was registered at genesis and has no registrant"))?;
                    let record = state.bond(&registrant).ok_or_else(|| format!("class {class_id}'s registrant bond is gone"))?;
                    let message = kaspa_consensus_core::palw_shard_licensing_v1::palw_class_shard_plan_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        class_id,
                        *shard_count,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_shard_licensing_v1::PALW_SHARD_PLAN_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("class {class_id}'s shard plan is not signed by its registrant"));
                    }
                }
                Obj::BondShardsDeclared { bond, class_id, shard_count, shards, signature } => {
                    if !self.palw_shard_licensing_at(point.daa_score) {
                        return Err(format!("bond {bond:?}: per-shard licensing is not armed on this network (ADR-0100)"));
                    }
                    let record =
                        state.bond(bond).ok_or_else(|| format!("a shard list names bond {bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_shard_licensing_v1::palw_bond_shards_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        bond,
                        class_id,
                        *shard_count,
                        shards,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_shard_licensing_v1::PALW_BOND_SHARDS_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s shard list is not signed by the bond"));
                    }
                }
                Obj::ShardReceiptLicensed { part } => {
                    if !self.palw_shard_licensing_at(point.daa_score) {
                        return Err(format!("claim {}: per-shard licensing is not armed on this network (ADR-0100)", part.claim));
                    }
                    kaspa_consensus_core::palw_panel_v2::validate_shard_receipt_part_v1(
                        state,
                        panel_params,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        part,
                        Self::verify_mldsa87_with_context_bool,
                        self.palw_unavailable_abstains_at(point.daa_score),
                    )
                    .map_err(|e| format!("claim {} shard {}: the part does not carry a quorum: {e}", part.claim, part.shard_index))?;
                }
                // **ADR-0062 SA-2: the disclosure is signed by the CLAIM's bond, and bounded by the
                // ruleset's close ceiling before a single Merkle path is walked.**
                //
                // The signature is the producer's, not the carrier's — which is what makes SA-3's
                // permissionless carriage safe: anyone may carry it, and nobody may write it. The
                // ceiling is applied here for the reason ADR-0049 Decision C's costs are (audit
                // H-03): a bound checked only at class admission is a bound nothing enforces where
                // it is actually spendable.
                Obj::MaterialDisclosed { claim, event_index, disclosure, signature } => {
                    if !self.palw_da_court_at(point.daa_score) {
                        return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a data-availability disclosure on a network with no V2 court parameters".to_string())?;
                    let disclosure_bytes = borsh::to_vec(disclosure).map(|b| b.len() as u64).unwrap_or(u64::MAX);
                    if disclosure_bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim}'s disclosure is {disclosure_bytes} bytes, above this ruleset's {}-byte close ceiling",
                            court.max_close_bytes()
                        ));
                    }
                    let record =
                        state.claim(claim).ok_or_else(|| format!("a disclosure names claim {claim} this chain does not have"))?;
                    let producer = state
                        .bond(&record.bond)
                        .ok_or_else(|| format!("claim {claim}'s producing bond is not in this chain's registry"))?;
                    let message = kaspa_consensus_core::palw_state_v2::palw_da_disclosure_message_v3(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        *event_index,
                        &kaspa_consensus_core::palw_state_v2::palw_da_disclosure_digest_v1(disclosure),
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &producer.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_DA_DISCLOSURE_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("claim {claim}'s disclosure is not signed by the bond that produced it"));
                    }
                }
                Obj::FreePromptCommitted { .. } => {}
                // ADR-0075: both certification objects are judged entirely by the transition —
                // the evidence by the court's grader, the class binding by the class's own profile
                // hash and kernel coverage — and neither needs a signature, a bundle or a store.
                Obj::FamilyCertified { .. } | Obj::ClassLaneCertified { .. } | Obj::ObjectChunk { .. } => {}
                // **ADR-0078: a derivation is authorised by the key it declares, on this chain.**
                // The ride list proved a signature is present and the shape is the object's; here
                // the signature is verified under the declared executor key, over the object's own
                // message under its own context, and an object naming another network's domain is
                // refused before any state is read. Whether the declared key is the claim's bond
                // key is the transition's comparison (`DerivedSignerIsNotTheExecutor`).
                Obj::DerivedArtifactV1 { object, signature } => {
                    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    if object.network_domain != network_domain {
                        return Err(format!("the derivation of claim {} names another network's domain", object.claim_id));
                    }
                    let message = kaspa_consensus_core::palw_derived_v1::palw_derived_message_v1(object);
                    if !Self::verify_mldsa87_with_context_bool(
                        &object.executor_pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!(
                            "the derivation of claim {} is not signed by the executor key it declares",
                            object.claim_id
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// `verify_mldsa87_with_context` as a `bool`, which is the shape every PALW verifier callback
    /// takes — one place, so two call sites cannot disagree about what an error means.
    /// ADR-0088: the network domain every registry message is signed under — the same one a class
    /// registration is signed under.
    fn palw_network_domain_v2(&self) -> kaspa_hashes::Hash64 {
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(self.network_id_bytes.as_slice(), Some(self.genesis.hash))
    }

    /// **B-4: the pre_pow-inclusive execution commitment of `attempt` as carried by `header`'s
    /// block** — the key the fold dedups on past `palw_audit_2026_09_11_deep`. It is exactly the
    /// anchor the class lottery drew under (`palw_admission_v2` derives the same
    /// `execution_anchor_v3` from the header): `execution_commitment_v3` blanks the challenge and
    /// keys under `(network_domain, pre_pow, class, bond, nonce-bucket)`. Two blocks share it iff
    /// they share pre_pow (same parents + payload) and the challenge-blanked attempt — nonce-siblings
    /// in one bucket, the one inference re-announced; a genuinely different position (different
    /// pre_pow) has a different key and is not deduped (the reason the reverted B-4's position-free
    /// `execution_root` key over-refused).
    fn palw_execution_key_v1(
        &self,
        header: &kaspa_consensus_core::header::Header,
        attempt: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2,
    ) -> kaspa_hashes::Hash64 {
        let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(
            self.palw_network_domain_v2(),
            kaspa_consensus_core::hashing::header::pre_pow_hash_64(header),
            attempt.class_id,
            &attempt.executor_bond,
            header.nonce,
        );
        kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(attempt, anchor)
    }

    /// ADR-0088: the bond a line's `role` ("owner" or "developer") names, read from the acceptance
    /// state — a founding line without a row answers from its class.
    fn palw_model_line_role(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        line_id: &kaspa_hashes::Hash64,
        role: &str,
    ) -> Result<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2, String> {
        let line = state.model_line_or_founding(line_id).ok_or_else(|| format!("line {line_id} does not exist"))?;
        let bond = match role {
            "owner" => line.owner,
            _ => line.developer_bond(),
        };
        bond.ok_or_else(|| format!("line {line_id} has no {role}: nobody may act on it"))
    }

    /// ADR-0088: the signature of a registry object, checked against the stored key of the bond
    /// it is attributed to, under the registry's own ML-DSA-87 context.
    fn palw_model_check_bond_signature(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        message: &kaspa_hashes::Hash64,
        signature: &[u8],
        what: &str,
    ) -> Result<(), String> {
        let record = state.bond(bond).ok_or_else(|| format!("{what} is attributed to a bond this chain does not have"))?;
        if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err(format!("{what} is attributed to a bond that is not Active"));
        }
        if !Self::verify_mldsa87_with_context_bool(
            &record.pubkey,
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_model_lines_v1::PALW_MODEL_LINE_MLDSA87_CONTEXT,
        ) {
            return Err(format!("{what} carries a signature the attributed bond's key does not verify"));
        }
        Ok(())
    }

    fn verify_mldsa87_with_context_bool(key: &[u8], message: &[u8], sig: &[u8], context: &[u8]) -> bool {
        verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
    }

    /// The panel's anchor, derived from THIS candidate's chain (Decision 7's sortition input).
    ///
    /// The first chain block at or past `accepted_daa + anchor_delay`, with the predecessor's DAA
    /// as the witness that makes "first at or past" checkable — the same shape as the free-prompt
    /// beacon, for the same reason: a producer that supplies its own anchor picks its own jury.
    fn palw_v2_anchor_fact_of_candidate(
        &self,
        from: BlockHash,
        accepted_daa: u64,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2> {
        let slot = accepted_daa.checked_add(panel_params.anchor_delay())?;
        let mut candidate: Option<(BlockHash, u64)> = None;
        for block in self.reachability_service.default_backward_chain_iterator(from) {
            let header = self.headers_store.get_header(block).ok()?;
            let daa = header.daa_score;
            if daa >= slot {
                // **The anchor has to cost an inference to move.**
                //
                // This block's hash is one of the two randomness inputs the panel draw runs on, so
                // whoever can cheaply produce blocks at the anchor slot can re-roll the panel that
                // will judge them. Every lane was eligible, and the receipt lane is precisely the
                // one whose headers cost nothing to re-produce — a producer could mint algo-7
                // blocks until one landed at the slot with a hash whose draw it liked.
                //
                // Restricting the anchor to the attempt lane prices that grind at one full
                // inference per try, which is the same charge the job anchor makes for the same
                // reason. Skipping over a receipt block only delays the anchor to the next attempt
                // block, and the attempt lane is the main lane on a V2 network.
                if !kaspa_consensus_core::pow_layer0::algo_id_carries_no_chain_position(header.pow_algo_id) {
                    candidate = Some((block, daa));
                }
                continue;
            }
            // The first block BELOW the slot is the witness; the last one recorded at or above it
            // is the anchor.
            let (anchor_block, anchor_daa) = candidate?;
            return Some(kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2 { anchor_block, anchor_daa, predecessor_daa: daa });
        }
        let (anchor_block, anchor_daa) = candidate?;
        Some(kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2 { anchor_block, anchor_daa, predecessor_daa: 0 })
    }

    /// **ADR-0065 D4, resolved in exactly one place.** Every consumer — the receipt tally, the
    /// object-acceptance rehearsal, the assembler and the fold — must get the same answer for the
    /// same block, because a tally that says "no quorum" beside a fold that still charges the
    /// dissenting seat is two rules wearing one name.
    fn palw_unavailable_abstains_at(&self, daa_score: u64) -> bool {
        self.palw_unavailable_abstains.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0069 Decision 7, resolved in exactly one place**, and resolved at the BLOCK's own
    /// acceptance DAA rather than at the evaluating node's tip.
    ///
    /// Fork choice compares candidate chains, so a rule that read the reader's point of view would
    /// make one block worth different amounts to two nodes — the same defect
    /// `PalwClassFactsViewV1` exists to close for the class target one line over.
    fn palw_uncertified_weightless_at(&self, daa_score: u64) -> bool {
        self.palw_uncertified_weightless.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0084 U-08, resolved in exactly one place, at the BLOCK's own DAA.** The step ladder
    /// a court close is adjudicated at: the ruleset's past the fence, `2^22` before it. Read at
    /// the block's acceptance DAA and not the tip's for the reason the weight fences above give —
    /// two nodes grading one close proof must read one ladder.
    /// **ADR-0087 Decision 6, resolved in exactly one place, at the BLOCK's own DAA.**
    fn palw_model_market_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_market.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0088 Decision 11, resolved in exactly one place, at the BLOCK's own DAA.**
    pub(super) fn palw_model_lines_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_lines.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0095 §4.11 as corrected, resolved at the BLOCK's own DAA like every other fence.
    pub(super) fn palw_model_benefits_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_benefits.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0114, resolved at the BLOCK's own DAA like every other fence.
    pub(super) fn palw_model_leg_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_leg_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0120, resolved at the BLOCK's own DAA like every other fence.
    pub(super) fn palw_model_seed_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_seed_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0124 Decisions 1–5, resolved at one DAA — the BLOCK's for the fold and the receipt
    /// door, the claim's ANCHOR for the draw.
    pub(super) fn palw_panel_economy_active_at(&self, daa_score: u64) -> bool {
        self.palw_panel_economy.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0124 Decision 6, resolved at the BLOCK's own DAA like every other fence.
    pub(super) fn palw_work_priced_reward_active_at(&self, daa_score: u64) -> bool {
        self.palw_work_priced_reward.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0130: the seat exposure floor's reward multiple at one DAA, in permille — `0` where the
    /// floor is dormant.** The claim's ANCHOR for the draw (inside `palw_panel_draw_policy_at`), the
    /// BLOCK's for the fold's reservation (`palw_transition_extras_at`); a binding the chain derives
    /// at its anchor block reads one number at both.
    pub(super) fn palw_panel_reward_multiple_permille_at(&self, daa_score: u64) -> u32 {
        self.palw_panel_exposure_floor
            .filter(|floor| floor.activation.is_active(daa_score))
            .map_or(0, |floor| floor.reward_multiple_permille)
    }

    /// **ADR-0126 Decision 2: the overlay's reward split for the block at `daa_score` — the one
    /// reader.** The coinbase carve and `coinbase_validator_pool`, on the template path and the
    /// validation path, read it here, and so do the quality sub-pool and the audit fee through the
    /// pool they are cut from; so a template and the block it becomes cannot disagree about it
    /// (SA-3). `None` where no overlay runs or below its activation; past `palw_overlay_carve`, where
    /// the full split is in force, the validator share is the fence's and the worker base takes the
    /// remainder.
    pub(super) fn fee_split_at(&self, daa_score: u64) -> Option<kaspa_consensus_core::dns_finality::FeeSplitParams> {
        kaspa_consensus_core::config::params::palw_overlay_fee_split_at_v1(
            self.dns_params.as_ref()?,
            self.palw_overlay_carve,
            daa_score,
        )
    }

    /// **ADR-0126 Decision 3: the carve a claim escrows** for an attempt carried at
    /// `attempt_daa_score` and paid by the block at `paying_daa_score`, resolved at the lower of the
    /// two ([`kaspa_consensus_core::config::params::palw_overlay_escrow_carve_at_v1`]) so it never
    /// outgrows the worker base of the split its payer withholds it from. The fold's escrow and the
    /// coinbase's withhold read the same resolved value. `None` below the fence, where the bundle's
    /// own `worker_carve_permille` applies.
    pub(super) fn palw_escrow_carve_at(
        &self,
        attempt_daa_score: u64,
        paying_daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2> {
        kaspa_consensus_core::config::params::palw_overlay_escrow_carve_at_v1(
            self.palw_overlay_carve,
            attempt_daa_score,
            paying_daa_score,
        )
    }

    /// ADR-0125: the execution lane's shape where it is open at `daa_score`.
    pub(super) fn palw_execution_lane_at(&self, daa_score: u64) -> Option<kaspa_consensus_core::config::params::PalwExecutionLaneV1> {
        self.palw_execution_lane.filter(|lane| lane.activation.is_active(daa_score))
    }

    /// **ADR-0125: the round blocks of a mergeset** — its reds carrying the round lane's id where the
    /// lane is open at their own DAA score. Headers only, so the template (which holds no verdicts)
    /// and the validating walk compute the same set; nothing is read where the lane is not configured.
    pub(super) fn palw_round_blocks_of(&self, ghostdag_data: &GhostdagData) -> BlockHashSet {
        let Some(lane) = self.palw_execution_lane else {
            return BlockHashSet::default();
        };
        ghostdag_data
            .mergeset_reds
            .iter()
            .copied()
            .filter(|red| {
                self.headers_store.get_header(*red).is_ok_and(|header| {
                    header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1
                        && lane.activation.is_active(header.daa_score)
                })
            })
            .collect()
    }

    /// **ADR-0125: which round blocks of a merging block's mergeset hold their permit** — decided
    /// against the merging block's PARENT state, so the template (building on the tip) and every
    /// validating node (judging the block that template becomes) read one state and give one answer.
    ///
    /// A round block holds its permit when all of these hold, and otherwise it is merged and its
    /// transactions are not accepted:
    ///
    /// * its envelope decodes (the header stage already refused one that does not);
    /// * the span its ANCHOR (its selected parent) lies in is this block's span or the one before —
    ///   the two spans the fold keeps a schedule and a permit ledger for;
    /// * that span's schedule grants `(round, permit index)` to the envelope's bond at the lane's
    ///   width ([`kaspa_consensus_core::palw_execution_lane_v1::palw_execution_permit_of_v1`]);
    /// * the bond is registered, `Active`, and its key is the key the envelope was signed with;
    /// * the round block's coinbase names the bond's registered payout — its fees are paid there,
    ///   and nowhere a stranger's block could redirect them;
    /// * the permit has not been accepted on this chain before.
    ///
    /// `None` where the lane is not open at `daa_score`.
    pub(super) fn palw_round_verdicts_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        ghostdag_data: &GhostdagData,
        daa_score: u64,
    ) -> Option<super::utxo_validation::PalwRoundVerdictsV1> {
        use kaspa_consensus_core::palw_execution_lane_v1::{
            PalwExecEnvelopeV1, PalwExecPermitUseV1, palw_execution_permit_of_v2, palw_execution_span_v1,
        };
        let lane = self.palw_execution_lane_at(daa_score)?;
        // Route-matrix #2: past ADR-0151's bundle with the quanta armed, a permit is a ticket.
        let tickets_only = self.palw_round_permits_are_tickets_at(daa_score);
        let span_daa = lane.schedule_span_daa_at(daa_score);
        let span_now = palw_execution_span_v1(daa_score, span_daa);
        let mut verdicts = super::utxo_validation::PalwRoundVerdictsV1 {
            round_blocks: self.palw_round_blocks_of(ghostdag_data),
            ..Default::default()
        };
        let mut ordered: Vec<BlockHash> = verdicts.round_blocks.iter().copied().collect();
        ordered.sort();
        for block in ordered {
            let permitted = (|| -> Option<PalwExecPermitUseV1> {
                let header = self.headers_store.get_header(block).ok()?;
                let envelope = PalwExecEnvelopeV1::decode(&header.palw_commitment).ok()?;
                let anchor = self.ghostdag_store.get_selected_parent(block).ok()?;
                let span = palw_execution_span_v1(self.headers_store.get_daa_score(anchor).ok()?, span_daa);
                if span > span_now || span + 1 < span_now {
                    return None;
                }
                // §7.3: a permit proven signed twice is granted to no block.
                if state.round_equivocated(span, envelope.round, envelope.permit_index) {
                    return None;
                }
                let schedule = state.round_schedule(span)?;
                // §7.2: the width of the anchor's span — the width the schedule was drawn at.
                palw_execution_permit_of_v2(
                    schedule,
                    envelope.round,
                    lane.width_of_span_len(span, span_daa),
                    envelope.permit_index,
                    &envelope.bond,
                    tickets_only,
                )?;
                let bond = state.bond(&envelope.bond)?;
                if !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
                    || bond.pubkey != envelope.pubkey
                {
                    return None;
                }
                let transactions = self.block_transactions_store.get(block).ok()?;
                let coinbase = self.coinbase_manager.deserialize_coinbase_payload(&transactions.first()?.payload).ok()?;
                if coinbase.miner_data.script_public_key
                    != kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&bond.payout_payload.as_bytes())
                {
                    return None;
                }
                if state.round_permit_used(span, envelope.round, envelope.permit_index) {
                    return None;
                }
                Some(PalwExecPermitUseV1 { span, round: envelope.round, permit_index: envelope.permit_index })
            })();
            if let Some(used) = permitted {
                verdicts.permitted.insert(block);
                verdicts.uses.push(used);
            }
        }
        verdicts.uses.sort();
        Some(verdicts)
    }

    /// **The bind's Valid-lock question, for the draw** (the 2026-09-23 route-matrix audit's #3):
    /// what one `Valid` signature on `claim` must lock and the clocks a bond's free collateral is
    /// read at, from the BINDING block's fold inputs (`extras`, `now_daa`) — the ones the fold's
    /// `require_panel_lock_eligible` reads, so the draw never seats a bond the bind would refuse.
    /// `None` below `palw_audit_2026_09_23` or where the lock ledger is not armed: testnet-11 arms
    /// the ledger without the fence, and its derived panels must stay the ones it always drew.
    pub(super) fn palw_panel_valid_lock_of_v1(
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        extras: &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1,
        claim: &kaspa_consensus_core::palw_state_v2::PalwClaimStateV2,
        now_daa: u64,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwPanelValidLockV1> {
        if !(extras.audit_2026_09_23_active && extras.objective_offence_at(now_daa)) {
            return None;
        }
        Some(kaspa_consensus_core::palw_panel_v2::PalwPanelValidLockV1 {
            required: kaspa_consensus_core::palw_state_v2::palw_panel_valid_lock_required_v1(state, state_params, extras, claim),
            now_daa,
            settled_anchor_depth: kaspa_consensus_core::palw_state_v2::palw_settled_anchor_depth_v1(extras),
        })
    }

    /// **ADR-0124's draw policy at a claim's anchor** — the deep fence's weighting and the panel
    /// economy's floor and ceiling, resolved at ONE point so the assembler and the acceptance layer
    /// recompute one identical panel. `economy` is `None` while the fence is dormant at the anchor.
    pub(super) fn palw_panel_draw_policy_at(&self, anchor_daa: u64) -> kaspa_consensus_core::palw_panel_v2::PalwPanelDrawPolicyV1 {
        let economy = if self.palw_panel_economy_active_at(anchor_daa) {
            self.palw_state_params_v2.as_ref().map(|state| kaspa_consensus_core::palw_panel_economy_v1::PalwSeatEconomyV1 {
                panel_floor_sompi: kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_v1(
                    state.min_collateral_sompi(),
                ),
                max_exposure_ratio_permille: state.fp_max_exposure_ratio_permille(),
                // ADR-0130: the floor at the same anchor, so the headroom the draw checks is the
                // reservation the seat will hold.
                reward_multiple_permille: self.palw_panel_reward_multiple_permille_at(anchor_daa),
            })
        } else {
            None
        };
        kaspa_consensus_core::palw_panel_v2::PalwPanelDrawPolicyV1 {
            weighted: self.palw_audit_2026_09_11_deep_at(anchor_daa),
            economy,
            readiness: self.palw_readiness_policy_at(anchor_daa),
            // ADR-0147: the fence's height, the floor's id and THIS anchor's DAA — the draw applies
            // it to a claim accepted at or past the height, whatever the anchor's own DAA is, and
            // cuts the population at the anchor it was handed.
            independence: match (self.palw_admission_independence_daa(), self.palw_state_params_v2.as_ref()) {
                (Some(from_daa), Some(state)) => Some(kaspa_consensus_core::palw_panel_v2::PalwPanelIndependenceV1 {
                    from_daa,
                    base_class_id: state.base_class_id(),
                    anchor_daa,
                }),
                _ => None,
            },
            // Per claim, at the binding block: `palw_panel_valid_lock_of_v1` fills it where armed.
            valid_lock: None,
        }
    }

    /// **ADR-0089 Decision 9, resolved in exactly one place, at the BLOCK's own DAA.**
    pub(super) fn palw_model_evm_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_evm.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0089: the three fences the executor reads, at one DAA.
    #[cfg(feature = "evm")]
    pub(super) fn palw_evm_market_fences_at(&self, daa_score: u64) -> kaspa_consensus_core::evm::model_market::PalwEvmMarketFencesV1 {
        kaspa_consensus_core::evm::model_market::PalwEvmMarketFencesV1 {
            market_active: self.palw_model_market_active_at(daa_score),
            lines_active: self.palw_model_lines_active_at(daa_score),
            evm_active: self.palw_model_evm_active_at(daa_score),
            leg_v2_active: self.palw_model_leg_v2_active_at(daa_score),
            seed_v2_active: self.palw_model_seed_v2_active_at(daa_score),
        }
    }

    /// The extras every production fold and every acceptance rehearsal carry (ADR-0088 D11,
    /// ADR-0089 D9). The action list is the EVM step's and is added by the caller that has it.
    /// The fold's extras for the block `point` names — every fence resolved at its DAA score, and
    /// (ADR-0132 Upgrade C) its own `bits`, read from its header where the store holds one.
    fn palw_transition_extras_for(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
        let daa_score = point.daa_score;
        kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
            model_lines_active: self.palw_model_lines_active_at(daa_score),
            model_benefits_active: self.palw_model_benefits_active_at(daa_score),
            evm_market_active: self.palw_model_evm_active_at(daa_score),
            // Written explicitly like the fences below it: this one decides what every move pays.
            model_leg_v2_active: self.palw_model_leg_v2_active_at(daa_score),
            // And this one decides which payment opens a market (ADR-0120).
            model_seed_v2_active: self.palw_model_seed_v2_active_at(daa_score),
            // Written explicitly, never left to `..Default::default()`: an unwritten default inside
            // a struct the fold reads is the one fault no golden can see, and this field decides
            // whether a producer's collateral is destroyed.
            court_responder_coverage_active: self.palw_court_responder_coverage_at(daa_score),
            // Written explicitly for the same reason as the line above: this one decides how long a
            // producer owes its retained trace, and an unwritten default would silently give every
            // claim the producer's own number back.
            fp_da_pins_active: self.palw_fp_da_pins_at(daa_score),
            // Written explicitly for the same reason again: this one decides whether a boundary
            // grows a class's share on accepted blocks or on Final work (ADR-0107).
            share_growth_final_active: self.palw_share_growth_final_at(daa_score),
            // ADR-0123: the fold re-runs admission on every merged attempt, so it must release what
            // admission released for this block. Explicit for the reason the lines above give.
            epoch_budget_release_active: self.palw_epoch_budget_release_at(daa_score),
            // ADR-0124: these two decide who a `Final` claim pays and what a seat reserves and can
            // lose. Explicit for the reason every line above gives.
            panel_economy_active: self.palw_panel_economy_active_at(daa_score),
            work_priced_reward_active: self.palw_work_priced_reward_active_at(daa_score),
            // ADR-0130: what a seat put on duty in this block reserves — the duty row stores it, so
            // this is read once per binding and never at release. Explicit for the same reason.
            panel_reward_multiple_permille: self.palw_panel_reward_multiple_permille_at(daa_score),
            // ADR-0125: the lane's span, where it is open. The permits a block accepted are the
            // chain walk's to add — it is the only caller holding the verdicts.
            round_lane: self.palw_execution_lane_at(daa_score).map(|lane| {
                kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1 {
                    schedule_span_daa: lane.schedule_span_daa_at(daa_score),
                    execution_quantum: if self.palw_execution_quanta_at(daa_score) {
                        kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXECUTION_QUANTUM_V1
                    } else {
                        0
                    },
                    span_open_round: self
                        .headers_store
                        .get_timestamp(point.block)
                        .ok()
                        .map(|ts| {
                            kaspa_consensus_core::palw_execution_lane_v1::palw_execution_round_v1(ts, self.genesis.timestamp)
                        })
                        .unwrap_or(0),
                }
            }),
            // **ADR-0151's economic-safety fold.** The two values the pricing needs that the
            // transition cannot see: the cadence (a `Params` quantity — the transition holds only
            // `PalwStateParamsV2`) and the declared worth of one stolen round. `None` where the
            // fence is dormant, and then every path it touches is byte-identical.
            economic_safety: self.palw_economic_safety_at(daa_score).then(|| {
                kaspa_consensus_core::palw_state_v2::PalwEconomicSafetyFoldV1 {
                    target_time_per_block_ms: self.palw_target_time_per_block_ms,
                    permit_value_sompi: kaspa_consensus_core::palw_economic_safety_v1::PALW_T12_PERMIT_FEE_CEILING_SOMPI,
                }
            }),
            round_permit_uses: Vec::new(),
            // ADR-0126 Decision 3: the carve this block's own attempt escrows at — the block that
            // carried it is this one, and the block that pays it is its selected-chain child, always
            // later, so the lower score is this one's. Explicit for the reason every line above
            // gives: it decides how much of the subsidy a claim holds.
            escrow_carve: self.palw_escrow_carve_at(daa_score, daa_score),
            model_registry: self.palw_model_registry_fold_at(daa_score),
            // ADR-0132 Upgrade C: the rate, the shares, the ceiling and this block's `bits`. The
            // same reading at every site that folds this block, because it is derived from the
            // point and the header store alone.
            economic_payout: self.palw_economic_payout_fold_for(point),
            // ADR-0137 (shadow): the work target's fold input — the rate, the clamp and every
            // class's work — on every ConsensusV2 network, so every node prints the same `W`.
            work_target: self.palw_work_target_fold_for(point),
            work_target_active: self.palw_work_target_at(daa_score),
            artifact_root_ownership_active: self.palw_artifact_root_ownership_at(daa_score),
            operator_id_unique_active: self.palw_operator_id_unique_at(daa_score),
            // **ADR-0145: the HEIGHT, not this block's answer.** Every other fence here is resolved
            // at `daa_score` because it decides something about THIS block. This one decides the
            // unit a CLAIM's work is carried in, and a claim is priced at the block that finalizes
            // it, re-priced at the block that retires it, and re-derived by every consistency check
            // in between — so each of those sites compares the height against the claim's own
            // `accepted_daa` instead. Resolving it here would price one claim under two bases and
            // the node would refuse its own tip. `None` on every shipped preset.
            canonical_work_daa: self.palw_canonical_work_daa,
            // **ADR-0147: the HEIGHT.** The block-level rules ask it at this block
            // (`admission_independence_at`); the claim-level one — the outsider seat and its veto on
            // the licence — compares it against the CLAIM's own `accepted_daa`, as the draw does.
            admission_independence_daa: self.palw_admission_independence_daa(),
            // ADR-0145 §5/§6's height; the fold asks it at each block (`fp_derived_work_at`).
            fp_derived_work_daa: self
                .palw_fp_derived_work
                .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
                .map(|fence| fence.daa_score()),
            single_lottery_active: self.palw_single_lottery_at(daa_score),
            verification_v2_active: self.palw_verification_v2_at(daa_score),
            verification_s3_active: self.palw_verification_s3_at(daa_score),
            verification_s2_active: self.palw_verification_s2_at(daa_score),
            readiness_v2_active: self.palw_readiness_v2_at(daa_score),
            evm_actions: Vec::new(),
            // ADR-0093 Decision 8: which form of move 1 opens a phase. Written explicitly for the
            // reason the lines above give — an unwritten default here would refuse, or admit, a
            // responder's whole defense by omission.
            attn_anchored_root_active: self.palw_attn_anchored_root_at(daa_score),
            audit_2026_09_11_active: self.palw_audit_2026_09_11_at(daa_score),
            audit_2026_09_11_deep_active: self.palw_audit_2026_09_11_deep_at(daa_score),
            audit_2026_09_23_active: self.palw_audit_2026_09_23_at(daa_score),
            settled_anchor_depth: self.palw_settled_anchor_depth_at(daa_score),
            // ADR-0100: the one-move court's ladder rides to the fold when the court is armed —
            // the SAME ladder the acceptance arm adjudicates at, so both derive one verdict.
            // Written explicitly for the reason the two lines above give.
            shard_court_ladder: if self.palw_shard_court_at(daa_score) {
                self.palw_court_params_v2.as_ref().map(|court| self.palw_court_step_ladder_at(daa_score, court))
            } else {
                None
            },
            // ADR-0100 Decision 4: one shard's seats and quorum ride to the fold when per-shard
            // licensing is armed — the panel parameters the draw and the acceptance layer use.
            shard_licensing: if self.palw_shard_licensing_at(daa_score) {
                self.palw_panel_params_v2.as_ref().map(|panel| {
                    kaspa_consensus_core::palw_shard_licensing_v1::PalwShardLicensingParamsV1 {
                        seats_per_shard: panel.seat_count(),
                        quorum_per_shard: panel.quorum(),
                    }
                })
            } else {
                None
            },
            // ADR-0103: the held regime's ladder rides to the fold when it is armed — the SAME leaf
            // cap the acceptance arms open against. Written explicitly for the reason every field
            // above gives: this one decides whether a bisection is a legal move at all.
            held_context_ladder: if self.palw_held_context_at(daa_score) {
                self.palw_court_params_v2.as_ref().map(|court| self.palw_court_step_ladder_at(daa_score, court))
            } else {
                None
            },
            // ADR-0118 Decision 4: the network's genesis prompt-ids form, which a held DA demand for
            // a prompt tile is judged against. Written explicitly for the reason every field above
            // gives: on a network minted flat a tile of a non-held claim has no answer.
            prompt_ids_merkle: self.palw_prompt_ids_form_at(daa_score)
                == kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            // ADR-0144 §9: the HEIGHT, not this block's yes/no. Written explicitly so an unwritten
            // default cannot silently leave the lock ledger off on a network that has armed it.
            objective_offence_daa: self.palw_objective_offence_daa(),
            // ADR-0133 §7: the HEIGHT, so the profile a row is derived with is a function of the
            // block that derived it and a resync folds what the live chain folded.
            seat_gate_possession_daa: self.palw_seat_gate_possession_daa(),
        }
    }

    /// **ADR-0072 Decision 8's free-prompt half, resolved in exactly one place, at the BLOCK's own
    /// DAA.** The retention obligation a free-prompt claim records is a function of the block that
    /// accepted it, so two nodes folding one block must read one answer — the reason every fence
    /// beside this one reads the block's score and not the tip's.
    fn palw_fp_da_pins_at(&self, daa_score: u64) -> bool {
        self.palw_fp_da_pins.is_some_and(|fence| fence.is_active(daa_score))
    }

    fn palw_objective_offence_at(&self, daa_score: u64) -> bool {
        self.palw_objective_offence.is_some_and(|fence| fence.is_active(daa_score))
    }

    fn palw_execution_quanta_at(&self, daa_score: u64) -> bool {
        self.palw_execution_quanta.is_some_and(|fence| fence.is_active(daa_score))
    }

    fn palw_economic_safety_at(&self, daa_score: u64) -> bool {
        self.palw_economic_safety.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **Whether a round's permits are its schedule's tickets alone at `daa_score`** (the 2026-09-23
    /// route-matrix audit's #2): ADR-0151's bundle and the quanta both armed. Every reader of a
    /// schedule's permits — the verdict, the equivocation check and the producer's view — asks this
    /// one question, so an armed mint that issued nothing grants nothing at all of them, and a network
    /// that arms the quanta alone (testnet-11 at 7,800) keeps its lottery fallback byte for byte.
    pub(super) fn palw_round_permits_are_tickets_at(&self, daa_score: u64) -> bool {
        self.palw_economic_safety_at(daa_score) && self.palw_execution_quanta_at(daa_score)
    }

    fn palw_objective_offence_daa(&self) -> Option<u64> {
        self.palw_objective_offence
            .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|fence| fence.daa_score())
    }

    fn palw_seat_gate_possession_daa(&self) -> Option<u64> {
        self.palw_seat_gate_possession
            .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|fence| fence.daa_score())
    }

    /// **ADR-0107, resolved in exactly one place, at the BLOCK's own DAA** — the block that
    /// crosses an epoch boundary decides that boundary's growth, so two nodes folding it must
    /// read one answer.
    fn palw_share_growth_final_at(&self, daa_score: u64) -> bool {
        self.palw_share_growth_final.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0123's budget release at `daa_score` — the one resolver every budget reader shares.
    pub(super) fn palw_epoch_budget_release_at(&self, daa_score: u64) -> bool {
        self.palw_epoch_budget_release.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// Resolve both epoch-budget fences at one block's DAA. Keeping them together at the
    /// admission boundary makes it impossible for a caller to transpose the two policies while
    /// forwarding adjacent booleans.
    fn palw_epoch_budget_fences_at(&self, daa_score: u64) -> kaspa_consensus_core::palw_admission_v2::PalwEpochBudgetFencesV1 {
        kaspa_consensus_core::palw_admission_v2::PalwEpochBudgetFencesV1 {
            boundary_budget_active: self.palw_epoch_boundary_budget.is_some_and(|fence| fence.is_active(daa_score)),
            budget_release_active: self.palw_epoch_budget_release_at(daa_score),
            // ADR-0137: the block's W₀ where the work target is in force; the block's subsidy is
            // `calc_block_subsidy` at its DAA, the same reading every attempt block's context takes.
            work_target_floor: self.palw_work_target_floor_for(daa_score, self.coinbase_manager.calc_block_subsidy(daa_score)),
            single_lottery: self.palw_single_lottery_at(daa_score),
            // **ADR-0145: the HEIGHT, not this block's answer** — the exposure ceiling must price
            // a claim the way the fold will price it when it writes `claim.reserved`, and the fold
            // compares this height against the CLAIM's own `accepted_daa`. Passing a flag resolved
            // here would price one claim under two bases; passing nothing would leave the ceiling
            // measuring the declared basis while the ledger measured the derived one, which is the
            // 2026-09-19 re-audit's finding (a) — the gate admitting what the ledger cannot record.
            canonical_work_daa: self.palw_canonical_work_daa,
            // ADR-0149 §5: the floor's draw from the table the registry copies into its row, so the
            // first blocks past a fence armed at the registry's own height can price the floor. Only
            // resolved where the fence is scheduled at all: a network without it never reads it.
            base_known_draw: self.palw_canonical_work_daa.and_then(|_| self.palw_base_known_draw_v1()),
            // 2026-09-23 audit H-1: the ceiling reserves `attempts x` one draw past the fence.
            audit_2026_09_23_active: self.palw_audit_2026_09_23_at(daa_score),
            // Option A: the carve this block's escrow is taken at — the value the fold's own-work
            // origin reads, so the ceiling's escrow term is the `escrowed_reward` the ledger stores.
            escrow_carve: self.palw_escrow_carve_at(daa_score, daa_score),
        }
    }

    /// **ADR-0149 §5: the floor class's derived draw as the registry will write it** — its entry in
    /// [`Self::palw_known_model_works_v1`], the map `palw_model_registry_fold_at` hands the fold as
    /// `genesis_works` and `step_model_registry` copies into the floor's row. One table, so the
    /// admission, the producer's facts and the fold price the floor's row-less attempts alike.
    fn palw_base_known_draw_v1(&self) -> Option<u128> {
        let base = self.palw_state_params_v2.as_ref()?.base_class_id();
        self.palw_known_model_works_v1().get(&base).map(|work| work.economic_ccu_per_claim).filter(|draw| *draw > 0)
    }

    /// ADR-0132 S: whether the single lottery is in force at `daa_score`.
    pub(super) fn palw_verification_v2_at(&self, daa_score: u64) -> bool {
        self.palw_verification_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_verification_s3_at(&self, daa_score: u64) -> bool {
        self.palw_verification_s3.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_verification_s2_at(&self, daa_score: u64) -> bool {
        self.palw_verification_s2.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0133 §11.2: whether the whole-artifact possession proof is in force at `daa_score`.
    pub(super) fn palw_readiness_v2_at(&self, daa_score: u64) -> bool {
        self.palw_readiness_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_single_lottery_at(&self, daa_score: u64) -> bool {
        self.palw_single_lottery.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **The fused terminal's responder-coverage rule, resolved in exactly one place** — the reason
    /// its neighbours give: the object-acceptance rehearsal, the fold and the state walk must agree
    /// about whether it is in force at THIS block, because one folding what another does not is two
    /// rules wearing one name.
    fn palw_court_responder_coverage_at(&self, daa_score: u64) -> bool {
        self.palw_court_responder_coverage.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0084 §7.5, resolved in exactly one place, at the BLOCK's acceptance DAA.**
    ///
    /// `pub(super)` so `tests.rs` can assert that this reads `palw_court_ladder` and not its
    /// neighbour: the 2026-09-06 merge left two ladder fences on `Params` and
    /// `git grep palw_refutation_leaf_cap` showed this site had no test at all, so nothing in the
    /// tree observed which field a card had to arm (mainnet audit 2026-09-06, M-15).
    pub(super) fn palw_court_step_ladder_at(
        &self,
        daa_score: u64,
        court: &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    ) -> u64 {
        kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
            court,
            self.palw_court_ladder.is_some_and(|fence| fence.is_active(daa_score)),
        )
    }

    /// **ADR-0062, resolved in exactly one place**, for the reason its neighbour above gives: the
    /// object-acceptance rehearsal, the fold and the state walk must agree about whether the DA
    /// court is in force at THIS block, because one admitting what another refuses is two rules
    /// wearing one name.
    /// **How long a retiring bond stays locked at this block** (ADR-0062; the DA court's windows
    /// ride on top past its fence). One spelling for both readers — the locked-outpoint set a
    /// wallet is told about and the burn obligation a released bond owes — because a node that
    /// answered them differently would offer a wallet an outpoint the fold still holds.
    fn palw_bond_withdrawal_delay_at(&self, daa_score: u64) -> u64 {
        match (self.palw_bond_params_v2.as_ref(), self.palw_v2_bundle.as_ref()) {
            (Some(_), Some(bundle)) => {
                kaspa_consensus_core::config::params::palw_v2_bond_withdrawal_delay_at_v1(bundle, self.palw_da_court, daa_score)
            }
            (Some(bond_params), None) => bond_params.withdrawal_delay_daa(),
            (None, _) => 0,
        }
    }

    fn palw_da_court_at(&self, daa_score: u64) -> bool {
        self.palw_da_court.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0103, resolved in exactly one place.**
    pub(super) fn palw_held_context_at(&self, daa_score: u64) -> bool {
        self.palw_held_context.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0099 Decision 5 / ADR-0100, resolved in exactly one place**, for the DA court's
    /// reason. The fold does not read this: it reads the ladder [`Self::palw_transition_extras_at`]
    /// carries, which is `Some` exactly when this is true.
    fn palw_shard_court_at(&self, daa_score: u64) -> bool {
        self.palw_shard_court.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0100 Decision 4, resolved in exactly one place.**
    fn palw_shard_licensing_at(&self, daa_score: u64) -> bool {
        self.palw_shard_licensing.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0102, resolved in exactly one place.**
    fn palw_token_lift_at(&self, daa_score: u64) -> bool {
        self.palw_token_lift.is_some_and(|fence| fence.is_active(daa_score))
    }

    fn palw_kimi_k3_at(&self, daa_score: u64) -> bool {
        self.palw_kimi_k3.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0093 Decision 6, resolved in exactly one place.**
    fn palw_fused_dissectable_at(&self, daa_score: u64) -> bool {
        self.palw_fused_dissectable.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0093 Decision 8, resolved in exactly one place.**
    fn palw_attn_anchored_root_at(&self, daa_score: u64) -> bool {
        self.palw_attn_anchored_root.is_some_and(|fence| fence.is_active(daa_score))
    }

    fn palw_audit_2026_09_11_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_11.is_some_and(|fence| fence.is_active(daa_score))
    }

    fn palw_audit_2026_09_11_deep_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_11_deep.is_some_and(|fence| fence.is_active(daa_score))
    }

    fn palw_audit_2026_09_23_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_23.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// The second clock's depth where the fence carries it; `None` is the DAA-only rule.
    fn palw_settled_anchor_depth_at(&self, daa_score: u64) -> Option<u64> {
        if self.palw_audit_2026_09_23_at(daa_score) { self.palw_settled_anchor_depth } else { None }
    }

    /// **ADR-0065 D1's window on both clocks** — `Params::palw_bond_maturity` widened so that the
    /// maturity floor is no later than the second clock's (`palw_settled_anchor_floor_daa_v1`): a
    /// bond may judge only once its window has elapsed AND the chain has settled `depth` anchors
    /// since it registered. ONE place, read by the validator, the assembler and the registry
    /// warning alike, so the three recompute one identical panel. Below the fence, or with fewer
    /// than `depth` anchors before the anchor (the bootstrap waiver), it is the window itself.
    fn palw_bond_maturity_window_at(&self, state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2, anchor_daa: u64) -> Option<u64> {
        let window = self.palw_bond_maturity_at(anchor_daa)?;
        let floor = self
            .palw_settled_anchor_depth_at(anchor_daa)
            .and_then(|depth| kaspa_consensus_core::palw_panel_v2::palw_settled_anchor_floor_daa_v1(state, anchor_daa, depth));
        Some(kaspa_consensus_core::palw_panel_v2::palw_bond_maturity_window_v2(anchor_daa, window, floor))
    }

    /// **Whether a claim's panel is drawn per shard, and into how many** — the ONE decision the
    /// binding and its validation share: the fence at the claim's ANCHOR (the panel is a pure
    /// function of the claim, the reason ADR-0065 D1 and ADR-0071 SA-3 are read there too) and a
    /// plan its class declared.
    fn palw_stratified_shard_count(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        class_id: &kaspa_hashes::Hash64,
        anchor_daa: u64,
    ) -> Option<u32> {
        if !self.palw_shard_licensing_at(anchor_daa) {
            return None;
        }
        state.class_shard_plan(class_id).map(|plan| plan.shard_count)
    }

    /// **ADR-0075 D14, resolved in exactly one place: WHAT a certification slot may be spent on.**
    ///
    /// Past the fence a slot belongs to a certification the grader ACCEPTED, and what an object
    /// costs before it is graded is the fault vectors it asks the court to walk
    /// (`PALW_CERTIFICATION_GRADING_VECTORS_PER_BLOCK`). Dormant, a slot is charged to anything
    /// shaped like a certification before any validation runs — which is why two ordinary
    /// lifecycle transactions carrying evidence the court refuses at its first line could drop
    /// every genuine certification in a block.
    ///
    /// One field for the whole rule, deliberately: it decides which objects a block accepts and
    /// therefore its state root, and two fences over one rule is a network that can arm half of
    /// it. The name is the field's history, not its scope — it began as the chunk index rule and
    /// that rule, alone, bought no adversarial delta.
    fn palw_chunk_cap_charge_at(&self, daa_score: u64) -> bool {
        self.palw_chunk_cap_charge.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **What a chunk has to BE for the block's grading cap to be spent on it** (ADR-0075 D14).
    ///
    /// The rehearsal charges the cap to a chunk that completes its group, because the
    /// `FamilyCertified` the group carried is applied inside the completing chunk's own arm. So
    /// the predicate has to be the transition's own completion test, not a cheaper stand-in for
    /// it: assemble the parts in index order, demand the whole hash to the declared group id
    /// (`ChunkGroupHashMismatch`), and demand it decode to a `FamilyCertified`
    /// (`ChunkedObjectUndecodable`, `ChunkedObjectKindNotAllowed`). Anything less is a rule with
    /// no adversarial delta — the attacker simply writes the field the rule does not read.
    ///
    /// **It answers with the WORK, not with a yes.** The caller charges the block's grading budget
    /// in fault vectors, and the only way to know a chunked certification's vector count is to
    /// decode it — which this already does. Returning the count means the payload is decoded once
    /// per chunk rather than twice, and it makes the chunked path and the direct path charge the
    /// identical figure (`PalwCertificationEvidenceV1::vector_count`).
    ///
    /// `parts` is the group's state so far, `None` for a lone `count == 1` chunk that opens and
    /// closes its group in one object; `bytes` is this chunk's payload, which is not in `parts`
    /// yet. Bounded work: the whole is at most `PALW_OBJECT_CHUNK_MAX_COUNT` ×
    /// `PALW_OBJECT_CHUNK_MAX_BYTES`, and it is the same reassembly the transition performs on
    /// the very next line for every chunk this answers `Some` for.
    fn palw_chunk_completes_a_certification_v1(
        parts: Option<&std::collections::BTreeMap<u8, Vec<u8>>>,
        index: u8,
        count: u8,
        bytes: &[u8],
        group: &kaspa_consensus_core::Hash64,
    ) -> Option<usize> {
        let mut whole: Vec<u8> = Vec::new();
        for i in 0..count {
            if i == index {
                whole.extend_from_slice(bytes);
            } else {
                let part = parts.and_then(|parts| parts.get(&i))?;
                whole.extend_from_slice(part);
            }
        }
        if kaspa_consensus_core::palw_state_v2::palw_object_chunk_group_id_v1(&whole) != *group {
            return None;
        }
        match borsh::from_slice::<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>(&whole) {
            Ok(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::FamilyCertified { evidence }) => {
                Some(evidence.vector_count())
            }
            _ => None,
        }
    }

    /// **ADR-0065 D1, resolved in exactly one place.** The window, or `None` when the rule is off.
    ///
    /// Both consumers — the acceptance layer that validates a proposed `PanelBound` and the node
    /// that assembles one — must get the same answer, because a panel is accepted only if it
    /// equals the derived one exactly: a node that resolved the window differently would propose
    /// panels its own peers refuse, and blame the claim.
    fn palw_bond_maturity_at(&self, daa_score: u64) -> Option<u64> {
        self.palw_bond_maturity.filter(|m| m.activation.is_active(daa_score)).map(|m| m.window_daa)
    }

    /// **ADR-0071 SA-1..SA-4, resolved in exactly one place**, for the D1 reason: a panel is
    /// accepted only if it equals the derived one exactly, and the fold's declaration refusals
    /// have to be the acceptance layer's. A node that resolved this differently would propose
    /// panels its peers refuse and compute a state root its peers do not.
    fn palw_capability_bound_at(&self, daa_score: u64) -> bool {
        self.palw_capability_bound.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0135: whether the model registry governs classes at `daa_score`.
    pub(super) fn palw_model_registry_at(&self, daa_score: u64) -> bool {
        self.palw_model_registry.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0137: whether the work target is in force at `daa_score`.
    pub(super) fn palw_work_target_at(&self, daa_score: u64) -> bool {
        self.palw_work_target.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0143: whether an artifact root has one recorded owner at `daa_score`.
    pub(super) fn palw_artifact_root_ownership_at(&self, daa_score: u64) -> bool {
        self.palw_artifact_root_ownership.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// The 2026-09-19 audit: whether one operator identity backs one bond at `daa_score`.
    pub(super) fn palw_operator_id_unique_at(&self, daa_score: u64) -> bool {
        self.palw_operator_id_unique.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0147 (the 2026-09-19 audit's F3): whether the block-level rules of independent admission
    /// apply at `daa_score`.
    pub(super) fn palw_admission_independence_at(&self, daa_score: u64) -> bool {
        self.palw_admission_independence.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0147: the fence's HEIGHT, which the claim-level rule compares against a claim's own
    /// `accepted_daa` — the draw, the binding and every licensing path read one claim at different
    /// chain points and must give it one answer. `None` where the fence is not configured.
    pub(super) fn palw_admission_independence_daa(&self) -> Option<u64> {
        self.palw_admission_independence
            .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|fence| fence.daa_score())
    }

    /// ADR-0145 §5/§6 at the FOLDING block's DAA — candidate-scoped, like every rule on this lane:
    /// two nodes folding one block must price it identically however far either has synced.
    pub(super) fn palw_fp_derived_work_at(&self, daa_score: u64) -> bool {
        self.palw_fp_derived_work.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0148: the chain's free-prompt pricing** — the canonical-work fence's height and the
    /// floor. The admission that draws a quantum's ticket and the producer's finder that looks for a
    /// winning one read the same value the fold priced the claim with.
    pub(super) fn palw_fp_pricing(&self) -> Option<kaspa_consensus_core::palw_state_v2::PalwFpPricingV1> {
        self.palw_state_params_v2
            .as_ref()
            .map(|state| kaspa_consensus_core::palw_state_v2::PalwFpPricingV1::of(state, self.palw_canonical_work_daa))
    }

    /// ADR-0137: `W₀` for a block of `daa_score` paying `subsidy`, where the work target is in
    /// force — the block's escrow at the payout's rate; `None` below the fence.
    /// **The audit's #5: what a free-prompt claim accepted at `daa_score` prices its receipt rights
    /// from** — the fold's `fp_receipt_rights_inputs_v1` from the node's side: the block's worker carve
    /// and `W₀`, past `palw_audit_2026_09_23` and while the work target is in force.
    pub(super) fn palw_fp_receipt_rights_inputs_at(
        &self,
        daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwFpRightsInputsV1> {
        if !self.palw_audit_2026_09_23_at(daa_score) {
            return None;
        }
        let subsidy = self.coinbase_manager.calc_block_subsidy(daa_score);
        let work_floor = self.palw_work_target_floor_for(daa_score, subsidy)?;
        let state_params = self.palw_state_params_v2.as_ref()?;
        Some(kaspa_consensus_core::palw_state_v2::PalwFpRightsInputsV1 {
            worker_carve: kaspa_consensus_core::palw_state_v2::palw_claim_escrow_v1(
                state_params,
                subsidy,
                self.palw_escrow_carve_at(daa_score, daa_score),
            ),
            work_floor,
        })
    }

    pub(super) fn palw_work_target_floor_for(&self, daa_score: u64, subsidy: u64) -> Option<u128> {
        if !self.palw_work_target_at(daa_score) {
            return None;
        }
        let state_params = self.palw_state_params_v2.as_ref()?;
        let rate = self
            .palw_economic_payout
            .filter(|payout| payout.activation.is_active(daa_score))
            .map(|payout| payout.rate_sompi_per_giga)
            .unwrap_or(kaspa_consensus_core::palw_work_target_v1::PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1);
        Some(kaspa_consensus_core::palw_state_v2::palw_work_floor_for_block_v1(
            state_params,
            subsidy,
            self.palw_escrow_carve_at(daa_score, daa_score),
            rate,
        ))
    }

    /// **ADR-0135: the work of every class this node can describe** — what the registry opens rows
    /// from. A genesis class carries no admission carriage in any shipped bundle (its profile is the
    /// catalog's), so its work comes from the canonical class table every node compiles
    /// (`canonical_classes_v1`: the floor and the shipped rows) — the same derivation the
    /// registration message uses for a post-genesis class. A class registered on the chain is
    /// described through the carriage the registration carried, whether it landed before or after
    /// the registry's fence (the store keeps it; a syncing node adopts it before the block that
    /// needs it). Derived once per class and cached; a class the node cannot describe is `None`
    /// and stays a legacy row. Deterministic across nodes running one binary: the table is
    /// compiled in and the carriages are the chain's.
    pub(super) fn palw_known_model_works_v1(
        &self,
    ) -> std::collections::BTreeMap<kaspa_hashes::Hash64, kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
        let mut cache = self.palw_model_work_cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for object in &self.palw_genesis_objects_v2 {
            let PalwConsensusObjectV2::ClassRegistered { class_id, .. } = object else { continue };
            if cache.contains_key(class_id) {
                continue;
            }
            // The bundle's own carriage first, then the typed catalog (the shipped rows), then the
            // canonical class table (the floor and the tiers' other rows).
            let work = self
                .palw_genesis_model_works
                .get(class_id)
                .copied()
                .or_else(|| kaspa_consensus_core::palw_model_registry_v1::palw_rc_typed_class_works_v1().get(class_id).copied())
                .or_else(|| self.palw_canonical_class_work_v1(*class_id));
            cache.insert(*class_id, work);
        }
        // **C-1 of the 2026-09-18 audit: this map decides rooted state, so it reads the chain and the
        // build — never this node's own storage.** It used to enumerate the classes of the node's OWN
        // TIP and take each one's graph from `palw_class_carriage_store`, a local RocksDB written at
        // acceptance (where a failure is a `warn!` and the block is still accepted) and synced
        // best-effort at the pruning point. Past the registry fence this map answers
        // `step_model_registry` (which writes the rooted `model_lifecycles`), `model_class_work`
        // (which writes the rooted `claim_economics`, and so the payout) and
        // `check_class_admits_claim`'s rowless arm — so a node holding a declaration another node
        // lacked folded a DIFFERENT state root for the same block, and a node that adopted one later
        // disagreed with its own earlier fold across a reorg.
        //
        // A class registered on a running chain PAST the fence opens its row from the object in the
        // same block (`open_model_lifecycle`): rooted, and needing no local copy. A class registered
        // before the fence gets no row on every node alike — it keeps the share it has, exactly as
        // the registry's own documentation says — and enters the registry by re-registering.
        cache.iter().filter_map(|(id, work)| work.map(|work| (*id, work))).collect()
    }

    /// A genesis class's work from the canonical class table (the floor and the shipped rows), by
    /// the class id its profile derives — `None` for a class the table does not carry.
    #[cfg(not(target_arch = "wasm32"))]
    fn palw_canonical_class_work_v1(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1> {
        let court = self.palw_court_params_v2.as_ref()?;
        let entry = misaka_palw_base0::classes::canonical_classes_v1(court)
            .into_iter()
            .find(|c: &misaka_palw_base0::classes::CanonicalClassV1| c.class_id() == class_id)?;
        let canonical =
            kaspa_consensus_core::palw_base0_profile::rc_job_context(&entry.profile, entry.canonical_job.0, entry.canonical_job.1);
        kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&entry.profile, &canonical)
    }

    /// The table is not compiled for wasm: a class is described there only through its carriage.
    #[cfg(target_arch = "wasm32")]
    fn palw_canonical_class_work_v1(
        &self,
        _class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1> {
        None
    }

    /// ADR-0135: the fold's registry input at `daa_score` — the globals, the lane's span clock and
    /// the genesis classes' work — or `None` below the fence (and where no lane is scheduled: the
    /// registry steps at span boundaries, and `validate_palw_v2` refuses a registry without a lane).
    pub(super) fn palw_model_registry_fold_at(
        &self,
        daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1> {
        if !self.palw_model_registry_at(daa_score) {
            return None;
        }
        let lane = self.palw_execution_lane_at(daa_score)?;
        // The panel's seat count is the network's (the bundle's panel params), not the global
        // constant's: a devnet with three-seat panels needs five ready seats, testnet-11 seven.
        let mut globals = kaspa_consensus_core::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1;
        if let Some(panel) = self.palw_panel_params_v2.as_ref() {
            globals.seat_count = panel.seat_count();
        }
        let activation_daa = self.palw_model_registry.map(|f| f.daa_score()).unwrap_or(0);
        Some(kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1 {
            globals,
            span_daa: lane.schedule_span_daa,
            genesis_works: self.palw_known_model_works_v1(),
            grace_until_daa: kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1::grace_until_v1(
                activation_daa,
                lane.schedule_span_daa,
                &globals,
            ),
            admission_audit_period_daa: self.palw_admission_audit_period_daa,
        })
    }

    /// ADR-0132 Upgrade C: the fold's payout input at the block `point` names — the fence's numbers
    /// where it is active at the block's DAA, with the block's compact `bits` (`0` where the store
    /// holds no header for it, which prices the network draw at one).
    /// **ADR-0137 (shadow): the work target's fold input.** The rate is the payout fence's where
    /// it is armed at `daa_score` and the shadow constant otherwise; the clamp is the class DAA's;
    /// the works are the registry's (the genesis classes from the canonical table, the registered
    /// ones from their carriages). `None` off ConsensusV2, where there is no work to price.
    pub(super) fn palw_work_target_fold_for(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Option<kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        self.palw_v2_bundle.as_ref()?;
        let rate = self
            .palw_economic_payout
            .filter(|payout| payout.activation.is_active(point.daa_score))
            .map(|payout| payout.rate_sompi_per_giga)
            .unwrap_or(kaspa_consensus_core::palw_work_target_v1::PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1);
        let block_bits = self.headers_store.get_header(point.block).map(|header| header.bits).unwrap_or(0);
        Some(kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1 {
            rate_sompi_per_giga: rate,
            block_bits,
            max_factor: state_params.class_daa_max_factor(),
            works: self.palw_known_model_works_v1(),
        })
    }

    pub(super) fn palw_economic_payout_fold_for(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Option<kaspa_consensus_core::palw_economic_payout_v1::PalwEconomicPayoutFoldV1> {
        let payout = self.palw_economic_payout.filter(|payout| payout.activation.is_active(point.daa_score))?;
        let bits = self.headers_store.get_header(point.block).map(|header| header.bits).unwrap_or(0);
        Some(payout.fold_v1(bits))
    }

    /// ADR-0135: the draw's readiness policy at an anchor — a seat judges a non-base class only
    /// with a possession proof no older than the readiness age.
    pub(super) fn palw_readiness_policy_at(
        &self,
        anchor_daa: u64,
    ) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwReadinessPolicyV1> {
        let fold = self.palw_model_registry_fold_at(anchor_daa)?;
        if !fold.governs_at(anchor_daa) {
            return None;
        }
        let state = self.palw_state_params_v2.as_ref()?;
        Some(kaspa_consensus_core::palw_model_registry_v1::PalwReadinessPolicyV1 {
            now_daa: anchor_daa,
            max_age_daa: (fold.globals.readiness_probe_max_age_spans as u64).saturating_mul(fold.span_daa.max(1)),
            base_class_id: state.base_class_id(),
        })
    }

    /// **ADR-0075 SA-1/SA-2, resolved in exactly one place.** `false` on every shipped preset.
    ///
    /// Two sites read it and they must agree: `calculate_utxo_state` decides whether to REMEMBER
    /// what each 0x4b carrier paid, and `palw_v2_accepted_objects` decides whether to SPEND that
    /// memory. If the first said no and the second said yes, every carrier would look like it paid
    /// nothing and no certification could ever be graded — the fail-closed direction, but a
    /// liveness halt all the same, and the reverse would collect fees nobody reads.
    pub(super) fn palw_certification_rent_at(&self, daa_score: u64) -> bool {
        self.palw_certification_rent.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0082 Decision 3, resolved in exactly one place.** `false` on every shipped preset.
    ///
    /// Two things read it and they must agree: the acceptance filter, which admits a dissection
    /// move only past the fence, and [`Self::palw_court_params_at`], which decides the arity the
    /// children of every round are cut at. A node that resolved them differently would accept a
    /// root claim its peers refused, or deal a different number of children from the same range.
    pub(super) fn palw_kary_court_active_at(&self, daa_score: u64) -> bool {
        self.palw_kary_court.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0077 Decision 16's `PanelDa` at this block, resolved in exactly one place.** `false` on
    /// every shipped preset; `true` from block one on a genesis that arms it.
    pub(super) fn palw_panel_da_at(&self, daa_score: u64) -> bool {
        self.palw_panel_da.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0081 Decision 3's form at this block, resolved in exactly one place.** `Flat` on every
    /// shipped preset; the tiled Merkle root from block one on a genesis that arms it
    /// (`validate_palw_v2` refuses any other height, so this never differs from
    /// `Params::palw_prompt_ids_form_v1`).
    pub(super) fn palw_prompt_ids_form_at(&self, daa_score: u64) -> kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1 {
        match self.palw_prompt_ids_merkle {
            Some(fence) if fence.is_active(daa_score) => kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            _ => kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        }
    }

    /// **ADR-0077 Phase B's fence at this block, resolved in exactly one place.** `false` on
    /// testnet-11 and the hash-lineage presets; armed from genesis on devnet and on a carded
    /// mainnet.
    pub(super) fn palw_context_ladder_at(&self, daa_score: u64) -> bool {
        self.palw_context_ladder.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **The court a session is judged under at this block** (ADR-0082 Decision 3, patch note 7).
    ///
    /// Under a dormant fence this is `bundle.court` byte for byte — every shipped preset's court,
    /// unchanged. Under an armed one the arity is the derivation's, over the ruleset's own
    /// quantities and the classes the genesis set registers; `None` from the derivation is a
    /// REFUSAL here, never a fallback to the binary ladder, because an arity of 2 is a value the
    /// derivation can legitimately return and using it for "no legal arity" would let a window
    /// that cannot hold its own dispute run a court that overruns it.
    pub(super) fn palw_court_params_at(
        &self,
        daa_score: u64,
    ) -> Option<Result<kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2, String>> {
        let bundle = self.palw_v2_bundle.as_ref()?;
        Some(
            kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2(
                bundle,
                self.palw_kary_court_active_at(daa_score),
                self.palw_held_context_at(daa_score),
            )
            .map_err(|e| e.to_string()),
        )
    }

    /// **ADR-0065 D1's blind spot, named in the log instead of left for an operator to find.**
    ///
    /// `validate_palw_v2` refuses to arm the maturity fence unless the GENESIS registry holds
    /// `palw_v2_maturity_armable_bonds_v1()` bonds. That check is config-time and can only see
    /// `bundle.genesis_objects`; the LIVE registry moves in both directions — a `BondRegistered`
    /// carrier is admitted on a running chain, and bonds leave by retirement or by a slash that
    /// drives them under `min_collateral_sompi`. So a network can arm D1 legally and drift below
    /// the bar afterwards, and nothing in `Params` can know.
    ///
    /// What that looks like without this: `derive_panel_v2_with_maturity` returns
    /// `InsufficientEligibleBonds`, the assembler's `else { continue }` skips the claim in
    /// silence, every claim voids at `BindTimeout`, and `safe_frontier` stops advancing. The
    /// operator sees a chain that has stopped finalizing and no line anywhere naming the maturity
    /// fence as the cause — the symptom is indistinguishable from a producer outage.
    ///
    /// **This changes nothing.** No refusal, no fence, no return value: a refusal here would be a
    /// consensus rule that two nodes could resolve differently from local state, which is a chain
    /// split, and the runtime already fails closed. It is a log line and a rate limiter.
    fn palw_warn_if_maturity_outruns_the_registry(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa: u64,
        min_collateral_sompi: u64,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
    ) {
        use std::sync::atomic::Ordering;

        // Free on every shipped preset: the fence is `None`, so this returns before walking the
        // registry. `daa` is the block's own score, threaded from the caller that already read the
        // header — this runs once per chain block added to the virtual chain, so a store re-read
        // here would be one avoidable lookup per block, and re-reading could in principle disagree
        // with the score the rest of the fold used.
        let Some(window) = self.palw_bond_maturity_window_at(state, daa) else { return };

        let floor = kaspa_consensus_core::palw_panel_v2::palw_seat_maturity_floor_v1(daa, Some(window));
        // ADR-0124 Decision 4: past the panel economy the seat floor is the panel's, not the
        // registry's — the counter must count what the draw would seat.
        let seat_floor =
            self.palw_panel_draw_policy_at(daa).economy.map(|economy| economy.panel_floor_sompi).unwrap_or(min_collateral_sompi);
        let seatable = kaspa_consensus_core::palw_panel_v2::palw_seatable_operators_v1(state, seat_floor, floor);
        let armable = kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_maturity_armable_bonds_v1();
        if seatable >= armable {
            // Recovered. Forget both, so a later relapse is reported at once rather than waiting
            // out an interval that was started by the previous episode.
            self.palw_maturity_warn_last_daa
                .store(kaspa_consensus_core::palw_panel_v2::PALW_SHORTFALL_NEVER_REPORTED, Ordering::Relaxed);
            self.palw_maturity_warn_last_band.store(0, Ordering::Relaxed);
            return;
        }
        // Both taken from the panel params the caller already resolved — a network that seats a
        // different number must be described with its own number, not this build's default.
        // `seat_count` is what a panel holds; `drawable` is what the registry must hold for a
        // claim whose own executor's operator is among the eligible, which is every healthy claim.
        let seat_count = panel_params.seat_count() as usize;
        let drawable = seat_count + 1;

        // Once per bind window: that is how long a claim waits before it voids, so the operator
        // hears once per cohort of claims the shortfall actually costs, rather than once a block.
        // The fallback is only reachable on a network with no V2 state params, which has no panels
        // to fail — it exists so this is total, not because the number matters there.
        let interval = self.palw_state_params_v2.as_ref().map_or(600, |p| p.window_bind()).max(1);
        // 0 = margin gone, 1 = healthy claims cannot bind, 2 = nothing can. A band that has
        // worsened is reported immediately: the interval is a bind window (~20 h live), and the
        // step from "the margin is gone" to "the chain has stopped binding" is the one an operator
        // most needs to see when it happens rather than the next morning.
        let band: u64 = if seatable < seat_count {
            2
        } else if seatable < drawable {
            1
        } else {
            0
        };
        let worsened = band > self.palw_maturity_warn_last_band.load(Ordering::Relaxed);
        let last = self.palw_maturity_warn_last_daa.load(Ordering::Relaxed);
        // Through the shared predicate, which is unit-tested: escalation, cold start, the window
        // boundary, a reorg to a lower score, and an interval large enough to overflow. Three of
        // those five arms were wrong while this was inline comparisons.
        if !kaspa_consensus_core::palw_panel_v2::palw_shortfall_report_is_due_v1(last, daa, interval, worsened) {
            return;
        }
        self.palw_maturity_warn_last_daa.store(daa, Ordering::Relaxed);
        self.palw_maturity_warn_last_band.store(band, Ordering::Relaxed);

        // **Three bands, because exactly three are provable.**
        //
        // The draw's three exclusions — executor bond, executor operator, executor key — collapse
        // to one. `pubkey` uniqueness is enforced at registration (`palw_state_v2.rs`, the
        // `DuplicateBondKey` arm), so the key clause can only ever match the executor's own bond,
        // which the first clause already matched. The exclusions therefore remove EXACTLY ONE
        // operator when the executor's operator is in the counted set, and ZERO when it is not —
        // and it is not whenever the executor's own bond is immature, Retiring, or under the
        // collateral floor, all of which a claim can enter after it was created.
        //
        // Hence:
        //   seatable >= seat_count + 1   every claim draws (the worst case still leaves seat_count)
        //   seatable == seat_count       a claim whose own bond is still eligible cannot draw; one
        //                                whose bond has itself left eligibility still can
        //   seatable <  seat_count       no claim can draw, whatever its executor is
        //
        // Collapsing the middle band into the bottom one is what made an earlier draft of this
        // print "no panel can be drawn" over a state where a panel was demonstrably being drawn
        // (`a_claim_whose_own_operator_has_left_can_still_seat_a_panel` measures that state).
        if band == 2 {
            warn!(
                "[palw-panel] ADR-0065 D1 (seat maturity) is IN FORCE and NO claim can seat a panel: {seatable} \
                 distinct operators are mature and eligible, and a panel needs {seat_count}. Every claim voids at \
                 BindTimeout and safe_frontier stops advancing. It recovers when enough bonds register and mature \
                 ({window} DAA) under operator keys not already in the registry, or when eligible ones return. The \
                 fence's arming check only ever saw the GENESIS registry; this is the live one. No rule changed — \
                 this is a diagnosis, not a refusal."
            );
        } else if band == 1 {
            warn!(
                "[palw-panel] ADR-0065 D1 (seat maturity) is IN FORCE and panels are failing: {seatable} distinct \
                 operators are mature and eligible, and a draw needs {seat_count} BESIDES the claim's own \
                 executor's. Every claim from a still-eligible bond voids at BindTimeout and safe_frontier stops \
                 advancing; only a claim whose own bond has itself left eligibility can still bind. It recovers \
                 when a bond registers and matures ({window} DAA) under an operator key not already in the \
                 registry, or when an eligible one returns. The fence's arming check only ever saw the GENESIS \
                 registry; this is the live one. No rule changed — this is a diagnosis, not a refusal."
            );
        } else {
            warn!(
                "[palw-panel] ADR-0065 D1 (seat maturity) is IN FORCE and the live registry has no spare seat: \
                 {seatable} distinct operators are mature and eligible, {armable} is the margin the fence was armed \
                 against and {drawable} is the point claims from healthy bonds stop binding. One retirement, or one \
                 slash under min_collateral, now stops those claims for a full maturity window ({window} DAA) — \
                 because the replacement is itself immature. No rule changed — this is a diagnosis, not a refusal."
            );
        }
    }

    /// **ADR-0042 Decision 6's consumer: an attempt-lane (algo-6) block's stateful admission.**
    ///
    /// Closes P0-2 and P0-10 at the point they bite. The stateless half — shape, and the challenge
    /// against the header's own position — already runs at `check_palw_carriage_stateless` before
    /// GHOSTDAG. This is everything that needs CHAIN STATE and therefore could not run there: the
    /// bond exists and is not retiring, its key IS the carried key (which is what turns the
    /// stateless signature into a statement about a bonded party rather than about a keypair), one
    /// operator per bond, the class is registered and unfrozen, the artifact root is the class's,
    /// the pwu is the derivation and not a claim, the epoch budget still has room, the class
    /// lottery admits the ticket, and the bond's immature exposure stays under its ceiling.
    ///
    /// `Ok(None)` on every block that is not an attempt-lane block, which is every block of every
    /// network that exists today — the arm is chosen by the header's declared algorithm.
    ///
    /// **The validated envelope is RETURNED, not discarded.** It is the block's own work, and the
    /// transition needs it to create the claim: for as long as this returned `()` the caller had
    /// nothing to pass and passed `None`, so a network could admit a perfectly valid attempt and
    /// then fold a transition that had never heard of it. No claim was created, so nothing could
    /// be bound, licensed, challenged or finalized, and `safe_weight` never left zero. Handing the
    /// envelope back is what makes the check and the fold speak about the same object.
    /// `bootstrap_state` is ADR-0064's mergeset bond view, and it is `Some` for exactly one
    /// caller: the CHAIN BLOCK's own attempt, past `palw_bootstrap_activation`. The merged-work
    /// callers pass `None` — a block merged from a sibling is not "this block's own attempt", and
    /// letting it self-bond would let one block license work for bonds nobody's chain had accepted.
    ///
    /// What is handed in is the state the ACCEPTED objects fold to — the same set
    /// `apply_palw_transition_v4` applies at step 3 before it applies own work at step 4 — so the
    /// bond resolves against exactly the registry the transition is about to have. Reading the
    /// first matching `BondRegistered` off the object list instead would answer a subtly different
    /// question: a block that registers a bond and then retires it in the same mergeset would be
    /// told the bond is Active while the transition holds it Retiring. The disagreement ADR-0064
    /// removes is closed by construction, not by two readings agreeing to agree.
    fn palw_v2_check_attempt_admission(
        &self,
        header: &Header,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        bootstrap_state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
    ) -> Result<Option<kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2>, String> {
        use kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2;
        // ADR-0072 SA-4: EITHER attempt id. Which one is a lane at this height was already decided
        // by the header processor and the pruning-proof gate, both of which refuse the closed side
        // by id; asking again here would be a second spelling of one rule. What this must NOT do is
        // keep naming only algo-6, which past the fence would skip admission entirely for every
        // block on the open lane.
        if !kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id) {
            return Ok(None);
        }
        let Some(admission) = self.palw_admission_params_v2.as_ref() else {
            return Err("an attempt-lane block on a network with no V2 admission params".to_string());
        };
        let envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).map_err(|e| e.to_string())?;
        // **The FULL admission, because the stateful half alone verifies no signature.**
        //
        // `check_palw_attempt_admission_v2` takes no verifier and cannot: its item 2 compares the
        // carried `executor_pubkey` against the bond record's key, and BOTH are public. Called
        // alone it establishes "this attempt names a key that matches an Active bond" and nothing
        // about who authored it — so an attacker copies a victim's bond outpoint and public key
        // off the chain, writes any bytes into `signature`, solves the PoW, and mines under
        // someone else's stake. `check_palw_attempt_admission_full_v2` is the composer that runs
        // the stateless binding and `validate_signature_v2` before delegating here.
        //
        // The signature is also outside `attempt_id_v2` and therefore outside the PoW digest
        // (ADR-0042 Decision 3c, deliberately), while the block-identity digest hashes the raw
        // carrier bytes. Unverified, that combination lets ANY third party flip one signature bit
        // and mint another distinct, valid block from one solved PoW. 3c's deferral rested on
        // "only the bond holder can mint valid-signature siblings", which is a statement about a
        // signature somebody checks.
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let pre_pow_hash = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        // ADR-0064: the record for THIS attempt's bond as this block's own mergeset leaves it.
        // Read out of the rehearsal's end state, so it is the transition's own answer rather than
        // a second opinion about what a fresh bond is.
        let wanted = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(envelope.attempt.executor_bond);
        let bootstrap_bond = bootstrap_state.and_then(|folded| folded.bond(&wanted)).cloned();
        kaspa_consensus_core::palw_admission_v2::check_palw_attempt_admission_full_v2_with_bootstrap(
            state,
            state_params,
            admission,
            point,
            network_domain,
            pre_pow_hash,
            header.timestamp,
            header.nonce,
            // The HEADER's own score (ADR-0072 Decision 8's retention pin): for a merged block this
            // is the merged block's, which is the one its producer derived from.
            header.daa_score,
            &envelope,
            |key, message, sig, context| verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false),
            bootstrap_bond.as_ref(),
            // ADR-0045/ADR-0123: resolve both budget policies from the same block DAA, and pass
            // them as named fields so boundary and release cannot be transposed at this callsite.
            self.palw_epoch_budget_fences_at(point.daa_score),
        )
        .map_err(|e| e.to_string())?;
        Ok(Some(envelope))
    }

    /// Unit C step 4's consumer: a receipt-lane (algo-7) block's spend, admitted against a beacon
    /// this node DERIVED from the candidate's chain rather than one the block asserted.
    ///
    /// `Ok(None)` on every block that is not a receipt-lane block, which is every block of every
    /// network that exists today — the arm is selected by the header's own declared algorithm, so
    /// "is this a spend" is a fact about the header rather than a discovery.
    ///
    /// Returns the validated envelope for the same reason the attempt arm does: the spend is the
    /// block's work, and `apply_receipt_spend` is what turns a certified quantum into chain
    /// weight. Checked-then-dropped, a receipt-lane block burned its quantum nowhere — the ledger
    /// never recorded the spend, so the same quantum stayed spendable forever and the weight it
    /// was supposed to add never arrived.
    /// ADR-0058: the works this chain block's WHOLE mergeset carries — blues and reds alike,
    /// consensus order, selected parent excluded (it applied its own work as the previous chain
    /// block), non-DAA blocks excluded (the coinbase's pay set — a timestamp-deviant block earns
    /// neither pay nor a claim). Reds are not an edge case here, they are the POINT: at the
    /// frozen 120 s cadence `ghostdag_k = 1`, so any block whose anticone holds two or more
    /// blocks — every block of every class slower than the floor — is a red by construction.
    /// A blues-only rule would measure nothing.
    ///
    /// The full admission — stateless shape, challenge binding, executor signature, and the
    /// stateful list against the WALK state — runs here once per merged block; the transition
    /// then re-runs the stateful half against its live fold state. A merged block that fails is
    /// returned with its reason and skipped: nothing about its anticone may disqualify the
    /// accepting block.
    pub(super) fn palw_v2_merged_works(
        &self,
        ghostdag_data: &GhostdagData,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        // The accepting block's mergeset non-DAA set — passed in rather than read from a stored
        // block, so the TEMPLATE path (whose virtual block has no hash yet) can supply
        // `virtual_state.mergeset_non_daa` and the two paths compute one identical work list.
        mergeset_non_daa: &BlockHashSet,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> (Vec<PalwMergedOwnedWorkV1>, Vec<(BlockHash, String)>) {
        let mut works = Vec::new();
        let mut skips = Vec::new();
        if ghostdag_data.mergeset_blues.len() <= 1 && ghostdag_data.mergeset_reds.is_empty() {
            return (works, skips);
        }
        let non_daa = mergeset_non_daa;
        let merged: Vec<BlockHash> =
            ghostdag_data.consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.as_ref()).collect();
        for blue in merged.iter() {
            if non_daa.contains(blue) {
                continue;
            }
            let header = match self.headers_store.get_header(*blue) {
                Ok(header) => header,
                Err(missing) => {
                    skips.push((*blue, format!("no header for a mergeset block: {missing}")));
                    continue;
                }
            };
            match self.palw_v2_check_attempt_admission(&header, state, state_params, point, None) {
                Ok(Some(envelope)) => works.push(PalwMergedOwnedWorkV1::Attempt(
                    *blue,
                    envelope,
                    // B-1: the merged block's own subsidy, the pool its escrow is carved from past
                    // the deep fence. An attempt block is never a heartbeat, so this equals the
                    // coinbase-declared subsidy body validation pinned.
                    self.coinbase_manager.calc_block_subsidy(header.daa_score),
                    // ADR-0126: and the carve, at the lower of the attempt block's score and the
                    // accepting block's — the block whose coinbase pays and withholds it.
                    self.palw_escrow_carve_at(header.daa_score, point.daa_score),
                    // ADR-0132 Upgrade C: the merged block's own `bits`, the lottery its forward faced.
                    header.bits,
                )),
                Ok(None) => match self.palw_v2_check_receipt_spend(&header, state, state_params, point) {
                    Ok(Some(envelope)) => works.push(PalwMergedOwnedWorkV1::Spend(*blue, envelope)),
                    Ok(None) => {}
                    Err(why) => skips.push((*blue, why)),
                },
                Err(why) => skips.push((*blue, why)),
            }
        }
        (works, skips)
    }

    fn palw_v2_check_receipt_spend(
        &self,
        header: &Header,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3>, String> {
        use kaspa_consensus_core::palw_freeprompt_v3::{PalwReceiptSpendEnvelopeV3, fp_draw_slot_v3};
        if header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3 {
            return Ok(None);
        }
        let Some(freeprompt) = self.palw_freeprompt_params_v3.as_ref() else {
            return Err("a receipt-lane block on a network with no free-prompt bundle".to_string());
        };
        let envelope = PalwReceiptSpendEnvelopeV3::decode(&header.palw_commitment).map_err(|e| e.to_string())?;
        let claim = state
            .claim(&envelope.spend.claim_id)
            .ok_or_else(|| format!("claim {} does not exist at this chain point", envelope.spend.claim_id))?;
        let kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
            return Err(format!("claim {} is not certified at this chain point", envelope.spend.claim_id));
        };
        let slot = fp_draw_slot_v3(final_daa, freeprompt.receipt_maturity_daa())
            .ok_or_else(|| "the draw slot overflows the DAA space".to_string())?;
        // Derived from the block's OWN selected parent, so the walk is the candidate's and the
        // beacon cannot be chosen by the party it decides for.
        //
        // **`direct_parents()[0]` is not the selected parent.** It is the first entry of an array
        // the block's own producer writes and orders, so the sentence above was describing a
        // property the code did not have: a producer could list whichever parent it liked first and
        // walk the beacon down that branch instead, re-rolling the draw it was about to be judged
        // by. GHOSTDAG's answer is the one nobody chooses.
        let selected_parent = self
            .ghostdag_store
            .get_selected_parent(header.hash)
            .map_err(|e| format!("the candidate's own ghostdag data is unreadable: {e}"))?;
        let beacon = self.palw_beacon_fact_of_candidate(selected_parent, slot).map_err(|e| e.to_string())?;
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        // **The composed admission, which is what this site was always missing.**
        //
        // `network_domain` was computed here and thrown away by the `let _` below — the input the
        // signature check needs, sitting one line from the call that never consumed it. The stateful
        // half alone was being run, and its item 7 (`bond.pubkey != spend.producer_pubkey`) compares
        // the bond's key against a field the block's author supplies. With no signature that
        // comparison is not authority, it is a copy: anyone could name a bonded key they do not
        // hold and spend that producer's certified quantum. `_full_v3` — documented as "the composed
        // admission a wiring layer should call" and covered by three tests — had no non-test caller.
        let pre_pow_hash = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        kaspa_consensus_core::palw_fp_admission_v3::check_palw_receipt_spend_admission_full_v4(
            state,
            point,
            network_domain,
            pre_pow_hash,
            header.timestamp,
            header.nonce,
            freeprompt.receipt_maturity_daa(),
            freeprompt.receipt_use_window_daa(),
            &beacon,
            &envelope,
            |key, message, sig, context| kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false),
            // ADR-0148: a compute-era claim draws against the lane's pooled target.
            self.palw_fp_pricing().as_ref(),
        )
        .map_err(|e| e.to_string())?;
        let _ = state_params;
        Ok(Some(envelope))
    }

    /// ADR-0044 / Unit C step 4: the beacon fact for `slot`, derived from THIS candidate's own
    /// selected-parent chain.
    ///
    /// The whole point is where the randomness comes from. A `PalwBeaconFactV3` taken from the
    /// spending block's own bytes would be the producer asserting its own draw — it would pick
    /// the beacon that makes its receipt win. So the fact is never read off a block: it is the
    /// first attempt-class chain block at or after the slot, found by walking the candidate's
    /// chain downward, with `prev_attempt_daa` — the last attempt-class block strictly BELOW the
    /// slot — as the witness that makes "first at or after" checkable by someone who did not walk.
    ///
    /// The walk starts at `from` and descends, which is what makes it candidate-scoped: two nodes
    /// with different sinks but the same candidate derive the same fact, because they walk the
    /// same chain.
    ///
    /// **ADR-0073 SA-1.** Past `palw_beacon_fold` the fact is the fold of the first `k` attempt
    /// blocks at or after the slot rather than the first one alone: re-rolling a beacon costs an
    /// inference, but WITHHOLDING one costs only a subsidy, and a producer whose block would be
    /// the beacon can drop it when the draw disfavours its own claims. With `k` folded it must
    /// hold all `k`. The width is resolved from the SLOT (see below), never from the walker's own
    /// height, so the producer reading spendable quanta at the tip and the validator checking a
    /// spend deep in the past agree about one claim's fold.
    pub(super) fn palw_beacon_fact_of_candidate(
        &self,
        from: BlockHash,
        slot: u64,
    ) -> Result<
        kaspa_consensus_core::palw_freeprompt_v3::PalwBeaconFactV3,
        kaspa_consensus_core::palw_fp_beacon_v3::PalwBeaconDeriveV3Error,
    > {
        // The network's attempt-lane id. This walk descends THROUGH ADR-0072's fence, so the id it
        // is given cannot be the whole answer — `derive_beacon_fact_to_genesis_v3` matches it with
        // `is_attempt_class_v3`, which admits either attempt id once this one is an attempt id at
        // all. Handing it a single number was a permanent liveness defect on an armed network: past
        // the fence every attempt block carries algo-9, the filter matched none of them, and
        // `prev_attempt_daa` froze at the last pre-fence attempt block for the rest of the chain's
        // life. `palw_required_algo_id` can never be 9 — `PalwRulesetV2::validate` requires the
        // bundle's `algorithm_id` to be 6 — so no configuration could have fixed it.
        let attempt_algo_id = self.palw_required_algo_id.unwrap_or(kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2);
        let fold_k = self.palw_beacon_fold_k_at(slot);
        let facts = self.reachability_service.default_backward_chain_iterator(from).filter_map(|block| {
            let header = self.headers_store.get_header(block).ok()?;
            Some(kaspa_consensus_core::palw_fp_beacon_v3::PalwChainBlockFactV3 {
                block,
                daa_score: header.daa_score,
                pow_algo_id: header.pow_algo_id,
            })
        });
        // `..._to_genesis` rather than the bounded form: the iterator really does reach genesis,
        // and the bounded form's `WalkTooShort` exists for callers that stop early. A caller that
        // stopped early and reported `prev_attempt_daa = 0` would be inventing a witness.
        kaspa_consensus_core::palw_fp_beacon_v3::derive_beacon_fact_to_genesis_v3(slot, attempt_algo_id, fold_k, facts)
    }

    /// **ADR-0073 SA-1's fold width at a draw slot** — the ONE place the fence is resolved, so a
    /// beacon two parties derive differently is unrepresentable rather than merely unlikely.
    ///
    /// `1` when the fence is dormant (every shipped preset) — the pre-SA-1 rule, and the width at
    /// which the derivation returns byte-identical facts. Resolved at the SLOT and not at the
    /// candidate's own DAA: the slot is `final_daa + receipt_maturity_daa` of the claim, a value
    /// both the producer at virtual and the validator at the spending block read identically,
    /// while their own heights differ by construction.
    fn palw_beacon_fold_k_at(&self, slot: u64) -> u8 {
        self.palw_beacon_fold.filter(|fold| fold.activation.is_active(slot)).map_or(1, |fold| fold.k)
    }

    /// ADR-0042 Unit C step 3: this chain block's PALW consensus objects, in the block's own
    /// deterministic acceptance order.
    ///
    /// The order is the acceptance order and nothing else, because the transition FOLDS these and
    /// a fold is order-dependent: two nodes reading one block's objects in different orders would
    /// reach different state roots for one chain, which is the divergence class Decision 5 exists
    /// to make unrepresentable. `accepted_txs_from_acceptance_data` already produces that order.
    ///
    /// `skipped` carriers are logged rather than dropped in silence — a transaction routed to the
    /// free-prompt subnetwork that produces no object is either a mistake someone should see or
    /// an attack someone should count, and a silent drop is how neither gets noticed.
    /// **The panel bindings this block owes, derived — not published (audit C5's tail).**
    ///
    /// A panel is `derive_panel_v2` of the anchor block and the bond registry: a pure function of
    /// chain state, with nothing for a publisher to choose. It was still an object someone had to
    /// send, and nobody was PAID to send one for a claim that was not theirs — so in practice the
    /// producer decided whether its own claim proceeded, which is the thing Decision 7's panel
    /// exists to prevent.
    ///
    /// The answer is not to price the publishing; it is to remove the publisher. The CHAIN binds
    /// the panel the moment a claim's anchor slot is reached, because the chain is the only party
    /// that can walk to that anchor and it has no preference about which claims advance.
    ///
    /// Deterministic by construction: the anchor comes from `palw_v2_anchor_fact_of_candidate`,
    /// which walks THIS candidate's chain, and the registry is the same candidate state every
    /// node holds. Emitted in claim-id order so two nodes build one list.
    fn palw_v2_derived_panel_bindings(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
        block_daa: u64,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
        let Some(panel_params) = self.palw_panel_params_v2.as_ref() else { return Vec::new() };
        // The seat-eligibility floor is the registry's own, so the derivation here and the one the
        // acceptance layer runs read the same number (see `palw_bond_may_take_work_v2`).
        let Some(min_collateral) = self.palw_state_params_v2.as_ref().map(|p| p.min_collateral_sompi()) else {
            return Vec::new();
        };
        // ADR-0065 D1: say so when the live registry has fallen under what the armed fence needs.
        // Log only, and before the loop so it is reported even on a block that binds no panel —
        // "no claims advanced" is exactly what a stalled chain looks like from here.
        self.palw_warn_if_maturity_outruns_the_registry(state, block_daa, min_collateral, panel_params);
        // The binding block's fold inputs, for the bind's Valid-lock question (route-matrix #3). The
        // lock reads none of the context's blue score or subsidy.
        let binding_extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block,
            daa_score: block_daa,
            blue_score: 0,
            subsidy: 0,
        });
        let mut out = Vec::new();
        for (claim_id, claim) in state.claims_iter() {
            if !matches!(claim.phase, PalwClaimPhaseV2::Provisional) {
                continue;
            }
            // The anchor is the first chain block at or past `accepted_daa + anchor_delay`. Until
            // one exists the claim simply waits — that delay is what stops a producer from
            // mining until it likes its own jury.
            // Same base as the validator's, for the same reason — see the sibling call site.
            let Some(anchor) = self.palw_v2_anchor_fact_of_candidate(block, claim.bind_base_daa(), panel_params) else {
                continue;
            };
            // A registry too small to seat a panel yields nothing rather than a short one: a
            // partial jury is `derive_panel_v2`'s fail-closed refusal, and it stays that.
            // ADR-0065 D1, from the claim's own anchor — the same input the acceptance layer
            // uses, through the same one-place subtraction.
            let maturity_floor = kaspa_consensus_core::palw_panel_v2::palw_seat_maturity_floor_v1(
                anchor.anchor_daa,
                self.palw_bond_maturity_window_at(state, anchor.anchor_daa),
            );
            // ADR-0071 SA-3, from the same anchor as the acceptance layer's sibling call.
            let capability_bound = self.palw_capability_bound_at(anchor.anchor_daa);
            // C-02 (deep fence) and ADR-0124 (the panel economy): the whole draw policy, from the
            // same anchor as the acceptance layer's sibling call — so this assembler builds the
            // exact panel that layer recomputes.
            let mut policy = self.palw_panel_draw_policy_at(anchor.anchor_daa);
            if let Some(state_params) = self.palw_state_params_v2.as_ref() {
                policy.valid_lock = Self::palw_panel_valid_lock_of_v1(state, state_params, &binding_extras, claim, block_daa);
            }
            // ADR-0100 Decision 4: a class with a plan draws per shard, or not at all — a flat
            // panel of a sharded class would ask shard seats to judge a whole model.
            let drawn = match self.palw_stratified_shard_count(state, &claim.class_id, anchor.anchor_daa) {
                Some(shard_count) => kaspa_consensus_core::palw_panel_v2::derive_stratified_panel_v2(
                    state,
                    panel_params,
                    claim_id,
                    anchor.anchor_block,
                    min_collateral,
                    maturity_floor,
                    capability_bound,
                    shard_count,
                ),
                None => kaspa_consensus_core::palw_panel_v2::derive_panel_v2_with_policy(
                    state,
                    panel_params,
                    claim_id,
                    anchor.anchor_block,
                    min_collateral,
                    maturity_floor,
                    capability_bound,
                    policy,
                ),
            };
            let Ok(seats) = drawn else {
                continue;
            };
            // ADR-0147: nothing to ask here. The outsider is IN the derived panel — the draw policy
            // carries the fence — and the acceptance layer demands that panel exactly, so a node
            // proposes only what its own fold will take. The identity test that stood here
            // compared fields a registrant writes for itself, and is gone with the rule it served.
            out.push(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::PanelBound {
                claim: *claim_id,
                anchor: anchor.anchor_block,
                seats,
            });
        }
        out
    }

    fn palw_v2_objects_of_block(
        &self,
        acceptance: &AcceptanceData,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
        block_daa: u64,
        carrier_fees: &std::collections::HashMap<TransactionId, u64>,
    ) -> Vec<PalwCarriedObjectV1> {
        let Some(freeprompt) = self.palw_freeprompt_params_v3.as_ref() else {
            return Vec::new();
        };
        let txs = self.accepted_txs_from_acceptance_data(acceptance);
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        // **The ladder this network froze, read off the bundle and never typed** (ADR-0082
        // Decision 1). `max_step_leaf_count` is a bundle field — inside `palw_ruleset_id_v2`, so
        // it cannot move on a running chain — and it is DAA-free: `palw_court_params_at_v2` only
        // ever changes the dissection arity, never the ladder, so reading it here rather than
        // through the fence keeps this walk a pure function of the accepted set and the ruleset.
        //
        // A node with no bundle has no ladder to apply and falls back to the structural top, which
        // is what the arming-free entry uses; the walk is then exactly as permissive as it was.
        let ladder = self
            .palw_v2_bundle
            .as_ref()
            .map(|bundle| bundle.court.max_step_leaf_count())
            .unwrap_or(kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
        let extraction = kaspa_consensus_core::palw_fp_objects_v3::palw_fp_objects_from_accepted_txs_by_class_v1(
            &txs,
            network_domain,
            freeprompt,
            kaspa_consensus_core::BlockHash::default(),
            // ADR-0077 Decision 16 at the ACCEPTING block's DAA — candidate-scoped, like every
            // rule on this lane — and ADR-0081 Decision 3's form, which `validate_palw_v2` keeps
            // genesis-only, so the height it is read at cannot matter. This used to pass a literal
            // `false`, which on a genesis that arms `PanelDa` would have skipped every private
            // commitment the door had admitted.
            self.palw_panel_da_at(block_daa),
            // **ADR-0119 Decision 5: each commitment at its CLASS's bounds**, off the parent state:
            // a class under the held regime recorded its own ladder when it registered, and every
            // other class meets the network's ladder, exactly as before.
            |class_id| kaspa_consensus_core::palw_fp_objects_v3::PalwFpClassCapsV1 {
                step_ladder: state.class_step_ladder_v1(class_id, ladder),
                held: state.class_is_held_v1(class_id),
                // **ADR-0145 §5: past the fence the walk prices the run itself** (the 2026-09-19
                // reward audit's F2). Three states, at the ACCEPTING block's DAA like every rule
                // on this lane: below the fence the declared leaves ride as they always have;
                // past it the class's published graph is what counts them; past it with no graph
                // published the commitment is skipped and the carrier says so, which is the
                // operator's cue to carry one `ClassLaneCertified` for the class.
                derived_work: if self.palw_fp_derived_work_at(block_daa) {
                    match state.fp_work_profile_of(class_id) {
                        Some(profile) => kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Derived(profile),
                        None => kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Unpublished,
                    }
                } else {
                    kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Declared
                },
            },
            // **ADR-0044 Decision 9's two advertised caps, at the same block's DAA** (mainnet audit
            // 2026-09-06, L-2). The bundle's `max_prompt_tokens` and `max_decode_tokens` are inside
            // every node's `palw_ruleset_id_v2` and were read by nothing; past this fence the walk
            // reads them off the bundle it already holds. `None` on every shipped preset — both
            // live chains have accepted jobs above them since genesis.
            self.palw_fp_ruleset_caps.is_some_and(|fence| fence.is_active(block_daa)),
            // ADR-0103 Decision 4: under the held regime no PublicDa carrier's ids ride past one
            // standard transaction.
            self.palw_held_context_at(block_daa),
            self.palw_prompt_ids_form_at(block_daa),
            // Who authored the commitment. Unverified, a 0x4a transaction from any stranger created
            // a claim bound to any bond outpoint it named — the genesis premine bond among them.
            Self::verify_mldsa87_with_context_bool,
        );
        for (carrier, reason) in &extraction.skipped {
            info!("[palw-fp] carrier {carrier} produced no object: {reason}");
        }
        // P0-11: the claim-lifecycle objects. Without this walk no block could carry a
        // `PanelBound`, so every claim on a V2 network voided at `BindTimeout` and PALW weight —
        // the network's whole fork choice — was permanently zero.
        //
        // Appended after the free-prompt objects rather than interleaved by transaction order:
        // the two walks are independent pure functions of the same accepted set, so a fixed
        // concatenation is a deterministic order every node reproduces, and it keeps a change to
        // one walk from reordering the other's objects.
        let lifecycle = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(&txs);
        for (carrier, reason) in &lifecycle.skipped {
            info!("[palw-lifecycle] carrier {carrier} produced no object: {reason}");
        }
        // The derived bindings go FIRST: a claim bound by this block may then be licensed by an
        // object the same block carries, which is the order a chain that is catching up needs.
        //
        // ADR-0075 SA-1/SA-2: each carried object keeps what its own carrier paid, resolved from
        // the map `calculate_utxo_state` filled while it had the composed UTXO view in hand. That
        // map holds EVERY accepted transaction, not a list of PALW bands, so a carrier this walk
        // can name is always in it: the two extractors below read `txs`, which is the accepted set
        // that loop just priced. The `unwrap_or(0)` is therefore unreachable rather than a
        // fail-closed branch — and it has to be, because for a priced object it is a drop, and for
        // certification a permanent halt on that network. Below the fence `PALW_RENT_UNPRICED`
        // makes the whole rule read as absent rather than as "everyone underpaid".
        let priced = self.palw_certification_rent_at(block_daa);
        let fee_of = |carrier: TransactionId| {
            if priced { carrier_fees.get(&carrier).copied().unwrap_or(0) } else { PALW_RENT_UNPRICED }
        };
        self.palw_v2_derived_panel_bindings(state, block, block_daa)
            .into_iter()
            // Nothing carried a derived binding, so nothing owes rent for it.
            .map(|object| PalwCarriedObjectV1 { object, carrier_fee: PALW_RENT_UNPRICED })
            .chain(
                extraction
                    .objects
                    .into_iter()
                    .map(|carried| PalwCarriedObjectV1 { carrier_fee: fee_of(carried.carrier), object: carried.object }),
            )
            .chain(
                lifecycle
                    .objects
                    .into_iter()
                    .map(|carried| PalwCarriedObjectV1 { carrier_fee: fee_of(carried.carrier), object: carried.object }),
            )
            .collect()
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

    /// [`BondMutation`]s for a block whose acceptance data is held in-memory
    /// (the `KeyNotFound` chain block currently being UTXO-validated, before
    /// its `acceptance_data_store` entry is committed). Mirrors
    /// [`Self::dns_bond_mutations_for_chain_block`] but sources the accepted
    /// txs from the provided acceptance data instead of the store.
    fn dns_bond_mutations_from_acceptance(
        &self,
        _chain_block: BlockHash,
        acceptance_data: &AcceptanceData,
        bond_view: &ActiveBondView,
        accepted_daa_score: u64,
    ) -> Vec<BondMutation> {
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        let txs = self.accepted_txs_from_acceptance_data(acceptance_data);
        self.dns_bond_mutations_from_txs(&txs, bond_view, accepted_daa_score, min_bond, unbonding_floor)
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
        let new_state = match self.dns_bft_gate.filter(|gate| gate.activation.is_active(sink_daa)) {
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

    /// kaspa-pq Phase 10/13 (ADR-0009 §"Decision" / ADR-0018 §H): the DNS finality reorg
    /// gate. Returns `true` (candidate sink allowed) unless the overlay is configured, in
    /// the `Active` rollout stage, has a confirmed anchor, and `candidate` would abandon
    /// that anchor's selected chain. **Inert** on every current network (`dns_params` is
    /// `None`) and outside the `Active` stage.
    ///
    /// `reorg_mode` (per-network, ADR-0018 §H) selects the rule when a candidate exits the
    /// confirmed prefix:
    /// - `HardCheckpoint` (PoC/testnet/devnet): reject any such exit.
    /// - `TwoDimensionalDominance` (mainnet): accept only if the candidate **strictly
    ///   out-Works AND out-Stakes** canonical since their common ancestor `I`, each by its
    ///   emergency margin (non-substitutability — neither dimension alone suffices).
    ///
    /// Safety: each branch's StakeScore-since-`I` is scored under **its own** bond set —
    /// `candidate_bond_view` (the sink-search view already advanced to `candidate`) for the
    /// candidate, and the persisted `stake_bonds_store` (still at `prev_sink`, because the
    /// bond store is written only at the final virtual commit, never during this sink
    /// search) for canonical. Scoring a branch under the wrong view could over-credit it
    /// and wrongly accept a confirmed-history-abandoning reorg. Both branches' acceptance
    /// data is committed by the time the gate runs (the candidate's by
    /// `calculate_utxo_state_relatively`), so the per-branch walks are deterministic.
    pub(super) fn dns_reorg_outcome(
        &self,
        candidate: BlockHash,
        prev_sink: BlockHash,
        candidate_bond_view: &ActiveBondView,
    ) -> DnsReorgOutcome {
        // **ADR-0128 Decision 5: the stake reorg gate follows the BFT vote, and it is asked FIRST.**
        //
        // Past the fence at the incumbent sink, a candidate that abandons the DNS-final anchor is
        // refused before the PALW comparator below is consulted — a reorg and an extension alike,
        // which is what the early return below used to skip for a reorg. A candidate this does not
        // refuse (it contains the anchor, the anchor is stale on this node's own chain, or nothing
        // is confirmed) goes on exactly as it does without the fence. See `dns_bft_gate_refusal`.
        if let Some(refusal) = self.dns_bft_gate_refusal(candidate, prev_sink) {
            return refusal;
        }
        // **Unit D, site 4: on a V2 network the deep-reorg gate IS the one comparator.**
        //
        // A private fork can pile blue work without limit, but its frontier died at the fork
        // point — piles do not mature, because nobody could collect receipts on a chain nobody
        // saw — so frontier-first ordering refuses it here with the same three keys the virtual
        // tip used. Consulting depth, raw blue work or the stake overlay INSTEAD would be the
        // second authority coming back through the basement, which is P0-5.
        //
        // **Scoped to actual REORGS, and the scoping is load-bearing.** A first version ran the
        // comparator on every sink-search candidate and the node wedged with "valid sink must
        // exist": `compare_palw_candidates_v1` ties a child of the sink with the sink on frontier
        // and both weights, so the candidate-hash key decided — and roughly half of all ordinary
        // forward progress was refused as a failed reorg. A block that EXTENDS the sink is not
        // abandoning anything, which is why the existing DNS gate scopes itself the same way (it
        // asks whether the candidate still contains the confirmed anchor).
        if self.palw_state_params_v2.is_some()
            && !matches!(self.reachability_service.try_is_chain_ancestor_of(prev_sink, candidate), Ok(true))
        {
            // **An unweighable candidate LOSES; it does not disappear** (launch blockers §7).
            //
            // This used to require BOTH orders to resolve, and fell through to the DNS gate — which
            // answers `GateInactive`, i.e. allow — whenever either was `None`. `palw_state_walk`
            // refuses a missing delta on purpose ("reading absent data as nothing is forbidden",
            // ADR-0042 Decision 5) and `.ok()?` two layers up turned that refusal into `None`, so
            // the deep-reorg gate failed OPEN on exactly the candidate it could not weigh — which
            // is the one an attacker supplies.
            //
            // Three cases, and none of them is "carry on": a weighed challenger against a weighed
            // incumbent is the comparator's; a challenger this node cannot weigh never beats one it
            // can; and an incumbent this node cannot weigh is a state fault, refused rather than
            // silently downgraded to blue work.
            return match (self.palw_candidate_order_v2(prev_sink), self.palw_candidate_order_v2(candidate)) {
                (Some(incumbent), Some(challenger)) => {
                    match kaspa_consensus_core::palw_fork_authority_v2::decide_deep_reorg_v2(&incumbent, &challenger) {
                        kaspa_consensus_core::palw_fork_authority_v2::PalwDeepReorgV2::Allow => {
                            self.palw_frontier_provenance_outcome(candidate, prev_sink)
                        }
                        kaspa_consensus_core::palw_fork_authority_v2::PalwDeepReorgV2::Refuse => DnsReorgOutcome::DominanceViolation,
                    }
                }
                (Some(_), None) => {
                    info!("deep reorg refused: candidate {candidate} cannot be weighed by this node's PALW authority");
                    DnsReorgOutcome::DominanceViolation
                }
                (None, _) => {
                    info!("deep reorg refused: this node cannot weigh its own sink {prev_sink} — PALW state is incomplete");
                    DnsReorgOutcome::DominanceViolation
                }
            };
        }
        let Some(dns_params) = self.dns_params.as_ref() else {
            return DnsReorgOutcome::GateInactive;
        };
        let Ok(state) = self.dns_state_store.read().get() else {
            return DnsReorgOutcome::GateInactive; // no DnsState written yet
        };
        if state.rollout_stage != DnsRolloutStage::Active {
            return DnsReorgOutcome::GateInactive; // gate dormant outside the Active stage
        }
        let confirmed = state.last_dns_confirmed_anchor;
        if confirmed == BlockHash::default() {
            return DnsReorgOutcome::GateInactive; // nothing confirmed yet
        }
        let includes = match self.reachability_service.try_is_chain_ancestor_of(confirmed, candidate) {
            Ok(v) => v,
            Err(_) => {
                debug!(
                    "DNS reorg gate: confirmed anchor {confirmed} has no reachability (behind the pruning point - attestation stalled?); gate is a no-op, subsumed by pruning-point finality"
                );
                true
            }
        };

        // Confirmed-anchor TTL (`dns_params.dns_veto_ttl_daa_score`). Measured on THIS node's own
        // canonical tip — never the candidate's, or an attacker could age out the anchor simply by
        // mining a branch far enough ahead and then presenting it (see the field doc). Evaluated
        // before the ancestor search so a node defending a support-less anchor pays nothing for it.
        //
        // The quantity is "my chain advanced this far with no new confirmation", i.e. exactly the
        // dead-branch wedge: the node keeps producing blocks while the branch's attestation flow is
        // gone, so `advance_dns_confirmation` carries the same anchor forward indefinitely. On a
        // chain that is still confirming, this distance stays at ~`lag + epoch` and never trips.
        let canonical_daa = self.headers_store.get_daa_score(prev_sink).unwrap_or_default();
        let anchor_age = canonical_daa.saturating_sub(state.last_dns_confirmed_anchor_daa_score);
        if !includes && dns_params.confirmed_anchor_is_stale(canonical_daa, state.last_dns_confirmed_anchor_daa_score) {
            warn!(
                "DNS reorg gate: confirmed anchor {confirmed} is STALE — this node's chain advanced {anchor_age} DAA past it (TTL {}) without a new confirmation, so the branch it protects has lost its validator support; releasing the veto for candidate {candidate}",
                dns_params.dns_veto_ttl_daa_score
            );
            return DnsReorgOutcome::ConfirmedAnchorStale;
        }

        // The heavy two-dimensional inputs (common ancestor + per-branch Work/Stake walks)
        // are computed ONLY when the candidate abandons the confirmed prefix AND the
        // network runs the mainnet dominance rule. HardCheckpoint and the includes-anchor
        // case ignore Work/Stake, so they skip the walks entirely.
        //
        // `stake_evaluated` records whether the StakeScore walks actually ran (§5-5 skips them
        // whenever the work dimension already settles the verdict), so the log below reports
        // "not evaluated" instead of printing the placeholder zeros as if they were measurements.
        let mut stake_evaluated = false;
        let inputs = if dns_params.reorg_mode == DnsReorgMode::TwoDimensionalDominance && !includes {
            // Selected-chain common ancestor I. Beyond the reorg horizon the DNS gate ABSTAINS
            // (incident 2026-08-03 §8): it has no Work/Stake deltas to judge on, and the base
            // ledger already refuses reorgs below `virtual_finality_point` in `sink_search`
            // (see `candidate_at_or_above_finality`) plus everything under the pruning point.
            //
            // This used to `return false` (unconditional reject). That is what made a partition
            // PERMANENT rather than merely long: once two branches diverged by more than
            // `max_reorg_horizon_blocks` (300) the gate stopped evaluating anything and rejected
            // outright, so no amount of subsequent work on the other branch could ever be
            // considered. On testnet-22 the branches were ~120k blocks apart by the time the
            // split was noticed — far past the horizon — so the deadlock was already sealed.
            // Abstaining hands the decision to GHOSTDAG + the real finality guard instead of
            // adding a second, unreleasable veto on top of them.
            //
            // The horizon is `gate_horizon_blocks()` — the gate's OWN reach, no longer tied to the
            // economic `max_reorg_horizon_blocks` (300 blocks = 30 s at 10 BPS, which made DNS
            // finality a 30-second property). The search is O(log horizon), so the reach is a
            // policy choice rather than a cost ceiling; see `chain_common_ancestor_within`.
            let candidate_work = self.ghostdag_store.get_blue_work(candidate).unwrap_or_default();
            let canonical_work = self.ghostdag_store.get_blue_work(prev_sink).unwrap_or_default();
            // Exact pre-check, before any ancestor search: blue work is cumulative, so for ANY
            // common ancestor `I`, `candidate_after ≤ canonical_after ⟺ candidate_work ≤
            // canonical_work`. A candidate that does not out-work canonical therefore fails
            // `work_ok` and cannot clear the override (multiplier ≥ 1) — `DominanceViolation` is
            // certain. This is the case that dominates a wedge, because every rejection pushes the
            // candidate's parents back onto the heap and those descend below the sink's work.
            if candidate_work <= canonical_work {
                debug!(
                    "DNS reorg gate: candidate {candidate} does not out-work sink {prev_sink} ({candidate_work} <= {canonical_work}); dominance violation without an ancestor search"
                );
                return DnsReorgOutcome::DominanceViolation;
            }
            let Some(ancestor) = self.chain_common_ancestor_within(candidate, prev_sink, dns_params.gate_horizon_blocks()) else {
                debug!(
                    "DNS reorg gate: candidate {candidate} vs sink {prev_sink} common ancestor is beyond the gate horizon ({} blocks); gate abstains (base-ledger finality point still applies)",
                    dns_params.gate_horizon_blocks()
                );
                return DnsReorgOutcome::GateInactive;
            };
            let ancestor_work = self.ghostdag_store.get_blue_work(ancestor).unwrap_or_default();

            // §5-5 cost mitigation (incident 2026-07-19). The two `stake_score_since_ancestor`
            // calls below are each an O(divergence) chain walk (`collect_stake_contributions_v2`
            // from tip back to the ancestor), plus a full `stake_bonds_store` scan — and
            // `sink_search` runs this gate for EVERY candidate of a heavier branch. That is the
            // amplification the report measured as ever-lengthening resolve times.
            //
            // The WORK dimension alone settles the two cases that dominate a wedge, so decide it
            // first from cheap blue_work lookups and skip the walks when they provably cannot
            // change the outcome:
            //   * the work override already accepts        ⇒ stake is irrelevant;
            //   * `work_ok` is false ⇒ the rule needs BOTH ⇒ certain `DominanceViolation`.
            // Both shortcuts are EXACT, not heuristic: neither branch of `check_dns_reorg_rule`
            // consults the stake values in these cases, so feeding it zeros yields the identical
            // verdict. Only the genuinely contested case (out-works canonical but not by the
            // override ratio) still pays for the walks.
            let candidate_after = candidate_work.saturating_sub(ancestor_work);
            let canonical_after = canonical_work.saturating_sub(ancestor_work);
            let work_ok = candidate_after > canonical_after.saturating_add(dns_params.emergency_work_margin);
            let override_ok = dns_params.emergency_work_override_multiplier > 0 && {
                let (bound, overflowed) = canonical_after.overflowing_mul_u64(dns_params.emergency_work_override_multiplier as u64);
                !overflowed && candidate_after > bound
            };
            stake_evaluated = work_ok && !override_ok;

            let (candidate_stake, canonical_stake) = if stake_evaluated {
                let net_id_hash = self.genesis.hash;
                let net_id = net_id_hash.as_byte_slice();
                // Per-branch bond sets (safety — each branch under its OWN view; see doc comment).
                let candidate_bonds = candidate_bond_view.records();
                let canonical_bonds: Vec<StakeBondRecord> =
                    self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
                (
                    self.stake_score_since_ancestor(candidate, ancestor, &candidate_bonds, dns_params, net_id),
                    self.stake_score_since_ancestor(prev_sink, ancestor, &canonical_bonds, dns_params, net_id),
                )
            } else {
                (StakeScore(0), StakeScore(0))
            };

            reorg_inputs_since_common_ancestor(
                state.rollout_stage,
                dns_params.reorg_mode,
                includes,
                candidate_work,
                canonical_work,
                ancestor_work,
                candidate_stake,
                canonical_stake,
                dns_params.emergency_work_margin,
                dns_params.emergency_stake_margin,
                dns_params.emergency_work_override_multiplier,
                anchor_age,
                dns_params.dns_veto_ttl_daa_score,
            )
        } else {
            // HardCheckpoint, or candidate keeps the confirmed anchor: Work/Stake unused.
            reorg_inputs_since_common_ancestor(
                state.rollout_stage,
                dns_params.reorg_mode,
                includes,
                BlueWorkType::from_u64(0),
                BlueWorkType::from_u64(0),
                BlueWorkType::from_u64(0),
                StakeScore(0),
                StakeScore(0),
                dns_params.emergency_work_margin,
                dns_params.emergency_stake_margin,
                dns_params.emergency_work_override_multiplier,
                anchor_age,
                dns_params.dns_veto_ttl_daa_score,
            )
        };
        let outcome = check_dns_reorg_rule(&inputs);
        if outcome == DnsReorgOutcome::WorkDominanceOverride {
            // Loud on purpose: the stake veto was deliberately released. Either this node is the
            // minority side of a partition and is (correctly) rejoining the work-dominant chain,
            // or an adversary is sustaining >N/(N+1) of total hashpower across the whole fork.
            // Both are operationally significant and must be visible in the log.
            warn!(
                "DNS reorg gate: partition-liveness override — candidate {candidate} out-works sink {prev_sink} by >{}x since the common ancestor (candidate_work_after={}, canonical_work_after={}); accepting despite an unsatisfied stake dimension (stake: {})",
                dns_params.emergency_work_override_multiplier,
                inputs.candidate_work_after,
                inputs.canonical_work_after,
                if stake_evaluated {
                    format!("candidate={}, canonical={}", inputs.candidate_stake_after.0, inputs.canonical_stake_after.0)
                } else {
                    "not evaluated (work dimension already decisive)".to_owned()
                },
            );
        }
        outcome
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

    /// Searches for the next valid sink block (SINK = Virtual selected parent). The search is performed
    /// in the inclusive past of `tips`.
    /// The provided `diff` is assumed to initially hold the UTXO diff of `prev_sink` from virtual.
    /// The function returns with `diff` being the diff of the new sink from previous virtual.
    /// In addition to the found sink the function also returns a queue of additional virtual
    /// parent candidates ordered in descending blue work order.
    /// Escape-from-a-dead-branch sink preference (see
    /// [`DnsParams::stake_preference_max_work_deficit_multiplier`]): when this node's OWN chain
    /// has demonstrably lost its DNS overlay (Active stage, confirmed anchor stale past the full
    /// veto TTL), screen `tips` for a branch whose overlay is demonstrably alive at
    /// confirmation grade within a bounded work deficit, and return the best qualifier.
    ///
    /// Screening only — the caller still runs UTXO validation and the reorg gate on the result.
    /// Every input is chain-derived and the tie-break is total (stake desc, work-after desc, hash
    /// asc), so all nodes evaluating the same DAG return the same tip; fork choice stays
    /// memoryless — the hysteresis the boundary needs lives in the verdict's asymmetric bars
    /// (own anchor dead past the FULL TTL vs candidate at FULL confirmation depth), not in state.
    ///
    /// The DNS coinbase-settlement context for MEMPOOL ADMISSION (see
    /// [`kaspa_consensus_core::dns_finality::coinbase_spend_settled`]): the current confirmed
    /// anchor from the node's DnsState singleton, plus the network's long-maturity fallback.
    ///
    /// Policy layer only. The singleton is "state as of this node's last virtual commit", which
    /// differs across nodes by their resolve batching — safe for admission (a policy disagreement
    /// keeps a tx out of a mempool, never out of a block's acceptance), disqualifying for
    /// validity. The consensus call site passes `None` and says why.
    pub(super) fn dns_coinbase_settlement(&self) -> Option<DnsCoinbaseSettlement> {
        let dns_params = self.dns_params.as_ref()?;
        let long_maturity_daa = dns_params.coinbase_settlement_long_maturity_daa;
        if long_maturity_daa == 0 {
            return None;
        }
        let confirmed_anchor_daa = self
            .dns_state_store
            .read()
            .get()
            .ok()
            .and_then(|s| (s.last_dns_confirmed_anchor != BlockHash::default()).then_some(s.last_dns_confirmed_anchor_daa_score));
        // The second clock (2026-09-23 heartbeat audit): past the fence, the long fallback also
        // needs `depth` PALW anchors settled since the coinbase's block. The anchors are the
        // `Final` attempt claims the PALW state holds at the tip — a lower bound past retirement,
        // which a 600-DAA maturity never reaches. Policy layer, like the rest of this record.
        let now_daa = self.lkg_virtual_state.load().daa_score;
        let depth = self.palw_settled_anchor_depth_at(now_daa);
        // The second clock's reading: the DAA of the `depth`-th most recent settled anchor, over
        // the whole chain (`u64::MAX` bounds nothing). The same one-place walk the panel floor
        // uses, so the two rules cannot come to disagree about what "settled" counts.
        let settled_anchor_floor_daa = depth.and_then(|depth| {
            let params = self.palw_state_params_v2.as_ref()?;
            let (_, state) = self.palw_state_v2_store.read().load_tip_cached(params).ok().flatten()?;
            kaspa_consensus_core::palw_panel_v2::palw_settled_anchor_floor_daa_v1(&state, u64::MAX, depth)
        });
        Some(DnsCoinbaseSettlement {
            long_maturity_daa,
            confirmed_anchor_daa,
            settled_anchor_armed: depth.is_some(),
            settled_anchor_floor_daa,
        })
    }

    /// Both stake walks run under the CANONICAL bond set: a bond created on the candidate branch
    /// above the ancestor is invisible here, which UNDER-counts the candidate — the conservative
    /// direction for a rule whose false positive is "sink moved onto the wrong branch". The cost
    /// note on the reorg gate (§5-5) does not apply: this path is entered only in the dead-anchor
    /// state, which the cheap staleness check settles first on every healthy resolve.
    fn dns_stake_preferred_tip(&self, prev_sink: BlockHash, tips: &[BlockHash], finality_point: BlockHash) -> Option<BlockHash> {
        let dns_params = self.dns_params.as_ref()?;
        let mult = dns_params.stake_preference_max_work_deficit_multiplier;
        if mult == 0 || tips.len() < 2 {
            return None;
        }
        let state = self.dns_state_store.read().get().ok()?;
        if state.rollout_stage != DnsRolloutStage::Active || state.last_dns_confirmed_anchor == BlockHash::default() {
            return None;
        }
        let canonical_daa = self.headers_store.get_daa_score(prev_sink).unwrap_or_default();
        if !dns_params.confirmed_anchor_is_stale(canonical_daa, state.last_dns_confirmed_anchor_daa_score) {
            // Own overlay is alive: symmetric-live contests stay work-decided. This is the cheap
            // early exit every healthy resolve takes.
            return None;
        }
        let own_anchor_age = canonical_daa.saturating_sub(state.last_dns_confirmed_anchor_daa_score);
        let canonical_work = self.ghostdag_store.get_blue_work(prev_sink).unwrap_or_default();
        let net_id_hash = self.genesis.hash;
        let net_id = net_id_hash.as_byte_slice();
        let canonical_bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();

        let mut best: Option<(u128, BlueWorkType, BlockHash)> = None;
        for &tip in tips {
            if tip == prev_sink || !self.reachability_service.try_is_chain_ancestor_of(finality_point, tip).unwrap_or(false) {
                continue;
            }
            let Some(ancestor) = self.chain_common_ancestor_within(tip, prev_sink, dns_params.gate_horizon_blocks()) else {
                // Deeper than the gate horizon: the preference abstains exactly where the veto does.
                continue;
            };
            let ancestor_work = self.ghostdag_store.get_blue_work(ancestor).unwrap_or_default();
            let candidate_work_after = self.ghostdag_store.get_blue_work(tip).unwrap_or_default().saturating_sub(ancestor_work);
            let canonical_work_after = canonical_work.saturating_sub(ancestor_work);
            // Work-deficit bound first: it needs two store reads, the stake walks need O(divergence).
            let (bound, overflowed) = candidate_work_after.overflowing_mul_u64(mult as u64);
            if !overflowed && bound <= canonical_work_after {
                continue;
            }
            let candidate_stake = self.stake_score_since_ancestor(tip, ancestor, &canonical_bonds, dns_params, net_id);
            let canonical_stake = self.stake_score_since_ancestor(prev_sink, ancestor, &canonical_bonds, dns_params, net_id);
            let qualifies = stake_preference_verdict(&StakePreferenceInputs {
                rollout_stage: state.rollout_stage,
                own_anchor_age_daa_score: own_anchor_age,
                veto_ttl_daa_score: dns_params.dns_veto_ttl_daa_score,
                multiplier: mult,
                candidate_work_after,
                canonical_work_after,
                candidate_stake_after: candidate_stake,
                canonical_stake_after: canonical_stake,
                emergency_stake_margin: dns_params.emergency_stake_margin,
                required_stake_depth: dns_params.required_stake_depth,
            });
            if qualifies {
                let better = match &best {
                    None => true,
                    Some((best_stake, best_work, best_hash)) => {
                        (candidate_stake.0, candidate_work_after) > (*best_stake, *best_work)
                            || ((candidate_stake.0, candidate_work_after) == (*best_stake, *best_work) && tip < *best_hash)
                    }
                };
                if better {
                    best = Some((candidate_stake.0, candidate_work_after, tip));
                }
            }
        }
        best.map(|(_, _, tip)| tip)
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
    ) -> (BlockHash, VecDeque<BlockHash>) {
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
            diff_point = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, preferred);
            if diff_point == preferred && self.dns_reorg_outcome(preferred, prev_sink, bond_view).is_accept() {
                info!(
                    "DNS stake preference: this chain's own overlay is dead (anchor stale past TTL) and tip {preferred} carries a confirmation-grade live overlay within the work-deficit bound; moving the sink there (previous sink {prev_sink}). Templates now extend the live-overlay branch."
                );
                return (preferred, VecDeque::new());
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
                let restored = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, prev_sink);
                assert_eq!(
                    restored, prev_sink,
                    "the previous sink {prev_sink} is not UTXO-valid — virtual cannot hold anywhere, which is a corrupted store rather than a rejected chain"
                );
                return (prev_sink, VecDeque::new());
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
                diff_point = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, candidate);
                if diff_point == candidate {
                    // This indicates that candidate has valid UTXO state and that `diff` represents its diff from virtual

                    // kaspa-pq Phase 10 (ADR-0009): the DNS finality reorg gate. Inert
                    // unless the overlay is configured and in the Active stage; it then
                    // rejects a candidate that would abandon a DNS-confirmed anchor. The
                    // rejection is soft — we fall through to push the candidate's parents
                    // and continue, converging on a DNS-valid sink (mirrors the
                    // invalid-UTXO handling below).
                    let dns_outcome = self.dns_reorg_outcome(candidate, prev_sink, bond_view);
                    if dns_outcome.is_accept() {
                        // Self-wedge signal (incident 2026-07-19 §2-1). The 7/19 freeze ran 3.5h
                        // with ZERO warnings: the gate refused every block of the network's chain
                        // while the node believed it was correctly repelling a deep reorg, and the
                        // only way to notice was comparing DAA against a peer by hand. Emitting
                        // this at the point virtual settles — once per search, not per candidate —
                        // keeps a healthy node quiet while making a wedged one impossible to miss.
                        if let Some((rejected, reason, rejected_work)) = gate_rejected {
                            warn!(
                                "DNS reorg gate: virtual settled on sink {} (blue_work {}) after refusing the heavier candidate {} (blue_work {}, reason {:?}). If this repeats on every resolve, this node is wedged off the network's chain — compare DAA against a peer.",
                                candidate,
                                self.ghostdag_store.get_blue_work(candidate).unwrap_or_default(),
                                rejected,
                                rejected_work,
                                reason,
                            );
                        }
                        // All blocks with lower blue work than filtering_root are:
                        // 1. not in its future (bcs blue work is monotonic),
                        // 2. will be removed eventually by the bounded merge check.
                        // Hence as an optimization we prefer removing such blocks in advance to allow valid tips to be considered.
                        let filtering_root = self.depth_store.merge_depth_root(candidate).unwrap();
                        let filtering_blue_work = self.ghostdag_store.get_blue_work(filtering_root).unwrap_or_default();
                        // The heap is GHOSTDAG's own order, so the sink is the maximum of every
                        // candidate left in it and `pick_virtual_parents`' assumption holds as is.
                        return (
                            candidate,
                            heap.into_sorted_iter().take_while(|s| s.blue_work >= filtering_blue_work).map(|s| s.hash).collect(),
                        );
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
        let max_block_parents = self.max_block_parents as usize;
        let mergeset_size_limit = self.mergeset_size_limit;
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
        let mut heartbeat_set: Vec<(u64, BlockHash)> = Vec::new();
        if track_heartbeats {
            let sp = self.headers_store.get_header(selected_parent).unwrap();
            if sp.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID {
                heartbeat_set.push((sp.blue_score, selected_parent));
            }
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
                        if !self.heartbeat_set_admissible(&mut combined) {
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

    /// ADR-0125: the round blocks among `tips`, where the lane is configured.
    pub(super) fn palw_round_tips(&self, tips: &[BlockHash]) -> Vec<BlockHash> {
        if self.palw_execution_lane.is_none() {
            return Vec::new();
        }
        tips.iter().copied().filter(|tip| self.ghostdag_manager.is_round_block(*tip)).collect()
    }

    /// ADR-0125: a round block stands for its anchor — its selected parent, never itself a round
    /// block — wherever the chain's own blocks are being chosen among.
    fn palw_project_round_block(&self, block: BlockHash) -> BlockHash {
        if self.palw_execution_lane.is_some() && self.ghostdag_manager.is_round_block(block) {
            self.ghostdag_store.get_selected_parent(block).unwrap_or(block)
        } else {
            block
        }
    }

    /// ADR-0125: [`Self::palw_project_round_block`] over a set, reduced back to an antichain (an
    /// anchor is often an ancestor of another candidate — the sink above all).
    fn palw_project_round_blocks(&self, blocks: Vec<BlockHash>) -> Vec<BlockHash> {
        if self.palw_execution_lane.is_none() {
            return blocks;
        }
        let mut projected: Vec<BlockHash> = Vec::with_capacity(blocks.len());
        for block in blocks {
            let block = self.palw_project_round_block(block);
            if !projected.contains(&block) {
                projected.push(block);
            }
        }
        let all = projected.clone();
        projected.retain(|block| {
            !all.iter()
                .any(|other| other != block && self.reachability_service.try_is_dag_ancestor_of(*block, *other).unwrap_or(false))
        });
        projected
    }

    /// **ADR-0125: merge the round lane into virtual.** Round tips are offered newest round first; a
    /// tip is taken when its anchor lies on virtual's selected chain (the header rule's anchor
    /// half), it is not already merged, it keeps the parents an antichain, and the mergeset it would
    /// make still passes both bounds — the chain's own `mergeset_size_limit` over its non-round
    /// members and the round lane's rule over its round members. So a template never builds a block
    /// the header stage refuses, and a tip that does not fit waits for a later template.
    fn palw_add_round_parents(
        &self,
        virtual_parents: &mut Vec<BlockHash>,
        selected_parent: BlockHash,
        mut round_tips: Vec<BlockHash>,
        max_block_parents: usize,
    ) {
        use kaspa_consensus_core::palw_execution_lane_v1::{PalwExecEnvelopeV1, palw_execution_mergeset_rule_v1};
        let Some(lane) = self.palw_execution_lane else {
            return;
        };
        let envelope_of = |block: BlockHash| {
            self.headers_store.get_header(block).ok().and_then(|h| PalwExecEnvelopeV1::decode(&h.palw_commitment).ok())
        };
        round_tips.sort_by_key(|tip| std::cmp::Reverse((envelope_of(*tip).map(|e| e.round).unwrap_or(0), *tip)));
        // §7.2: the width in force at the selected parent. Virtual's own DAA score is at least that
        // and widths only grow, so this bound is never wider than the one virtual's block is judged by.
        let width = lane.width_at_daa(self.headers_store.get_daa_score(selected_parent).unwrap_or_default());
        for tip in round_tips {
            if virtual_parents.len() >= max_block_parents {
                break;
            }
            let Ok(anchor) = self.ghostdag_store.get_selected_parent(tip) else {
                continue;
            };
            if !self.reachability_service.try_is_chain_ancestor_of(anchor, selected_parent).unwrap_or(false) {
                continue;
            }
            if virtual_parents.iter().any(|parent| {
                self.reachability_service.try_is_dag_ancestor_of(tip, *parent).unwrap_or(true)
                    || self.reachability_service.try_is_dag_ancestor_of(*parent, tip).unwrap_or(true)
            }) {
                continue;
            }
            let mut tentative = virtual_parents.clone();
            tentative.push(tip);
            let ghostdag = self.ghostdag_manager.ghostdag(&tentative);
            let mut members = Vec::new();
            let mut malformed = false;
            for red in ghostdag.mergeset_reds.iter().copied() {
                if !self.ghostdag_manager.is_round_block(red) {
                    continue;
                }
                match envelope_of(red) {
                    Some(envelope) => members.push((envelope.round, envelope.permit_index, envelope.bond)),
                    None => malformed = true,
                }
            }
            if malformed || ghostdag.mergeset_size() as u64 - members.len() as u64 > self.mergeset_size_limit {
                continue;
            }
            if palw_execution_mergeset_rule_v1(None, &members, width, lane.max_per_mergeset).is_err() {
                continue;
            }
            virtual_parents.push(tip);
        }
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
        let mut heartbeat_members: Vec<(u64, BlockHash)> = Vec::new();
        let mut note_heartbeat = |hash: BlockHash| {
            if !track_heartbeats {
                return;
            }
            let header = self.headers_store.get_header(hash).unwrap();
            if header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID {
                heartbeat_members.push((header.blue_score, hash));
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
    /// is never ancestor-related and fails). Sorts the given buffer in place.
    fn heartbeat_set_admissible(&self, set: &mut [(u64, BlockHash)]) -> bool {
        if set.len() as u64 <= kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET {
            return true;
        }
        set.sort_unstable_by_key(|(blue_score, _)| *blue_score);
        set.windows(2).all(|pair| {
            let ((_, older), (_, newer)) = (pair[0], pair[1]);
            self.reachability_service.is_dag_ancestor_of(older, newer)
        })
    }

    fn remove_bounded_merge_breaking_parents(
        &self,
        mut virtual_parents: Vec<BlockHash>,
        current_pruning_point: BlockHash,
    ) -> (Vec<BlockHash>, GhostdagData) {
        let mut ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        let merge_depth_root = self.depth_manager.calc_merge_depth_root(&ghostdag_data, current_pruning_point);
        let mut kosherizing_blues: Option<Vec<BlockHash>> = None;
        let mut bad_reds = Vec::new();

        //
        // Note that the code below optimizes for the usual case where there are no merge-bound-violating blocks.
        //

        // Find red blocks violating the merge bound and which are not kosherized by any blue
        for red in ghostdag_data.mergeset_reds.iter().copied() {
            if self.reachability_service.is_dag_ancestor_of(merge_depth_root, red) {
                continue;
            }
            // Lazy load the kosherizing blocks since this case is extremely rare
            if kosherizing_blues.is_none() {
                kosherizing_blues = Some(self.depth_manager.kosherizing_blues(&ghostdag_data, merge_depth_root).collect());
            }
            if !self.reachability_service.is_dag_ancestor_of_any(red, &mut kosherizing_blues.as_ref().unwrap().iter().copied()) {
                bad_reds.push(red);
            }
        }

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
        // (`palw_v2_locked_bonds`), read at the tip the mempool judges against.
        if let Some(locked) = self.palw_mempool_locked_bonds(virtual_daa_score)
            && let Some(outpoint) = first_locked_input(&mutable_tx.tx, &locked)
        {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::SpendsNonReleasableBond(outpoint));
        }
        self.validate_mempool_transaction_in_utxo_context(mutable_tx, virtual_utxo_view, virtual_daa_score, args)?;
        Ok(())
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

    fn validate_block_template_transactions_in_parallel<V: UtxoView + Sync>(
        &self,
        txs: &[Transaction],
        virtual_state: &VirtualState,
        utxo_view: &V,
    ) -> Vec<TxResult<u64>> {
        self.thread_pool
            .install(|| txs.par_iter().map(|tx| self.validate_block_template_transaction(tx, virtual_state, &utxo_view)).collect())
    }

    fn validate_block_template_transaction(
        &self,
        tx: &Transaction,
        virtual_state: &VirtualState,
        utxo_view: &impl UtxoView,
    ) -> TxResult<u64> {
        // No need to validate the transaction in isolation since we rely on the mining manager to submit transactions
        // which were previously validated through `validate_mempool_transaction_and_populate`, hence we only perform
        // in-context validations
        self.transaction_validator.validate_tx_in_header_context_with_args(
            tx,
            virtual_state.daa_score,
            virtual_state.past_median_time,
        )?;
        let ValidatedTransaction { calculated_fee, .. } =
            // `None`: mempool/template single-tx context, not mergeset acceptance (bond spend-gate inert here).
            self.validate_transaction_in_utxo_context(tx, utxo_view, virtual_state.daa_score, TxValidationFlags::Full, None)?;
        Ok(calculated_fee)
    }

    fn latest_ready_epoch_for_template_snapshot(&self, virtual_state: &VirtualState) -> Option<u64> {
        let dns_params = self.dns_params.as_ref()?;
        ready_epoch_from_tip_blue_score(
            virtual_state.ghostdag_data.blue_score,
            dns_params.attestation_epoch_length_blue_score,
            dns_params.attestation_lag_blue_score,
        )
    }

    pub fn build_block_template(
        &self,
        miner_data: MinerData,
        tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
        // kaspa-pq EVM Lane v0.4 (§15 step 6 / §16): the node's own payload
        // candidates + declared EVM coinbase. Assembled into the template
        // payload by `evm_template_fields`; ignored pre-activation.
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        self.build_block_template_with_selector_provider(miner_data, build_mode, evm_template_data, move |_| tx_selector)
    }

    pub fn build_block_template_with_selector_factory(
        &self,
        miner_data: MinerData,
        tx_selector_factory: &dyn TemplateTransactionSelectorFactory,
        build_mode: TemplateBuildMode,
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        self.build_block_template_with_selector_provider(miner_data, build_mode, evm_template_data, |latest_ready_epoch| {
            tx_selector_factory.build_selector(latest_ready_epoch)
        })
    }

    /// **ADR-0060 Decision 1: adapt a standard template into the heartbeat lane's shape.**
    ///
    /// The mining manager's template is the bonded attempt lane's — algo-6, the global bits, a
    /// full-subsidy coinbase, a `palw_commitment` the caller was going to fill. A heartbeat
    /// block differs in exactly the lane facts, and in nothing else: the algo id, the lane's own
    /// bits (the same window arithmetic validation runs), an EMPTY commitment (there is no
    /// attempt — that is the lane), and a coinbase payload declaring ZERO subsidy (Decision 1.4;
    /// the merkle root moves with it). The transactions, parents, pruning point, reward
    /// outputs, EVM fields and the committed parent state root all stand — a heartbeat block is
    /// an ordinary block in every other respect, which is the point: bond registrations ride it.
    ///
    /// Returns the template plus the EARLIEST timestamp (ms) the slot rule admits. When that is
    /// in the future the caller must wait and rebuild — a block ground early is refused with
    /// `HeartbeatTooEarly` by every validating node. The lane facts are computed from the
    /// CURRENT virtual POV; if the virtual moves between this call and submission the block is
    /// refused and the caller simply rebuilds, the same staleness contract every template has.
    pub fn heartbeat_adapt_block_template(&self, mut template: BlockTemplate) -> Result<(BlockTemplate, u64), RuleError> {
        use kaspa_consensus_core::palw_heartbeat_v1 as hb;
        let virtual_state = self.virtual_stores.read().state.get().unwrap();
        // **Refuse when the lane is not open, rather than hand back a block every peer rejects.**
        //
        // This is a public API. The daemon gates the miner on the same fence, but a caller that
        // did not would otherwise get a well-formed algo-8 template, grind it, submit it, and be
        // told `UnknownPowAlgoId` — work burned against a rule this node could answer before the
        // first hash. Answering with the validator's own error keeps the two sides saying one
        // thing.
        if !self.palw_heartbeat_lane.is_some_and(|fence| fence.is_active(virtual_state.daa_score)) {
            return Err(RuleError::UnknownPowAlgoId(hb::PALW_HEARTBEAT_ALGO_ID));
        }
        // **The SAME one-block-deep question validation asks** (ADR-0066 Decision 2): the selected
        // parent's timestamp and lane, and nothing else. Construction and validation must read one
        // answer or the template refuses itself — and the old shared answer was a chain walk that
        // terminated on a node-local fact, so a pruned node and an archival one built templates
        // the other rejected.
        let parent = self
            .headers_store
            .get_header(virtual_state.ghostdag_data.selected_parent)
            .map_err(|_| RuleError::MissingParents(vec![virtual_state.ghostdag_data.selected_parent]))?;
        // The slot: at or after the selected parent's timestamp plus the interval its lane sets —
        // through the SAME function `pre_pow_validation` calls, with the same two fence answers.
        // ADR-0138 §3c changed that interval, and a template built on the old one would be refused
        // by the node that built it: past the anchor clock validation grants the recovery cadence
        // where the parent paces no clock, while the old call still stamped the nominal hour, which
        // the future-drift rule then rejects outright.
        // **ADR-0142: past `palw_clock_cursor` there is no slot to wait for.** The rule retires
        // with the validator's — a beat may be minted whenever its producer can pay for it, and it
        // earns the chain a DAA only where the cursor says a slot is open. Stamping a future
        // timestamp here would be the template refusing itself, which is the drift ADR-0066
        // Decision 2 named and ADR-0138 §3c reintroduced.
        let clock_cursor_governs = self.palw_clock_cursor.is_some_and(|fence| fence.is_active(virtual_state.daa_score));
        let anchor_clock_active = self.palw_anchor_clock.is_some_and(|fence| fence.is_active(virtual_state.daa_score));
        let parent_advances_daa = crate::processes::difficulty::palw_lane_advances_daa_v1(
            parent.pow_algo_id,
            parent.daa_score,
            self.palw_anchor_clock,
            self.palw_single_lottery,
            self.palw_receipt_rows_unpriced,
        );
        let earliest = if clock_cursor_governs {
            // Past the cursor there is nothing to wait for: the template stands as built, and the
            // beat earns a DAA only where the cursor says a slot is open.
            template.block.header.timestamp
        } else {
            match hb::check_heartbeat_slot_v2(
                parent.timestamp,
                parent.pow_algo_id,
                anchor_clock_active,
                parent_advances_daa,
                template.block.header.timestamp,
            ) {
                Ok(()) => template.block.header.timestamp,
                Err(early) => early.last_heartbeat_timestamp.saturating_add(early.interval_ms),
            }
        };
        template.block.header.timestamp = template.block.header.timestamp.max(earliest);
        template.block.header.pow_algo_id = hb::PALW_HEARTBEAT_ALGO_ID;
        template.block.header.palw_commitment = vec![];
        // **`bits` are left exactly as the global calculation set them** (ADR-0066 Decision 1).
        // This line used to overwrite them with the lane's own retarget, which is the write that
        // fed the lane's price into the global difficulty window and made a heartbeat-only chain
        // unrecoverable. The lane's price is a constant in `StateLayer0::new` now, and the header
        // this template produces is an ordinary difficulty row.
        // Decision 1.4: the declared subsidy is what descendants read into the reward fan-out —
        // zero is what makes the lane fee-only and the ADR-0059 supply exact.
        let blue_score = template.block.header.blue_score;
        let coinbase = &mut template.block.transactions[0];
        let zeroed = kaspa_consensus_core::coinbase::CoinbaseData { blue_score, subsidy: 0, miner_data: template.miner_data.clone() };
        coinbase.payload =
            self.coinbase_manager.serialize_coinbase_payload(&zeroed).expect("a payload the manager just built reserializes");
        coinbase.finalize();
        template.block.header.hash_merkle_root = calc_hash_merkle_root(template.block.transactions.iter());
        template.block.header.finalize();
        Ok((template, earliest))
    }

    /// **ADR-0125: the permits of one round, as the selected-parent snapshot grants them to a round
    /// block built now.** The anchor such a block hangs from is the sink's selected parent — the chain
    /// block the next chain block can merge it beside — so the schedule is that anchor's span's.
    pub fn palw_round_view_v1(&self, round: u64) -> Option<kaspa_consensus_core::palw_execution_lane_v1::PalwExecRoundViewV1> {
        self.palw_round_lane_status_v1(round).map(|status| status.view)
    }

    /// **ADR-0125 §7.4: the round's view from the selected-parent PALW snapshot**, never the store tip.
    ///
    /// The schedule reported is the one in force for the view's span — the state's `round_schedule`,
    /// written at the span's first chain block. A pending snapshot (ADR-0130: the next span's
    /// participants, waiting for their seed) is not a schedule and is never reported as one, and a
    /// span whose snapshot found no seed anchor answers with no schedule and no permits.
    pub fn palw_round_lane_status_v1(&self, round: u64) -> Option<kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneStatusV1> {
        let lane = self.palw_execution_lane?;
        let virtual_state = self.virtual_stores.read().state.get().unwrap();
        if !lane.activation.is_active(virtual_state.daa_score) {
            return None;
        }
        let sink = virtual_state.ghostdag_data.selected_parent;
        let anchor = self
            .ghostdag_store
            .get_selected_parent(sink)
            .ok()
            .filter(|anchor| !kaspa_consensus_core::blockhash::BlockHashExtensions::is_origin(anchor))?;
        let span_daa = lane.schedule_span_daa_at(virtual_state.daa_score);
        let span = kaspa_consensus_core::palw_execution_lane_v1::palw_execution_span_v1(
            self.headers_store.get_daa_score(anchor).ok()?,
            span_daa,
        );
        let (_at, state) = self.palw_v2_state_at(sink)?;
        let width = lane.width_of_span_len(span, span_daa);
        // The verdict's own rule (route-matrix #2), read at virtual's score — the score the next chain
        // block, which merges a round block built now, is judged at.
        let tickets_only = self.palw_round_permits_are_tickets_at(virtual_state.daa_score);
        let permits = state
            .round_schedule(span)
            .map(|schedule| {
                kaspa_consensus_core::palw_execution_lane_v1::palw_execution_permits_v2(schedule, round, width, tickets_only)
            })
            .unwrap_or_default();
        let used = (0..width).filter(|index| state.round_permit_used(span, round, *index)).collect();
        let (finals_span, finals) = state.round_finals();
        Some(kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneStatusV1 {
            view: kaspa_consensus_core::palw_execution_lane_v1::PalwExecRoundViewV1 {
                round,
                span,
                width,
                genesis_timestamp_ms: self.genesis.timestamp,
                permits,
                used,
            },
            schedule: state.round_schedule(span).cloned(),
            accepted_in_span: state.round_permits_accepted(span),
            finals_span,
            finals: finals.len() as u64,
            tickets_only,
        })
    }

    /// **ADR-0125: re-shape a standard template into a round block.**
    ///
    /// The mining manager's template carries transactions selected against the virtual UTXO state;
    /// a round block keeps them and changes everything the lane decides:
    ///
    /// * **parents** — the round tips whose round is older than this one and whose anchor lies on
    ///   the sink's selected chain up to the sink's selected parent, newest round first, while the
    ///   mergeset stays inside the lane's rule and the parent limit; plus that anchor itself when no
    ///   chosen tip already hangs from it (naming it beside such a tip would name an ancestor of a
    ///   parent). With no tip to extend, the anchor alone;
    /// * **GHOSTDAG, DAA score, bits, median time and pruning point** — recomputed for those
    ///   parents, exactly as the header stage will;
    /// * **timestamp** — the start of the round, or one past the median time if that is later; a
    ///   round the median time has already passed is refused;
    /// * **algo 10**, an empty `palw_commitment` (the caller signs after solving), zero state and
    ///   overlay roots, an empty EVM payload, and a coinbase declaring zero subsidy to `payout` with
    ///   no outputs — a round block is never a chain block, so nothing reads the roots or outputs.
    pub fn round_adapt_block_template(
        &self,
        mut template: BlockTemplate,
        round: u64,
        payout: kaspa_consensus_core::tx::ScriptPublicKey,
    ) -> Result<BlockTemplate, RuleError> {
        use kaspa_consensus_core::palw_execution_lane_v1::{PALW_EXEC_ROUND_MS, PalwExecEnvelopeV1, palw_execution_mergeset_rule_v1};
        use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1;
        let virtual_state = self.virtual_stores.read().state.get().unwrap();
        let Some(lane) = self.palw_execution_lane.filter(|lane| lane.activation.is_active(virtual_state.daa_score)) else {
            return Err(RuleError::UnknownPowAlgoId(POW_ALGO_ID_PALW_ROUND_V1));
        };
        let sink = virtual_state.ghostdag_data.selected_parent;
        let anchor = self
            .ghostdag_store
            .get_selected_parent(sink)
            .ok()
            .filter(|anchor| !kaspa_consensus_core::blockhash::BlockHashExtensions::is_origin(anchor))
            .ok_or_else(|| RuleError::BadRoundLaneParents("the sink has no chain block beneath it to anchor a round block".into()))?;
        // §7.2: the width in force at the anchor. The round block's own DAA score is at least the
        // anchor's and widths only grow, so a mergeset bounded by this passes the header's bound.
        let round_width = lane.width_at_daa(self.headers_store.get_daa_score(anchor).unwrap_or_default());
        let _prune_guard = self.pruning_lock.blocking_read();
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let envelope_of = |block: BlockHash| {
            self.headers_store.get_header(block).ok().and_then(|h| PalwExecEnvelopeV1::decode(&h.palw_commitment).ok())
        };
        let mut round_tips: Vec<(u64, BlockHash)> =
            self.body_tips_store
                .read()
                .get()
                .unwrap()
                .read()
                .iter()
                .copied()
                .filter(|tip| self.ghostdag_manager.is_round_block(*tip))
                .filter_map(|tip| envelope_of(tip).map(|envelope| (envelope.round, tip)))
                .filter(|(tip_round, _)| *tip_round < round)
                .filter(|(_, tip)| {
                    self.ghostdag_store.get_selected_parent(*tip).ok().is_some_and(|tip_anchor| {
                        self.reachability_service.try_is_chain_ancestor_of(tip_anchor, anchor).unwrap_or(false)
                    })
                })
                .collect();
        round_tips.sort_by_key(|(tip_round, tip)| std::cmp::Reverse((*tip_round, *tip)));
        let max_block_parents = self.max_block_parents as usize;
        let mut parents: Vec<BlockHash> = Vec::new();
        let parents_with_anchor = |chosen: &[BlockHash]| -> Vec<BlockHash> {
            let anchor_is_behind_a_tip =
                chosen.iter().any(|tip| self.reachability_service.try_is_dag_ancestor_of(anchor, *tip).unwrap_or(true));
            let mut all = chosen.to_vec();
            if !anchor_is_behind_a_tip {
                all.push(anchor);
            }
            all
        };
        for (_, tip) in round_tips {
            if parents_with_anchor(&parents).len() >= max_block_parents {
                break;
            }
            if parents.iter().any(|chosen| {
                self.reachability_service.try_is_dag_ancestor_of(tip, *chosen).unwrap_or(true)
                    || self.reachability_service.try_is_dag_ancestor_of(*chosen, tip).unwrap_or(true)
            }) {
                continue;
            }
            let mut tentative = parents.clone();
            tentative.push(tip);
            let tentative = parents_with_anchor(&tentative);
            let ghostdag = self.ghostdag_manager.ghostdag(&tentative);
            if ghostdag.selected_parent != anchor {
                continue;
            }
            let members: Option<Vec<_>> = ghostdag
                .mergeset_reds
                .iter()
                .filter(|red| self.ghostdag_manager.is_round_block(**red))
                .map(|red| envelope_of(*red).map(|envelope| (envelope.round, envelope.permit_index, envelope.bond)))
                .collect();
            let Some(members) = members else { continue };
            if ghostdag.mergeset_size() as u64 - members.len() as u64 > self.mergeset_size_limit
                || palw_execution_mergeset_rule_v1(Some(round), &members, round_width, lane.max_per_mergeset).is_err()
            {
                continue;
            }
            parents.push(tip);
        }
        let parents = parents_with_anchor(&parents);
        let ghostdag = self.ghostdag_manager.ghostdag(&parents);
        if ghostdag.selected_parent != anchor {
            return Err(RuleError::BadRoundLaneParents(format!(
                "the round block's selected parent resolved to {} rather than its anchor {anchor}",
                ghostdag.selected_parent
            )));
        }
        let daa_window = self.window_manager.block_daa_window(&ghostdag)?;
        if !lane.activation.is_active(daa_window.daa_score) {
            return Err(RuleError::UnknownPowAlgoId(POW_ALGO_ID_PALW_ROUND_V1));
        }
        let bits = self.window_manager.calculate_difficulty_bits(&ghostdag, &daa_window);
        let (past_median_time, _) = self.window_manager.calc_past_median_time(&ghostdag)?;
        let round_start = self.genesis.timestamp.saturating_add(round.saturating_mul(PALW_EXEC_ROUND_MS));
        let timestamp = round_start.max(past_median_time + 1);
        if timestamp >= round_start.saturating_add(PALW_EXEC_ROUND_MS) {
            return Err(RuleError::TimeTooOld(round_start, past_median_time));
        }
        let header_pruning_point = self.pruning_point_manager.expected_header_pruning_point(ghostdag.to_compact()).pruning_point;
        let parents_by_level = self.parents_manager.calc_block_parents(pruning_point, &parents);
        let miner_data = MinerData::new(payout, template.miner_data.extra_data.clone());
        let payload = self
            .coinbase_manager
            .serialize_coinbase_payload(&kaspa_consensus_core::coinbase::CoinbaseData {
                blue_score: ghostdag.blue_score,
                subsidy: 0,
                miner_data: miner_data.clone(),
            })
            .map_err(RuleError::BadCoinbasePayload)?;
        let mut transactions = std::mem::take(&mut template.block.transactions);
        let mut coinbase = transactions.remove(0);
        coinbase.outputs.clear();
        coinbase.payload = payload;
        coinbase.finalize();
        transactions.insert(0, coinbase);
        let version = if daa_window.daa_score >= self.evm_activation_daa_score {
            kaspa_consensus_core::constants::EVM_HEADER_VERSION
        } else {
            BLOCK_VERSION
        };
        let evm_payload = kaspa_consensus_core::evm::EvmExecutionPayload::default();
        let mut header = Header::new_finalized(
            version,
            parents_by_level,
            calc_hash_merkle_root(transactions.iter()),
            Default::default(),
            Default::default(),
            timestamp,
            bits,
            0,
            POW_ALGO_ID_PALW_ROUND_V1,
            daa_window.daa_score,
            ghostdag.blue_work,
            ghostdag.blue_score,
            header_pruning_point,
        );
        if version >= kaspa_consensus_core::constants::EVM_HEADER_VERSION {
            header = header.with_evm_payload_hash(evm_payload.payload_hash());
        }
        let calculated_fees =
            if template.calculated_fees.len() + 1 == transactions.len() { template.calculated_fees } else { Vec::new() };
        let mut block = MutableBlock::new(header, transactions);
        block.evm_payload = evm_payload;
        Ok(BlockTemplate::new(
            block,
            miner_data,
            false,
            Vec::new(),
            self.headers_store.get_timestamp(anchor).unwrap_or_default(),
            self.headers_store.get_daa_score(anchor).unwrap_or_default(),
            anchor,
            calculated_fees,
            Vec::new(),
            Vec::new(),
        ))
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
    pub fn heartbeat_yield_hint(&self) -> kaspa_consensus_core::palw_heartbeat_v1::HeartbeatYieldHintV1 {
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
        hb::heartbeat_yield_hint_v2(
            selected_parent.pow_algo_id,
            anchor_clock_active,
            parent_advances_daa,
            attempt_advances_daa,
            merged,
        )
    }

    fn build_block_template_with_selector_provider<F>(
        &self,
        miner_data: MinerData,
        build_mode: TemplateBuildMode,
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
        tx_selector_provider: F,
    ) -> Result<BlockTemplate, RuleError>
    where
        F: FnOnce(Option<u64>) -> Box<dyn TemplateTransactionSelector>,
    {
        //
        // TODO (relaxed): additional tests
        //

        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let virtual_utxo_view = &virtual_read.utxo_set;

        // kaspa-pq DNS-finality (E3/§6.2): capture the template's as-of-selected-parent
        // bond view INSIDE the same read lock as `virtual_state`, BEFORE the selection
        // loop, so each selected `StakeAttestationShard` tx can be classified for
        // §B.4 eligibility AT SELECTION TIME (instead of the old late `retain` that ran
        // after selection/validation and could not refill). The template extends the
        // current tip, so the bond set as-of its selected parent is the `StakeBonds`
        // store snapshot (= state at the sink) — `initial_active_bond_view`. Reused
        // below for the reward fan-out + overlay commitment (one coherent generation).
        // Inert (every tx `KeepNonShard`) below the activation gate, so non-overlay nets
        // are byte-identical to before.
        let template_bond_view = self.initial_active_bond_view();
        let latest_ready_epoch = self.latest_ready_epoch_for_template_snapshot(&virtual_state);
        let mut tx_selector = tx_selector_provider(latest_ready_epoch);
        let mut txs = tx_selector.select_transactions();
        let mut calculated_fees = Vec::with_capacity(txs.len());
        // kaspa-pq DNS-finality (§6.5): per-reason drop counters for diagnostics.
        let mut shards_seen = 0usize;
        let mut shards_kept = 0usize;
        let mut dropped_bond_inactive = 0usize;
        let mut dropped_id_mismatch = 0usize;
        let mut dropped_bad_sig = 0usize;
        let mut dropped_malformed = 0usize;
        // kaspa-pq DNS-finality (audit v24 H-5): the dropped shards (id + hygiene kind)
        // returned to the mining manager so it can evict terminal drops and quarantine
        // transient ones — otherwise a dropped shard stays in the mempool and is
        // re-selected into every subsequent template forever (the live-testnet stall).
        let mut dropped_attestation_shards: Vec<kaspa_consensus_core::block::AttestationTemplateDrop> = Vec::new();
        // Classify one selected tx for the template. `true` ⇒ keep (push to txs +
        // calculated_fees in lockstep); `false` ⇒ reject back to the selector (it will
        // refill from the next candidate) and DO NOT push, so `txs` and `calculated_fees`
        // stay 1:1. A `Drop` is counted by reason. A `KeepNonShard`/`KeepEligible` is kept.
        let classify_keep = |this: &Self,
                             tx: &Transaction,
                             shards_seen: &mut usize,
                             shards_kept: &mut usize,
                             dropped_bond_inactive: &mut usize,
                             dropped_id_mismatch: &mut usize,
                             dropped_bad_sig: &mut usize,
                             dropped_malformed: &mut usize,
                             dropped_attestation_shards: &mut Vec<kaspa_consensus_core::block::AttestationTemplateDrop>|
         -> bool {
            use crate::pipeline::virtual_processor::utxo_validation::{AttestationDropReason, AttestationShardDecision};
            match this.classify_attestation_shard_for_template(tx, &template_bond_view, virtual_state.daa_score) {
                AttestationShardDecision::KeepNonShard => true,
                AttestationShardDecision::KeepEligible { .. } => {
                    *shards_seen += 1;
                    *shards_kept += 1;
                    true
                }
                AttestationShardDecision::Drop { reason, bond, epoch } => {
                    *shards_seen += 1;
                    match reason {
                        AttestationDropReason::BondNotActiveAtTarget => *dropped_bond_inactive += 1,
                        AttestationDropReason::ValidatorIdMismatch => *dropped_id_mismatch += 1,
                        AttestationDropReason::BadSignature => *dropped_bad_sig += 1,
                        // Below-fence-only (audit #4 relocated); counted with malformed — the
                        // shard is intrinsically unusable as-is, same hygiene class.
                        AttestationDropReason::NonZeroValidatorSetCommitment => *dropped_malformed += 1,
                        AttestationDropReason::MalformedPayload => *dropped_malformed += 1,
                    }
                    dropped_attestation_shards.push(kaspa_consensus_core::block::AttestationTemplateDrop {
                        tx_id: tx.id(),
                        kind: reason.template_drop_kind(),
                    });
                    debug!(
                        "[attestation-template] dropping ineligible shard tx {} (reason={:?}, bond={}, epoch={})",
                        tx.id(),
                        reason,
                        bond.transaction_id,
                        epoch
                    );
                    false
                }
            }
        };

        let mut invalid_transactions = HashMap::new();
        // kaspa-pq DNS-finality (E3): shards dropped by the classifier (eligible-filter),
        // tracked separately from validation-`invalid_transactions` so the
        // `is_successful`/`InvalidTransactionsInNewBlock` decision is unaffected — a
        // dropped-but-valid shard is a refill, not a template failure.
        let mut dropped_shard_ids: std::collections::HashSet<kaspa_consensus_core::tx::TransactionId> =
            std::collections::HashSet::new();
        let results = self.validate_block_template_transactions_in_parallel(&txs, &virtual_state, &virtual_utxo_view);
        for (tx, res) in txs.iter().zip(results) {
            match res {
                Err(e) => {
                    invalid_transactions.insert(tx.id(), e);
                    tx_selector.reject_selection(tx.id());
                }
                Ok(fee) => {
                    if classify_keep(
                        self,
                        tx,
                        &mut shards_seen,
                        &mut shards_kept,
                        &mut dropped_bond_inactive,
                        &mut dropped_id_mismatch,
                        &mut dropped_bad_sig,
                        &mut dropped_malformed,
                        &mut dropped_attestation_shards,
                    ) {
                        calculated_fees.push(fee);
                    } else {
                        dropped_shard_ids.insert(tx.id());
                        // kaspa-pq audit v26 (H-3): a classifier DROP (valid tx, ineligible
                        // shard) — free its slot for the refill WITHOUT counting it as a
                        // validation rejection that could flip the selector to unsuccessful.
                        tx_selector.reject_selection_for_refill(tx.id());
                    }
                }
            }
        }

        let mut has_rejections = !invalid_transactions.is_empty() || !dropped_shard_ids.is_empty();
        if has_rejections {
            txs.retain(|tx| !invalid_transactions.contains_key(&tx.id()) && !dropped_shard_ids.contains(&tx.id()));
        }

        while has_rejections {
            has_rejections = false;
            let next_batch = tx_selector.select_transactions(); // Note that once next_batch is empty the loop will exit
            let next_batch_results =
                self.validate_block_template_transactions_in_parallel(&next_batch, &virtual_state, &virtual_utxo_view);
            for (tx, res) in next_batch.into_iter().zip(next_batch_results) {
                match res {
                    Err(e) => {
                        invalid_transactions.insert(tx.id(), e);
                        tx_selector.reject_selection(tx.id());
                        has_rejections = true;
                    }
                    Ok(fee) => {
                        if classify_keep(
                            self,
                            &tx,
                            &mut shards_seen,
                            &mut shards_kept,
                            &mut dropped_bond_inactive,
                            &mut dropped_id_mismatch,
                            &mut dropped_bad_sig,
                            &mut dropped_malformed,
                            &mut dropped_attestation_shards,
                        ) {
                            txs.push(tx);
                            calculated_fees.push(fee);
                        } else {
                            // kaspa-pq audit v26 (H-3): classifier DROP during the refill loop —
                            // free the slot but do not count it as a validation rejection.
                            tx_selector.reject_selection_for_refill(tx.id());
                            has_rejections = true;
                        }
                    }
                }
            }
        }

        // kaspa-pq DNS-finality (§6.5): emit the attestation-template diagnostics once
        // per build when any shard was seen (kept or dropped). Inert (no log) on a chain
        // with no attestation traffic / overlay dormant.
        if shards_seen > 0 {
            info!(
                "[attestation-template] shards seen={} kept={} dropped(bond_inactive={}, id_mismatch={}, bad_sig={}, malformed={})",
                shards_seen, shards_kept, dropped_bond_inactive, dropped_id_mismatch, dropped_bad_sig, dropped_malformed
            );
        }

        // Check whether this was an overall successful selection episode. We pass this decision
        // to the selector implementation which has the broadest picture and can use mempool config
        // and context
        match (build_mode, tx_selector.is_successful()) {
            (TemplateBuildMode::Standard, false) => {
                return Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions)
                    .with_attestation_template_drops(&dropped_attestation_shards));
            }
            (TemplateBuildMode::Standard, true) | (TemplateBuildMode::Infallible, _) => {}
        }

        // kaspa-pq narrow P0-1: `template_bond_view` was captured at the top of this
        // function INSIDE the same read lock as `virtual_state` (the SAME virtual
        // generation = the template's selected parent), so the §6.2 selection-loop
        // classifier, the reward fan-out, the overlay commitment, and the EVM claim
        // payload all reference one coherent generation — never a later re-read of a
        // possibly-advanced view (the mixed-generation TOCTOU). `virtual_state.daa_score`
        // is exactly the template header's daa_score (see `Header::new_finalized` below).
        // Producer policy only: when local DNS finality is stale, this node emits an
        // empty EVM payload for the template (deposit claims, normal EVM txs, and the
        // EVM coinbase all stay out). Base L1 txs and PoW/GHOSTDAG liveness continue.
        // Block validation deliberately does not reject by reading the current
        // dns_state_store; validity must stay determined by the candidate block and
        // its selected-parent state.
        // ADR-0109 Decision 1: every lock the virtual set holds is claimed here, unasked — read under
        // the same lock as `virtual_utxo_view`, so `prepare_deposit_claims` below validates each
        // claim against exactly the generation it was read from.
        let evm_template_data = self.with_indexed_deposit_claims(evm_template_data, virtual_state.daa_score);
        // ADR-0109 Decision 2: under `Label` the anchor's distance decides nothing here; under `Pause`
        // (the behaviour before ADR-0109) a stale anchor empties the payload.
        let bridge_finality_fresh = self.evm_bridge_finality == kaspa_consensus_core::evm::EvmBridgeFinalityPolicy::Label
            || self.bridge_finality_is_fresh(virtual_state.ghostdag_data.selected_parent);
        let evm_template_data = if bridge_finality_fresh {
            evm_template_data
        } else {
            if !evm_template_data.transactions.is_empty() || !evm_template_data.system_ops.is_empty() {
                warn!(
                    "EVM lane producer paused: DNS finality is unconfirmed or stale at DAA {}; emitting an empty EVM payload this template (txs={}, deposit_claims={})",
                    virtual_state.daa_score,
                    evm_template_data.transactions.len(),
                    evm_template_data.system_ops.len()
                );
            }
            kaspa_consensus_core::evm::EvmTemplateData::default()
        };
        let prepared_claims =
            crate::processes::evm::prepare_deposit_claims(&evm_template_data.system_ops, virtual_utxo_view, virtual_state.daa_score);

        // At this point we can safely drop the read lock
        drop(virtual_read);

        // Build the template
        self.build_block_template_from_virtual_state(
            virtual_state,
            template_bond_view,
            prepared_claims,
            miner_data,
            txs,
            calculated_fees,
            evm_template_data,
            dropped_attestation_shards,
        )
    }

    pub(crate) fn validate_block_template_transactions(
        &self,
        txs: &[Transaction],
        virtual_state: &VirtualState,
        utxo_view: &impl UtxoView,
    ) -> Result<(), RuleError> {
        // Search for invalid transactions
        let mut invalid_transactions = HashMap::new();
        for tx in txs.iter() {
            if let Err(e) = self.validate_block_template_transaction(tx, virtual_state, utxo_view) {
                invalid_transactions.insert(tx.id(), e);
            }
        }
        if !invalid_transactions.is_empty() { Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions)) } else { Ok(()) }
    }

    pub(crate) fn build_block_template_from_virtual_state(
        &self,
        virtual_state: Arc<VirtualState>,
        // kaspa-pq narrow P0-1: the bond view + deposit-claim snapshot, both
        // captured in the SAME virtual generation as `virtual_state` by the caller
        // (under one read lock) — so the reward fan-out, the overlay commitment and
        // the EVM claim payload all reference one coherent generation.
        template_bond_view: ActiveBondView,
        prepared_claims: crate::processes::evm::PreparedDepositClaims,
        miner_data: MinerData,
        mut txs: Vec<Transaction>,
        calculated_fees: Vec<u64>,
        // kaspa-pq EVM Lane v0.4 (§15 step 6 / §16): own-payload inputs.
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
        // kaspa-pq DNS-finality (audit v24 H-5): shards the selection-loop classifier dropped,
        // forwarded into the `BlockTemplate` so the mining manager can reconcile the mempool.
        dropped_attestation_shards: Vec<kaspa_consensus_core::block::AttestationTemplateDrop>,
    ) -> Result<BlockTemplate, RuleError> {
        // [`calc_block_parents`] can use deep blocks below the pruning point for this calculation, so we
        // need to hold the pruning lock.
        let _prune_guard = self.pruning_lock.blocking_read();
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let header_pruning_point =
            self.pruning_point_manager.expected_header_pruning_point(virtual_state.ghostdag_data.to_compact()).pruning_point;
        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4/§B.5): the validator
        // reward fan-out for this template. The template extends the current
        // tip, so the bond set as-of its selected parent is the `StakeBonds`
        // store snapshot (= state at the sink) — `initial_active_bond_view`.
        // Then compute the reward outputs with the SAME
        // `validator_reward_outputs_for_block` the validation path uses, so a
        // block mined from this template reproduces the coinbase byte-for-byte.
        // No-op on every current network (overlay dormant). The bond view is
        // captured by the caller in the template's virtual generation (narrow P0-1)
        // and passed in, not re-read here.
        //
        // kaspa-pq DNS-finality (E3/§6.2): the PRIMARY ineligible-shard drop now
        // happens AT SELECTION TIME in `build_block_template` (with reject/refill +
        // `calculated_fees` lockstep), so by the time this function runs on that path
        // `txs` already carries only eligible shards and the late `retain` finds
        // nothing — `calculated_fees` therefore stays 1:1 with `txs`. The `retain` is
        // retained ONLY for the alternate `test_block_builder` path, which passes a
        // pre-built `txs` (and an empty `calculated_fees`) without going through the
        // selection-loop classifier; there dropping a shard is harmless to fee
        // alignment (no fees are tracked). In debug builds we assert the post-state.
        self.retain_reward_eligible_attestation_shards(&mut txs, &template_bond_view, virtual_state.daa_score);
        // The §6.2 selection loop already aligns the two on the production path; assert
        // that invariant in debug builds (skipped when `calculated_fees` is the test
        // helper's empty sentinel, which legitimately does not track per-tx fees).
        debug_assert!(
            calculated_fees.is_empty() || calculated_fees.len() == txs.len(),
            "calculated_fees ({}) must stay 1:1 with non-coinbase txs ({}) after attestation-shard filtering",
            calculated_fees.len(),
            txs.len()
        );
        // kaspa-pq Phase 13 (ADR-0018 §F+§E): the §F carve + §E validator pool for
        // this template, computed identically to the validation path so a block
        // mined from this template reproduces the coinbase byte-for-byte. `None`/0
        // on every current network (overlay dormant).
        // ADR-0018 §F staged rollout: None (Stage 1) / bootstrap (Stage 2) / full
        // (Stage 3) selected by DAA, identically to the validation path — and past ADR-0126's fence
        // the full split lowered, through the one reader both paths call.
        let carve = self.fee_split_at(virtual_state.daa_score);
        // **One PALW state, at the selected parent** (mainnet audit H-1 + testnet-11 2026-09-21).
        // The five reads below and the header root all ask about the SAME block the validator
        // reconstructs — virtual's selected parent — not the store tip. Walking from the tip is
        // empty when they agree (the common case, one cached materialization); when they do not,
        // stamping the tip would mine a block this node then disqualifies from virtual.
        let palw_at_selected_parent = self.palw_v2_state_at(virtual_state.ghostdag_data.selected_parent);
        let validator_pool = carve.as_ref().map_or(0, |fs| {
            // The template computes the SAME set the validator will, from the same state it is
            // building on — a template whose pool disagreed with validation would build a coinbase
            // its own node then refuses.
            // No params means no V2 bundle, which means no entitlement question — and an `expect`
            // here panics every hash-only network's template, because the DNS carve this closure
            // computes exists on networks PALW does not.
            let unentitled = palw_at_selected_parent
                .as_ref()
                .map(|(_, state)| {
                    self.palw_v2_unentitled_blues(
                        state,
                        &virtual_state.ghostdag_data,
                        &virtual_state.mergeset_non_daa,
                        &self.palw_v2_template_point(&virtual_state),
                    )
                })
                .unwrap_or_default();
            self.coinbase_manager.coinbase_validator_pool(
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_rewards,
                &virtual_state.mergeset_non_daa,
                fs,
                &unentitled,
                &self.palw_round_blocks_of(&virtual_state.ghostdag_data),
            )
        });
        let (validator_reward_outputs, _rewarded_keys, newly_included_stake, expected_stake) = self
            .validator_reward_outputs_for_block(
                &txs,
                &template_bond_view,
                virtual_state.daa_score,
                virtual_state.ghostdag_data.selected_parent,
                validator_pool,
            );
        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): append the reserve-drip outputs so a block
        // mined from this template reproduces the validated coinbase byte-for-byte. Reads the sink's
        // committed reserve balance (= the template's selected parent). Inert below the v2 fence.
        let mut validator_reward_outputs = validator_reward_outputs;
        if let Some(dns_params) = self.dns_params.as_ref() {
            let parent_balance = self.reserve_balance_store.get(virtual_state.ghostdag_data.selected_parent).unwrap_or(0);
            let (drip_outputs, _) = self.reserve_drip_outputs(
                dns_params,
                virtual_state.daa_score,
                virtual_state.ghostdag_data.selected_parent,
                &template_bond_view,
                parent_balance,
            );
            validator_reward_outputs.extend(drip_outputs);
        }
        // ADR-0042 Decision 10, construction side. The selected parent is the block being built
        // on, so its queue is the same one the validating walk will read; appended last, matching
        // `verify_expected_utxo_state`'s order exactly. A template that got this wrong would mine
        // blocks its own node rejects.
        if palw_at_selected_parent.is_some() {
            let payouts = palw_at_selected_parent.as_ref().map(|(_, state)| self.palw_v2_payout_outputs(state)).unwrap_or_default();
            validator_reward_outputs.extend(payouts);
        }
        // ADR-0042 Decision 10's funding side, construction path. The selected parent is the block
        // being built on, so its escrow is the one the validating walk will withhold. Computed from
        // the same state the payouts came from, so the two halves cannot disagree.
        let palw_escrow_withheld =
            palw_at_selected_parent.as_ref().map(|(block, state)| self.palw_v2_escrow_withheld_at(state, *block)).unwrap_or(0);
        // Launch blockers §8, construction side: the merged blues this template may not pay. Same
        // state, same question, same answer as the validating walk — a template that disagreed
        // would mine blocks its own node rejects.
        let palw_unentitled_blues = palw_at_selected_parent
            .as_ref()
            .map(|(_, state)| {
                self.palw_v2_unentitled_blues(
                    state,
                    &virtual_state.ghostdag_data,
                    &virtual_state.mergeset_non_daa,
                    &self.palw_v2_template_point(&virtual_state),
                )
            })
            .unwrap_or_default();
        // B-1 (deep fence), construction side: the carve withheld from each OTHER merged block.
        // Same selected-parent state, same virtual ghostdag/non-DAA and same template point as the
        // validating walk resolves for the block this template becomes — so the withheld map, and
        // thus the coinbase, are byte-identical on both paths. Empty below the fence.
        let palw_merged_escrow_withheld = palw_at_selected_parent
            .as_ref()
            .map(|(_, state)| {
                self.palw_v2_merged_escrow_withheld(
                    state,
                    &virtual_state.ghostdag_data,
                    &virtual_state.mergeset_non_daa,
                    &self.palw_v2_template_point(&virtual_state),
                )
            })
            .unwrap_or_default();
        let coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                virtual_state.daa_score,
                // The template path always builds the bonded lanes' coinbase; a heartbeat
                // template is derived from it by `heartbeat_adapt_block_template`, which
                // re-declares the subsidy as zero (ADR-0060 Decision 1.4).
                self.coinbase_manager.calc_block_subsidy(virtual_state.daa_score),
                miner_data.clone(),
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_rewards,
                &virtual_state.mergeset_non_daa,
                &validator_reward_outputs,
                carve.as_ref(),
                (newly_included_stake, expected_stake),
                palw_escrow_withheld,
                &palw_unentitled_blues,
                self.palw_state_params_v2.is_some(),
                &palw_merged_escrow_withheld,
                // ADR-0125: the merged round blocks, whose fees go to their payouts.
                &self.palw_round_blocks_of(&virtual_state.ghostdag_data),
            )
            .unwrap();
        txs.insert(0, coinbase.tx);
        // kaspa-pq EVM Lane v0.4 (§4.3/§15): the template declares the
        // fork-correct header version — v2 (two EVM commitments) at/after
        // activation, v1 before (mirrors the check_header_version rule).
        let version = if virtual_state.daa_score >= self.evm_activation_daa_score {
            kaspa_consensus_core::constants::EVM_HEADER_VERSION
        } else {
            BLOCK_VERSION
        };
        let parents_by_level = self.parents_manager.calc_block_parents(pruning_point, &virtual_state.parents);
        let hash_merkle_root = calc_hash_merkle_root(txs.iter());

        let accepted_id_merkle_root = self
            .calc_accepted_id_merkle_root(virtual_state.accepted_tx_ids.iter().copied(), virtual_state.ghostdag_data.selected_parent);
        let utxo_commitment = virtual_state.multiset.clone().finalize();
        // Past median time is the exclusive lower bound for valid block time, so we increase by 1 to get the valid min
        let min_block_time = virtual_state.past_median_time + 1;
        let header = Header::new_finalized(
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            u64::max(min_block_time, unix_now()),
            virtual_state.bits,
            0,
            // kaspa-pq ADR-0007: the template declares the network-correct Layer-1 algo for this
            // DAA score — PALW LLM (algo_id = 4) once activated, else BLAKE2b-512 ∥ SHA3-512 (3)
            // once activated, else kHeavyHash (1).
            {
                let declared = kaspa_consensus_core::pow_layer0::required_algo_id_for_mode(
                    self.palw_required_algo_id,
                    self.pow_palw_ollama_activation.is_active(virtual_state.daa_score),
                    self.pow_palw_activation.is_active(virtual_state.daa_score),
                    self.pow_blake2b_sha3_activation.is_active(virtual_state.daa_score),
                );
                // **ADR-0072 SA-4: past the fence, the template declares the OPEN attempt lane.**
                //
                // The cascade above cannot know about a top-level fence, and a producer that kept
                // stamping algo-6 past the fence would build blocks its own validator refuses —
                // the chain would stop at the fence rather than cross it. Only the attempt id is
                // rewritten: the receipt and heartbeat lanes are other fences' business.
                let lane = kaspa_consensus_core::pow_layer0::PalwAttemptLaneV1::from_fence(
                    self.palw_attempt_activation.map(|fence| fence.is_active(virtual_state.daa_score)),
                );
                if kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(declared) { lane.attempt_algo_id() } else { declared }
            },
            virtual_state.daa_score,
            virtual_state.ghostdag_data.blue_work,
            virtual_state.ghostdag_data.blue_score,
            header_pruning_point,
        );
        // kaspa-pq EVM Lane v0.4 (§15): on an evm-active template, execute the
        // mergeset acceptance NOW (the producer-side run of the exact verifier
        // code) and commit both EVM header fields. The own payload is empty
        // until the EVM mempool lands (§16 phase) — its (non-zero) hash is
        // still committed. Inert (returns the header unchanged) pre-activation.
        let (header, evm_payload, stale_evm_claims) = self
            .evm_template_fields(header, &virtual_state, evm_template_data, prepared_claims)
            .map_err(|err| err.with_attestation_template_drops(&dropped_attestation_shards))?;
        // kaspa-pq ADR-0022: commit the DNS/PoS-v2 overlay snapshot as-of the template's
        // selected parent (the sink) — the SAME `compute_overlay_snapshot` the validation
        // path re-derives, so a block mined from this template reproduces the
        // `overlay_commitment_root` byte-for-byte (construction == validation). Inert
        // (header unchanged) when the overlay is dormant. Appended after the EVM fields;
        // `with_overlay_commitment` re-finalizes over the full preimage.
        let header = if self.dns_params.is_some() {
            let overlay_root =
                self.compute_overlay_snapshot(virtual_state.ghostdag_data.selected_parent, &template_bond_view).commitment_root();
            header.with_overlay_commitment(overlay_root)
        } else {
            header
        };
        // ADR-0042 Unit C step 5: the template commits the PARENT's root — the state this block's
        // transition starts from — computed from the SAME walked state the validation path holds
        // at the selected parent, so a block mined from this template reproduces the root
        // byte-for-byte (construction == validation). Inert (header unchanged, root stays zero)
        // wherever the mode carries no V2 bundle or the parent state cannot be established,
        // where the preimage gate reads zero as absent.
        let header = match palw_at_selected_parent.as_ref() {
            Some((_, parent_state)) => {
                // The PARENT's root: what this block's transition starts from. Non-circular by
                // construction — it is the selected parent's post-transition state, fixed before
                // this header exists, so stamping it cannot move the hash it would then have to
                // match. Not the store tip: that row can stand one or more blocks away from this
                // template's selected parent (testnet-11 2026-09-21 `df80394b`).
                header.with_palw_state_root(parent_state.state_root())
            }
            None => header,
        };
        let selected_parent_hash = virtual_state.ghostdag_data.selected_parent;
        let selected_parent_timestamp = self.headers_store.get_timestamp(selected_parent_hash).unwrap();
        let selected_parent_daa_score = self.headers_store.get_daa_score(selected_parent_hash).unwrap();
        let mut template_block = MutableBlock::new(header, txs);
        template_block.evm_payload = evm_payload;
        Ok(BlockTemplate::new(
            template_block,
            miner_data,
            coinbase.has_red_reward,
            coinbase.miner_script_output_indices,
            selected_parent_timestamp,
            selected_parent_daa_score,
            selected_parent_hash,
            calculated_fees,
            stale_evm_claims,
            dropped_attestation_shards,
        ))
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
    pub fn process_genesis(self: &Arc<Self>) {
        // Write the UTXO state of genesis
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
            None, // ADR-0042 Unit C: genesis stages no PALW delta — the tip is installed below.
        );

        // ADR-0042 Decision 5 / Unit C: the V2 state's zero point. Installed beside the genesis
        // UTXO commit so a fresh database has a tip to walk from, and skipped entirely on a
        // network with no V2 bundle (every shipped preset), where the store stays empty.
        if let Some(state_params) = self.palw_state_params_v2.as_ref() {
            // The genesis block APPLIES the bundle's registration list. A V2 network has no class
            // and no bond until something registers them, and the only block that can is this one:
            // admission refuses an attempt naming a bond the chain does not have, so a genesis
            // that registers nothing produces a network that boots and then cannot make a block.
            // Found by measurement — the harness wedged exactly there the moment admission was
            // wired, with every block refused for a bond that existed nowhere.
            let objects = self.palw_genesis_objects_v2.clone();
            let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                block: self.genesis.hash,
                daa_score: self.genesis.daa_score,
                blue_score: 0,
                // Genesis funds no escrow: its coinbase pays the premine, and its object list
                // registers — it creates no claim, so there is nothing for a carve to attach to.
                subsidy: 0,
            };
            let (genesis_state, delta) = kaspa_consensus_core::palw_state_v2::apply_palw_transition_v2_with_extras(
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
            .expect("the bundle's genesis registrations must apply — `validate_palw_v2` ran them at construction");
            let mut batch = WriteBatch::default();
            let mut store = self.palw_state_v2_store.write();
            // The delta rides too, so the genesis point is walkable like every other chain block
            // rather than a special case the reorg walk has to know about.
            store.insert_delta_batch(&mut batch, self.genesis.hash, genesis_state.state_root(), &delta).unwrap();
            store.set_tip_batch(&mut batch, self.genesis.hash, &genesis_state).unwrap();
            drop(store);
            self.db.write(batch).unwrap();
        }

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
    }

    /// Finalizes the pruning point utxoset state and imports the pruning point utxoset *to* virtual utxoset
    pub fn import_pruning_point_utxo_set(
        &self,
        new_pruning_point: BlockHash,
        mut imported_utxo_multiset: MuHash,
    ) -> PruningImportResult<()> {
        info!("Importing the UTXO set of the pruning point {}", new_pruning_point);
        let new_pruning_point_header = self.headers_store.get_header(new_pruning_point).unwrap();
        let imported_utxo_multiset_hash = imported_utxo_multiset.finalize();
        if imported_utxo_multiset_hash != new_pruning_point_header.utxo_commitment {
            return Err(PruningImportError::ImportedMultisetHashMismatch(
                new_pruning_point_header.utxo_commitment,
                imported_utxo_multiset_hash,
            ));
        }

        {
            // Set the pruning point utxoset position to the new point we just verified
            let mut batch = WriteBatch::default();
            let mut pruning_meta_write = self.pruning_meta_stores.write();
            pruning_meta_write.set_utxoset_position(&mut batch, new_pruning_point).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_meta_write);
        }

        {
            // Copy the pruning-point UTXO set into virtual's UTXO set
            let pruning_meta_read = self.pruning_meta_stores.read();
            let mut virtual_write = self.virtual_stores.write();

            virtual_write.utxo_set.clear().unwrap();
            for chunk in &pruning_meta_read.utxo_set.iterator().map(|iter_result| iter_result.unwrap()).chunks(1000) {
                virtual_write.utxo_set.write_from_iterator_without_cache(chunk).unwrap();
            }
            // ADR-0109 Decision 1: the set was replaced wholesale; the index follows it wholesale.
            self.rebuild_evm_deposit_lock_index(&virtual_write.utxo_set);
        }

        let virtual_read = self.virtual_stores.upgradable_read();

        // Validate transactions of the pruning point itself
        let new_pruning_point_transactions = self.block_transactions_store.get(new_pruning_point).unwrap();
        let validated_transactions = self.validate_transactions_in_parallel(
            &new_pruning_point_transactions,
            &virtual_read.utxo_set,
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
            self.db.write(batch).unwrap();
            drop(statuses_write);
        }

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
        let l1_header = self.headers_store.get_header(pruning_point).unwrap();

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

        // (3) Persist the rows and pin the finalized EVM head to the pruning point.
        let state_root = evm_header.state_root; // captured before `evm_header` is moved below
        let evm_number_for_checkpoint = evm_header.evm_number; // ditto, for the F2a anchor checkpoint
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
        self.evm_header_store.insert_batch(&mut batch, pruning_point, evm_header).unwrap();
        // F2a (t10 recovery): the imported, root-verified snapshot is also this
        // node's FIRST pruning-point state anchor — persist it as a checkpoint so
        // the pruning processor's pp-anchor induction (`ensure_pp_evm_anchor`) has
        // a base once the pp advances, even on a retired-206 node (which writes no
        // per-block 206 rows of its own; §12 gathering anchors on checkpoints).
        {
            use crate::model::stores::evm::EvmStateCheckpointStoreReader;
            if self.evm_state_checkpoint_store.get(pruning_point).ok().flatten().is_none() {
                let checkpoint = kaspa_consensus_core::evm::EvmStateCheckpointV1::build(
                    pruning_point,
                    evm_number_for_checkpoint,
                    state_root,
                    &snapshot,
                );
                self.evm_state_checkpoint_store.insert_batch(&mut batch, pruning_point, checkpoint).unwrap();
            }
        }
        self.evm_state_store.insert_batch(&mut batch, pruning_point, snapshot).unwrap();
        {
            let mut heads_write = self.evm_heads_store.write();
            let prev = heads_write.get().ok();
            let latest = prev.as_ref().map(|h| h.latest).unwrap_or(pruning_point);
            let safe = prev.as_ref().map(|h| h.safe).unwrap_or(pruning_point);
            let heads = kaspa_consensus_core::evm::CanonicalEvmHeads { latest, safe, finalized: pruning_point };
            heads_write.set_batch(&mut batch, heads).unwrap();
        }
        self.db.write(batch).unwrap();
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
        let Some(params) = self.palw_state_params_v2.as_ref() else {
            return Ok(()); // not a ConsensusV2 network — nothing reads this store.
        };
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
        let state = carriage
            // ADR-0069 Decision 7: the consistency check has to know the rule the snapshot was
            // built under, or it refuses a state for having obeyed it.
            .into_state_v3(params, Some(expected_root), self.palw_uncertified_weightless_at(witness_daa), self.palw_canonical_work_daa)
            .map_err(|e| PruningImportError::ImportedPalwStateInvalid(pruning_point, expected_root, e.to_string()))?;
        // Written through `set_tip_batch`, which RE-DERIVES the root from the state it is handed —
        // so what becomes durable is a function of what was verified, and a caller cannot store a
        // snapshot under a root it did not compute. The peer's bytes never reach the database.
        let mut batch = WriteBatch::default();
        {
            let mut store = self.palw_state_v2_store.write();
            store.set_tip_batch(&mut batch, pruning_point, &state).expect("writing the verified PALW tip cannot fail");
            // The same state, also as this node's servable snapshot. Without it a node that just
            // joined by a pruned sync holds the pruning-point state but cannot HAND it on until
            // its own pruning point next advances — so the fix would not propagate past the first
            // hop, and a young network would have exactly one node able to serve.
            store
                .set_pruning_snapshot_batch(&mut batch, pruning_point, &state)
                .expect("writing the verified PALW pruning snapshot cannot fail");
        }
        self.db.write(batch).unwrap();
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
        Some(carriage.as_ref().clone())
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
        let mut batch = WriteBatch::default();
        {
            let mut bonds_write = self.stake_bonds_store.write();
            for rec in &snapshot.bonds {
                bonds_write.insert_batch(&mut batch, rec.bond_outpoint, std::sync::Arc::new(rec.clone())).unwrap();
            }
        }
        if snapshot.reserve_balance > 0 {
            self.reserve_balance_store.insert_batch(&mut batch, pruning_point, snapshot.reserve_balance).unwrap();
        }
        self.pruning_overlay_snapshot_store
            .write()
            .set_batch(&mut batch, PruningPointOverlaySnapshot { pruning_point, snapshot })
            .unwrap();
        self.db.write(batch).unwrap();
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
        let loaded = match store.load_tip(params) {
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
            match crate::processes::palw_state_walk::walk_chain_path(&store, params, tip_state, &removed, &added) {
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
        if let Err(e) = store.set_pruning_snapshot_batch(&mut batch, pruning_point, &state) {
            warn!("PALW: the pruning-point snapshot did not stage ({e})");
            return;
        }
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
        self.pruning_overlay_snapshot_store
            .write()
            .set_batch(&mut batch, PruningPointOverlaySnapshot { pruning_point, snapshot })
            .unwrap();
        self.db.write(batch).unwrap();
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
        /// `(blue_score, hash)`, empty when the lane is not armed. The CALLER decides
        /// admissibility over the whole accumulated set (`heartbeat_set_admissible`) —
        /// width with the chain exemption is a property of the final mergeset, not of one
        /// candidate's increase, and a rejected candidate is simply skipped (the excess IS
        /// a heartbeat or reaches one; there is nothing to substitute).
        heartbeat_members: Vec<(u64, BlockHash)>,
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
        O::OptimisticLicensed { .. } => "OptimisticLicensed",
        O::SeatReadinessProvedV2 { .. } => "SeatReadinessProvedV2",
        O::ObjectiveOffence { .. } => "ObjectiveOffence",
        O::ModelBuy { .. } => "ModelBuy",
        O::ModelSeed { .. } => "ModelSeed",
        O::ShardCourtAccused { .. } => "ShardCourtAccused",
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
        O::CourtAttnDissected { .. } => "CourtAttnDissected",
        O::CourtAttnChildChosen { .. } => "CourtAttnChildChosen",
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

/// ADR-0109 Decision 3: the first input of `tx` that spends a locked PALW bond, if any.
pub(super) fn first_locked_input(
    tx: &Transaction,
    locked: &std::collections::HashSet<TransactionOutpoint>,
) -> Option<TransactionOutpoint> {
    tx.inputs.iter().map(|input| input.previous_outpoint).find(|outpoint| locked.contains(outpoint))
}
