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
            compute_capabilities::DbComputeCapabilityStore,
            daa::DbDaaStore,
            depth::{DbDepthStore, DepthStoreReader},
            dns_finality_certificate::DbDnsFinalityCertificateStore,
            dns_state::{DbDnsStateStore, DbVltActivationStore, DnsStateStoreReader, VltActivationStoreReader},
            epoch_accumulator::{DbBlockQualityPoolStore, DbEpochAccumulatorStore, DbReserveBalanceStore},
            evm::{
                DbEvmCanonicalHeadsStore, DbEvmHeaderStore, DbEvmPayloadStore, DbEvmStateStore, EvmCanonicalHeadsStoreReader,
                EvmHeaderStore, EvmHeaderStoreReader, EvmStateStore, EvmStateStoreReader,
            },
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            palw_carriage::DbPalwCarriageStore,
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
            token_ledger::DbTokenStore,
            utxo_diffs::{DbUtxoDiffsStore, UtxoDiffsStoreReader},
            utxo_multisets::{DbUtxoMultisetsStore, UtxoMultisetsStoreReader},
            virtual_state::{LkgVirtualState, VirtualState, VirtualStateStoreReader, VirtualStores},
            vlt_credits::DbVltCreditStore,
            vlt_voting_snapshot::DbVltVotingSnapshotStore,
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
    BlockHash, BlockHashSet, BlueWorkType, ChainPath, Hash64,
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
        BondMutation, CanonicalLaggedEpochAnchor, ComputeCapabilityRecord, ComputeCommitmentRecord, ComputeCreditContribution,
        ComputeStatusView, ComputeVerdictRecord, DnsCoinbaseSettlement, DnsParams, DnsReorgMode, DnsReorgOutcome, DnsRolloutStage,
        MandatoryAttestationContributionKey, MandatoryAttestationDeficit, MandatoryAttestationValidator, OpenComputeCommitment,
        OverlaySnapshot, PRECOMMIT_MLDSA87_CONTEXT, PendingComputeVerdict, PrecommitDuty, PrecommitLock, PrecommitRecord,
        PruningPointOverlaySnapshot, StakeBondRecord, StakePreferenceInputs, StakeScore, UNBOND_REQUEST_CONTEXT,
        advance_dns_confirmation, aggregate_compute_credits, aggregate_epoch_tallies, anchor_cutoff_blue_score, apply_bond_stamp,
        attestations_from_accepted_txs, bond_mutations_from_accepted_txs, build_finality_certificate, build_voting_snapshot,
        canonical_lagged_epoch_anchor, capability_candidate_pool, capability_set_root, check_dns_reorg_rule, commitment_beacon_epoch,
        compute_capabilities_from_accepted_txs, compute_capabilities_with_ids_from_accepted_txs,
        compute_certificates_from_accepted_txs, compute_challenges_from_accepted_txs, compute_commitments_from_accepted_txs,
        compute_stake_score, compute_verdicts_from_accepted_txs, derive_dns_health, dns_finality_fresh_for_bridge,
        effective_bond_status, epoch_meets_quality_floor, epoch_start_blue_score, held_precommit_lock, is_bond_active_at,
        is_dns_confirmed, lock_consistent_precommits, mandatory_attestation_mass_capacity, p2pkh_mldsa87_spk,
        precommits_from_accepted_txs, quorum_epochs, ready_epoch_from_tip_blue_score, recompute_epoch_tallies,
        reorg_inputs_since_common_ancestor, required_stake_for_quality_floor, revert_bond_stamp, stake_attestation_message,
        stake_precommit_message, stake_preference_verdict, total_active_stake_by_epoch, total_voting_weight_by_epoch,
        unbond_request_message, unbond_requests_from_accepted_txs, validator_id_from_pubkey, validator_voting_weight_of_bond,
        verdicts_for_certificate, voting_epoch_for_target,
    },
    header::Header,
    merkle::calc_hash_merkle_root,
    mining_rules::MiningRules,
    palw_carriage::palw_carriage_records_from_accepted_txs,
    pruning::PruningPointsList,
    subnets::{SUBNETWORK_ID_TOKEN_BURN, SUBNETWORK_ID_TOKEN_TRANSFER},
    token::{
        TOK_ASSET_ID, TOKEN_BURN_MLDSA87_CONTEXT, TOKEN_TRANSFER_MLDSA87_CONTEXT, TokenAccount, TokenEmissionSettlement, TokenSupply,
        apply_token_burn, apply_token_transfer, decode_token_burn_payload, decode_token_transfer_payload, emission_epoch_budget,
        emission_rewards_v2, token_burn_message, token_transfer_message,
    },
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, TransactionOutput},
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
    vlt::{
        COMPUTE_CAPABILITY_MLDSA87_CONTEXT, COMPUTE_CERT_MLDSA87_CONTEXT, COMPUTE_COMMITMENT_MLDSA87_CONTEXT, ChallengeOutcome,
        ComputeCertificatePayload, ComputeChallengePayload, VERIFIER_VERDICT_MLDSA87_CONTEXT, VltActivationState, VltCreditSkipReason,
        VltCreditTally, VltEpochCredits, VltEpochSnapshot, VltMetrics, VltRejection, VltVotingSnapshot, adjudicate_compute_challenge,
        bft_quorum, commitment_dependency_horizon, compute_capability_message, compute_certificate_message,
        compute_commitment_message, compute_receipt_hash, job_input_commitment, job_spec_id, meets_bft_quorum, normalize_vlt,
        select_verifiers, tick_vlt_activation, verifier_verdict_message, verify_compute_certificate, vlt_activation_eligibility,
        vlt_epoch_finalized,
    },
};
use kaspa_consensus_notify::{
    notification::{
        NewBlockTemplateNotification, Notification, SinkBlueScoreChangedNotification, UtxosChangedNotification,
        VirtualChainChangedNotification, VirtualDaaScoreChangedNotification,
    },
    root::ConsensusNotificationRoot,
};
use kaspa_consensusmanager::SessionLock;
use kaspa_core::{debug, info, time::unix_now, trace, warn};
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
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque},
    iter::once,
    ops::Deref,
    sync::{Arc, Mutex, atomic::Ordering},
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

/// Which weight an attestation pledges when it is collected — the MISAKA Verified LLM
/// Token-Weighted BFT replacement, made an explicit argument so every call site declares its unit
/// rather than inferring it.
///
/// The two consumers genuinely want different things, and conflating them would be a bug:
///
/// * The **finality** path (`update_dns_state`, the reorg gate) picks by
///   [`DnsParams::vlt_weighting_active_at`]: bonded stake below the VLT fence, verified compute at
///   and above it. That is the voting-power replacement.
/// * The **mandatory-inclusion / mining** paths are stake-denominated block-inclusion policy
///   (`required_stake_for_quality_floor` against `min_active_stake_sompi`) and always pass
///   [`Self::BondedStake`], so they are untouched by the VLT switch.
#[derive(Copy, Clone)]
pub(crate) enum ContributionWeight<'a> {
    /// The bond's stake in sompi.
    BondedStake,
    /// `W_i(E) = min{C_i(E), λ·B_i(E)}` in µRTE, read from a [`VltEpochSnapshot`] pinned at a
    /// block every branch in the comparison contains — never from a per-branch walk.
    Vlt { snapshot: &'a VltEpochSnapshot, vlt: &'a kaspa_consensus_core::vlt::VltParams },
}

impl ContributionWeight<'_> {
    /// The weight this bond's VALIDATOR carries for `epoch`, in whichever unit the variant
    /// denotes.
    ///
    /// Under `Vlt` the answer is a property of the validator, not of the bond handed in: the
    /// collateral cap applies once to the validator's aggregate bond, so `bonds` (the set the
    /// caller is walking) and `anchor_daa` are required to resolve it. Weighing the single record
    /// would let one identity's `C_i` be converted once per bond it holds.
    ///
    /// `BondedStake` stays per-bond: below the VLT fence the weight IS the stake, and stake does
    /// not double-count when it is split — each output is counted once wherever it appears.
    fn of(&self, bond: &StakeBondRecord, bonds: &[StakeBondRecord], anchor_daa: u64, epoch: u64) -> u128 {
        match self {
            Self::BondedStake => bond.amount as u128,
            Self::Vlt { snapshot, vlt } => validator_voting_weight_of_bond(bond, bonds, anchor_daa, epoch, snapshot, vlt),
        }
    }
}

/// Everything the compute overlay contributed on one chain segment, from a single backward walk
/// ([`VirtualStateProcessor::walk_compute_overlay`]).
///
/// Held as one struct because the records are mutually dependent — a certificate is only
/// resolvable against the commitments, capabilities and verdicts collected in the same pass — and
/// passing them as five separate arguments to every consumer invites a caller to walk one of them
/// over a different range than the others.
pub(crate) struct ComputeOverlayWalk {
    /// Fraud proofs accepted on this chain, with the certificate each names. Kept whole rather
    /// than reduced to a set of accused certificates because a challenge is a claim that has to be
    /// *adjudicated* against that certificate's verdicts before it does anything — see
    /// [`adjudicate_compute_challenge`].
    challenges: Vec<ComputeChallengePayload>,
    capabilities: Vec<ComputeCapabilityRecord>,
    commitments: HashMap<TransactionId, ComputeCommitmentRecord>,
    verdicts: Vec<ComputeVerdictRecord>,
    /// `(certificate_tx_id, payload, accepted_daa_score)` for certificates whose declared epoch is
    /// their own accepting block's epoch.
    certificates: Vec<(TransactionId, ComputeCertificatePayload, u64)>,
    /// Whether the dependency search below the certificate floor covered its whole range.
    ///
    /// `true` ⇒ a commitment still missing from [`Self::commitments`] is genuinely absent from the
    /// canonical history under the pin. `false` ⇒ the search stopped early (a header would not
    /// read), so a missing commitment says nothing about the chain and everything about this
    /// node's storage. The two lead to `CommitmentAbsentFromCanonicalHistory` and
    /// `CommitmentNotLoaded` respectively, and only the first may be cached as a permanent zero.
    dependency_scan_complete: bool,
}

/// A certificate whose verifier committee has been drawn — the output of
/// [`VirtualStateProcessor::resolve_certificate`].
///
/// What remains after this is only the verdicts: whether enough of `committee` published, and
/// whether any of them refuted.
pub(crate) struct ResolvedCertificate {
    job_id: Hash64,
    /// `R_j` as the executor claimed it — what a verdict must judge, and what a verifier's own
    /// replay has to reproduce.
    receipt_hash: Hash64,
    /// The sortitioned verifiers, already restricted to bonds Active at the epoch anchor.
    committee: HashSet<Hash64>,
    /// The phase-1 commitment this certificate completes, which is where the job's input lives.
    commitment_tx_id: TransactionId,
}

pub struct VirtualStateProcessor {
    // Channels
    receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
    pruning_sender: CrossbeamSender<PruningProcessingMessage>,
    pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    db: Arc<DB>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) max_block_parents: u8,
    pub(super) mergeset_size_limit: u64,
    pub(super) max_block_mass: u64,
    /// kaspa-pq Phase 3 PoW (ADR-0007): BLAKE2b-512 ∥ SHA3-512 (`algo_id = 3`) activation — sets the
    /// block template's `pow_algo_id` so miners produce the network-correct Layer-1 algorithm.
    pub(super) pow_blake2b_sha3_activation: kaspa_consensus_core::config::params::ForkActivation,
    /// MISAKA Phase 4 PoW: PALW deterministic-LLM (`algo_id = 4`) activation — supersedes the
    /// BLAKE2b-SHA3 rule for the template's `pow_algo_id` where active.
    pub(super) pow_palw_activation: kaspa_consensus_core::config::params::ForkActivation,
    /// ADR-0039 W4′: the tip-ordering rule, cloned from `Params` at construction.
    pub(super) palw_tip_order: kaspa_consensus_core::palw_chain_weight::PalwTipOrderV1,
    /// MISAKA Phase 4b PoW: PALW-Ollama (`algo_id = 5`) activation — supersedes everything.
    pub(super) pow_palw_ollama_activation: kaspa_consensus_core::config::params::ForkActivation,

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

    // kaspa-pq Phase 10 (ADR-0009): DNS finality overlay. `dns_params` is the
    // dormancy guard — `None` on every current network, so the bond-population
    // pass below is a single `Option` check and a return.
    pub(super) stake_bonds_store: Arc<RwLock<DbStakeBondsStore>>,
    /// Accepted capability declarations. A store rather than a walk product: a declaration
    /// outlives the credit window by three orders of magnitude, so a walk-scoped copy vanishes
    /// while it is still in force and takes the certificate's whole committee with it.
    pub(super) compute_capability_store: Arc<RwLock<DbComputeCapabilityStore>>,
    /// MISAKA PALW chain carriage (ADR-0029 Stage 1): accepted carriage objects, keyed by
    /// carrying tx. Written/reverted by `stage_palw_carriages` beside the capability walk;
    /// an index — NO consensus rule reads it yet (Stage 2 is the reader).
    pub(super) palw_carriage_store: Arc<RwLock<DbPalwCarriageStore>>,
    pub(super) palw_class_state_store: Arc<RwLock<crate::model::stores::palw_class_state::DbPalwClassStateStore>>,
    pub(super) dns_state_store: Arc<RwLock<DbDnsStateStore>>,
    /// MISAKA VLT PR 1: the persisted §6 activation record, stepped once per blue-score epoch in
    /// `update_dns_state` and written in the same batch as the `DnsState`.
    pub(super) vlt_activation_store: Arc<RwLock<DbVltActivationStore>>,
    // kaspa-pq ADR-0022: overlay snapshot as-of the pruning point (serve + below-pp window consult).
    pub(super) pruning_overlay_snapshot_store: Arc<RwLock<DbPruningPointOverlaySnapshotStore>>,
    pub(super) dns_params: Option<DnsParams>,

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
    pub(super) vlt_credit_store: Arc<DbVltCreditStore>,
    /// MISAKA Compute Token Program (design v0.1 §9.2): the TOK ledger/supply/settlement
    /// family the buried-chain fold and the emission settlement write. Inert while every
    /// preset's token fence is `u64::MAX`.
    pub(super) token_store: Arc<DbTokenStore>,
    /// Audit-emission v0.2: whether this process already logged the base-coin audit fee's
    /// retirement (log-once marker, no consensus meaning).
    pub(super) audit_fee_retired_logged: std::sync::atomic::AtomicBool,
    /// MISAKA VLT PR 2: per-epoch frozen voting snapshots (§5). Frozen write-once at each wall
    /// epoch's boundary recompute; a cache and audit surface for the chain-derived denominator,
    /// never a verification source on branches whose derivation pins elsewhere.
    pub(super) vlt_voting_snapshot_store: Arc<DbVltVotingSnapshotStore>,
    /// MISAKA VLT PR 4: per-epoch §7.2 finality certificates, written once when an epoch's
    /// precommit quorum first counts on the selected chain.
    pub(super) dns_finality_certificate_store: Arc<DbDnsFinalityCertificateStore>,
    /// MISAKA VLT PR 3: whether this process has logged the frozen snapshot it RESUMED with.
    /// One line per process start, so a restart leaves grep-able proof that the persisted roots
    /// equal the ones the previous run froze.
    pub(super) vlt_snapshot_resume_logged: std::sync::atomic::AtomicBool,
    /// MISAKA: the last reported activation state, so a TRANSITION can be announced rather than
    /// left for an operator to infer from a periodic line changing shape. In-memory only — it is
    /// a report about the chain, not a fact of it, and it re-derives on the next recompute.
    pub(crate) vlt_state: Arc<Mutex<Option<VltActivationState>>>,
    /// MISAKA: the scrape-shaped gauges behind that state, updated on the same cadence. Read by
    /// `ConsensusApi::get_vlt_status` without re-walking anything.
    pub(crate) vlt_metrics: Arc<VltMetrics>,
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
            palw_tip_order: params.palw_tip_order_v1(),
            pow_palw_ollama_activation: params.pow_palw_ollama_activation,
            max_block_parents: params.max_block_parents(),
            mergeset_size_limit: params.mergeset_size_limit(),
            max_block_mass: params.max_block_mass,

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
            stake_bonds_store: storage.stake_bonds_store.clone(),
            compute_capability_store: storage.compute_capability_store.clone(),
            palw_carriage_store: storage.palw_carriage_store.clone(),
            palw_class_state_store: storage.palw_class_state_store.clone(),
            dns_state_store: storage.dns_state_store.clone(),
            vlt_activation_store: storage.vlt_activation_store.clone(),
            pruning_overlay_snapshot_store: storage.pruning_overlay_snapshot_store.clone(),
            evm_header_store: storage.evm_header_store.clone(),
            evm_state_store: storage.evm_state_store.clone(),
            evm_payload_store: storage.evm_payload_store.clone(),
            evm_heads_store: storage.evm_heads_store.clone(),
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
            evm_history_mode,
            evm_activation_daa_score: params.evm_activation_daa_score,
            evm_gas_pool_v2_activation_daa_score: params.evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score: params.evm_f002_withdraw_cap_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score: params.evm_f003_mldsa_verify_activation_daa_score,
            evm_typed_receipt_root_activation_daa_score: params.evm_typed_receipt_root_activation_daa_score,
            evm_lane_kpi: EvmLaneKpi::default(),
            dns_params: params.dns_params.clone(),
            palw_credit_params: params.palw_credit.clone(),
            utxo_diffs_store: storage.utxo_diffs_store.clone(),
            rewarded_epochs_store: storage.rewarded_epochs_store.clone(),
            epoch_accumulator_store: storage.epoch_accumulator_store.clone(),
            vlt_credit_store: storage.vlt_credit_store.clone(),
            token_store: storage.token_store.clone(),
            audit_fee_retired_logged: std::sync::atomic::AtomicBool::new(false),
            vlt_voting_snapshot_store: storage.vlt_voting_snapshot_store.clone(),
            dns_finality_certificate_store: storage.dns_finality_certificate_store.clone(),
            vlt_snapshot_resume_logged: std::sync::atomic::AtomicBool::new(false),
            vlt_state: Arc::new(Mutex::new(None)),
            vlt_metrics: Arc::new(VltMetrics::default()),
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

    fn bridge_finality_is_fresh(&self, current_daa_score: u64) -> bool {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return false;
        };
        let Ok(state) = self.dns_state_store.read().get() else {
            return false;
        };
        let dns_confirmed =
            is_dns_confirmed(state.work_depth, state.stake_depth, dns_params.required_work_depth, dns_params.required_stake_depth);
        dns_finality_fresh_for_bridge(
            dns_confirmed,
            state.last_dns_confirmed_anchor,
            state.last_dns_confirmed_anchor_daa_score,
            current_daa_score,
            dns_params.bridge_finality_max_staleness_daa_score,
        )
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

        let (new_sink, virtual_parent_candidates) = self.sink_search_algorithm(
            &virtual_read,
            &mut accumulated_diff,
            &mut accumulated_bond_view,
            prev_sink,
            tips,
            finality_point,
            pruning_point,
        );
        let (virtual_parents, virtual_ghostdag_data) = self.pick_virtual_parents(new_sink, virtual_parent_candidates, pruning_point);
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
    fn calculate_utxo_state_relatively(
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
                    self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, &*bond_view, pov_daa_score);

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

                        // Accumulate the diff
                        diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
                        // Update the diff point
                        diff_point = current;
                        if track_bonds {
                            // Advance the bond view by THIS block's mutations,
                            // derived from the in-memory acceptance data (its
                            // store entry is written by the commit just below).
                            let bond_muts = self.dns_bond_mutations_from_acceptance(
                                current,
                                &ctx.mergeset_acceptance_data,
                                &bond_view,
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
                        );
                        // Count the number of UTXO-processed chain blocks
                        chain_block_counter += 1;
                    }
                }
                Err(err) => panic!("unexpected error {err}"),
            }
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
        let pipelined = pipeline.and_then(|p| p.recv(current));
        let staged = match pipelined {
            Some(Ok(staged)) => Some(staged),
            Some(Err(msg)) => return Err(msg),
            None => {
                // AcceptedEvmTxs(B) source: the consensus-ordered mergeset (selected
                // parent first, then ascending blue work — §3.1 canonical order).
                let sorted_mergeset: Vec<BlockHash> =
                    ctx.ghostdag_data.consensus_ordered_mergeset(self.ghostdag_store.as_ref()).collect();
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
    fn update_evm_canonical_heads(&self, batch: &mut WriteBatch, sink: BlockHash) {
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
        let heads = kaspa_consensus_core::evm::CanonicalEvmHeads { latest: sink, safe: sink, finalized };
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
        if !consumed_locks.is_empty() || !result.withdrawals.is_empty() {
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
    ) {
        let mut batch = WriteBatch::default();
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

        // Update virtual state (capture the new sink first — `set_batch` moves the Arc).
        let dns_sink = new_virtual_state.ghostdag_data.selected_parent;
        virtual_write.state.set_batch(&mut batch, new_virtual_state).unwrap();

        // Update the virtual selected chain
        selected_chain_write.apply_changes(&mut batch, chain_path).unwrap();

        // kaspa-pq Phase 10 (ADR-0009 A.4): stage the DNS stake-bond set
        // changes into the same batch so they commit atomically with the
        // virtual state. Inert unless the overlay is configured.
        self.stage_dns_bond_mutations(&mut batch, chain_path);
        // Capability declarations, staged into the same batch so the pool a committee is drawn
        // from commits atomically with the bonds it is filtered against.
        if let Some(dns_params) = self.dns_params.as_ref() {
            let sink_daa = self.headers_store.get_header(dns_sink).map(|h| h.daa_score).unwrap_or_default();
            self.stage_compute_capabilities(&mut batch, chain_path, dns_params, sink_daa);
            // PALW carriage objects (ADR-0029 Stage 1), same accept/revert/backfill discipline,
            // same batch. An index only — nothing in consensus reads it yet.
            self.stage_palw_carriages(&mut batch, chain_path, dns_params, sink_daa);
        }
        // The class's last-credit DAA, in the SAME batch as everything else this chain move
        // commits, so a class's memory of when it last minted cannot end up one block out of step
        // with the chain that minted it.
        self.stage_palw_class_credit_marks(&mut batch, chain_path);

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

        // MISAKA Verified LLM Token-Weighted BFT: persist every credit epoch that has just become
        // finalized, so later commits serve `C_i(E)`'s old terms from the store instead of
        // re-verifying every certificate's ML-DSA-87 signatures over the whole credit window.
        // Inert below the VLT fence (`u64::MAX` on every shipped preset).
        if let Some(dns_params) = self.dns_params.as_ref() {
            let sink_daa = self.headers_store.get_daa_score(dns_sink).unwrap_or_default();
            self.stage_vlt_credits(&mut batch, dns_sink, sink_daa, dns_params);
            // MISAKA Compute Token Program (design v0.1 §9.2): fold newly-buried token ops
            // into the TOK ledger and settle emission for finalized epochs. Inert below the
            // token shadow fence (`u64::MAX` on every shipped preset). The held selected-chain
            // WRITE guard is passed through as the fold's reader: taking `.read()` here would
            // self-deadlock against it — which is exactly what froze five devnet nodes at the
            // first shadow-fence commit (daa 1640, 2026-08-10) before this parameter existed.
            self.stage_token(&mut batch, dns_sink, sink_daa, dns_params, &*selected_chain_write);
        }

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

    /// Seeds the per-block [`ActiveBondView`] walk (ADR-0009 Addendum B §B.1)
    /// from the `StakeBonds` store snapshot — which, at the start of
    /// `resolve_virtual`, reflects the bond set as-of the previous sink (the
    /// same anchor `accumulated_diff` starts from). Returns an empty view on
    /// networks without the overlay (`dns_params` is `None`), so the bond-view
    /// walk is a no-op there.
    /// The per-class state as this node's store holds it.
    ///
    /// Same shape as [`Self::initial_active_bond_view`] and the same reason: consumers read a VIEW,
    /// so a class fact is scoped to the chain being evaluated rather than to wherever the virtual
    /// tip happens to point (blocker 6(b)).
    pub(crate) fn initial_palw_class_state_view(&self) -> crate::model::stores::palw_class_state::PalwClassStateView {
        crate::model::stores::palw_class_state::PalwClassStateView::from_records(
            self.palw_class_state_store.read().iterator().filter_map(|r| r.ok()).map(|(id, rec)| (id, (*rec).clone())),
        )
    }

    pub(crate) fn initial_active_bond_view(&self) -> ActiveBondView {
        if self.dns_params.is_none() {
            return ActiveBondView::new();
        }
        ActiveBondView::from_records(
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (rec.bond_outpoint, (*rec).clone()))),
        )
    }

    /// Persist the capability declarations a selected-chain change adds, and drop those it
    /// removes.
    ///
    /// Same discipline as the bond mutations beside it, and simpler: a declaration has no state
    /// machine — it cannot be slashed or unbonded — so reverting one is deleting it. Both
    /// directions re-derive from retained acceptance data, so apply and revert cannot disagree.
    fn stage_compute_capabilities(&self, batch: &mut WriteBatch, chain_path: &ChainPath, dns_params: &DnsParams, sink_daa: u64) {
        if !dns_params.vlt_shadow_active_at(sink_daa) {
            return; // the whole compute overlay is dormant; write nothing
        }
        // The bond set as-of BEFORE this chain path (the WriteBatch is not yet applied, so the
        // store read excludes it). It is a set of stamped RECORDS, not a point-in-time view:
        // `verified_capability` judges activity against each record's own created/slashed/unbond
        // stamps, so for any block already durable this set answers as-of-that-block exactly.
        //
        // What it cannot answer is this path's OWN blocks — and reading the live snapshot here
        // was a consensus split in waiting: a node PRESENT at the time processed the bond and the
        // declaration that rides on it in separate commits, so its snapshot had the bond; a node
        // REPLAYING the same chain (IBD from genesis) carries both in one batch, the snapshot
        // misses the bond, `verified_capability` rejects the declaration, and the capability
        // store — hence the committee draw, the audit-fee outputs, and every certificate's
        // verified/unverified verdict — permanently diverges. Found live: a from-genesis node
        // disqualified 86 chain blocks on `[coinbase-mismatch] act=3 exp=2` (the two audit-fee
        // outputs it refused to expect), froze `capability_root=bee4c439…` against the mesh's
        // `578636b0…`, and credited zero weight forever.
        //
        // So the apply loop below advances this set block by block — verify a block's
        // declarations against the strict PREFIX, then fold in that block's own bond mutations —
        // which is exactly the order a present node experienced them in.
        let mut bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        let net_id = self.genesis.hash;

        // Derive every added block's bond mutations BEFORE the write guard, for exactly the reason
        // `stage_dns_bond_mutations` derives its own up front: the derivation reaches
        // `compute_challenge_adjudication_slashes` → `resolve_certificate`, which READS this very
        // store to draw a committee, and `parking_lot::RwLock` is not reentrant. Deriving inside
        // the guard parks the virtual processor against itself the first time a challenge crosses
        // its window under an active shadow fence — no thread visibly holding anything, the chain
        // simply stops. (The apply loop below still folds them in block by block: the ORDER is
        // what the strict-prefix rule needs, not the moment of derivation.)
        // Advanced across the path for the same reason `stage_dns_bond_mutations` advances its
        // copy: block N's evidence must be judged against the bonds blocks 0..N created.
        let mut evidence_view = self.initial_active_bond_view();
        let mut added_muts: Vec<Vec<BondMutation>> = Vec::with_capacity(chain_path.added.len());
        for h in chain_path.added.iter().copied() {
            let muts = self.dns_bond_mutations_for_chain_block(h, &evidence_view);
            evidence_view.apply(&muts);
            added_muts.push(muts);
        }

        let mut store = self.compute_capability_store.write();

        // A database whose chain predates this store has declarations accepted and no rows for
        // them, and an empty pool reads exactly like "nobody declared". Sweep history once, bounded
        // by the furthest back a declaration could still be live at any beacon this walk will ask
        // about: `max_capability_validity_blocks` before the oldest anchor in the credit window.
        //
        // The sweep starts at the PRE-PATH tip, never inside this path: its `bonds` answers
        // as-of-block only for already-durable blocks (see above), and every block of this path
        // is the apply loop's job anyway.
        if !store.is_backfilled() {
            // The sweep judges each historical block's declarations at THAT block's DAA, and it
            // never walks above the pre-path tip — so the live `bonds` above is the right input
            // even though its stamps are current: `effective_bond_status` is strictly DAA-monotone
            // (a stamp at `s` only answers differently for `pov >= s`), and bond records are never
            // deleted, only stamped. A bond slashed after a block was produced is therefore still
            // Active at that block's score, which is what a node present at the time saw. Flagged
            // by the 2026-08-11 audit as a live-store read; it is one, and it is sound for this
            // query — do not "fix" it into an as-of view that would answer the same and cost a
            // walk per historical block.
            let sweep_tip = chain_path
                .added
                .first()
                .and_then(|first| self.ghostdag_store.get_selected_parent(*first).ok())
                .unwrap_or_else(|| chain_path.added.last().copied().unwrap_or(self.genesis.hash));
            let horizon = dns_params.vlt.max_capability_validity_blocks.saturating_add(dns_params.vlt_credit_window_blue_score);
            let mut swept = 0usize;
            if let Ok(sink_blue) = self.headers_store.get_blue_score(sweep_tip) {
                for block in std::iter::once(sweep_tip).chain(self.reachability_service.default_backward_chain_iterator(sweep_tip)) {
                    let Ok(bs) = self.headers_store.get_blue_score(block) else { break };
                    if sink_blue.saturating_sub(bs) > horizon {
                        break;
                    }
                    let Ok(header) = self.headers_store.get_header(block) else { break };
                    for (tx_id, cap) in compute_capabilities_with_ids_from_accepted_txs(&self.accepted_txs_of_chain_block(block)) {
                        if let Some(record) =
                            verified_capability(cap, &bonds, net_id.as_byte_slice(), block, header.daa_score, &dns_params.vlt)
                        {
                            store.insert_batch(batch, tx_id, Arc::new(record)).unwrap();
                            swept += 1;
                        }
                    }
                }
            }
            info!("[vlt-credit] swept {swept} capability declaration(s) out of history into the capability store");
            // The marker goes into the SAME batch as the inserts, so it becomes durable with them
            // or not at all. A crash before the batch is written leaves the marker unset and the
            // next start sweeps again — idempotently, since a declaration keyed by its own
            // transaction id rewrites to the same value.
            store.mark_backfilled(batch).unwrap();
        }

        let mut reverted = 0usize;
        for removed in chain_path.removed.iter().rev() {
            for (tx_id, _) in compute_capabilities_with_ids_from_accepted_txs(&self.accepted_txs_of_chain_block(*removed)) {
                store.delete_batch(batch, tx_id).unwrap();
                reverted += 1;
            }
        }
        // The revert path is the half of a new consensus store that never runs until it matters,
        // and then runs during a reorg. Say so when it does: a declaration silently surviving a
        // branch it is not in would put that branch's verifiers on another branch's committee.
        if reverted > 0 {
            info!("[capability-store] reverted {reverted} declaration(s) that left the selected chain");
        }
        for (i, added) in chain_path.added.iter().enumerate() {
            let Ok(header) = self.headers_store.get_header(*added) else { continue };
            for (tx_id, cap) in compute_capabilities_with_ids_from_accepted_txs(&self.accepted_txs_of_chain_block(*added)) {
                // The same bond-binding and signature checks the walk applied, so a row can only
                // hold a declaration the credit walk would itself have accepted. `bonds` is the
                // strict prefix — earlier paths plus the added blocks already folded in below —
                // so a batch spanning a bond and its declaration verifies the declaration exactly
                // as a node that processed them in separate commits did.
                if let Some(record) =
                    verified_capability(cap, &bonds, net_id.as_byte_slice(), *added, header.daa_score, &dns_params.vlt)
                {
                    store.insert_batch(batch, tx_id, Arc::new(record)).unwrap();
                }
            }
            // THIS block's bond mutations join the set only after its own declarations were
            // judged: a same-block bond must not validate a same-block declaration on a replayer
            // when it could not on a present node.
            for mutation in added_muts.get(i).map(Vec::as_slice).unwrap_or_default() {
                match mutation {
                    BondMutation::Insert(outpoint, record) => {
                        if !bonds.iter().any(|b| b.bond_outpoint == *outpoint) {
                            bonds.push(record.clone());
                        }
                    }
                    BondMutation::Slash(outpoint, _) | BondMutation::Unbond(outpoint, _, _) => {
                        if let Some(existing) = bonds.iter_mut().find(|b| b.bond_outpoint == *outpoint) {
                            apply_bond_stamp(existing, mutation);
                        }
                    }
                }
            }
        }
    }

    /// Persist the PALW carriage objects a selected-chain change adds, and drop those it removes
    /// (ADR-0029 Stage 1 — the capability-store walk beside this one, verbatim).
    ///
    /// A pure index: NOTHING in consensus rules reads this store yet. Stage 2 (the credit gate,
    /// duty classification, offense grounding) is explicitly out of scope, exactly as the
    /// capability store landed before its committee-draw consumer. And unlike the capability walk
    /// there is no bond set to thread: admission already refused any PALW-band transaction whose
    /// payload fails `validate_palw_carriage_stage1_tx`, and the extractor re-applies the same
    /// context-free rules — stateless in, stateless out, so apply and revert cannot disagree and
    /// no strict-prefix ordering question exists here.
    ///
    /// Rows are keyed by carrying tx id (the revert-friendly capability key). Logical dedup —
    /// first-accepted-wins per `committed_root`, `(commitment_root, attester_id)`, `call_tx_id` —
    /// is the Stage-2 reader's business, exactly where ADR-0029 §2 assigns it: an index that
    /// dropped "duplicate" rows would erase the very carriers a reorg could promote to first.
    /// Verify a commitment carriage's ML-DSA-87 signature under ITS OWN context.
    ///
    /// The two PALW signature families use different contexts on purpose — a commitment signature
    /// must not verify as an attestation or vice versa — so they get two helpers rather than one
    /// with a parameter a caller can pass the wrong value for. A verification error (malformed key
    /// or signature) is `false`: unverifiable is not verified.
    fn verify_palw_commitment_signature(public_key: &[u8], digest: &kaspa_hashes::Hash, signature: &[u8]) -> bool {
        kaspa_txscript::verify_mldsa87_with_context(
            public_key,
            digest.as_bytes().as_slice(),
            signature,
            kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_MLDSA87_COMMITMENT_CONTEXT,
        )
        .unwrap_or(false)
    }

    /// Verify an execution attestation's ML-DSA-87 signature under the attestation context.
    fn verify_palw_attestation_signature(public_key: &[u8], digest: &kaspa_hashes::Hash, signature: &[u8]) -> bool {
        kaspa_txscript::verify_mldsa87_with_context(
            public_key,
            digest.as_bytes().as_slice(),
            signature,
            kaspa_consensus_core::palw_slash::PALW_S_MLDSA87_ATTESTATION_CONTEXT,
        )
        .unwrap_or(false)
    }

    /// Remember, per class, the DAA at which it last had a commitment credited.
    ///
    /// ADR-0033 §4e bounds an attacker's pre-unbonding gain as
    /// `base(C) × (unbonding / min_credit_interval + 1)`, which ASSUMES one credited job per
    /// interval. Nothing enforced it because nothing remembered the previous credit: the credit
    /// walk spans `w_challenge` backward and a commitment crosses `w_challenge` AFTER acceptance,
    /// so past credits are outside the walk by construction (audit B4). This is the memory.
    ///
    /// The mark is derived from the SAME function that mints — `compute_palw_credit_outputs` is
    /// re-run for each added block — rather than from a second predicate that could drift from it.
    /// Reverting a removed block clears the mark it set, so a reorg cannot leave a class believing
    /// it minted on a chain that no longer exists; a class whose mark is cleared is *permissive*
    /// again, which is the correct direction (the credits it was counting no longer exist either).
    fn stage_palw_class_credit_marks(&self, batch: &mut WriteBatch, chain_path: &ChainPath) {
        let Some(credit) = self.palw_credit_params.as_ref() else { return };
        let class_id = credit.registration.runtime_class_id;
        let mut store = self.palw_class_state_store.write();
        let Some(existing) = store.get(class_id) else { return };

        // Walk the removed side first, then the added side, so a block that appears in both nets
        // out — the same order `stage_dns_bond_mutations` uses for the same reason.
        let mut mark = existing.last_credited_daa;
        for removed in chain_path.removed.iter() {
            if let Ok(header) = self.headers_store.get_header(*removed)
                && mark == Some(header.daa_score)
            {
                mark = None;
            }
        }
        // The bond set must ADVANCE with the walk, at each added block's own chain point.
        //
        // `initial_active_bond_view()` is the node's pre-batch store snapshot — the set as of the
        // OLD sink — while the mint paths score a block against the set as of its selected parent
        // (`template_bond_view` when building, `selected_parent_bond_view` when validating). Using
        // the pre-batch snapshot for every added block scored blocks 2..n against a set that
        // excludes the bonds their own predecessors created, and scored every added block against
        // bonds that the removed side had created and that this chain move deletes. The mark it
        // writes is a consensus store row that `credit_interval_elapsed` reads back to gate the
        // MINT, so the disagreement does not stay local: it decides whether a later block is
        // allowed to credit at all.
        //
        // The construction mirrors `stage_dns_bond_mutations` deliberately, including its choice to
        // derive the removed side's mutations against the STARTING view — those mutations were
        // derived under that view when they were applied, so re-deriving under it is what makes the
        // revert an exact inverse. Two different reconstructions of the same walk are two chances
        // to disagree with the store, so this one copies rather than improves.
        let mut bond_view = self.initial_active_bond_view();
        let removed_muts: Vec<Vec<BondMutation>> =
            chain_path.removed.iter().rev().copied().map(|h| self.dns_bond_mutations_for_chain_block(h, &bond_view)).collect();
        for muts in removed_muts {
            bond_view.revert(&muts);
        }
        for added in chain_path.added.iter() {
            let Ok(header) = self.headers_store.get_header(*added) else { continue };
            let Ok(parent) = self.ghostdag_store.get_selected_parent(*added) else { continue };
            // Scored against the parent's set, then advanced past this block — the same order the
            // mint paths see, and the reason `apply` happens after `compute_palw_credit_outputs`.
            let bonds = bond_view.records();
            let view = crate::model::stores::palw_class_state::PalwClassStateView::from_records([(
                class_id,
                crate::model::stores::palw_class_state::PalwClassStateRecord { last_credited_daa: mark, ..(*existing).clone() },
            )]);
            if !self.compute_palw_credit_outputs(credit, header.daa_score, parent, &bonds, &view).is_empty() {
                mark = Some(header.daa_score);
            }
            bond_view.apply(&self.dns_bond_mutations_for_chain_block(*added, &bond_view));
        }
        if mark != existing.last_credited_daa {
            let updated =
                crate::model::stores::palw_class_state::PalwClassStateRecord { last_credited_daa: mark, ..(*existing).clone() };
            if let Err(err) = store.insert_batch(batch, class_id, std::sync::Arc::new(updated)) {
                kaspa_core::warn!("[palw-class-state] could not stage the last-credit mark: {err}");
            }
        }
    }

    fn stage_palw_carriages(&self, batch: &mut WriteBatch, chain_path: &ChainPath, dns_params: &DnsParams, sink_daa: u64) {
        if !dns_params.vlt_shadow_active_at(sink_daa) {
            // Same dormancy fence as the capability walk it mirrors: the carriage objects bind to
            // the same bond registry, their Stage-2 consumer is fenced with the compute overlay,
            // and below the fence the backfill's history walk would be pure cost. (The band's
            // ADMISSION is deliberately unfenced, like every overlay band's — see
            // `check_transaction_subnetwork` — so acceptance below the fence is possible but
            // meaningless: the first post-fence commit backfills whatever it carried.)
            return;
        }
        let mut store = self.palw_carriage_store.write();

        // A database whose chain predates this store has carriers accepted and no rows for them,
        // and an empty index reads exactly like "nothing was carried". Sweep history once, under
        // the same horizon discipline as the capability sweep — the furthest back any overlay
        // read this store will ever serve can reach.
        if !store.is_backfilled() {
            let sweep_tip = chain_path
                .added
                .first()
                .and_then(|first| self.ghostdag_store.get_selected_parent(*first).ok())
                .unwrap_or_else(|| chain_path.added.last().copied().unwrap_or(self.genesis.hash));
            let horizon = dns_params.vlt.max_capability_validity_blocks.saturating_add(dns_params.vlt_credit_window_blue_score);
            let mut swept = 0usize;
            if let Ok(sink_blue) = self.headers_store.get_blue_score(sweep_tip) {
                for block in std::iter::once(sweep_tip).chain(self.reachability_service.default_backward_chain_iterator(sweep_tip)) {
                    let Ok(bs) = self.headers_store.get_blue_score(block) else { break };
                    if sink_blue.saturating_sub(bs) > horizon {
                        break;
                    }
                    let Ok(header) = self.headers_store.get_header(block) else { break };
                    for (tx_id, record) in
                        palw_carriage_records_from_accepted_txs(&self.accepted_txs_of_chain_block(block), header.daa_score)
                    {
                        store.insert_batch(batch, tx_id, Arc::new(record)).unwrap();
                        swept += 1;
                    }
                }
            }
            info!("[palw-carriage] swept {swept} carriage object(s) out of history into the carriage store");
            // The marker goes into the SAME batch as the inserts, so it becomes durable with them
            // or not at all. A crash before the batch is written leaves the marker unset and the
            // next start sweeps again — idempotently, since a row keyed by its own transaction id
            // rewrites to the same value.
            store.mark_backfilled(batch).unwrap();
        }

        let mut reverted = 0usize;
        for removed in chain_path.removed.iter().rev() {
            // Only the keys matter on revert; the stamp the extractor puts on the discarded
            // records is irrelevant, so a header miss defaults it rather than skipping deletes.
            let daa = self.headers_store.get_header(*removed).map(|h| h.daa_score).unwrap_or_default();
            for (tx_id, _) in palw_carriage_records_from_accepted_txs(&self.accepted_txs_of_chain_block(*removed), daa) {
                store.delete_batch(batch, tx_id).unwrap();
                reverted += 1;
            }
        }
        // The revert path of a new store never runs until it matters, and then runs during a
        // reorg. Say so when it does: a carriage row silently surviving a branch it is not in is
        // exactly the indexing divergence Stage 1 exists to rule out.
        if reverted > 0 {
            info!("[palw-carriage] reverted {reverted} carriage object(s) that left the selected chain");
        }
        for added in chain_path.added.iter() {
            let Ok(header) = self.headers_store.get_header(*added) else { continue };
            for (tx_id, record) in palw_carriage_records_from_accepted_txs(&self.accepted_txs_of_chain_block(*added), header.daa_score)
            {
                store.insert_batch(batch, tx_id, Arc::new(record)).unwrap();
            }
        }
    }

    /// Re-derives the [`BondMutation`]s a chain block contributed, from its
    /// retained acceptance data (ADR-0009 Addendum A.4). Deterministic, so it
    /// serves both apply (added) and revert (removed).
    fn dns_bond_mutations_for_chain_block(&self, chain_block: BlockHash, bond_view: &ActiveBondView) -> Vec<BondMutation> {
        let accepted_daa_score = self.headers_store.get_header(chain_block).unwrap().daa_score;
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        let txs = self.accepted_txs_of_chain_block(chain_block);
        let mut muts = self.dns_bond_mutations_from_txs(&txs, bond_view, accepted_daa_score, min_bond, unbonding_floor);
        // §7(b)/(c): the mutations a challenge ADJUDICATION implies at this block. Appended after
        // the transaction-derived ones so the order is deterministic, and derived from the same
        // chain data on both apply and revert.
        muts.extend(self.compute_challenge_adjudication_slashes(chain_block, bond_view, accepted_daa_score));
        muts
    }

    /// §7(b)/(c): the bonds a settled challenge slashes, for the chain block where its certificate
    /// leaves the challenge window.
    ///
    /// A compute fraud proof is a claim about a computation, and consensus cannot re-run the job to
    /// test it. What it can do is wait for the certificate's own sortitioned committee, whose
    /// confirmations each carry a [`ReplayResiduals`](kaspa_consensus_core::vlt::ReplayResiduals)
    /// proof of independent execution, and read the answer off that:
    ///
    /// * a drawn verifier refuted ⇒ the challenge stands ⇒ the **executor** loses its bond (§7(b));
    /// * the certificate cleared verification ⇒ the challenge is disproved ⇒ the **challenger**
    ///   loses its bond (§7(c) — "Challenge に失敗した実行を正しいものとして claim すること");
    /// * anything less ⇒ undecided, and nobody is slashed. A quiet committee is a reason to wait.
    ///
    /// The window crossing is the trigger because it happens exactly once per certificate per
    /// chain, so the mutation applies once and reverts cleanly, with no dedup state.
    ///
    /// Inert below the **shadow** fence — every shipped network — where no verdict is ever counted.
    /// Above it this walks the compute overlay per chain block, the same cost noted on
    /// [`Self::compute_audit_fee_outputs`] and fixable the same way.
    ///
    /// Shadow, not weight: a credit table accumulated without slashing is a table nobody was
    /// policed for producing, and switching the vote onto it later would weight exactly that. The
    /// overlay's enforcement has to be live for the whole soak, not switched on with the vote.
    fn compute_challenge_adjudication_slashes(
        &self,
        chain_block: BlockHash,
        bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> Vec<BondMutation> {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return Vec::new();
        };
        if !dns_params.vlt_shadow_active_at(daa_score) {
            return Vec::new();
        }
        let Ok(parent) = self.ghostdag_store.get_selected_parent(chain_block) else {
            return Vec::new();
        };
        let (Ok(parent_daa), Ok(parent_blue)) = (self.headers_store.get_daa_score(parent), self.headers_store.get_blue_score(parent))
        else {
            return Vec::new();
        };
        let net_id_hash = self.genesis.hash;
        let net_id = net_id_hash.as_byte_slice();
        // The bond set as the CALLER's chain view holds it, never the live store: a replayer
        // batches many blocks into one virtual advance, so its store lacks every bond created
        // earlier in that batch, and an adjudication that cannot resolve a bond silently reaches
        // a different verdict than a node that lived the blocks one at a time. That is the same
        // IBD/live divergence class the capability staging fix closed (86 disqualified blocks).
        let bonds: Vec<StakeBondRecord> = bond_view.records();
        let window = dns_params.vlt.challenge_window_blocks;
        let anchors = self.canonical_anchors_in_window(parent, dns_params, dns_params.vlt_credit_window_blue_score);
        let oldest_blue = parent_blue.saturating_sub(dns_params.vlt_credit_window_blue_score);
        let walk = self.walk_compute_overlay(parent, &bonds, net_id, dns_params, oldest_blue, parent_blue);

        let mut crossing: Vec<&(TransactionId, ComputeCertificatePayload, u64)> = walk
            .certificates
            .iter()
            .filter(|(_, _, accepted)| parent_daa.saturating_sub(*accepted) <= window && daa_score.saturating_sub(*accepted) > window)
            .collect();
        crossing.sort_by_key(|(tx_id, _, accepted)| (*accepted, *tx_id));

        let mut muts = Vec::new();
        for (cert_tx_id, cert, accepted) in crossing {
            let mut challenges: Vec<&ComputeChallengePayload> =
                walk.challenges.iter().filter(|c| c.certificate_tx_id == *cert_tx_id).collect();
            if challenges.is_empty() {
                continue;
            }
            challenges.sort_by_key(|c| (c.challenger_bond_outpoint.transaction_id, c.challenger_bond_outpoint.index, c.kind as u8));

            let resolution = anchors.get(&cert.epoch).and_then(|anchor| {
                self.resolve_certificate(cert, *accepted, anchor, &bonds, &walk, dns_params, net_id, &anchors).ok().map(|resolved| {
                    let attestations = verdicts_for_certificate(
                        &walk.verdicts,
                        *cert_tx_id,
                        resolved.job_id,
                        resolved.receipt_hash,
                        *accepted,
                        &resolved.committee,
                    );
                    (resolved.receipt_hash, attestations)
                })
            });
            let (receipt_hash, attestations) = match &resolution {
                Some((h, a)) => (*h, a.as_slice()),
                None => (Hash64::default(), [].as_slice()),
            };
            for c in challenges {
                match adjudicate_compute_challenge(
                    c.kind,
                    resolution.is_some(),
                    receipt_hash,
                    attestations,
                    dns_params.vlt.min_verifier_confirmations,
                    dns_params.vlt.min_verifier_refutations,
                ) {
                    // The challenge stands: the bond it named — the executor's, pinned to this
                    // certificate by the walk — is the one that loses.
                    ChallengeOutcome::Succeeded => muts.push(BondMutation::Slash(cert.executor_bond_outpoint, daa_score)),
                    // The challenge is disproved: the challenger staked its own collateral on the
                    // claim, and that is what §7(c) takes.
                    ChallengeOutcome::Failed => muts.push(BondMutation::Slash(c.challenger_bond_outpoint, daa_score)),
                    ChallengeOutcome::Undecided => {}
                }
            }
        }
        muts
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
        muts.extend(self.palw_equivocation_slashes(txs, bond_view, accepted_daa_score));
        muts
    }

    /// Slashes proved by PALW executor-equivocation certificates accepted in this block — the
    /// first PALW offence that reaches a bond at all (re-audit blocker 8: nothing did, so every
    /// `P(detection) × slash` in the design multiplied by zero).
    ///
    /// Unlike the VLT half above, this does not derive-then-filter. There, the mutations come
    /// from payload decoding and a second pass drops the ones whose evidence was never proved —
    /// necessary because evidence arriving by MERGE skips the own-body genuineness gates. Here
    /// the adjudication IS the proof and it runs per certificate, so the only mutation that can
    /// be emitted is one already proved against the accused bond's own key. There is nothing for
    /// a filter to remove.
    ///
    /// Every rejection is a silent skip rather than an error: a certificate that fails to prove
    /// an equivocation is simply not evidence, and a transaction carrying one is still a valid
    /// transaction. Nothing here can reject a block.
    ///
    /// Fenced on `palw_credit`, so it is inert on every shipped preset. Enabling it means
    /// enabling the PALW machinery as a whole, which `Params::validate_palw_v1` gates — and the
    /// credit walk behind that same fence still carries the audit's blocker 11, so the fence is
    /// not flippable for this path alone.
    fn palw_equivocation_slashes(
        &self,
        txs: &[Transaction],
        bond_view: &ActiveBondView,
        accepted_daa_score: u64,
    ) -> Vec<BondMutation> {
        use kaspa_consensus_core::palw_slash::PALW_S_MLDSA87_ATTESTATION_CONTEXT;
        let verify = |key: &[u8], digest: &kaspa_hashes::Hash, signature: &[u8]| {
            matches!(verify_mldsa87_with_context(key, &digest.as_bytes(), signature, PALW_S_MLDSA87_ATTESTATION_CONTEXT), Ok(true))
        };
        let fence = self.palw_credit_params.is_some();
        // ADR-0009 Addendum A.3: the network discriminator IS the genesis hash. Passed from the
        // chain rather than read out of the evidence, so a certificate honestly signed on another
        // network cannot slash a bond here.
        let chain_network_id = self.genesis.hash;
        let mut out =
            palw_equivocation_slashes_v1(txs, bond_view, accepted_daa_score, chain_network_id.as_byte_slice(), fence, verify);
        // Arithmetic convictions need the model's weight rows to recompute a step. Serving them
        // is a node-local capability, not a consensus input, and a node that cannot serve them
        // adjudicates `Unadjudicable` — which convicts nobody, so the derivation stays a pure
        // function of the chain for every node that CAN. Wiring a real oracle is the Track-D
        // step that turns this arm on; until then it is structurally present and derives nothing.
        out.extend(palw_step_conviction_slashes_v1(
            txs,
            bond_view,
            accepted_daa_score,
            chain_network_id.as_byte_slice(),
            fence,
            &NoStepWeights,
            verify,
        ));
        out
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
            let block_txs = self.block_transactions_store.get(mergeset.block_hash).unwrap();
            for entry in mergeset.accepted_transactions.iter() {
                if let Some(tx) = block_txs.get(entry.index_within_block as usize) {
                    txs.push(tx.clone());
                }
            }
        }
        txs
    }

    /// Resolves the accepted transactions represented by the current virtual state. Unlike a
    /// committed chain block, the virtual state has no persisted `AcceptanceData`; it keeps only the
    /// accepted tx ids. Re-walk the virtual selected-parent + mergeset in consensus order and keep
    /// the ids the virtual UTXO calculation accepted. This lets template-only consensus checks see
    /// the same parent-body attestations that block validation later receives through
    /// `ctx.mergeset_acceptance_data`.
    pub(super) fn accepted_txs_from_virtual_state(&self, virtual_state: &VirtualState) -> Vec<Transaction> {
        if virtual_state.accepted_tx_ids.is_empty() {
            return Vec::new();
        }
        let accepted: HashSet<_> = virtual_state.accepted_tx_ids.iter().copied().collect();
        once(virtual_state.ghostdag_data.selected_parent)
            .chain(virtual_state.ghostdag_data.consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.deref()))
            .flat_map(|block| (*self.block_transactions_store.get(block).unwrap()).clone())
            .filter(|tx| accepted.contains(&tx.id()))
            .collect()
    }

    /// [`BondMutation`]s for a block whose acceptance data is held in-memory
    /// (the `KeyNotFound` chain block currently being UTXO-validated, before
    /// its `acceptance_data_store` entry is committed). Mirrors
    /// [`Self::dns_bond_mutations_for_chain_block`] but sources the accepted
    /// txs from the provided acceptance data instead of the store.
    fn dns_bond_mutations_from_acceptance(
        &self,
        chain_block: BlockHash,
        acceptance_data: &AcceptanceData,
        bond_view: &ActiveBondView,
        accepted_daa_score: u64,
    ) -> Vec<BondMutation> {
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        let txs = self.accepted_txs_from_acceptance_data(acceptance_data);
        let mut muts = self.dns_bond_mutations_from_txs(&txs, bond_view, accepted_daa_score, min_bond, unbonding_floor);
        // The adjudication reads only the SELECTED PARENT's chain, never this block's own
        // acceptance data, so it produces the same mutations here as it does from the store — which
        // it must, or the in-memory bond view and the persisted one would drift apart.
        muts.extend(self.compute_challenge_adjudication_slashes(chain_block, bond_view, accepted_daa_score));
        muts
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
        // MISAKA VLT PR 3: once per process, surface the frozen snapshot this node RESUMED with —
        // grep-able proof that the persisted roots equal the ones the previous run's "frozen"
        // line logged for the same epoch. Before the throttle, because the first recompute after
        // a restart usually lands inside an already-frozen epoch, which the freeze block below
        // never revisits.
        if dns_params.vlt_shadow_active_at(sink_daa)
            && !self.vlt_snapshot_resume_logged.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            let wall_epoch = sink_blue / epoch_len_blue;
            for w in [wall_epoch, wall_epoch.saturating_sub(1)] {
                if let Ok(row) = self.vlt_voting_snapshot_store.get(w) {
                    info!(
                        "[vlt-voting-snapshot] resumed epoch={w} snapshot_root={} validator_set_root={} vote_commitment={} total_weight={} quorum_weight={} (persisted across restart)",
                        row.snapshot_root,
                        row.validator_set_root,
                        row.vote_commitment(),
                        row.total_weight,
                        row.quorum_weight,
                    );
                    break;
                }
            }
        }
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
        let active_validators = active_stakes_at_sink.len() as u32;
        let hard_mandatory_active = sink_daa >= dns_params.mandatory_attestation_inclusion_daa_score;
        let capacity = mandatory_attestation_mass_capacity(
            active_stakes_at_sink.iter().copied(),
            total_active,
            0,
            dns_params.stake_event_quality_floor_bps,
            self.max_block_mass,
            dns_params.max_attestation_shard_mass,
        );
        let rollout_stage = if sink_daa >= dns_params.dns_activation_daa_score
            && total_active >= dns_params.min_active_stake_sompi
            && active_validators >= dns_params.min_active_validators
            // kaspa-pq DNS v3 (PR6): refuse Active unless the blue_score canonical-anchor params
            // are self-consistent. In Active the reorg gate's finality depends entirely on them,
            // so an invalid config fails safe (stay Bootstrap, gate dormant) rather than splitting.
            && dns_params.dns_v3_params_consistent()
            // MISAKA VLT: the same fail-safe for the VLT knobs, and for the reason the fence was
            // split in two. A preset whose weight fence does not sit a full credit window above
            // its shadow fence would move the vote onto a `C_i(E)` that has not finished filling —
            // `W(E)` short or zero, no epoch at quorum, the reorg gate armed over a denominator
            // that means nothing. Staying in Bootstrap keeps the gate dormant instead. Trivially
            // true on every shipped (inert) preset, so this cannot demote a current network.
            && dns_params.vlt_params_consistent()
            // kaspa-pq optional hard mandatory capacity: only hard-inclusion deployments require
            // proving that the current stake distribution can physically reach φS in one block.
            // Shipped liveness-first presets keep mandatory inclusion at u64::MAX, so capacity
            // cannot demote DNS to Bootstrap or halt finality/reward accounting.
            && (!hard_mandatory_active || capacity.fits)
        {
            DnsRolloutStage::Active
        } else {
            DnsRolloutStage::Bootstrap
        };

        // kaspa-pq DNS v3: canonical, blue_score-coordinated StakeScore. Credit only
        // attestations naming THIS chain's canonical lagged anchor for their (ready,
        // non-duplicate) epoch, with the per-epoch denominator keyed by the canonical anchor
        // DAA and zero-attestation ready epochs included (`collect_stake_contributions_v2`).
        // MISAKA Verified LLM Token-Weighted BFT: read each validator's challenge-window-survived
        // verified compute first — it is the input to every weight below. Empty (and free) while
        // the VLT fence is inert.
        //
        // The pin here is the sink itself, because this recompute has exactly one chain in view:
        // it is scoring the selected chain, not comparing it to anything. The place where two
        // branches ARE compared is `dns_reorg_outcome`, and that one pins at the block they share
        // (see `stake_score_since_ancestor`) — which is what stops a competing branch from
        // bringing its own denominator to the comparison this state feeds.
        // Built from the SHADOW fence, so the table is already full — and already observable —
        // when the weight fence opens below.
        let snapshot = self.vlt_epoch_snapshot(
            sink,
            sink_daa,
            &bonds,
            net_id.as_byte_slice(),
            dns_params,
            dns_params.vlt_shadow_active_at(sink_daa),
            // The reporting caller: this is the per-epoch state recompute, and the one place a
            // credit diagnostic belongs.
            true,
        );
        let credit_rule = dns_params.epoch_credit_rule(sink_daa);
        let weight = if dns_params.vlt_weighting_active_at(sink_daa) {
            ContributionWeight::Vlt { snapshot: &snapshot, vlt: &dns_params.vlt }
        } else {
            ContributionWeight::BondedStake
        };

        let (contributions, epoch_anchor_daa) =
            self.collect_stake_contributions_v2(sink, None, &bonds, net_id.as_byte_slice(), dns_params, weight);

        let totals = self.total_weight_by_epoch(sink, &bonds, net_id.as_byte_slice(), dns_params, &epoch_anchor_daa, weight);
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

        // MISAKA: report where this network sits on the activation path, POSITIVELY.
        //
        // The first version of this reported only the shadow phase, so "the weight fence engaged"
        // had to be inferred from a log line no longer appearing — which is indistinguishable from
        // the node having stopped, and is not a thing to build an operational check on. Every
        // state now announces itself by name, and the numbers behind the decision come with it.
        //
        // `FenceReachedNoSnapshot` is the state two booleans could not express and the one this
        // exists for: the fence is behind us and there is still nothing to vote with, so finality
        // is waiting rather than live. Reporting that as "active" is how a network says it is
        // healthy while finalizing nothing.
        let vlt_state = {
            let shadow_active = dns_params.vlt_shadow_active_at(sink_daa);
            let weight_active = dns_params.vlt_weighting_active_at(sink_daa);
            // Under the weight rule the tallies above are already VLT-denominated; in the soak
            // they are stake-denominated, so the shadow view has to be computed separately. That
            // second pass is the price of being able to see the soak at all.
            let vlt_epochs = if weight_active {
                per_epoch.clone()
            } else if shadow_active {
                let shadow_weight = ContributionWeight::Vlt { snapshot: &snapshot, vlt: &dns_params.vlt };
                let (shadow_contribs, shadow_anchor_daa) =
                    self.collect_stake_contributions_v2(sink, None, &bonds, net_id.as_byte_slice(), dns_params, shadow_weight);
                let shadow_totals =
                    self.total_weight_by_epoch(sink, &bonds, net_id.as_byte_slice(), dns_params, &shadow_anchor_daa, shadow_weight);
                aggregate_epoch_tallies(&shadow_contribs, &shadow_totals)
            } else {
                Vec::new()
            };
            let newest = vlt_epochs.last().map(|t| (t.epoch, t.total_weight));
            let snapshot_root = snapshot.commitment_root();
            let last_anchor = prev_dns_state.as_ref().map_or(Hash64::default(), |p| p.last_dns_confirmed_anchor);
            // Eligibility, not just magnitude. The snapshot must be a complete answer, pinned at a
            // DNS-confirmed anchor (so it is a fact about the shared prefix rather than about
            // whichever tip this node happens to hold), carry post-cap weight at or above the
            // floor, and have that weight spread over enough validators that a quorum is not one
            // of them. See `vlt_activation_eligibility`.
            let eligibility = vlt_activation_eligibility(
                snapshot.resolution_complete(),
                newest.map_or(0, |(_, w)| w),
                snapshot.credits().len(),
                last_anchor != Hash64::default(),
                &dns_params.vlt,
            );
            // MISAKA VLT PR 1: the activation guard is a persisted state machine, not a
            // per-recompute opinion. Step the stored record one epoch forward — reserve on the
            // first eligible snapshot (for the NEXT epoch, never this one), re-evaluate at the
            // boundary, cancel if the proof stopped holding — and persist the step in the same
            // batch as the DnsState it travels with, so a restart resumes the reservation instead
            // of re-deriving a fresh opinion. "Already active" is the record's word now: the old
            // `last_anchor != default` proxy let bootstrap confirmations masquerade as a moved
            // vote, reporting Recovery on networks that had never activated.
            let prev_record = self.vlt_activation_store.read().get().ok();
            // The chain-canonical reservation epoch: walk the write-once frozen-snapshot rows
            // backward from the wall epoch and find where the current contiguous run of
            // magnitude-eligible snapshots begins. Those rows are reconstructed identically by a
            // replaying or importing node (the §5 roots equality), so the reservation this stamp
            // produces is the same on every sync path — unlike the observation epoch, which
            // trails the chain by however coarsely this node's recomputes happened to land.
            let canonical_scheduled_epoch = {
                let wall_epoch = sink_blue / epoch_len_blue;
                let floor = wall_epoch.saturating_sub(kaspa_consensus_core::vlt::CANONICAL_RESERVATION_SCAN_CAP);
                let row_eligible = |e: u64| {
                    self.vlt_voting_snapshot_store
                        .get(e)
                        .map(|row| {
                            kaspa_consensus_core::vlt::snapshot_row_magnitude_eligible(
                                row.total_weight,
                                row.validators.len(),
                                &dns_params.vlt,
                            )
                        })
                        .unwrap_or(false)
                };
                // The current wall epoch's row freezes at its boundary, so mid-epoch the newest
                // row can sit one (or, after a stall, a few) epochs back — skip the missing head
                // first, then walk the contiguous eligible run to its start.
                let mut e = wall_epoch;
                while e > floor && !self.vlt_voting_snapshot_store.has(e).unwrap_or(false) {
                    e -= 1;
                }
                let mut run_start = wall_epoch;
                while row_eligible(e) {
                    run_start = e;
                    if e == floor || e == 0 {
                        break;
                    }
                    e -= 1;
                }
                run_start
            };
            let (new_record, state) = tick_vlt_activation(
                shadow_active,
                weight_active,
                prev_record.as_ref(),
                sink_blue / epoch_len_blue,
                newest,
                snapshot_root,
                last_anchor,
                eligibility,
                dns_params.vlt.vlt_activation_daa_score,
                canonical_scheduled_epoch,
            );
            if let Some(record) = new_record
                && prev_record.as_ref() != Some(&record)
            {
                // A record transition is rarer than an epoch and is the durable event an
                // operator audits after the fact, so it is announced at warn like the state
                // transitions below — with the numbers the decision was made on.
                warn!(
                    "[vlt-activation-record] {} -> {} at epoch={} (scheduled_at={} activation_epoch={} snapshot_epoch={} snapshot_root={} total_weight={} quorum_weight={})",
                    prev_record.as_ref().map_or("none", |r| r.state.as_str()),
                    record.state.as_str(),
                    sink_blue / epoch_len_blue,
                    record.scheduled_at_epoch,
                    record.activation_epoch,
                    record.snapshot_epoch,
                    record.snapshot_root,
                    record.total_weight,
                    record.quorum_weight,
                );
                self.vlt_activation_store.write().set_batch(batch, record).unwrap();
            }
            let quorum_epochs = vlt_epochs
                .iter()
                .filter(|t| meets_bft_quorum(t.signed_weight, t.total_weight, dns_params.vlt.min_network_compute))
                .count();
            match &state {
                VltActivationState::PreShadow => {}
                VltActivationState::Shadow => info!(
                    "[vlt-shadow] sink_daa={sink_daa} {} validator(s) with credit; {quorum_epochs}/{} epoch(s) would reach quorum; newest epoch {:?}: W(E)={} signed={} snapshot_root={snapshot_root} — the weight fence is at {}",
                    snapshot.credits().len(),
                    vlt_epochs.len(),
                    newest.map(|(e, _)| e),
                    newest.map(|(_, w)| w).unwrap_or(0),
                    vlt_epochs.last().map(|t| t.signed_weight).unwrap_or(0),
                    dns_params.vlt.vlt_activation_daa_score,
                ),
                // The fence and the activation are no longer the same event, so they are no longer
                // the same line: this one says the fence is behind us and names the single
                // condition still in the way.
                VltActivationState::AwaitingEligibleSnapshot { weight_fence_daa, blocker } => info!(
                    "[vlt-weight-fence-reached] daa={sink_daa} fence={weight_fence_daa} — [vlt-activation-delayed] reason=no_eligible_snapshot blocker={} weight_source=bootstrap validators_with_credit={}",
                    blocker.as_str(),
                    snapshot.credits().len(),
                ),
                VltActivationState::ActivationScheduled { activation_epoch, source_anchor, snapshot_root, total_weight } => info!(
                    "[vlt-activation-scheduled] activation_epoch={activation_epoch} source_anchor={source_anchor} snapshot_root={snapshot_root} total_weight={total_weight}"
                ),
                VltActivationState::Recovery { last_finalized_anchor, total_weight, min_network_compute } => info!(
                    "[vlt-finality-inactive] reason={} total_weight={total_weight} min_network_compute={min_network_compute} last_finalized_anchor={last_finalized_anchor} — holding the last finalized anchor until weight returns",
                    if *total_weight == 0 { "zero_total_weight" } else { "below_min_network_compute" },
                ),
                VltActivationState::Active { epoch, snapshot_root, total_weight, quorum_weight } => info!(
                    "[vlt-weight-snapshot-activated] epoch={epoch} snapshot_root={snapshot_root} total_weight={total_weight} quorum_weight={quorum_weight} quorum_epochs={quorum_epochs}/{} validators_with_credit={}",
                    vlt_epochs.len(),
                    snapshot.credits().len(),
                ),
            }
            state
        };
        // Announce a CHANGE at warn level too. The per-epoch lines above are the steady state; a
        // transition is the thing an operator wants paged on, and the two fences are each a
        // one-time event that must not be buried in a periodic report.
        //
        // By LABEL, not by value: `Active` carries the live epoch/root/weights, so comparing
        // values pages "active -> active" once per epoch forever — steady state dressed as an
        // event. The stored value still updates, so gauges and future consumers see the numbers.
        {
            let mut last = self.vlt_state.lock().unwrap();
            if last.as_ref().map(|s| s.label()) != Some(vlt_state.label()) {
                warn!("[vlt-state] {} -> {} at daa={sink_daa}", last.as_ref().map_or("none", |s| s.label()), vlt_state.label());
            }
            *last = Some(vlt_state.clone());
        }
        self.vlt_metrics.record(&vlt_state, sink_daa);

        // MISAKA VLT PR 2 (§5): freeze this wall epoch's voting snapshot, once, at its boundary
        // recompute — "the validator set and its weights are fixed within an epoch" as a write.
        // From the SHADOW fence like the credit table itself, so the frozen rows are observable a
        // full credit window before any vote binds them. Write-once (`has` guards), never an
        // incomplete resolution (a local loading limit must not become "the" denominator), and in
        // the same batch as the DnsState this recompute produces.
        if dns_params.vlt_shadow_active_at(sink_daa) {
            let wall_epoch = sink_blue / epoch_len_blue;
            if !self.vlt_voting_snapshot_store.has(wall_epoch).unwrap_or(false)
                && let Some(snap) = self.voting_snapshot_for_wall_epoch(sink, wall_epoch, &bonds, net_id.as_byte_slice(), dns_params)
            {
                if snap.resolution_complete {
                    Self::log_frozen_snapshot(
                        &snap,
                        wall_epoch,
                        &format!("pinned at {} daa={}", snap.source_finalized_anchor, snap.source_anchor_daa),
                    );
                    self.vlt_voting_snapshot_store.set_batch(batch, wall_epoch, snap).unwrap();
                } else {
                    info!(
                        "[vlt-voting-snapshot] epoch={wall_epoch}: resolution incomplete at the boundary; not freezing (will retry lazily on the sign path)"
                    );
                }
            }
        }

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

        // MISAKA §5 round 2. Everything above this is the prevote tally — enough weight approved
        // the anchor. Round 2 asks the stronger question: has enough weight **locked** on it,
        // each signer having published the lock it was carrying at the time. Only then is the
        // anchor confirmed, so two conflicting anchors cannot both finalize without more than a
        // third of the weight having signed both locks.
        //
        // `true` below the VLT weight fence, where the round does not exist: every current network
        // keeps its single-round confirmation byte-for-byte.
        let anchor_epoch_precommitted = if dns_params.vlt_weighting_active_at(sink_daa) {
            let prevoted = quorum_epochs(&per_epoch, dns_params.vlt.min_network_compute);
            let (precommitted, counted_records) =
                self.precommitted_epochs(sink, &bonds, net_id.as_byte_slice(), dns_params, weight, &totals, &prevoted);
            // PR 4: quorum lines for the newest few tallied epochs — signed weight against the
            // frozen W and Q, and where the two rounds stand. The newest epoch's votes are still
            // landing when its boundary prints, so the SETTLED picture an operator (and the §8
            // four-case experiment) needs is in the epochs just behind it — three lines covers
            // both without turning the recompute into a table dump.
            for t in per_epoch.iter().rev().take(3).rev() {
                info!(
                    "[vlt-quorum] epoch={} signed={} total={} quorum={} prevote={} precommit={}",
                    t.epoch,
                    t.signed_weight,
                    t.total_weight,
                    bft_quorum(t.total_weight),
                    if prevoted.contains(&t.epoch) { "met" } else { "no" },
                    if precommitted.contains(&t.epoch) { "met" } else { "no" },
                );
            }
            // §7.2: persist the finality certificate for every epoch whose precommit quorum is
            // newly counted — the durable proof, with signatures, that outlives the vote window.
            // Ascending order + write-once makes the certified sequence monotone by construction.
            for &epoch in precommitted.iter() {
                if self.dns_finality_certificate_store.has(epoch).unwrap_or(false) {
                    continue;
                }
                let Some(anchor) = self.canonical_anchor_by_blue_score(epoch, sink, dns_params) else {
                    continue;
                };
                let Some(snap) = self
                    .frozen_snapshots_for_targets(sink, std::iter::once(epoch), &bonds, net_id.as_byte_slice(), dns_params)
                    .remove(&epoch)
                else {
                    continue;
                };
                let source_anchor = prev_dns_state.as_ref().map_or(Hash64::default(), |p| p.last_dns_confirmed_anchor);
                if let Some(cert) = build_finality_certificate(
                    epoch,
                    source_anchor,
                    anchor.anchor_hash,
                    anchor.anchor_daa_score,
                    &counted_records,
                    &snap,
                ) {
                    info!(
                        "[dns-finality-certificate] persisted epoch={} target_anchor={} signed={}/{} quorum={} signers={} snapshot_root={}",
                        cert.epoch,
                        cert.target_anchor,
                        cert.signed_weight,
                        cert.total_weight,
                        cert.quorum_weight,
                        cert.precommit_signatures.len(),
                        cert.snapshot_root,
                    );
                    self.dns_finality_certificate_store.set_batch(batch, epoch, cert).unwrap();
                }
            }
            confirmable.is_some_and(|a| precommitted.contains(&a.epoch))
        } else {
            true
        };

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
        let new_state = advance_dns_confirmation(
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
            anchor_epoch_precommitted,
        );
        // MISAKA VLT PR 7 (§12): the IDENTITY TUPLE — the values every node kind must agree on
        // once it has caught up, whichever way it got there: a node that has run since genesis, a
        // node that restarted, a node that IBD'd from headers, and (once the overlay rows ride the
        // pruning snapshot) a node that imported a pruning point.
        //
        // One line, keyed by epoch, because that is what makes disagreement *findable*. These
        // values are already committed individually — what was missing was a way to compare five
        // nodes with one grep instead of five subsystem walks, which is exactly the diff an IBD
        // that silently derived a different denominator would show up in.
        if dns_params.vlt_shadow_active_at(sink_daa) {
            let wall_epoch = sink_blue / epoch_len_blue;
            // Same accessor as the sign and verify paths: an identity line read straight from the
            // store would report a root this chain no longer uses after a reorg, and two honest
            // nodes that reorged at different moments would look like they disagreed.
            if let Some(row) = self.voting_snapshot_for_wall_epoch(sink, wall_epoch, &bonds, net_id.as_byte_slice(), dns_params) {
                info!(
                    "[vlt-identity] epoch={wall_epoch} finalized_anchor={} snapshot_root={} validator_set_root={} credit_table_root={} capability_root={} model_table={} activation_epoch={} total_weight={}",
                    new_state.last_dns_confirmed_anchor,
                    row.snapshot_root,
                    row.validator_set_root,
                    row.credit_table_root,
                    row.capability_set_root,
                    row.model_table_hash,
                    self.vlt_activation_store.read().get().map(|r| r.activation_epoch).unwrap_or(0),
                    row.total_weight,
                );
            }
        }
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

    /// MISAKA §5 round 2: the epochs whose **precommit** quorum is met on this chain.
    ///
    /// Walks the same window and applies the same eligibility rules the prevote round applies —
    /// canonical anchor for the epoch, bond `Active` at that anchor, `validator_id` bound to the
    /// bond, ML-DSA-87 signature — and then two rules that only exist in round 2:
    ///
    /// 1. **The lock chain.** A precommit counts only if its declared `(locked_epoch,
    ///    locked_hash)` is exactly what this chain shows as that validator's previous counted
    ///    precommit ([`lock_consistent_precommits`]). The declaration is what the validator can be
    ///    held to later, so it has to be a faithful running record, not a field filled in when
    ///    convenient.
    /// 2. **Round order.** A precommit is a lock on an anchor the network already approved, so it
    ///    counts only for an epoch whose *prevote* quorum is met here — passed in as `prevote_ok`.
    ///    Locking on an anchor two thirds have not approved is not round 2 of anything.
    ///
    /// The weight is read through the same [`ContributionWeight`] the prevote round used, from the
    /// same pinned snapshot. Two rounds counted in different units would not compose into a
    /// commit, and the quorum-intersection argument needs both to be fractions of one `W(E)`.
    #[allow(clippy::too_many_arguments)]
    fn precommitted_epochs(
        &self,
        tip: BlockHash,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
        weight: ContributionWeight<'_>,
        totals: &BTreeMap<u64, u128>,
        prevote_quorum: &BTreeSet<u64>,
    ) -> (BTreeSet<u64>, Vec<PrecommitRecord>) {
        let records = self.collect_precommits(tip, bonds, net_id, dns_params, weight, prevote_quorum);
        let counted_contribs = lock_consistent_precommits(&records);
        let quorum = quorum_epochs(&aggregate_epoch_tallies(&counted_contribs, totals), dns_params.vlt.min_network_compute);
        // The records behind the counted contributions, for §7.2 certificate assembly: keep a
        // record iff its (validator, bond, epoch) survived the lock-consistency filter.
        let counted_keys: HashSet<(Hash64, TransactionOutpoint, u64)> =
            counted_contribs.iter().map(|c| (c.validator_id, c.bond_outpoint, c.epoch)).collect();
        let counted_records =
            records.into_iter().filter(|r| counted_keys.contains(&(r.validator_id, r.bond_outpoint, r.epoch))).collect();
        (quorum, counted_records)
    }

    /// The eligibility-checked precommits on this chain, in no particular order —
    /// [`lock_consistent_precommits`] imposes the chain order the lock rule needs.
    fn collect_precommits(
        &self,
        tip: BlockHash,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
        weight: ContributionWeight<'_>,
        prevote_quorum: &BTreeSet<u64>,
    ) -> Vec<PrecommitRecord> {
        let anchors = self.canonical_anchors_in_window(tip, dns_params, dns_params.stake_score_window_blue_score);
        let Ok(tip_blue) = self.headers_store.get_blue_score(tip) else {
            return Vec::new();
        };
        let mut records: Vec<PrecommitRecord> = Vec::new();
        // §5.1/§7.1: one frozen denominator per target epoch, on THIS walk's chain — the round
        // only exists above the weight fence, so unlike the prevote walk there is no below-fence
        // arm. See `collect_stake_contributions_v2`.
        let frozen_by_target = self.frozen_snapshots_for_targets(tip, anchors.keys().copied(), bonds, net_id, dns_params);
        for chain_block in self.reachability_service.default_backward_chain_iterator(tip) {
            let Ok(bs) = self.headers_store.get_blue_score(chain_block) else {
                break;
            };
            if tip_blue.saturating_sub(bs) > dns_params.stake_score_window_blue_score {
                break;
            }
            let Ok(block_daa) = self.headers_store.get_daa_score(chain_block) else {
                break;
            };
            let txs = self.accepted_txs_of_chain_block(chain_block);
            for p in precommits_from_accepted_txs(&txs) {
                if !p.lock_is_self_consistent() {
                    continue;
                }
                // Round order: a precommit is a lock on an anchor the network has already
                // approved. Locking on one two thirds have not prevoted is not round 2 of
                // anything, so it counts for nothing.
                if !prevote_quorum.contains(&p.epoch) {
                    continue;
                }
                // The anchor must be THIS chain's canonical one for the epoch, exactly as a
                // prevote's target must be — a lock on a target nobody else is voting on is not a
                // lock on anything.
                let Some(anchor) = anchors.get(&p.epoch) else {
                    continue;
                };
                if p.target_hash != anchor.anchor_hash || p.target_daa_score != anchor.anchor_daa_score {
                    continue;
                }
                let Some(bond) = bonds.iter().find(|b| b.bond_outpoint == p.bond_outpoint) else {
                    continue;
                };
                if p.validator_id != bond.validator_pubkey_hash || !is_bond_active_at(bond, anchor.anchor_daa_score) {
                    continue;
                }
                // §5.1: a lock is a lock under ONE denominator — its target epoch's frozen
                // snapshot. The commitment is inside the signed digest below, so a mismatched
                // one is unusable rather than merely uncounted. No frozen snapshot derivable for
                // the target ⇒ the same deterministic abstention as the prevote walk.
                let frozen = frozen_by_target.get(&p.epoch);
                if let Some(snap) = frozen
                    && p.snapshot_commitment != snap.vote_commitment()
                {
                    continue;
                }
                let digest = stake_precommit_message(
                    net_id,
                    p.epoch,
                    p.target_hash,
                    p.target_daa_score,
                    p.locked_epoch,
                    p.locked_hash,
                    p.snapshot_commitment,
                    p.bond_outpoint,
                )
                .as_bytes();
                if !matches!(
                    verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &p.signature, PRECOMMIT_MLDSA87_CONTEXT),
                    Ok(true)
                ) {
                    continue;
                }
                records.push(PrecommitRecord {
                    epoch: p.epoch,
                    validator_id: p.validator_id,
                    bond_outpoint: p.bond_outpoint,
                    target_hash: p.target_hash,
                    declared_lock: PrecommitLock { epoch: p.locked_epoch, anchor: p.locked_hash },
                    accepted_daa_score: block_daa,
                    // Numerator from the same frozen snapshot the vote signed — see the prevote walk.
                    signed_weight: match frozen {
                        Some(snap) => {
                            snap.validators.iter().find(|v| v.validator_id == p.validator_id).map_or(0, |v| v.effective_weight)
                        }
                        None => weight.of(bond, bonds, anchor.anchor_daa_score, p.epoch),
                    },
                    snapshot_commitment: p.snapshot_commitment,
                    signature: p.signature.clone(),
                });
            }
        }
        records
    }

    /// MISAKA §5 round 2, from one validator's side: the lock it is carrying on this chain and the
    /// epochs it still owes a precommit for. Backs `ConsensusApi::get_precommit_duty`.
    ///
    /// The lock comes from the chain, never from the caller's memory — see [`PrecommitDuty`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn precommit_duty_view(
        &self,
        tip: BlockHash,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
        sink_daa: u64,
        validator_id: Hash64,
        bond_outpoint: TransactionOutpoint,
    ) -> PrecommitDuty {
        let mut duty = PrecommitDuty {
            round_active: dns_params.vlt_weighting_active_at(sink_daa),
            sink_daa_score: sink_daa,
            ..Default::default()
        };
        if !duty.round_active {
            return duty;
        }

        let snapshot =
            self.vlt_epoch_snapshot(tip, sink_daa, bonds, net_id, dns_params, dns_params.vlt_shadow_active_at(sink_daa), false);
        let weight = ContributionWeight::Vlt { snapshot: &snapshot, vlt: &dns_params.vlt };
        let (contributions, epoch_anchor_daa) = self.collect_stake_contributions_v2(tip, None, bonds, net_id, dns_params, weight);
        let totals = self.total_weight_by_epoch(tip, bonds, net_id, dns_params, &epoch_anchor_daa, weight);
        let prevoted = quorum_epochs(&aggregate_epoch_tallies(&contributions, &totals), dns_params.vlt.min_network_compute);

        let records = self.collect_precommits(tip, bonds, net_id, dns_params, weight, &prevoted);
        duty.held = held_precommit_lock(&records, validator_id, bond_outpoint);
        // Only epochs strictly above the held lock are still owed: a precommit at or below it
        // would declare a lock that is not below the epoch it locks, which consensus rejects at the
        // stateless layer, and one already counted needs no repeat.
        let anchors = self.canonical_anchors_in_window(tip, dns_params, dns_params.stake_score_window_blue_score);
        duty.due = prevoted
            .iter()
            .filter(|&&e| e > duty.held.epoch)
            .filter_map(|&e| {
                let a = anchors.get(&e)?;
                // §5.1, per TARGET epoch: each due precommit binds ITS epoch's frozen
                // denominator. An epoch whose commitment cannot be derived yet is simply not
                // due — signing it with a wrong (or zero) commitment would burn a fee on a
                // signature that counts nowhere.
                let commitment = self.vote_commitment_for_target(tip, sink_daa, e, bonds, net_id, dns_params)?;
                Some((e, a.anchor_hash, a.anchor_daa_score, commitment))
            })
            .collect();
        duty
    }

    /// kaspa-pq Phase 13 (ADR-0018 §H) + DNS v3 (PR6): the StakeScore a branch accumulated
    /// **since the common ancestor** — the selected chain from `tip` back to (but excluding)
    /// `ancestor`, scored under `bonds` (that branch's bond set) and this network's `φS`. Uses
    /// the v3 canonical-anchor verifier (`collect_stake_contributions_v2`) with
    /// `stop_at = ancestor`, so the branch is scored only on canonical attestations for the
    /// epochs anchored strictly above the common ancestor (its OWN segment) — byte-identical to
    /// the sink-side StakeScore and immune to a branch inflating its score with non-canonical
    /// (current-sink / fabricated) targets. Inert wherever the overlay is dormant.
    ///
    /// `snapshot` is the VLT weight table, and it is an argument rather than something this
    /// function derives because **both branches must be handed the same one** — see
    /// [`VltEpochSnapshot`]. Its pin is the `ancestor` both calls share, so a certificate that
    /// exists only on the branch being scored contributes nothing to either the weight that
    /// branch signs with or the `W(E)` it is measured against. Deriving it here from `tip`, as
    /// this used to, let each branch write its own denominator: omit the other side's
    /// certificates, `W(E)` falls, `Q(E) = ⌊2W(E)/3⌋ + 1` falls with it, and the branch clears a
    /// quorum bar it set for itself. Two branches could then both "reach quorum" for one epoch
    /// over disjoint validators, which is precisely what the §8.1 intersection argument forbids.
    fn stake_score_since_ancestor(
        &self,
        tip: BlockHash,
        ancestor: BlockHash,
        bonds: &[StakeBondRecord],
        dns_params: &DnsParams,
        net_id: &[u8],
        pov_daa_score: u64,
        snapshot: &VltEpochSnapshot,
    ) -> StakeScore {
        // The weight SOURCE is still decided at this branch's own tip — around the activation
        // fence the two branches can straddle it, and scoring one in µRTE against the other in
        // sompi would compare nothing. Only the table is shared.
        let weight = if dns_params.vlt_weighting_active_at(pov_daa_score) {
            ContributionWeight::Vlt { snapshot, vlt: &dns_params.vlt }
        } else {
            ContributionWeight::BondedStake
        };
        let (contributions, epoch_anchor_daa) =
            self.collect_stake_contributions_v2(tip, Some(ancestor), bonds, net_id, dns_params, weight);
        let totals = self.total_weight_by_epoch(tip, bonds, net_id, dns_params, &epoch_anchor_daa, weight);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        compute_stake_score(&per_epoch, dns_params.epoch_credit_rule(pov_daa_score))
    }

    /// The per-epoch quorum denominator, matching `weight`'s numerator unit.
    ///
    /// Both branches consume the same `epoch_anchor_daa` and the same bond set, so switching the
    /// VLT fence changes only what a validator's weight *is*, never which validators are counted.
    ///
    /// MISAKA VLT PR 4: above the weight fence (judged at `tip`, the same pov the weight source
    /// keys on) each target epoch's denominator IS its frozen snapshot's `total_weight` — the
    /// §7.1 `W(E)` the votes signed. A target with no derivable complete snapshot keeps the
    /// pinned-table total, matching the numerator fallback in the vote walks.
    fn total_weight_by_epoch(
        &self,
        tip: BlockHash,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
        epoch_anchor_daa: &BTreeMap<u64, u64>,
        weight: ContributionWeight<'_>,
    ) -> BTreeMap<u64, u128> {
        let base = match weight {
            ContributionWeight::BondedStake => total_active_stake_by_epoch(bonds, epoch_anchor_daa),
            ContributionWeight::Vlt { snapshot, vlt } => total_voting_weight_by_epoch(bonds, epoch_anchor_daa, snapshot, vlt),
        };
        let pov_daa = self.headers_store.get_daa_score(tip).unwrap_or(0);
        if !dns_params.vlt_weighting_active_at(pov_daa) || matches!(weight, ContributionWeight::BondedStake) {
            return base;
        }
        let frozen = self.frozen_snapshots_for_targets(tip, base.keys().copied(), bonds, net_id, dns_params);
        base.into_iter().map(|(e, old)| (e, frozen.get(&e).map_or(old, |s| s.total_weight))).collect()
    }

    /// §5's `capability_set_root` at a pin: every declaration live at the pin's DAA **and**
    /// contained in the pin's own chain history, hashed in canonical order. Ancestry, not DAA,
    /// for the same reason the committee pool filters by ancestry — two branches can carry
    /// different declarations at one DAA score, and a root that ignored that would commit one
    /// branch's committee pool on the other.
    fn capability_set_root_at(&self, pin: BlockHash, pin_daa: u64) -> Hash64 {
        let mut records: Vec<ComputeCapabilityRecord> = self
            .compute_capability_store
            .read()
            .all()
            .into_iter()
            .filter(|r| {
                r.is_live_at(pin_daa)
                    && (r.declaration_block == pin || self.reachability_service.is_chain_ancestor_of(r.declaration_block, pin))
            })
            .collect();
        capability_set_root(&mut records)
    }

    /// MISAKA VLT PR 2 (§5): the frozen voting snapshot for `wall_epoch`, as `tip`'s chain
    /// derives it — the denominator a vote accepted in that epoch must have signed.
    ///
    /// A pure function of the chain, which is the property everything else here leans on: the
    /// pin is the canonical lagged anchor of the newest epoch that was ready at `wall_epoch`'s
    /// first blue score — arithmetic plus a selected-parent walk, no local clock, no local
    /// store. Two nodes on one chain derive identical bytes; two branches agree wherever they
    /// share that anchor, which lag-burial guarantees for every fork shallower than the
    /// attestation lag. That is what lets the credit walk *enforce* the §5.1 commitment without
    /// the enforcement itself becoming a partition vector.
    ///
    /// The write-once store row is served only when it is pinned at the very anchor this chain
    /// derives — a row frozen on the selected chain must not weight a candidate branch whose own
    /// derivation pins elsewhere.
    ///
    /// `None` when the chain is too young to have a ready epoch at that boundary, or the anchor
    /// walk cannot reach it.
    fn voting_snapshot_for_wall_epoch(
        &self,
        tip: BlockHash,
        wall_epoch: u64,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
    ) -> Option<VltVotingSnapshot> {
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let boundary_blue = epoch_start_blue_score(wall_epoch, epoch_len);
        let ready = ready_epoch_from_tip_blue_score(boundary_blue, epoch_len, dns_params.attestation_lag_blue_score)?;
        let anchor = self.canonical_anchor_by_blue_score(ready, tip, dns_params)?;
        if let Ok(row) = self.vlt_voting_snapshot_store.get(wall_epoch)
            && row.source_finalized_anchor == anchor.anchor_hash
        {
            return Some(row);
        }
        let table = self.vlt_epoch_snapshot(
            anchor.anchor_hash,
            anchor.anchor_daa_score,
            bonds,
            net_id,
            dns_params,
            dns_params.vlt.shadow_active_at(anchor.anchor_daa_score),
            false,
        );
        Some(build_voting_snapshot(
            anchor.anchor_hash,
            anchor.anchor_daa_score,
            ready,
            wall_epoch,
            dns_params.vlt.model_table_hash(),
            self.capability_set_root_at(anchor.anchor_hash, anchor.anchor_daa_score),
            &table,
            bonds,
            &dns_params.vlt,
        ))
    }

    /// One canonical log line per frozen snapshot, shared by the boundary freeze and the lazy
    /// sign-path freeze so every row on disk has exactly one grep-able record of its birth —
    /// `suffix` says which path froze it. The verify harness parses this line; treat its shape
    /// as an operator API.
    fn log_frozen_snapshot(snap: &VltVotingSnapshot, wall_epoch: u64, suffix: &str) {
        // Per-row weights for small sets (a devnet's whole point is comparing them); capped so a
        // production-sized set cannot turn the line into a table dump.
        let weights = if snap.validators.len() <= 8 {
            format!(
                " weights=[{}]",
                snap.validators
                    .iter()
                    .map(|v| format!("{}:{}", &v.validator_id.to_string()[..8], v.effective_weight))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        } else {
            String::new()
        };
        info!(
            "[vlt-voting-snapshot] frozen epoch={wall_epoch} snapshot_root={} validator_set_root={} vote_commitment={} validators={} total_weight={} quorum_weight={}{weights} ({suffix})",
            snap.snapshot_root,
            snap.validator_set_root,
            snap.vote_commitment(),
            snap.validators.len(),
            snap.total_weight,
            snap.quorum_weight,
        );
    }

    /// MISAKA VLT PR 4: the frozen denominators for a set of target epochs, on `tip`'s chain —
    /// `e → frozen(w(e))` with `w` = [`voting_epoch_for_target`]. One derivation per DISTINCT
    /// voting epoch (long lags make several targets share one), store-served after the boundary
    /// freeze, and **complete resolutions only**: an incomplete table is a fact about this
    /// node's storage, and a denominator built from one would make quorum a function of local
    /// loading limits. A target absent from the result gets the deterministic fallback its
    /// caller documents.
    fn frozen_snapshots_for_targets<I: IntoIterator<Item = u64>>(
        &self,
        tip: BlockHash,
        targets: I,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
    ) -> HashMap<u64, Arc<VltVotingSnapshot>> {
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let lag = dns_params.attestation_lag_blue_score;
        let mut by_wall: HashMap<u64, Option<Arc<VltVotingSnapshot>>> = HashMap::new();
        let mut out = HashMap::new();
        for e in targets {
            let w = voting_epoch_for_target(e, epoch_len, lag);
            let snap = by_wall
                .entry(w)
                .or_insert_with(|| {
                    self.voting_snapshot_for_wall_epoch(tip, w, bonds, net_id, dns_params)
                        .filter(|s| s.resolution_complete)
                        .map(Arc::new)
                })
                .clone();
            if let Some(s) = snap {
                out.insert(e, s);
            }
        }
        out
    }

    /// The §5.1 commitment a vote on `target_epoch` must sign — the frozen denominator of that
    /// epoch's voting wall — or `None` below the weight fence / when no complete snapshot can be
    /// derived yet.
    ///
    /// The sign path's read: serves the frozen row when one exists, and otherwise derives and
    /// **lazily freezes** it (direct write-once) — a node that restarted mid-epoch, or whose
    /// boundary derivation was incomplete, still pins the epoch the first time it needs to sign.
    /// Withholds the commitment rather than returning one from an incomplete table: votes signed
    /// under a table this node could not fully load would be uncountable everywhere else. Only
    /// ever called with the SINK (the lazy freeze must not write branch-pinned rows).
    pub(crate) fn vote_commitment_for_target(
        &self,
        sink: BlockHash,
        sink_daa: u64,
        target_epoch: u64,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
    ) -> Option<Hash64> {
        if !dns_params.vlt_weighting_active_at(sink_daa) {
            return None;
        }
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let w = voting_epoch_for_target(target_epoch, epoch_len, dns_params.attestation_lag_blue_score);
        // Through the SAME accessor the credit walk verifies with — never the store directly.
        //
        // `voting_snapshot_for_wall_epoch` serves the frozen row only while its pin is still the
        // anchor this chain derives, and re-derives otherwise. Reading the store here instead
        // handed signers a row pinned on a chain a reorg had since replaced, while the walk
        // re-derived a different one: every vote then carried a commitment the walk rejected, and
        // round 2 stopped dead with `held 0` forever and no error anywhere. A signer and its
        // verifier must consult one function, not two that agree most of the time.
        let snap = self.voting_snapshot_for_wall_epoch(sink, w, bonds, net_id, dns_params)?;
        if !snap.resolution_complete {
            warn!(
                "[vlt-voting-snapshot] epoch={w}: table resolution incomplete; votes signed now would count nowhere — withholding the commitment"
            );
            return None;
        }
        let commitment = snap.vote_commitment();
        // Freeze only what is not frozen yet: the row is write-once, so a stale one left by a
        // since-reorged chain simply stops being served (both paths re-derive) rather than being
        // rewritten.
        if !self.vlt_voting_snapshot_store.has(w).unwrap_or(false) {
            Self::log_frozen_snapshot(&snap, w, "lazy sign-path freeze");
            if let Err(e) = self.vlt_voting_snapshot_store.set(w, snap) {
                warn!("[vlt-voting-snapshot] lazy freeze of epoch {w} failed: {e}");
            }
        }
        Some(commitment)
    }

    /// MISAKA Verified LLM Token-Weighted BFT (§3, §6): walk the chain ending at `pin` and fold
    /// every creditable compute certificate into the [`VltEpochSnapshot`] that
    /// [`validator_voting_weight`] turns into voting weight.
    ///
    /// **`pin` is the whole point of this function's shape.** The table it produces is a quorum
    /// denominator, and a denominator each branch derives for itself is not one (see
    /// [`VltEpochSnapshot`]). `pin` must therefore be a block every branch that will be weighted
    /// by the result contains — the selected-chain common ancestor when two branches are being
    /// compared. Every DAA-stamped decision below is taken at `pin_daa_score`, never at a branch
    /// tip: `bonds` is cut to those that existed at the pin, the anchor map and the walk both
    /// start from `pin`, and challenge-window survival and epoch finalization are measured
    /// against `pin_daa_score`. That is what makes two branches derive the identical table.
    ///
    /// `active` is the fence decision, passed in rather than taken here, because the branches
    /// being compared can straddle `vlt_activation_daa_score` and the pin below them can sit on
    /// the other side of it again. Returns [`VltEpochSnapshot::inert`] — doing **no** walking at
    /// all — when it is false, so the added walk cost is paid only by a network that has actually
    /// switched its weight source.
    ///
    /// A certificate is creditable only if all of the following hold. Each is a place an executor
    /// would otherwise be able to mint weight it did not earn:
    ///
    /// 1. Its epoch has a canonical lagged anchor on **this** chain, and the certificate was
    ///    accepted in a chain block belonging to that same epoch — so credit cannot be back-dated
    ///    into an epoch that is already weighting votes.
    /// 2. The executor's bond exists, is `Active` at the epoch anchor, and matches the declared
    ///    `executor_id` (the same bond↔identity binding the attestation path enforces, without
    ///    which varying the declared id would evade dedup).
    /// 3. The executor's ML-DSA-87 signature over [`compute_certificate_message`] verifies.
    /// 4. A phase-1 commitment for the same `(job_id, executor, bond)` was accepted on **this**
    ///    chain, no more than `max_commitment_age_blocks` earlier, publishing the input the
    ///    certificate's spec commits to; its beacon epoch has an anchor; and the certificate does
    ///    not predate that anchor.
    /// 5. Every collected verdict carries a signature that verifies against a bond `Active` at the
    ///    anchor, and its verifier is in the committee [`select_verifiers`] draws for
    ///    `(job_id, executor_id, beacon)` from the validators that declared this job's profile.
    /// 6. `Verify(S_j, R_j, C_j) = 1` and the job normalizes to non-zero VLT under the network's
    ///    model cost table.
    ///
    /// The challenge window and the `(executor, job)` dedup are applied afterwards by
    /// [`aggregate_compute_credits`], which also drops any certificate a challenge on this chain
    /// has named.
    ///
    /// §6 asks that verifiers be drawn from randomness the executor could not see when it
    /// committed, and (4) is what delivers it: the beacon is the canonical anchor of the epoch
    /// **after** the one that accepted the commitment, so it did not exist when the executor fixed
    /// `sampling_seed` and therefore `job_id`. Grinding at commitment time is grinding against
    /// randomness that has not been drawn.
    /// `diag` selects the ONE caller that reports — the per-epoch state recompute. The other three
    /// call sites run per block (and one of them per candidate branch), so a line emitted from here
    /// unconditionally is a line several times a second for as long as the condition holds.
    fn vlt_epoch_snapshot(
        &self,
        pin: BlockHash,
        pin_daa_score: u64,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
        active: bool,
        diag: bool,
    ) -> VltEpochSnapshot {
        if !active {
            return VltEpochSnapshot::inert();
        }
        // Bonds that did not exist at the pin are cut here, once, rather than trusted not to
        // matter at each of the four places below that look a bond up by outpoint. A record
        // created above the pin exists on one branch and not the other, so leaving it in would
        // let the branch that has it resolve a certificate, a verdict or a committee seat the
        // other branch cannot — the table would stop being a function of the shared prefix. Every
        // *status* test below is already evaluated at an anchor at or under the pin, where a
        // slash or unbond stamped above the pin is invisible to `effective_bond_status`, so this
        // filter is the only branch-dependence the bond set can still introduce.
        let pinned_bonds: Vec<StakeBondRecord> = bonds.iter().filter(|b| b.activation_daa_score <= pin_daa_score).cloned().collect();
        let bonds = pinned_bonds.as_slice();
        // The anchor map MUST span the same depth as the credit walk below. Using the (much
        // shorter) attestation window here would leave every certificate older than
        // `stake_score_window_blue_score` without an anchor, silently truncating `C_i(E)` from
        // `credit_window_epochs` down to the attestation window — an under-count that no test
        // would fail on and that would make weight depend on an unrelated parameter.
        let anchors = self.canonical_anchors_in_window(pin, dns_params, dns_params.vlt_credit_window_blue_score);
        let Ok(pin_blue) = self.headers_store.get_blue_score(pin) else {
            // Could not read the pin's own blue score: an empty table, but NOT an answer. Returning
            // `inert()` here would licence the caller to cache a zero it never actually computed.
            return VltEpochSnapshot::unresolved();
        };

        // Finalized epochs come from the accumulator store instead of being re-derived. An epoch
        // is finalized once it is buried past BOTH the challenge window and the reorg horizon, so
        // its credits can no longer change under any challenge or any branch — which is exactly
        // what makes one epoch-keyed row valid for the sink and for a candidate branch alike.
        //
        // This is the point of the store: without it every virtual-state commit re-verifies an
        // ML-DSA-87 executor signature plus a whole verifier committee's signatures for every
        // certificate in a `credit_window_epochs`-deep window, to rebuild a sum whose old terms
        // did not move.
        let mut credited: HashMap<Hash64, BTreeMap<u64, u128>> = HashMap::new();
        let mut audited: HashMap<Hash64, BTreeMap<u64, u128>> = HashMap::new();
        let mut cached_epochs: HashSet<u64> = HashSet::new();
        let mut oldest_uncached_blue = pin_blue.saturating_sub(dns_params.vlt_credit_window_blue_score);
        for (&epoch, anchor) in anchors.iter() {
            if !vlt_epoch_finalized(
                anchor.anchor_daa_score,
                pin_daa_score,
                dns_params.vlt.challenge_window_blocks,
                dns_params.max_reorg_horizon_blocks,
            ) {
                continue;
            }
            let Ok(row) = self.vlt_credit_store.get(epoch) else {
                continue; // not accumulated yet — fall through to the walk for this epoch
            };
            for (validator_id, x) in row.credits {
                credited.entry(validator_id).or_default().insert(epoch, x);
            }
            for (verifier_id, x) in row.audit {
                audited.entry(verifier_id).or_default().insert(epoch, x);
            }
            cached_epochs.insert(epoch);
        }
        // The walk only has to reach back to the oldest epoch NOT served from the store. Anchors
        // are ascending by epoch, so the first uncached one bounds it.
        if let Some((_, anchor)) = anchors.iter().find(|(e, _)| !cached_epochs.contains(e)) {
            oldest_uncached_blue = oldest_uncached_blue.max(anchor.epoch_start_blue_score);
        } else if !anchors.is_empty() {
            // Everything in the window is cached: nothing left to walk for.
            oldest_uncached_blue = pin_blue;
        }

        let walk = self.walk_compute_overlay(pin, bonds, net_id, dns_params, oldest_uncached_blue, pin_blue);

        let mut contributions: Vec<ComputeCreditContribution> = Vec::new();
        // Audit-emission v0.2: one roster per certificate that reached a decided, non-zero
        // normalization — the counted verdicts' authors and the job's µRTE they each earn.
        let mut audit_contributions: Vec<kaspa_consensus_core::dns_finality::AuditCreditContribution> = Vec::new();
        // Certificates whose challenge was ADJUDICATED as standing. §6 zeroes the credit of a
        // receipt whose fraud proof 成立した — not of every receipt somebody pointed at.
        let mut refuted: HashSet<TransactionId> = HashSet::new();
        // Why each certificate credited nothing. Every `continue` below was a bare one, and the
        // only outward sign was `0 validator(s) with credit` — which is also exactly what a network
        // running no compute at all looks like. `skips` carries the per-certificate detail so an
        // operator can tell "not yet" from "never" without reading this function.
        let mut tally = VltCreditTally::default();
        let mut skips: Vec<(TransactionId, Hash64, u64, VltCreditSkipReason)> = Vec::new();
        let note = |tally: &mut VltCreditTally,
                    skips: &mut Vec<(TransactionId, Hash64, u64, VltCreditSkipReason)>,
                    tx: TransactionId,
                    who: Hash64,
                    epoch: u64,
                    reason: VltCreditSkipReason| {
            tally.note_skipped(reason);
            // Bounded: a wide credit window on a busy network holds a lot of certificates, and a
            // diagnostic that can print all of them is a diagnostic that can fill a disk.
            if skips.len() < MAX_REPORTED_CREDIT_SKIPS {
                skips.push((tx, who, epoch, reason));
            }
        };
        for (cert_tx_id, cert, block_daa) in walk.certificates.iter() {
            let (cert_tx_id, block_daa) = (*cert_tx_id, *block_daa);
            tally.note_candidate();
            let Some(anchor) = anchors.get(&cert.epoch) else {
                // Two different facts wear the same absence. An epoch above the newest anchor is
                // simply not buried by `attestation_lag_blue_score` yet, which every certificate
                // is for a while right after it lands; one below the oldest is out of the credit
                // window and never coming back. Reporting both as permanent turns a routine wait
                // into an apparently dead certificate.
                let reason = match anchors.keys().next_back() {
                    Some(newest) if cert.epoch > *newest => VltCreditSkipReason::EpochAnchorNotReady,
                    None => VltCreditSkipReason::EpochAnchorNotReady,
                    _ => VltCreditSkipReason::EpochAnchorOutsideWindow,
                };
                note(&mut tally, &mut skips, cert_tx_id, cert.executor_id, cert.epoch, reason);
                continue;
            };
            let resolved = match self.resolve_certificate(cert, block_daa, anchor, bonds, &walk, dns_params, net_id, &anchors) {
                Ok(resolved) => resolved,
                Err(reason) => {
                    // A certificate that does not resolve credits nothing anyway, but an
                    // `InvalidCertificate` challenge against it is thereby proved — recorded so the
                    // challenger is not slashed for a claim that turned out to be right.
                    refuted.insert(cert_tx_id);
                    note(&mut tally, &mut skips, cert_tx_id, cert.executor_id, cert.epoch, reason);
                    continue;
                }
            };
            // Gather THIS certificate's verdicts from the chain and apply Verify(S,R,C). A verdict
            // set below the confirmation threshold — including the common case of verifiers simply
            // not having published yet — mints nothing, and will mint once they do.
            let attestations = verdicts_for_certificate(
                &walk.verdicts,
                cert_tx_id,
                resolved.job_id,
                resolved.receipt_hash,
                block_daa,
                &resolved.committee,
            );
            // Adjudicate this certificate's challenges against its own settled evidence, and let
            // only a challenge that STANDS deny the credit.
            for c in walk.challenges.iter().filter(|c| c.certificate_tx_id == cert_tx_id) {
                if adjudicate_compute_challenge(
                    c.kind,
                    true,
                    resolved.receipt_hash,
                    &attestations,
                    dns_params.vlt.min_verifier_confirmations,
                    dns_params.vlt.min_verifier_refutations,
                ) == ChallengeOutcome::Succeeded
                {
                    refuted.insert(cert_tx_id);
                }
            }
            let verified = verify_compute_certificate(
                resolved.receipt_hash,
                &attestations,
                dns_params.vlt.min_verifier_confirmations,
                dns_params.vlt.min_verifier_refutations,
            );
            let vlt = match normalize_vlt(&cert.spec, &cert.receipt, &dns_params.vlt, verified) {
                Ok(vlt) => vlt,
                // `VerificationFailed` is the committee not having spoken yet — the ordinary case
                // while verdicts land — and everything else is a spec consensus never accepted.
                Err(VltRejection::VerificationFailed) => {
                    note(&mut tally, &mut skips, cert_tx_id, cert.executor_id, cert.epoch, VltCreditSkipReason::NotVerified);
                    continue;
                }
                Err(VltRejection::UnregisteredModel) => {
                    note(&mut tally, &mut skips, cert_tx_id, cert.executor_id, cert.epoch, VltCreditSkipReason::UnregisteredProfile);
                    continue;
                }
                Err(_) => {
                    note(&mut tally, &mut skips, cert_tx_id, cert.executor_id, cert.epoch, VltCreditSkipReason::ZeroValued);
                    continue;
                }
            };
            if vlt == 0 {
                note(&mut tally, &mut skips, cert_tx_id, cert.executor_id, cert.epoch, VltCreditSkipReason::ZeroValued);
                continue;
            }
            contributions.push(ComputeCreditContribution {
                validator_id: cert.executor_id,
                bond_outpoint: cert.executor_bond_outpoint,
                epoch: cert.epoch,
                certificate_tx_id: cert_tx_id,
                job_id: resolved.job_id,
                vlt,
                accepted_daa_score: block_daa,
            });
            if !attestations.is_empty() {
                audit_contributions.push(kaspa_consensus_core::dns_finality::AuditCreditContribution {
                    certificate_tx_id: cert_tx_id,
                    executor_id: cert.executor_id,
                    job_id: resolved.job_id,
                    epoch: cert.epoch,
                    vlt,
                    accepted_daa_score: block_daa,
                    verifiers: attestations.iter().map(|a| a.verifier_id).collect(),
                });
            }
        }
        // The three tests `aggregate_compute_credits` applies, re-run here ONLY to attribute a
        // reason to each contribution it drops — it returns the survivors, not the verdicts. Kept
        // in the same order as the real thing, which is the property that makes the attribution
        // true rather than plausible.
        {
            let mut seen: HashSet<(Hash64, Hash64)> = HashSet::new();
            for c in contributions.iter() {
                let reason = if refuted.contains(&c.certificate_tx_id) {
                    VltCreditSkipReason::CertificateRefuted
                } else if pin_daa_score.saturating_sub(c.accepted_daa_score) < dns_params.vlt.challenge_window_blocks {
                    VltCreditSkipReason::ChallengeNotMature
                } else if !seen.insert((c.validator_id, c.job_id)) {
                    VltCreditSkipReason::AlreadyCredited
                } else {
                    tally.note_accepted();
                    continue;
                };
                note(&mut tally, &mut skips, c.certificate_tx_id, c.validator_id, c.epoch, reason);
            }
        }
        // Drop anything the store already answered for, so a certificate straddling the boundary
        // cannot be counted twice, then merge the freshly-derived tail onto the cached rows.
        let walked = aggregate_compute_credits(&contributions, &refuted, pin_daa_score, dns_params.vlt.challenge_window_blocks);
        for (validator_id, per_epoch) in walked {
            let slot = credited.entry(validator_id).or_default();
            for (epoch, x) in per_epoch {
                if !cached_epochs.contains(&epoch) {
                    slot.insert(epoch, x);
                }
            }
        }
        // Audit-emission v0.2: same pin, same survivorship machinery, same cached-row merge —
        // refuted certificates keep paying their committees (the one deliberate divergence).
        let walked_audit = kaspa_consensus_core::dns_finality::aggregate_audit_credits(
            &audit_contributions,
            &refuted,
            pin_daa_score,
            dns_params.vlt.challenge_window_blocks,
        );
        for (verifier_id, per_epoch) in walked_audit {
            let slot = audited.entry(verifier_id).or_default();
            for (epoch, x) in per_epoch {
                if !cached_epochs.contains(&epoch) {
                    slot.insert(epoch, x);
                }
            }
        }
        // Only from the per-epoch caller: the other three call sites run per block (one per
        // candidate branch), so an unconditional line here is several a second for as long as the
        // condition holds.
        if diag {
            self.vlt_metrics.record_credit(&tally);
            // Certificates exist and not one of them credited: the condition an operator has to act
            // on, and the one the overlay used to report only as `0 validator(s) with credit` —
            // indistinguishable from a network running no compute at all. A healthy network never
            // reaches this branch, and one that is merely early names only transient reasons.
            if tally.candidates > 0 && tally.accepted == 0 {
                info!("[vlt-credit] {} certificate(s) in the window, none credited: {}", tally.candidates, tally.summary());
                for (tx, who, epoch, reason) in skips.iter() {
                    info!(
                        "[vlt-credit-skipped] certificate={tx} executor={who} certificate_epoch={epoch} reason={} ({})",
                        reason.as_str(),
                        if reason.is_transient() { "not yet" } else { "permanent" }
                    );
                }
                if tally.candidates as usize > skips.len() {
                    info!("[vlt-credit-skipped] … and {} more not listed", tally.candidates as usize - skips.len());
                }
            }
        }
        // An unloaded dependency makes this table an unknown answer rather than a smaller one.
        // `stage_vlt_credits` refuses to cache such a table, and (from step 5) the activation guard
        // refuses to act on one — the two places where "this node has not got it yet" would
        // otherwise become "nobody ever will".
        if tally.is_incomplete() {
            return VltEpochSnapshot::pinned_incomplete(pin, pin_daa_score, credited).with_audit(audited);
        }
        VltEpochSnapshot::pinned(pin, pin_daa_score, credited).with_audit(audited)
    }

    /// ONE backward walk over `[oldest_blue, tip]` collecting every compute-overlay contribution;
    /// certificates are resolved afterwards by [`Self::resolve_certificate`].
    ///
    /// The two-pass shape is forced by the direction of the walk: a certificate is audited by
    /// validators whose capability declarations sit DEEPER in the chain than it does, and a
    /// backward walk has not seen those yet when it reaches the certificate. Only the small,
    /// bounded overlay records are buffered, never the block bodies.
    ///
    /// Signature and bond checks that do not depend on a certificate are applied here; anything
    /// that needs a certificate's beacon (committee membership above all) belongs to the second
    /// pass, since the committee is not known until the beacon epoch is resolved.
    fn walk_compute_overlay(
        &self,
        tip: BlockHash,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
        oldest_blue: u64,
        tip_blue: u64,
    ) -> ComputeOverlayWalk {
        let mut challenges: Vec<ComputeChallengePayload> = Vec::new();
        let mut capabilities: Vec<ComputeCapabilityRecord> = Vec::new();
        let mut commitments: HashMap<TransactionId, ComputeCommitmentRecord> = HashMap::new();
        let mut verdicts: Vec<ComputeVerdictRecord> = Vec::new();
        let mut pending: Vec<(TransactionId, ComputeCertificatePayload, u64)> = Vec::new();
        for chain_block in self.reachability_service.default_backward_chain_iterator(tip) {
            let Ok(bs) = self.headers_store.get_blue_score(chain_block) else {
                break;
            };
            // Stop at the oldest epoch the caller still needs. For the credit walk everything
            // below it is finalized AND cached, so re-deriving it would recompute an identical
            // answer.
            if bs < oldest_blue || tip_blue.saturating_sub(bs) > dns_params.vlt_credit_window_blue_score {
                break;
            }
            let Ok(block_daa) = self.headers_store.get_daa_score(chain_block) else {
                break;
            };
            let txs = self.accepted_txs_of_chain_block(chain_block);

            challenges.extend(compute_challenges_from_accepted_txs(&txs));

            // Standalone verdicts. Signature-checked here against the verifier's own bond; the
            // committee-membership and ordering rules are applied per certificate below, since
            // the committee is not known until the certificate's beacon is resolved.
            for v in compute_verdicts_from_accepted_txs(&txs) {
                if !v.is_self_consistent() {
                    continue;
                }
                let Some(vb) = bonds.iter().find(|b| b.bond_outpoint == v.bond_outpoint) else {
                    continue;
                };
                if v.verifier_id != vb.validator_pubkey_hash {
                    continue;
                }
                let digest = verifier_verdict_message(
                    net_id,
                    v.certificate_tx_id,
                    v.job_id,
                    v.executor_receipt_hash,
                    v.verdict,
                    v.replay_receipt_hash,
                    v.bond_outpoint,
                )
                .as_bytes();
                if !matches!(
                    verify_mldsa87_with_context(&vb.validator_pubkey, &digest, &v.signature, VERIFIER_VERDICT_MLDSA87_CONTEXT),
                    Ok(true)
                ) {
                    continue;
                }
                verdicts.push(ComputeVerdictRecord {
                    certificate_tx_id: v.certificate_tx_id,
                    payload: v,
                    accepted_daa_score: block_daa,
                });
            }

            // Phase-1 commitments. Recorded with the blue_score that accepted them, because that
            // is what fixes the beacon epoch and hence the committee.
            for (commit_tx_id, commit) in compute_commitments_from_accepted_txs(&txs) {
                if let Some(record) = verified_commitment(commit, bonds, net_id, bs, block_daa) {
                    commitments.insert(commit_tx_id, record);
                }
            }

            // Capability declarations: bond-bound, signature-verified, expiry capped at
            // `max_capability_validity_blocks` past THIS block so a stale declaration cannot
            // name a far-future expiry and squat in committees.
            for cap in compute_capabilities_from_accepted_txs(&txs) {
                let Some(bond) = bonds.iter().find(|b| b.bond_outpoint == cap.bond_outpoint) else {
                    continue;
                };
                if cap.validator_id != bond.validator_pubkey_hash {
                    continue;
                }
                // The declared profile must be one consensus actually registered, declared under
                // its registered class — otherwise a validator could self-assign a class and
                // join committees it cannot reproduce.
                let Some(entry) = dns_params.vlt.model_cost_table.lookup(cap.model_weights_hash, cap.runtime_hash) else {
                    continue;
                };
                if cap.runtime_class_id != entry.runtime_class_id {
                    continue;
                }
                let digest = compute_capability_message(
                    net_id,
                    cap.validator_id,
                    cap.bond_outpoint,
                    cap.model_weights_hash,
                    cap.runtime_hash,
                    cap.runtime_class_id,
                    cap.expiry_daa_score,
                )
                .as_bytes();
                if !matches!(
                    verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &cap.signature, COMPUTE_CAPABILITY_MLDSA87_CONTEXT),
                    Ok(true)
                ) {
                    continue;
                }
                capabilities.push(ComputeCapabilityRecord {
                    declaration_block: chain_block,
                    accepted_daa_score: block_daa,
                    validator_id: cap.validator_id,
                    bond_outpoint: cap.bond_outpoint,
                    model_weights_hash: cap.model_weights_hash,
                    runtime_hash: cap.runtime_hash,
                    runtime_class_id: cap.runtime_class_id,
                    expiry_daa_score: cap
                        .expiry_daa_score
                        .min(block_daa.saturating_add(dns_params.vlt.max_capability_validity_blocks)),
                });
            }

            let block_epoch = bs / dns_params.attestation_epoch_length_blue_score.max(1);
            for (cert_tx_id, cert) in compute_certificates_from_accepted_txs(&txs) {
                // (1) The credited epoch must be this block's own epoch (no back-dating into an
                // epoch that is already weighting votes).
                if cert.epoch == block_epoch {
                    pending.push((cert_tx_id, cert, block_daa));
                }
            }
        }

        // ---- dependency horizon -------------------------------------------------------------
        //
        // A certificate's phase-1 commitment legitimately sits BELOW the floor that bounds
        // certificates: by at least one full epoch, because the certificate cannot be built until
        // the beacon — the anchor of the epoch AFTER the commitment's — exists, and by up to
        // `max_commitment_age_blocks` in general. `oldest_blue` is raised to the oldest epoch the
        // caller still needs re-derived, which in the steady state is one epoch back, so bounding
        // dependencies by it made every certificate unresolvable. That is the 2026-08-09 failure:
        // twenty jobs executed, certified and confirmed, and not one credited.
        //
        // So the floor applies to certificates and the horizon applies to their dependencies. The
        // second pass is conditional and stops the moment the last wanted commitment is found, so
        // a healthy chain — where commitments are a couple of epochs down and inside the first
        // pass anyway — pays nothing for it.
        let mut wanted: HashSet<TransactionId> =
            pending.iter().map(|(_, cert, _)| cert.commitment_tx_id).filter(|id| !commitments.contains_key(id)).collect();
        let mut dependency_scan_complete = true;
        if !wanted.is_empty() {
            // Blue score here against a DAA-denominated parameter: `resolve_certificate` applies
            // the authoritative DAA test, so this only has to be no *shallower* than the real
            // bound. Blue score advances no faster than DAA, which makes the budget generous in
            // the safe direction.
            let horizon = commitment_dependency_horizon(oldest_blue, dns_params.vlt.max_commitment_age_blocks);
            for chain_block in self.reachability_service.default_backward_chain_iterator(tip) {
                if wanted.is_empty() {
                    break;
                }
                let Ok(bs) = self.headers_store.get_blue_score(chain_block) else {
                    dependency_scan_complete = false;
                    break;
                };
                if bs >= oldest_blue {
                    continue; // the first pass already read this block
                }
                if bs < horizon {
                    break; // searched the whole range a commitment could legally occupy
                }
                let Ok(block_daa) = self.headers_store.get_daa_score(chain_block) else {
                    dependency_scan_complete = false;
                    break;
                };
                // Dependencies only. Certificates, verdicts, challenges and capabilities down here
                // are outside the credit window and must NOT re-enter the tally through this pass.
                for (commit_tx_id, commit) in compute_commitments_from_accepted_txs(&self.accepted_txs_of_chain_block(chain_block)) {
                    if !wanted.contains(&commit_tx_id) {
                        continue;
                    }
                    // Same verification as the first pass, and the walk is along the selected chain
                    // from the pin, so anything found here is an ancestor of the pin by
                    // construction. A commitment with a matching id on a competing branch is not
                    // reachable from this iterator at all.
                    if let Some(record) = verified_commitment(commit, bonds, net_id, bs, block_daa) {
                        commitments.insert(commit_tx_id, record);
                        wanted.remove(&commit_tx_id);
                    }
                }
            }
        }

        ComputeOverlayWalk { challenges, capabilities, commitments, verdicts, certificates: pending, dependency_scan_complete }
    }

    /// Resolve one accepted certificate against the chain: check the executor's claim, find its
    /// phase-1 commitment, derive the sortition beacon, and draw the verifier committee.
    ///
    /// `None` means the certificate credits nothing *at this point of view*. That covers both the
    /// permanently invalid (a bad signature, an unregistered profile) and the merely early (a
    /// beacon epoch that has not formed yet) — the walk re-runs, so there is no need to tell them
    /// apart here.
    ///
    /// Shared with [`Self::pending_compute_verdicts`] on purpose. A validator deciding whether it
    /// was drawn onto a committee has to reach the same answer consensus will: a node using its
    /// own approximation would publish verdicts it is not drawn for (wasted fees) or skip ones it
    /// is (a job that never mints, and an honest executor that never gets paid).
    #[allow(clippy::too_many_arguments)]
    fn resolve_certificate(
        &self,
        cert: &ComputeCertificatePayload,
        block_daa: u64,
        anchor: &CanonicalLaggedEpochAnchor,
        bonds: &[StakeBondRecord],
        walk: &ComputeOverlayWalk,
        dns_params: &DnsParams,
        net_id: &[u8],
        anchors: &BTreeMap<u64, CanonicalLaggedEpochAnchor>,
    ) -> Result<ResolvedCertificate, VltCreditSkipReason> {
        // (2) Executor bond: exists, bound to the declared id, Active at the anchor.
        let Some(bond) = bonds.iter().find(|b| b.bond_outpoint == cert.executor_bond_outpoint) else {
            return Err(VltCreditSkipReason::BondMissing);
        };
        if cert.executor_id != bond.validator_pubkey_hash || !is_bond_active_at(bond, anchor.anchor_daa_score) {
            return Err(VltCreditSkipReason::BondInactive);
        }
        // (3) Executor signature over the receipt it is claiming.
        let job_id = job_spec_id(&cert.spec);
        let receipt_hash = compute_receipt_hash(&cert.spec, &cert.receipt);
        let digest = compute_certificate_message(net_id, cert.epoch, job_id, receipt_hash, cert.executor_bond_outpoint).as_bytes();
        if !matches!(
            verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &cert.executor_signature, COMPUTE_CERT_MLDSA87_CONTEXT),
            Ok(true)
        ) {
            return Err(VltCreditSkipReason::ExecutorSignatureInvalid);
        }
        // (4a) Phase-1 commitment: must exist on THIS chain, name the same executor and the
        // same job, and lie within `max_commitment_age_blocks` behind the certificate.
        let Some(commitment) = walk.commitments.get(&cert.commitment_tx_id) else {
            // Absent only if the dependency pass actually searched the whole range it could
            // occupy. Otherwise this node simply has not loaded it, which is not a fact about the
            // chain and must never be cached as one.
            return Err(if walk.dependency_scan_complete {
                VltCreditSkipReason::CommitmentAbsentFromCanonicalHistory
            } else {
                VltCreditSkipReason::CommitmentNotLoaded
            });
        };
        if commitment.job_id != job_id
            || commitment.executor_id != cert.executor_id
            || commitment.bond_outpoint != cert.executor_bond_outpoint
        {
            return Err(VltCreditSkipReason::CommitmentMismatch);
        }
        // The published input must be the one this spec commits to. `job_id` already covers
        // `p_j`, so this is what ties the *bytes* on chain to the digest in the spec — without
        // it an executor could commit to an input nobody can use and certify against a
        // different one, leaving its committee unable to replay the job it is auditing.
        if job_input_commitment(&commitment.input) != cert.spec.input_commitment {
            return Err(VltCreditSkipReason::CommitmentMismatch);
        }
        if block_daa < commitment.accepted_daa_score
            || block_daa.saturating_sub(commitment.accepted_daa_score) > dns_params.vlt.max_commitment_age_blocks
        {
            return Err(VltCreditSkipReason::CommitmentOutOfRange);
        }

        // (4b) The sortition BEACON is the canonical anchor of the epoch AFTER the one that
        // accepted the commitment — a block that did not exist when the executor fixed
        // `job_id`. This is what makes the committee unguessable: grinding `sampling_seed` at
        // commitment time is grinding against randomness that has not been drawn yet.
        let beacon_epoch = commitment_beacon_epoch(commitment.accepted_blue_score, dns_params.attestation_epoch_length_blue_score);
        // Beacon epoch not ready yet ⇒ not creditable YET, rather than invalid.
        let Some(beacon_anchor) = anchors.get(&beacon_epoch) else {
            return Err(VltCreditSkipReason::BeaconNotReady);
        };
        // A certificate may not predate its own beacon, or the executor would have revealed
        // before the randomness that picks its auditors was fixed.
        if block_daa < beacon_anchor.anchor_daa_score {
            return Err(VltCreditSkipReason::CertificatePredatesBeacon);
        }

        // (4c) Verifier committee: sortitioned from validators that declared THIS job's
        // profile and are Active-bonded at the anchor. Class matching is inside
        // `select_verifiers` and is a correctness requirement, not a filter — see its
        // doc comment on PALW's fp-per-vendor determinism.
        //
        // Unregistered profile ⇒ mints nothing anyway.
        let Some(entry) = dns_params.vlt.model_cost_table.lookup(cert.spec.model_weights_hash, cert.spec.runtime_hash) else {
            return Err(VltCreditSkipReason::UnregisteredProfile);
        };
        // The pool is taken AT THE BEACON, not at the certificate. The beacon is the moment the
        // randomness is fixed, so measuring the candidates there is what makes the committee a
        // function of that draw alone. Measured at the certificate instead, the executor picks the
        // pool by choosing when to publish — and anyone can change a drawn committee after the fact
        // by declaring a capability, invalidating verdicts the real committee already gave.
        // From the STORE, not the walk. A declaration is valid for `max_capability_validity_blocks`
        // and the walk spans `vlt_credit_window_blue_score` — three orders of magnitude less — so
        // drawing the pool from whatever the walk happened to collect empties it the moment the
        // walk floor rises past a declaration that is still perfectly in force. Every honest
        // verdict then belongs to no committee and the certificate reads as unverified for good.
        // Ancestry, not DAA. The store follows the SELECTED chain, and this resolution also runs
        // while scoring a candidate branch (pinned at the two branches' shared ancestor). Without
        // this filter that scoring would draw committee candidates from declarations the branch
        // under evaluation does not contain — one branch borrowing another's verifiers, which is a
        // consensus split rather than a slow path. `accepted_daa_score <= pov` does not substitute:
        // a DAA score is a number like a clock, and two branches can carry blocks at the same one.
        //
        // UNION of the store and this walk (2026-08-11 audit, IBD/live divergence class). The
        // store is written by `stage_compute_capabilities` at commit time, so during a virtual
        // advance it holds only what was committed BEFORE this batch. A node that lived the
        // chain commits every block separately and therefore sees block N-1's declarations while
        // resolving block N; a replayer batches many blocks and sees none of them — a different
        // committee, hence a different verified/unverified verdict, hence divergent credit and
        // audit-fee outputs. The walk covers exactly the segment the store is missing, and the
        // ancestry filter below is what keeps the union honest: a declaration from a branch this
        // beacon does not contain is dropped whichever source it came from.
        // Keyed on (declaration block, validator, bond, profile): one declaration is one row, and
        // the same declaration reached from both sources must collapse to one candidate.
        let mut seen_caps: HashSet<(BlockHash, Hash64, TransactionOutpoint, Hash64, Hash64)> = HashSet::new();
        let stored_capabilities: Vec<ComputeCapabilityRecord> = self
            .compute_capability_store
            .read()
            .all()
            .into_iter()
            .chain(walk.capabilities.iter().cloned())
            .filter(|r| seen_caps.insert((r.declaration_block, r.validator_id, r.bond_outpoint, r.model_weights_hash, r.runtime_hash)))
            .filter(|r| {
                r.declaration_block == beacon_anchor.anchor_hash
                    || self.reachability_service.is_chain_ancestor_of(r.declaration_block, beacon_anchor.anchor_hash)
            })
            .collect();
        let declared = capability_candidate_pool(
            &stored_capabilities,
            cert.spec.model_weights_hash,
            cert.spec.runtime_hash,
            beacon_anchor.anchor_daa_score,
        );
        let candidates: Vec<(Hash64, Hash64)> = declared
            .into_iter()
            .filter(|(id, _)| bonds.iter().any(|b| b.validator_pubkey_hash == *id && is_bond_active_at(b, anchor.anchor_daa_score)))
            .collect();
        let committee: HashSet<Hash64> = select_verifiers(
            job_id,
            cert.executor_id,
            beacon_anchor.anchor_hash,
            entry.runtime_class_id,
            &candidates,
            dns_params.vlt.verifier_committee_size as usize,
        )
        .into_iter()
        .collect();
        Ok(ResolvedCertificate { job_id, receipt_hash, committee, commitment_tx_id: cert.commitment_tx_id })
    }

    /// The accepted certificates `validator_id` was sortitioned to audit and has not yet judged,
    /// newest first, capped at `limit`. Backs [`ConsensusApi::get_pending_compute_verdicts`].
    ///
    /// Walks the SAME depth the credit walk does. That is not a conservative choice, it is the
    /// only correct one: the committee is drawn from the capability declarations visible in the
    /// window, so a shallower walk would see a smaller candidate pool and draw a different
    /// committee than the one consensus credits against.
    ///
    /// "Has not yet judged" is deliberately coarse — it means no verdict of ours for this
    /// certificate is on THIS chain. A verdict still sitting in the mempool will therefore be
    /// re-offered; deduplicating that is the caller's job, since only the caller knows what it has
    /// in flight, and one verdict per verifier is enforced by
    /// [`verdicts_for_certificate`] regardless.
    pub(crate) fn pending_compute_verdicts(
        &self,
        tip: BlockHash,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
        pov_daa_score: u64,
        validator_id: Hash64,
        limit: usize,
    ) -> Vec<PendingComputeVerdict> {
        if limit == 0 || !dns_params.vlt_shadow_active_at(pov_daa_score) {
            return Vec::new();
        }
        let anchors = self.canonical_anchors_in_window(tip, dns_params, dns_params.vlt_credit_window_blue_score);
        let Ok(tip_blue) = self.headers_store.get_blue_score(tip) else {
            return Vec::new();
        };
        let oldest_blue = tip_blue.saturating_sub(dns_params.vlt_credit_window_blue_score);
        let walk = self.walk_compute_overlay(tip, bonds, net_id, dns_params, oldest_blue, tip_blue);

        // Challenged certificates FIRST. A challenge is an accusation, not a finding, and only the
        // committee's verdicts can settle it — so a challenged job is the one the network most
        // needs audited, not the one to skip. Skipping it was a free denial-of-service: file a
        // challenge, no verifier looks, no verdict arrives, the challenge stays Undecided forever
        // and the executor's credit never lands. Within each group the backward walk's newest-first
        // order stands, so the executor still waiting on its committee is served before older work.
        let mut queue: Vec<&(TransactionId, ComputeCertificatePayload, u64)> = walk.certificates.iter().collect();
        queue.sort_by_key(|(cert_tx_id, _, _)| !walk.challenges.iter().any(|c| c.certificate_tx_id == *cert_tx_id));

        let mut out = Vec::new();
        for (cert_tx_id, cert, block_daa) in queue {
            if out.len() == limit {
                break;
            }
            let (cert_tx_id, block_daa) = (*cert_tx_id, *block_daa);
            // Auditing our own job is the one thing sortition guarantees we are never asked to do;
            // check it here too so a bug in the pool cannot make us build a certainly-invalid tx.
            if cert.executor_id == validator_id {
                continue;
            }
            let Some(anchor) = anchors.get(&cert.epoch) else {
                continue;
            };
            let Ok(resolved) = self.resolve_certificate(cert, block_daa, anchor, bonds, &walk, dns_params, net_id, &anchors) else {
                continue;
            };
            if !resolved.committee.contains(&validator_id) {
                continue;
            }
            // Already voted on this chain — one verdict per verifier is all that counts, and a
            // second one is evidence against us rather than a vote.
            if walk.verdicts.iter().any(|v| v.certificate_tx_id == cert_tx_id && v.payload.verifier_id == validator_id) {
                continue;
            }
            let Some(commitment) = walk.commitments.get(&resolved.commitment_tx_id) else {
                continue;
            };
            out.push(PendingComputeVerdict {
                certificate_tx_id: cert_tx_id,
                job_id: resolved.job_id,
                spec: cert.spec.clone(),
                input: commitment.input.clone(),
                executor_id: cert.executor_id,
                executor_receipt_hash: resolved.receipt_hash,
                executor_bond_outpoint: cert.executor_bond_outpoint,
                certificate_daa_score: block_daa,
            });
        }
        out
    }

    /// §6 audit fee: the coinbase outputs paying each verifier whose verdict was counted for a
    /// certificate that leaves its challenge window **at this block**, and their total.
    ///
    /// # Why the challenge-window crossing is the trigger
    ///
    /// It is the one moment in a certificate's life that happens exactly once per chain, is
    /// determined by data every node already has, and is late enough that the verdict set is
    /// settled. Paying at verdict-inclusion time is not an option: the verdict names a certificate
    /// the coinbase path cannot resolve, so a fabricated verdict against a fabricated certificate
    /// would be indistinguishable from real work and the fee would fund spam. Paying up front, on
    /// the certificate, is worse still — a committee member paid before it audits is paid *not* to,
    /// since publishing then only costs it a transaction fee.
    ///
    /// Both verdicts are paid. A refutation is the same work as a confirmation and reports it
    /// honestly; charging for it would make the fraud-detection role the expensive one.
    ///
    /// # Cost
    ///
    /// This runs the full compute-overlay walk on every block once VLT weighting is live, because
    /// committee membership is only decidable against the capability declarations in the credit
    /// window. Below the fence — every shipped network — it returns immediately and costs nothing.
    /// Activating VLT weighting should be preceded by giving this the same treatment
    /// [`collect_compute_credits`] got: a store that serves settled epochs so the walk only covers
    /// the unfinalized tail.
    pub(super) fn compute_audit_fee_outputs(
        &self,
        dns_params: &DnsParams,
        daa_score: u64,
        selected_parent: BlockHash,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        budget: u64,
    ) -> (Vec<TransactionOutput>, u64) {
        let fee = dns_params.vlt.audit_fee_sompi;
        if fee == 0 || budget < fee || !dns_params.vlt_shadow_active_at(daa_score) {
            return (Vec::new(), 0);
        }
        // Audit-emission v0.2 §2.3: past the token fence, verification is paid from R(E) in TOK
        // and the base-coin fee retires — base-coin issuance returns to the coinbase schedule
        // alone. One info line at the flip (per process), because "silently stopped minting"
        // and "never minted" must stay distinguishable in a log.
        if dns_params.tkn.active_at(daa_score) {
            if !self.audit_fee_retired_logged.swap(true, std::sync::atomic::Ordering::Relaxed) {
                info!("[token] audit-fee(base) retired at daa={daa_score} — verification now pays from R(E) (audit-emission v0.2)");
            }
            return (Vec::new(), 0);
        }
        let Ok(parent_daa) = self.headers_store.get_daa_score(selected_parent) else {
            return (Vec::new(), 0);
        };
        let Ok(parent_blue) = self.headers_store.get_blue_score(selected_parent) else {
            return (Vec::new(), 0);
        };
        let window = dns_params.vlt.challenge_window_blocks;
        let anchors = self.canonical_anchors_in_window(selected_parent, dns_params, dns_params.vlt_credit_window_blue_score);
        let oldest_blue = parent_blue.saturating_sub(dns_params.vlt_credit_window_blue_score);
        let walk = self.walk_compute_overlay(selected_parent, bonds, net_id, dns_params, oldest_blue, parent_blue);

        // A certificate crosses its window at this block iff the parent had not yet passed it and
        // this block has. Each certificate therefore pays out exactly once per chain, with no
        // cross-block dedup state to keep.
        let mut crossing: Vec<&(TransactionId, ComputeCertificatePayload, u64)> = walk
            .certificates
            .iter()
            .filter(|(_, _, accepted)| parent_daa.saturating_sub(*accepted) <= window && daa_score.saturating_sub(*accepted) > window)
            .collect();
        // The walk yields newest-first; pin a total order so construction and validation build
        // byte-identical outputs when the budget truncates the tail.
        crossing.sort_by_key(|(tx_id, _, accepted)| (*accepted, *tx_id));

        let mut outputs = Vec::new();
        let mut spent = 0u64;
        for (cert_tx_id, cert, accepted) in crossing {
            let Some(anchor) = anchors.get(&cert.epoch) else {
                continue;
            };
            let Ok(resolved) = self.resolve_certificate(cert, *accepted, anchor, bonds, &walk, dns_params, net_id, &anchors) else {
                continue;
            };
            for att in verdicts_for_certificate(
                &walk.verdicts,
                *cert_tx_id,
                resolved.job_id,
                resolved.receipt_hash,
                *accepted,
                &resolved.committee,
            ) {
                // Pay the verifier's bond owner, the same payee the attestation rewards use.
                let Some(bond) = bonds.iter().find(|b| b.bond_outpoint == att.bond_outpoint) else {
                    continue;
                };
                // Whole-output budget cap, value-conserving: stop at the first payment that would
                // overrun the validator pool and leave the ordered tail unpaid rather than mint
                // past it.
                if spent.saturating_add(fee) > budget {
                    return (outputs, spent);
                }
                spent = spent.saturating_add(fee);
                outputs.push(TransactionOutput::new(fee, p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload)));
            }
        }
        (outputs, spent)
    }

    /// ADR-0033 §4 (B14): the PALW credit outputs of the block being built or validated at
    /// `daa_score` on top of `selected_parent` — `base(C)` to each creditable commitment's
    /// executor and `ρ_v · base(C)` to each paid panel attester, resolved to bond reward
    /// spks exactly like the audit fee.
    ///
    /// A commitment crosses its `challenge_close_daa` at this block iff the parent had not
    /// yet passed it and this block has (the audit-fee crossing rule), so each commitment
    /// is decided exactly once per chain with no cross-block dedup state. Every fact the
    /// gate reads is assembled by walking THIS block's selected-parent chain backward
    /// across the challenge horizon — never the virtual-maintained store — so construction
    /// and validation of the same block compute identical outputs even while virtual
    /// points elsewhere, and a reorg re-decides identically on every node (ADR-0033 §5).
    ///
    /// # Cost
    ///
    /// Nothing when the fence is `None` (every shipped network — the caller gates). Active,
    /// it reads `w_challenge + Δ_bind + slack` chain blocks' acceptance data per block: the
    /// same activation-gated posture as [`Self::compute_audit_fee_outputs`], and like it,
    /// a settled index is the optimization ADR-0033's preconditions require before any
    /// real network carries the fence.
    pub(super) fn compute_palw_credit_outputs(
        &self,
        credit: &kaspa_consensus_core::palw_credit::PalwCreditParamsV1,
        daa_score: u64,
        selected_parent: BlockHash,
        bonds: &[StakeBondRecord],
        class_state: &crate::model::stores::palw_class_state::PalwClassStateView,
    ) -> Vec<TransactionOutput> {
        use kaspa_consensus_core::blockhash::BlockHashExtensions;
        use kaspa_consensus_core::palw_carriage::{PalwCarriageV1, decode_palw_stage1_body};
        use kaspa_consensus_core::palw_credit::{PalwObservedAttestationV1, PalwObservedCommitmentV1, decide_credit_v1};

        let windows = &credit.registration.windows;
        // Nothing can have crossed before activation plus one full window.
        if daa_score <= credit.activation_daa.saturating_add(windows.w_challenge) {
            return Vec::new();
        }
        let Ok(parent_daa) = self.headers_store.get_daa_score(selected_parent) else {
            return Vec::new();
        };
        // Walk the selected-parent chain back across the whole challenge horizon.
        let depth = windows.w_challenge.saturating_add(windows.delta_bind).saturating_add(windows.prosecution_slack);
        let mut chain_rev: Vec<(BlockHash, u64)> = Vec::new(); // newest-first
        let mut commitments = Vec::new();
        let mut attestations = Vec::new();
        let mut refutations = Vec::new();
        let mut current = selected_parent;
        loop {
            let Ok(cur_daa) = self.headers_store.get_daa_score(current) else { break };
            if parent_daa.saturating_sub(cur_daa) > depth {
                break;
            }
            chain_rev.push((current, cur_daa));
            for (tx_id, record) in palw_carriage_records_from_accepted_txs(&self.accepted_txs_of_chain_block(current), cur_daa) {
                match decode_palw_stage1_body(record.kind, &record.body) {
                    Ok(PalwCarriageV1::Commitment(c)) => commitments.push((tx_id, c, cur_daa)),
                    Ok(PalwCarriageV1::Attestation(a)) => attestations.push((a, cur_daa)),
                    Ok(PalwCarriageV1::Refutation(r)) => refutations.push((r.evidence, cur_daa)),
                    _ => {}
                }
            }
            let Ok(parent) = self.ghostdag_store.get_selected_parent(current) else { break };
            if parent == current || parent.is_origin() {
                break;
            }
            current = parent;
        }
        // Crossing commitments, in one pinned order (construction == validation).
        let mut crossing: Vec<&(TransactionId, kaspa_consensus_core::palw_carriage::PalwCommitmentCarriageV1, u64)> = commitments
            .iter()
            .filter(|(_, _, accepted)| {
                parent_daa.saturating_sub(*accepted) <= windows.w_challenge
                    && daa_score.saturating_sub(*accepted) > windows.w_challenge
            })
            .collect();
        crossing.sort_by_key(|(tx_id, _, accepted)| (*accepted, *tx_id));
        // B2: one credit per COMMITTED ROOT. The same root carried by two transactions crossed
        // twice and was paid twice — the carriage is relayable, so duplicating it costs a fee and
        // mints a second base(C). Dedup happens after the pinned sort, so which copy survives is a
        // function of `(accepted_daa, tx_id)` and not of walk order.
        {
            let mut seen: std::collections::BTreeSet<kaspa_consensus_core::Hash64> = std::collections::BTreeSet::new();
            crossing.retain(|(_, c, _)| seen.insert(c.committed_root));
        }
        if crossing.is_empty() {
            return Vec::new();
        }
        let subsidy = self.coinbase_manager.calc_block_subsidy(daa_score);
        // B3/B4: the per-block mint ceiling that makes ADR-0033 §4e non-vacuous.
        //
        // `max_leverage_holds_v1` bounds an attacker's pre-unbonding gain as
        // `g_max = base(C) × (unbonding / min_credit_interval + 1)` — i.e. the inequality ASSUMES
        // one credited job per `min_credit_interval_daa`. Nothing enforced that, and a block credited
        // every commitment that crossed in it, so the real ceiling was `base(C) × commitments` and
        // the safety margin the registration was validated against was fiction.
        //
        // One job's full payout — `base(C)` plus its `q` attester shares — is therefore the budget
        // for the whole block. Draining is PREFIX-MANDATORY, matching `palw_credit_batch`'s rule
        // (ADR-0037 D7): stop at the first record that does not fit rather than skipping it, so the
        // set credited is a prefix of the pinned order and cannot be cherry-picked.
        // The emergency stop, finally reachable. `class_frozen` was hardcoded `false` at the panel
        // site, so the ladder's Frozen state existed as a type and could never halt anything. It is
        // now read from chain state through a view, and it fails CLOSED: a class this chain point
        // cannot establish as Active mints nothing (audit §3.4).
        let class_id = credit.registration.runtime_class_id;
        if class_state.is_frozen(&class_id) {
            return Vec::new();
        }
        let one_job_ceiling = credit.one_job_ceiling_sompi(subsidy);
        let mut spent: u64 = 0;
        let mut outputs = Vec::new();
        for (_, commitment, accepted) in crossing {
            let logits_root = match commitment.binding.as_ref() {
                Some(binding) => binding.full_logits_trace_root,
                None => commitment.committed_root, // bare v2: the committed root IS the logits root
            };
            // Anchor: the first chain block at or past accepted + Δ_bind (ADR-0028 §2).
            let anchor_daa = accepted.saturating_add(windows.delta_bind);
            let Some((anchor_hash, anchor_block_daa)) = chain_rev.iter().rev().find(|(_, daa)| *daa >= anchor_daa) else {
                continue; // no anchor on this chain — the job is not decidable here
            };
            // AUTHENTICATE THE COMMITMENT before it is treated as a claim at all. Nothing in this
            // walk verified a signature, so a single bonded attacker minted base(C) per crossing
            // with zero inference: the carriage's ML-DSA-87 signature and the digest it covers both
            // existed and neither was ever checked (audit, credit-path critical).
            //
            // Resolved by bond OUTPOINT and then required to match the claimed validator id — the
            // outpoint is the unique identity, and without the cross-check a commitment could name
            // one bond's outpoint while claiming another's id, which is also the id that excludes
            // the executor from its own panel.
            let Some(executor_bond) = bonds.iter().find(|b| b.bond_outpoint == commitment.bond_outpoint) else { continue };
            if executor_bond.validator_pubkey_hash != commitment.validator_id
                || !kaspa_consensus_core::dns_finality::is_bond_active_at(executor_bond, *accepted)
            {
                continue;
            }
            let commitment_digest = kaspa_consensus_core::palw_carriage::palw_commitment_carriage_message_v1(
                commitment.validator_id,
                commitment.bond_outpoint,
                commitment.committed_form,
                commitment.committed_root,
                kaspa_consensus_core::palw_carriage::palw_carriage_envelope_hash_v1(&commitment.envelope),
            );
            if !Self::verify_palw_commitment_signature(&executor_bond.validator_pubkey, &commitment_digest, &commitment.signature) {
                continue;
            }
            // The candidate set, built HERE rather than hoisted, because `bonded` is a question
            // about a point of view and the point of view is this commitment's anchor.
            //
            // It used to be the constant `true` for every record in `bonds`, and `bonds` is the
            // whole view (`ActiveBondView::records()` returns every record it holds, not the active
            // ones). So a Slashed, Unbonding or not-yet-Active bond took a panel seat and could be
            // PAID for attesting — the eligibility rule ADR-0028 §2 states, and that
            // `select_replay_panel_v1`'s own doc says lives in the function, was being satisfied by
            // a hardcoded answer from the caller. `effective_bond_status` at the anchor's DAA is
            // that answer, and it is the same function the rest of the overlay judges bonds with.
            //
            // `frozen` stays `false` here on purpose: freezing is decided CLASS-wide, once, and
            // fail-closed at the top of this function (`class_state.is_frozen`), so a per-candidate
            // copy could only ever disagree with it.
            let candidates = kaspa_consensus_core::palw_credit::panel_candidates_at_anchor_v1(
                bonds,
                credit.registration.runtime_class_id,
                *anchor_block_daa,
            );
            let observed = PalwObservedCommitmentV1 {
                committed_root: commitment.committed_root,
                logits_root,
                executor_id: commitment.validator_id,
                runtime_class_id: commitment.envelope.runtime_class_id,
                accepted_daa: *accepted,
            };
            let observed_atts: Vec<PalwObservedAttestationV1> = attestations
                .iter()
                .filter(|(a, _): &&(kaspa_consensus_core::palw_carriage::PalwAttestationCarriageV1, u64)| {
                    a.commitment_root == commitment.committed_root
                })
                // AUTHENTICATE each attestation the same way. A forged attestation naming a drawn
                // panel member paid an attacker-chosen bond, because the payee is the filing bond
                // and nothing checked that the filer signed anything.
                .filter(|(a, daa)| {
                    let Some(bond) = bonds.iter().find(|b| b.bond_outpoint == a.bond_outpoint) else { return false };
                    bond.validator_pubkey_hash == a.attester_id
                        && kaspa_consensus_core::dns_finality::is_bond_active_at(bond, *daa)
                        && Self::verify_palw_attestation_signature(
                            &bond.validator_pubkey,
                            // ADR-0009 Addendum A.3: the network discriminator IS the genesis hash,
                            // the same one every other PALW signature path on this node uses. Binding
                            // it means a devnet attestation cannot be replayed on mainnet.
                            &a.attestation.message(self.genesis.hash.as_byte_slice()),
                            &a.attestation.signature,
                        )
                })
                .map(|(a, daa)| PalwObservedAttestationV1 {
                    attester_id: a.attester_id,
                    // Carried through, not dropped: this is the payee (audit B5).
                    bond_outpoint: a.bond_outpoint,
                    attested_logits_root: a.attestation.full_logits_trace_root,
                    accepted_daa: *daa,
                })
                .collect();
            let refutation_daas: Vec<u64> = refutations
                .iter()
                .filter(|(e, _): &&(kaspa_consensus_core::palw_carriage::PalwCarriedEvidenceV1, u64)| {
                    e.refutes(&commitment.committed_root, &logits_root)
                })
                .map(|(_, daa)| *daa)
                .collect();
            // ADR-0033 §4e assumes ONE credited job per `min_credit_interval_daa`, and until now
            // nothing remembered the last one — the walk spans `w_challenge` backward while a
            // commitment crosses `w_challenge` AFTER acceptance, so previous credits are outside it
            // by construction (audit B4). The view remembers; this is where the assumption becomes
            // a rule. `continue`, not `break`: a commitment too close to the last credit is not a
            // budget exhaustion, and a later one in the pinned order may still be far enough.
            if !class_state.credit_interval_elapsed(&class_id, *accepted, credit.registration.leverage_remedy.min_credit_interval_daa)
            {
                continue;
            }
            let decision = decide_credit_v1(credit, &observed, anchor_hash, &candidates, &observed_atts, &refutation_daas, subsidy);
            if !decision.creditable {
                continue;
            }
            // What this record would mint, counted BEFORE any output is pushed so a record either
            // pays in full or not at all.
            let this_job =
                decision.base_sompi.saturating_add(decision.attester_share_sompi.saturating_mul(decision.paid_attesters.len() as u64));
            let Some(remaining) = one_job_ceiling.checked_sub(spent) else { break };
            if this_job > remaining {
                break; // prefix-mandatory: stop, never skip past it to a smaller one
            }
            spent = spent.saturating_add(this_job);
            // base(C) to the executor's bond owner — an unbonded executor has no payout
            // target and no stake at risk, so it earns nothing; attester shares still pay,
            // because their liability (signature ∧ refutation) is their own.
            //
            // Resolved by BOND OUTPOINT, which the carriage carries, not by
            // `validator_pubkey_hash`. That hash is explicitly NOT unique (`dns_finality` says so),
            // and this used to `.find()` the first bond matching it — so with two bonds under one
            // validator key the reward went to whichever the walk happened to reach first, i.e. a
            // payee decided by iteration order rather than by the claim (audit B5). The outpoint is
            // unique by construction and is the same key the panel, the receipts and the slash
            // paths use. `executor_bond` is the one resolved and AUTHENTICATED at the top of this
            // iteration — resolving it a second time here would be a second chance to disagree with
            // the bond whose signature was actually checked.
            if decision.base_sompi > 0 {
                outputs.push(TransactionOutput::new(decision.base_sompi, p2pkh_mldsa87_spk(&executor_bond.owner_reward_spk_payload)));
            }
            for paid in &decision.paid_attesters {
                let Some(bond) = bonds.iter().find(|b| b.bond_outpoint == paid.bond_outpoint) else { continue };
                if decision.attester_share_sompi > 0 {
                    outputs.push(TransactionOutput::new(
                        decision.attester_share_sompi,
                        p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload),
                    ));
                }
            }
        }
        outputs
    }

    /// This validator's own standing in the compute overlay. Backs
    /// [`ConsensusApi::get_compute_status`].
    ///
    /// Same walk depth and same records as the credit walk, so what a node believes about its own
    /// capability and commitments is what consensus will do with them.
    pub(crate) fn compute_status(
        &self,
        tip: BlockHash,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
        pov_daa_score: u64,
        validator_id: Hash64,
        bond_outpoint: TransactionOutpoint,
    ) -> ComputeStatusView {
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let Ok(tip_blue) = self.headers_store.get_blue_score(tip) else {
            return ComputeStatusView { sink_daa_score: pov_daa_score, ..Default::default() };
        };
        let mut view = ComputeStatusView {
            shadow_active: dns_params.vlt_shadow_active_at(pov_daa_score),
            vlt_active: dns_params.vlt_weighting_active_at(pov_daa_score),
            sink_daa_score: pov_daa_score,
            epoch: tip_blue / epoch_len,
            ..Default::default()
        };
        // Keyed on the shadow fence: during the soak an operator has real work to do — commitments
        // to certify, committee seats to serve — and a view that went dark until the weight fence
        // would hide exactly the period it exists to make visible.
        if !view.shadow_active {
            return view;
        }
        let anchors = self.canonical_anchors_in_window(tip, dns_params, dns_params.vlt_credit_window_blue_score);
        let oldest_blue = tip_blue.saturating_sub(dns_params.vlt_credit_window_blue_score);
        let walk = self.walk_compute_overlay(tip, bonds, net_id, dns_params, oldest_blue, tip_blue);

        // Our live capability, as the committee draw would see it: latest expiry wins, and a
        // declaration that has lapsed at the sink is not one.
        view.capability_expiry_daa_score = walk
            .capabilities
            .iter()
            .filter(|c| c.validator_id == validator_id && c.bond_outpoint == bond_outpoint && c.is_live_at(pov_daa_score))
            .map(|c| c.expiry_daa_score)
            .max();

        // In-class peers for the profile we declared. Counted per registered entry rather than per
        // declaration so a validator that renewed does not count twice; the executor needs
        // `min_verifier_confirmations` of these or its jobs cannot reach a verdict.
        let mut peers: HashSet<Hash64> = HashSet::new();
        for entry in dns_params.vlt.model_cost_table.live() {
            let declared_ours = walk.capabilities.iter().any(|c| {
                c.validator_id == validator_id && c.covers(entry.model_weights_hash, entry.runtime_hash) && c.is_live_at(pov_daa_score)
            });
            if !declared_ours {
                continue;
            }
            for (id, class) in
                capability_candidate_pool(&walk.capabilities, entry.model_weights_hash, entry.runtime_hash, pov_daa_score)
            {
                if id != validator_id
                    && class == entry.runtime_class_id
                    && bonds.iter().any(|b| b.validator_pubkey_hash == id && is_bond_active_at(b, pov_daa_score))
                {
                    peers.insert(id);
                }
            }
        }
        view.in_class_peer_count = peers.len();

        // Our commitments that no certificate of ours has completed. A certificate that failed to
        // resolve (for instance because its beacon is not ready) still consumed the commitment as
        // far as the executor is concerned — re-certifying the same job would only produce a
        // second claim that the `(executor, job)` dedup drops — so the certificate's mere presence
        // closes it.
        let certified: HashSet<TransactionId> = walk
            .certificates
            .iter()
            .filter(|(_, cert, _)| cert.executor_id == validator_id)
            .map(|(_, cert, _)| cert.commitment_tx_id)
            .collect();
        for (commitment_tx_id, c) in walk.commitments.iter() {
            if c.executor_id != validator_id || c.bond_outpoint != bond_outpoint || certified.contains(commitment_tx_id) {
                continue;
            }
            let beacon_epoch = commitment_beacon_epoch(c.accepted_blue_score, epoch_len);
            view.open_commitments.push(OpenComputeCommitment {
                commitment_tx_id: *commitment_tx_id,
                job_id: c.job_id,
                input: c.input.clone(),
                accepted_daa_score: c.accepted_daa_score,
                beacon_epoch,
                beacon_ready: anchors.contains_key(&beacon_epoch),
                expired: pov_daa_score.saturating_sub(c.accepted_daa_score) > dns_params.vlt.max_commitment_age_blocks,
            });
        }
        // Oldest first: a job that is running out of `max_commitment_age_blocks` is the one worth
        // certifying next.
        view.open_commitments.sort_by_key(|c| (c.accepted_daa_score, c.commitment_tx_id));
        view
    }

    /// Persist every epoch in the credit window that has just become finalized, so later commits
    /// serve it from [`DbVltCreditStore`] instead of re-deriving it.
    ///
    /// Write-once: an epoch already present is skipped, and only epochs [`vlt_epoch_finalized`]
    /// accepts are written — caching a live epoch would freeze a value a later challenge or reorg
    /// could still change, and every subsequent read would prefer the stale row over the truth.
    ///
    /// Inert while the VLT fence is: no row is ever written on today's networks.
    fn stage_vlt_credits(&self, batch: &mut WriteBatch, sink: BlockHash, sink_daa: u64, dns_params: &DnsParams) {
        if !dns_params.vlt_shadow_active_at(sink_daa) {
            return;
        }
        let bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        let net_id_hash = self.genesis.hash;
        // Pinned at the sink, and only finalized epochs are read out of it below — past both the
        // challenge window and the reorg horizon, where every branch already agrees, so the rows
        // this writes are the same rows a branch-pinned snapshot would later serve.
        let snapshot = self.vlt_epoch_snapshot(sink, sink_daa, &bonds, net_id_hash.as_byte_slice(), dns_params, true, false);
        // Write-once means a row written now is the answer forever. A table with a dependency this
        // node could not load is not an answer, and caching it seals a local storage limit into
        // consensus history as a permanent zero — which is precisely how twenty executed,
        // certified and confirmed jobs came to be worth nothing on 2026-08-09. Skip the write and
        // try again on a later commit, when the walk can see what it needs.
        if !snapshot.resolution_complete() {
            return;
        }
        let credits = snapshot.credits();
        let anchors = self.canonical_anchors_in_window(sink, dns_params, dns_params.vlt_credit_window_blue_score);
        for (&epoch, anchor) in anchors.iter() {
            if !vlt_epoch_finalized(
                anchor.anchor_daa_score,
                sink_daa,
                dns_params.vlt.challenge_window_blocks,
                dns_params.max_reorg_horizon_blocks,
            ) {
                continue;
            }
            if self.vlt_credit_store.has(epoch).unwrap_or(false) {
                continue; // immutable once written
            }
            let mut row =
                VltEpochCredits::from_unordered(credits.iter().filter_map(|(v, per_epoch)| per_epoch.get(&epoch).map(|x| (*v, *x))));
            // Audit-emission v0.2 §2.3: audit weight exists only for epochs whose anchor sits at
            // or above the token fence — the same flip that retires the base-coin audit fee. A
            // pre-fence epoch's row keeps an empty audit vec, so settlement stays v0.1-shaped
            // for exactly the epochs the sompi fee already paid.
            if dns_params.tkn.active_at(anchor.anchor_daa_score) {
                row = row.with_audit(snapshot.audit().iter().filter_map(|(v, per_epoch)| per_epoch.get(&epoch).map(|x| (*v, *x))));
            }
            self.vlt_credit_store.set_batch(batch, epoch, row).unwrap();
        }
    }

    /// MISAKA Compute Token Program (design v0.1 §9.2): fold accepted token ops from
    /// newly-buried selected-chain blocks into the TOK ledger, then settle emission for
    /// epochs whose credits are finalized. One function, one staging view — fold and
    /// settlement may touch the same account row in one commit, and two independent
    /// writers into one `WriteBatch` would silently drop the earlier delta (last write
    /// per key wins).
    ///
    /// The ledger is an append-only fold with **no undo machinery**. An op applies only
    /// once its accepting chain block is buried past `max_reorg_horizon_blocks` — the
    /// same depth below which the credit accumulator trusts an epoch and the DNS reorg
    /// gate refuses to rewind — so the fold never touches a block a reorg can still
    /// remove, and "rollback" is a state this design cannot reach. The cost is latency
    /// (an op binds ~one horizon after acceptance), which is the §9.2 trade taken
    /// deliberately: at 10 bps the horizon is ~30 s, a payment-finality delay, not a
    /// liveness problem.
    ///
    /// Every effect is fenced. Below `tkn_shadow_activation_daa_score` this returns at
    /// once (every shipped preset, forever). In `[shadow, active)` it walks, verifies
    /// and logs but stages nothing — and the cursors still advance, which is what makes
    /// shadow ops void *forever* rather than retroactively binding at the fork.
    fn stage_token(
        &self,
        batch: &mut WriteBatch,
        sink: BlockHash,
        sink_daa: u64,
        dns_params: &DnsParams,
        selected_chain: &impl SelectedChainStoreReader,
    ) {
        let tkn = &dns_params.tkn;
        if !tkn.shadow_active_at(sink_daa) {
            return;
        }
        // Read-through staging: later ops (and settlement) see earlier effects within
        // this commit, and each touched key is written to the batch exactly once.
        let mut accounts: HashMap<(u64, Hash64), TokenAccount> = HashMap::new();
        let mut supplies: HashMap<u64, TokenSupply> = HashMap::new();

        self.fold_token_ledger(batch, sink, sink_daa, dns_params, selected_chain, &mut accounts, &mut supplies);
        self.settle_token_emission(batch, sink, sink_daa, dns_params, &mut accounts, &mut supplies);

        for ((asset, owner), account) in accounts {
            self.token_store.set_account_batch(batch, asset, owner, account).unwrap();
        }
        for (asset, supply) in supplies {
            self.token_store.set_supply_batch(batch, asset, supply).unwrap();
        }
    }

    /// The staged view of one `(asset, owner)` row: this commit's pending value if the
    /// fold already touched it, else the store's.
    fn staged_token_account(&self, staged: &HashMap<(u64, Hash64), TokenAccount>, asset: u64, owner: Hash64) -> TokenAccount {
        staged.get(&(asset, owner)).copied().unwrap_or_else(|| self.token_store.get_account(asset, owner).unwrap())
    }

    /// Walk the selected chain from the fold cursor while blocks are buried past the
    /// reorg horizon, applying each buried block's accepted 0x30/0x31 ops in acceptance
    /// order. Bounded per commit so a node that was down for a week amortizes the
    /// catch-up instead of stalling one commit on it.
    fn fold_token_ledger(
        &self,
        batch: &mut WriteBatch,
        sink: BlockHash,
        sink_daa: u64,
        dns_params: &DnsParams,
        selected_chain: &impl SelectedChainStoreReader,
        accounts: &mut HashMap<(u64, Hash64), TokenAccount>,
        supplies: &mut HashMap<u64, TokenSupply>,
    ) {
        const MAX_FOLD_BLOCKS_PER_COMMIT: u64 = 4096;
        let tkn = &dns_params.tkn;
        let Ok(sink_index) = selected_chain.get_by_hash(sink) else { return };
        let mut next = match self.token_store.fold_cursor().unwrap() {
            Some(v) => v,
            // First run: nothing below the shadow fence can carry a bindable op, so start
            // the fold there instead of walking the whole pre-program history.
            None => {
                Self::first_chain_index_at_daa(&self.headers_store, selected_chain, tkn.tkn_shadow_activation_daa_score, sink_index)
            }
        };
        let net_id = self.genesis.hash;
        let horizon = dns_params.max_reorg_horizon_blocks;
        let end = sink_index.min(next.saturating_add(MAX_FOLD_BLOCKS_PER_COMMIT));
        while next <= end {
            let Ok(block) = selected_chain.get_by_index(next) else { break };
            let Ok(block_daa) = self.headers_store.get_daa_score(block) else { break };
            if sink_daa.saturating_sub(block_daa) <= horizon {
                break; // not buried yet — resume on a later commit
            }
            let live = tkn.active_at(block_daa);
            for tx in self.accepted_txs_of_chain_block(block) {
                if tx.subnetwork_id == SUBNETWORK_ID_TOKEN_TRANSFER {
                    self.fold_token_transfer(&tx, live, net_id.as_byte_slice(), accounts);
                } else if tx.subnetwork_id == SUBNETWORK_ID_TOKEN_BURN {
                    self.fold_token_burn(&tx, live, net_id.as_byte_slice(), accounts, supplies);
                }
            }
            next += 1;
        }
        self.token_store.set_fold_cursor_batch(batch, next).unwrap();
    }

    /// The first selected-chain index whose block DAA is at/above `daa` — binary search
    /// over the chain index (DAA is non-decreasing along the selected chain). Holes
    /// (pruned rows) push the bound up, which errs toward re-scanning, never skipping.
    fn first_chain_index_at_daa(
        headers: &Arc<DbHeadersStore>,
        selected_chain: &impl SelectedChainStoreReader,
        daa: u64,
        hi_index: u64,
    ) -> u64 {
        let (mut lo, mut hi) = (0u64, hi_index);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match selected_chain.get_by_index(mid).ok().and_then(|h| headers.get_daa_score(h).ok()) {
                Some(d) if d < daa => lo = mid + 1,
                Some(_) => hi = mid,
                None => lo = mid + 1,
            }
        }
        lo
    }

    /// One accepted transfer op. Stateless shape was enforced at admission
    /// (`validate_token_transfer_payload`); everything stateful is judged here, and a
    /// failing op is **void** — logged and skipped, never consensus-fatal (design §4.4:
    /// the skip-class stance).
    fn fold_token_transfer(&self, tx: &Transaction, live: bool, net_id: &[u8], accounts: &mut HashMap<(u64, Hash64), TokenAccount>) {
        let Some(p) = decode_token_transfer_payload(&tx.payload) else {
            trace!("[token] transfer {} void: payload does not decode", tx.id());
            return;
        };
        let from = validator_id_from_pubkey(&p.from_pubkey);
        let digest = token_transfer_message(net_id, p.asset_id, from, p.to, p.amount, p.nonce);
        if !matches!(
            verify_mldsa87_with_context(&p.from_pubkey, &digest.as_bytes(), &p.signature, TOKEN_TRANSFER_MLDSA87_CONTEXT),
            Ok(true)
        ) {
            trace!("[token] transfer {} void: signature does not verify", tx.id());
            return;
        }
        let from_acc = self.staged_token_account(accounts, p.asset_id, from);
        let to_acc = self.staged_token_account(accounts, p.asset_id, p.to);
        match apply_token_transfer(from_acc, to_acc, p.amount, p.nonce) {
            Ok((from2, to2)) if live => {
                accounts.insert((p.asset_id, from), from2);
                accounts.insert((p.asset_id, p.to), to2);
                info!("[token] transfer {}: {from} -> {} amount {} (asset {})", tx.id(), p.to, p.amount, p.asset_id);
            }
            Ok(_) => info!("[token-shadow] transfer {} would move {} (asset {})", tx.id(), p.amount, p.asset_id),
            // Shadow mode is an observability contract: EVERY shadow-era op gets an info
            // line, would-be-void included — a trace-only outcome reads as "never folded",
            // which is exactly the false alarm the first e2e run raised.
            Err(e) if !live => info!("[token-shadow] transfer {} would be void: {e} (asset {})", tx.id(), p.asset_id),
            Err(e) => trace!("[token] transfer {} void: {e}", tx.id()),
        }
    }

    /// One accepted burn op — same void-not-fatal stance as the transfer fold.
    fn fold_token_burn(
        &self,
        tx: &Transaction,
        live: bool,
        net_id: &[u8],
        accounts: &mut HashMap<(u64, Hash64), TokenAccount>,
        supplies: &mut HashMap<u64, TokenSupply>,
    ) {
        let Some(p) = decode_token_burn_payload(&tx.payload) else {
            trace!("[token] burn {} void: payload does not decode", tx.id());
            return;
        };
        let owner = validator_id_from_pubkey(&p.owner_pubkey);
        let digest = token_burn_message(net_id, p.asset_id, owner, p.amount, p.nonce);
        if !matches!(
            verify_mldsa87_with_context(&p.owner_pubkey, &digest.as_bytes(), &p.signature, TOKEN_BURN_MLDSA87_CONTEXT),
            Ok(true)
        ) {
            trace!("[token] burn {} void: signature does not verify", tx.id());
            return;
        }
        let owner_acc = self.staged_token_account(accounts, p.asset_id, owner);
        let supply = supplies.get(&p.asset_id).copied().unwrap_or_else(|| self.token_store.get_supply(p.asset_id).unwrap());
        match apply_token_burn(owner_acc, supply, p.amount, p.nonce) {
            Ok((owner2, supply2)) if live => {
                accounts.insert((p.asset_id, owner), owner2);
                supplies.insert(p.asset_id, supply2);
                info!("[token] burn {}: {owner} destroyed {} (asset {})", tx.id(), p.amount, p.asset_id);
            }
            Ok(_) => info!("[token-shadow] burn {} would destroy {} (asset {})", tx.id(), p.amount, p.asset_id),
            Err(e) if !live => info!("[token-shadow] burn {} would be void: {e} (asset {})", tx.id(), p.asset_id),
            Err(e) => trace!("[token] burn {} void: {e}", tx.id()),
        }
    }

    /// Settle emission strictly in epoch order from the **finalized** credit rows
    /// (design §5): `reward_i(E) = ⌊R(E)·X_i(E)/X(E)⌋`, `settlement_delay_epochs`
    /// behind the wall clock. Reading only `vlt_credit_store` rows is the whole §5.3
    /// fork-invariance argument: those rows exist only for epochs buried past the
    /// challenge window and the reorg horizon, where every branch already agrees —
    /// which is also why settlement, like the fold, needs no undo.
    fn settle_token_emission(
        &self,
        batch: &mut WriteBatch,
        sink: BlockHash,
        sink_daa: u64,
        dns_params: &DnsParams,
        accounts: &mut HashMap<(u64, Hash64), TokenAccount>,
        supplies: &mut HashMap<u64, TokenSupply>,
    ) {
        const MAX_SETTLEMENTS_PER_COMMIT: u64 = 256;
        let tkn = &dns_params.tkn;
        if tkn.emission_epoch_budget_r0_atomic == 0 {
            return;
        }
        let epoch_len = dns_params.attestation_epoch_length_blue_score;
        if epoch_len == 0 {
            return;
        }
        let Ok(sink_blue) = self.headers_store.get_blue_score(sink) else { return };
        let current_epoch = sink_blue / epoch_len;
        let live = tkn.active_at(sink_daa);
        // An epoch this deep past the credit window will never gain a credit row (its
        // history predates the walkable window); recording an empty settlement and moving
        // on is what keeps the cursor from stalling forever on pre-program history.
        let never_slack = (dns_params.vlt_credit_window_blue_score / epoch_len).saturating_add(16);
        let mut next = self.token_store.settlement_cursor().unwrap().unwrap_or(tkn.emission_activation_epoch);
        let start = next;
        while next.saturating_add(tkn.settlement_delay_epochs as u64) <= current_epoch
            && next.saturating_sub(start) < MAX_SETTLEMENTS_PER_COMMIT
        {
            if self.token_store.has_settlement(next).unwrap() {
                next += 1;
                continue;
            }
            let credits = match self.vlt_credit_store.get(next) {
                Ok(credits) => credits,
                Err(StoreError::KeyNotFound(_)) => {
                    if current_epoch.saturating_sub(next) > never_slack {
                        if live {
                            let skipped = TokenEmissionSettlement { budget: emission_epoch_budget(tkn, next), ..Default::default() };
                            self.token_store.set_settlement_batch(batch, next, skipped).unwrap();
                        }
                        next += 1;
                        continue;
                    }
                    break; // young enough that the credit row may still be sealed — wait
                }
                Err(e) => panic!("settle_token_emission: vlt_credit_store.get({next}) failed: {e}"),
            };
            let settlement = emission_rewards_v2(
                emission_epoch_budget(tkn, next),
                &credits.credits,
                &credits.audit,
                tkn.emission_min_network_compute,
            );
            if live {
                for reward in settlement.rewards.iter() {
                    let mut account = self.staged_token_account(accounts, TOK_ASSET_ID, reward.owner);
                    account.balance = account.balance.saturating_add(reward.amount);
                    accounts.insert((TOK_ASSET_ID, reward.owner), account);
                }
                let mut supply =
                    supplies.get(&TOK_ASSET_ID).copied().unwrap_or_else(|| self.token_store.get_supply(TOK_ASSET_ID).unwrap());
                supply.minted = supply.minted.saturating_add(settlement.paid_total);
                supplies.insert(TOK_ASSET_ID, supply);
                info!(
                    "[token] epoch {next} settled: R={} X={} paid={} to {} recipient(s) audit={} root={}",
                    settlement.budget,
                    settlement.network_compute,
                    settlement.paid_total,
                    settlement.rewards.len(),
                    settlement.audit_paid,
                    settlement.digest(),
                );
                self.token_store.set_settlement_batch(batch, next, settlement).unwrap();
            } else {
                info!(
                    "[token-shadow] epoch {next} would settle: R={} X={} paid={} audit={} root={}",
                    settlement.budget,
                    settlement.network_compute,
                    settlement.paid_total,
                    settlement.audit_paid,
                    settlement.digest(),
                );
            }
            next += 1;
        }
        self.token_store.set_settlement_cursor_batch(batch, next).unwrap();
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
        weight: ContributionWeight<'_>,
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
        // MISAKA VLT PR 4 (§5.1/§7.1): one frozen denominator per TARGET epoch — `e → frozen(w(e))`
        // — derived on THIS walk's chain (`tip`), so a candidate branch is judged against its own
        // derivation, never against a row the selected chain froze. Target-keyed rather than
        // accepting-block-keyed: one epoch has ONE denominator, so a late vote for `e` binds and
        // is weighed by the same snapshot as a prompt one, with no grace window to reason about.
        // A target with no derivable complete snapshot falls back to the pinned-table weight,
        // unchecked — the same deterministic abstention as before (a chain too young to have
        // frozen `w(e)` is a chain where nothing was ever signed under it).
        let pov_daa = self.headers_store.get_daa_score(tip).unwrap_or(0);
        let weighting_active = dns_params.vlt_weighting_active_at(pov_daa);
        let frozen_by_target = if weighting_active {
            self.frozen_snapshots_for_targets(tip, creditable.keys().copied(), bonds, net_id, dns_params)
        } else {
            HashMap::new()
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
                // §5.1: above the weight fence a vote must have signed ITS epoch's denominator;
                // below it the audit-#4 fixed-zero invariant holds, now stateful. A vote under a
                // different denominator is not a smaller vote — it is no vote.
                let frozen = if weighting_active { frozen_by_target.get(&att.epoch) } else { None };
                if let Some(snap) = frozen {
                    if att.validator_set_commitment != snap.vote_commitment() {
                        continue;
                    }
                } else if !weighting_active && att.validator_set_commitment != Hash64::default() {
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
                    // Numerator from the SAME frozen snapshot the vote signed (its rows, not the
                    // live pinned table): §7.1's quorum is a fraction of one fixed `W(E)`, and a
                    // numerator from a different table than the denominator is not a fraction.
                    // By `validator_id` alone: a frozen row is per VALIDATOR (its aggregate
                    // bond), so requiring the vote's bond to be the row's canonical one would
                    // silently zero every vote signed under a validator's other bond.
                    let signed_weight = match frozen {
                        Some(snap) => {
                            snap.validators.iter().find(|v| v.validator_id == att.validator_id).map_or(0, |v| v.effective_weight)
                        }
                        None => weight.of(bond, bonds, anchor.anchor_daa_score, att.epoch),
                    };
                    contributions.push(AttestationContribution {
                        epoch: att.epoch,
                        validator_id: att.validator_id,
                        bond_outpoint: att.bond_outpoint,
                        signed_weight,
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
    fn dns_reorg_outcome(&self, candidate: BlockHash, prev_sink: BlockHash, candidate_bond_view: &ActiveBondView) -> DnsReorgOutcome {
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
                // Each branch's weight source is decided at its OWN tip's DAA score. Around the
                // VLT activation fence the two branches can straddle it, and scoring both under
                // one branch's rule would compare µRTE against sompi. (`canonical_daa` is the same
                // value the TTL check above read from `prev_sink`.)
                let candidate_daa = self.headers_store.get_daa_score(candidate).unwrap_or_default();
                // ONE VLT weight table, pinned at the common ancestor, for both sides. This is the
                // quorum denominator: `Q(E) = ⌊2W(E)/3⌋ + 1` only means "two thirds" if both
                // branches divide by the same `W(E)`, and `ancestor` is the deepest block they are
                // known to share, so it is the deepest point at which they cannot disagree. Read
                // at either tip instead, each branch would derive a `W(E)` over its own
                // certificates and clear a bar it wrote itself (§8.1). It is also strictly less
                // work than the two per-branch walks it replaces.
                //
                // The fence is checked at the TIPS, not at the pin: a pin below
                // `vlt_activation_daa_score` with both tips above it is a live VLT comparison
                // whose shared history predates activation, and it still needs the table.
                //
                // Handing it `canonical_bonds` is not a bias toward the canonical side. The
                // builder keeps only bonds that existed at the pin, and those the two branches
                // hold identically — a record created above the ancestor is filtered out of
                // either set, and a slash or unbond stamped above the ancestor is invisible to
                // `effective_bond_status` at every anchor the pinned walk evaluates. Passing
                // `candidate_bonds` would produce the same table.
                let ancestor_daa = self.headers_store.get_daa_score(ancestor).unwrap_or_default();
                let vlt_live = dns_params.vlt_weighting_active_at(candidate_daa) || dns_params.vlt_weighting_active_at(canonical_daa);
                let snapshot = self.vlt_epoch_snapshot(ancestor, ancestor_daa, &canonical_bonds, net_id, dns_params, vlt_live, false);
                (
                    self.stake_score_since_ancestor(
                        candidate,
                        ancestor,
                        &candidate_bonds,
                        dns_params,
                        net_id,
                        candidate_daa,
                        &snapshot,
                    ),
                    self.stake_score_since_ancestor(
                        prev_sink,
                        ancestor,
                        &canonical_bonds,
                        dns_params,
                        net_id,
                        canonical_daa,
                        &snapshot,
                    ),
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
        Some(DnsCoinbaseSettlement { long_maturity_daa, confirmed_anchor_daa })
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
            let ancestor_daa = self.headers_store.get_daa_score(ancestor).unwrap_or_default();
            let candidate_daa = self.headers_store.get_daa_score(tip).unwrap_or_default();
            let vlt_live = dns_params.vlt_weighting_active_at(candidate_daa) || dns_params.vlt_weighting_active_at(canonical_daa);
            let snapshot = self.vlt_epoch_snapshot(ancestor, ancestor_daa, &canonical_bonds, net_id, dns_params, vlt_live, false);
            let candidate_stake =
                self.stake_score_since_ancestor(tip, ancestor, &canonical_bonds, dns_params, net_id, candidate_daa, &snapshot);
            let canonical_stake =
                self.stake_score_since_ancestor(prev_sink, ancestor, &canonical_bonds, dns_params, net_id, canonical_daa, &snapshot);
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

        // ADR-0039 W4′: sink selection goes through the ONE seam, same as the header-selected
        // tip. `RankedTip`'s `Ord` calls `order_tips_v1`, which under `BlueWorkOnly` — every
        // shipped preset — compares the inner `SortableBlock`s and is therefore byte-identical to
        // the heap this replaces, hash tie-break included.
        let tip_order = self.palw_tip_order;
        let mut heap = tips
            .into_iter()
            .map(|block| RankedTip {
                block: SortableBlock { hash: block, blue_work: self.ghostdag_store.get_blue_work(block).unwrap() },
                palw: None,
                rule: tip_order,
            })
            .collect::<BinaryHeap<_>>();

        // Self-wedge diagnostics (incident 2026-07-19 §2-1): the heaviest candidate the DNS gate
        // refused during this search, if any. The heap is blue-work ordered, so the first refusal
        // is the heaviest. Reported once per search when virtual settles lower than it.
        let mut gate_rejected: Option<(BlockHash, DnsReorgOutcome, BlueWorkType)> = None;

        // We maintain the following invariant: `heap` is an antichain.
        // It holds at step 0 since tips are an antichain, and remains through the loop
        // since we check that every pushed block is not in the past of current heap
        // (and it can't be in the future by induction)
        loop {
            let candidate = heap.pop().expect("valid sink must exist").block.hash;
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
                        return (
                            candidate,
                            heap.into_sorted_iter()
                                .take_while(|s| s.block.blue_work >= filtering_blue_work)
                                .map(|s| s.block.hash)
                                .collect(),
                        );
                    }
                    if gate_rejected.is_none() {
                        gate_rejected =
                            Some((candidate, dns_outcome, self.ghostdag_store.get_blue_work(candidate).unwrap_or_default()));
                    }
                    debug!(
                        "Block candidate {} rejected by the DNS finality reorg gate ({:?}); ignored from Virtual chain.",
                        candidate, dns_outcome
                    );
                } else {
                    debug!("Block candidate {} has invalid UTXO state and is ignored from Virtual chain.", candidate)
                }
            } else if finality_point != pruning_point {
                // `finality_point == pruning_point` indicates we are at IBD start hence no warning required
                warn!("Finality Violation Detected. Block {} violates finality and is ignored from Virtual chain.", candidate);
            }
            // PRUNE SAFETY: see comment within [`resolve_virtual`]
            let prune_guard = self.pruning_lock.blocking_read();
            for parent in self.relations_service.get_parents(candidate).unwrap().iter().copied() {
                if self.reachability_service.is_dag_ancestor_of(finality_point, parent)
                    && !self.reachability_service.is_dag_ancestor_of_any(parent, &mut heap.iter().map(|sb| sb.block.hash))
                {
                    heap.push(RankedTip {
                        block: SortableBlock { hash: parent, blue_work: self.ghostdag_store.get_blue_work(parent).unwrap() },
                        palw: None,
                        rule: tip_order,
                    });
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

        // Try adding parents as long as mergeset size and number of parents limits are not reached
        while let Some(candidate) = candidates.pop_front() {
            if mergeset_size >= mergeset_size_limit || virtual_parents.len() >= max_block_parents {
                break;
            }
            match self.mergeset_increase(&virtual_parents, candidate, mergeset_size_limit - mergeset_size) {
                MergesetIncreaseResult::Accepted { increase_size } => {
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
        self.remove_bounded_merge_breaking_parents(virtual_parents, pruning_point)
    }

    fn mergeset_increase(&self, selected_parents: &[BlockHash], candidate: BlockHash, budget: u64) -> MergesetIncreaseResult {
        /*
        Algo:
            Traverse past(candidate) \setminus past(selected_parents) and make
            sure the increase in mergeset size is within the available budget
        */

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

            let current_parents = self.relations_service.get_parents(current).unwrap();
            for &parent in current_parents.iter() {
                if visited.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
        MergesetIncreaseResult::Accepted { increase_size: mergeset_increase }
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

    pub(crate) fn mandatory_attestation_deficits_for_template_snapshot(
        &self,
        selected_parent: BlockHash,
        daa_score: u64,
        selected_parent_bond_view: &ActiveBondView,
        candidate_accepted_txs: &[Transaction],
    ) -> Vec<MandatoryAttestationDeficit> {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return Vec::new();
        };
        if daa_score < dns_params.dns_activation_daa_score
            || daa_score < dns_params.mandatory_attestation_inclusion_daa_score
            || !dns_params.dns_v3_params_consistent()
        {
            return Vec::new();
        }

        let anchors = self.canonical_anchors_in_window(selected_parent, dns_params, dns_params.stake_score_window_blue_score);
        if anchors.is_empty() {
            return Vec::new();
        }

        let bonds = selected_parent_bond_view.records();
        let (parent_contributions, _) = self.collect_stake_contributions_v2(
            selected_parent,
            None,
            &bonds,
            self.genesis.hash.as_byte_slice(),
            dns_params,
            ContributionWeight::BondedStake,
        );
        let mut seen_parent: HashSet<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::Hash64, u64)> =
            HashSet::new();
        let mut seen_candidate: HashSet<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::Hash64, u64)> =
            HashSet::new();
        let mut signed_by_epoch: HashMap<u64, u64> = HashMap::new();
        let mut contributed_by_epoch: HashMap<u64, Vec<MandatoryAttestationContributionKey>> = HashMap::new();
        for c in parent_contributions {
            let key = (c.bond_outpoint, c.validator_id, c.epoch);
            if !seen_parent.insert(key) {
                continue;
            }
            let entry = signed_by_epoch.entry(c.epoch).or_insert(0);
            *entry = entry.saturating_add(c.signed_weight as u64);
            contributed_by_epoch.entry(c.epoch).or_default().push(MandatoryAttestationContributionKey {
                bond_outpoint: c.bond_outpoint,
                validator_id: c.validator_id,
                epoch: c.epoch,
            });
        }

        let bond_by_outpoint: HashMap<_, _> = bonds.iter().map(|b| (b.bond_outpoint, b)).collect();
        for att in attestations_from_accepted_txs(candidate_accepted_txs) {
            let Some(anchor) = anchors.get(&att.epoch) else {
                continue;
            };
            if att.target_hash != anchor.anchor_hash || att.target_daa_score != anchor.anchor_daa_score {
                continue;
            }
            let key = (att.bond_outpoint, att.validator_id, att.epoch);
            if seen_parent.contains(&key) || !seen_candidate.insert(key) {
                continue;
            }
            let Some(bond) = bond_by_outpoint.get(&att.bond_outpoint) else {
                continue;
            };
            if att.validator_id != bond.validator_pubkey_hash || !is_bond_active_at(bond, anchor.anchor_daa_score) {
                continue;
            }
            let digest = stake_attestation_message(
                self.genesis.hash.as_byte_slice(),
                att.epoch,
                att.target_hash,
                att.target_daa_score,
                att.validator_set_commitment,
                att.bond_outpoint,
            )
            .as_bytes();
            if !matches!(
                verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
                Ok(true)
            ) {
                continue;
            }
            let entry = signed_by_epoch.entry(att.epoch).or_insert(0);
            *entry = entry.saturating_add(bond.amount);
            contributed_by_epoch.entry(att.epoch).or_default().push(MandatoryAttestationContributionKey {
                bond_outpoint: att.bond_outpoint,
                validator_id: att.validator_id,
                epoch: att.epoch,
            });
        }

        let mut deficits = Vec::new();
        for (&epoch, anchor) in &anchors {
            let mut active_validators: Vec<_> = bonds
                .iter()
                .filter(|bond| is_bond_active_at(bond, anchor.anchor_daa_score))
                .map(|bond| MandatoryAttestationValidator {
                    bond_outpoint: bond.bond_outpoint,
                    validator_id: bond.validator_pubkey_hash,
                    stake_sompi: bond.amount,
                })
                .collect();
            active_validators.sort_by(|a, b| {
                a.validator_id
                    .cmp(&b.validator_id)
                    .then(a.bond_outpoint.transaction_id.cmp(&b.bond_outpoint.transaction_id))
                    .then(a.bond_outpoint.index.cmp(&b.bond_outpoint.index))
            });

            let expected_stake = active_validators.iter().fold(0u64, |acc, v| acc.saturating_add(v.stake_sompi));
            if expected_stake == 0
                || expected_stake < dns_params.min_active_stake_sompi
                || (active_validators.len() as u32) < dns_params.min_active_validators
            {
                continue;
            }

            let included_stake = signed_by_epoch.get(&epoch).copied().unwrap_or(0);
            if epoch_meets_quality_floor(included_stake as u128, expected_stake as u128, dns_params.stake_event_quality_floor_bps) {
                continue;
            }

            let required_stake = required_stake_for_quality_floor(expected_stake, dns_params.stake_event_quality_floor_bps);
            deficits.push(MandatoryAttestationDeficit {
                epoch,
                target_hash: anchor.anchor_hash,
                target_daa_score: anchor.anchor_daa_score,
                validator_set_commitment: kaspa_consensus_core::Hash64::default(),
                pre_body_included_stake: included_stake,
                expected_stake,
                required_stake,
                required_stake_delta: required_stake.saturating_sub(included_stake),
                quality_floor_bps: dns_params.stake_event_quality_floor_bps,
                already_contributed: contributed_by_epoch.remove(&epoch).unwrap_or_default(),
                active_validators,
            });
        }

        deficits
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
        self.build_block_template_with_selector_provider(miner_data, build_mode, evm_template_data, move |_, _| tx_selector)
    }

    pub fn build_block_template_with_selector_factory(
        &self,
        miner_data: MinerData,
        tx_selector_factory: &dyn TemplateTransactionSelectorFactory,
        build_mode: TemplateBuildMode,
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        self.build_block_template_with_selector_provider(miner_data, build_mode, evm_template_data, |latest_ready_epoch, deficits| {
            tx_selector_factory.build_selector(latest_ready_epoch, deficits)
        })
    }

    fn build_block_template_with_selector_provider<F>(
        &self,
        miner_data: MinerData,
        build_mode: TemplateBuildMode,
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
        tx_selector_provider: F,
    ) -> Result<BlockTemplate, RuleError>
    where
        F: FnOnce(Option<u64>, &[MandatoryAttestationDeficit]) -> Box<dyn TemplateTransactionSelector>,
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
        let candidate_accepted_txs = self.accepted_txs_from_virtual_state(&virtual_state);
        let latest_ready_epoch = self.latest_ready_epoch_for_template_snapshot(&virtual_state);
        let mandatory_deficits = self.mandatory_attestation_deficits_for_template_snapshot(
            virtual_state.ghostdag_data.selected_parent,
            virtual_state.daa_score,
            &template_bond_view,
            &candidate_accepted_txs,
        );
        let mut tx_selector = tx_selector_provider(latest_ready_epoch, &mandatory_deficits);
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
        let bridge_finality_fresh = self.bridge_finality_is_fresh(virtual_state.daa_score);
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
        // kaspa-pq optional DNS-finality hard inclusion: in shipped liveness-first presets this is
        // inert (`mandatory_attestation_inclusion_daa_score = u64::MAX`), so missing attestations
        // never block template production. Private hard-inclusion forks still use the deterministic
        // selected-parent + candidate-accepted + body view below.
        let candidate_accepted_txs = self.accepted_txs_from_virtual_state(&virtual_state);
        self.check_mandatory_attestation_inclusion(
            &txs,
            &candidate_accepted_txs,
            &template_bond_view,
            virtual_state.ghostdag_data.selected_parent,
            virtual_state.daa_score,
        )
        .map_err(|err| err.with_attestation_template_drops(&dropped_attestation_shards))?;
        // kaspa-pq Phase 13 (ADR-0018 §F+§E): the §F carve + §E validator pool for
        // this template, computed identically to the validation path so a block
        // mined from this template reproduces the coinbase byte-for-byte. `None`/0
        // on every current network (overlay dormant).
        // ADR-0018 §F staged rollout: None (Stage 1) / bootstrap (Stage 2) / full
        // (Stage 3) selected by DAA, identically to the validation path.
        let carve = self.dns_params.as_ref().and_then(|p| p.reward_fee_split(virtual_state.daa_score));
        let validator_pool = carve.map_or(0, |fs| {
            self.coinbase_manager.coinbase_validator_pool(
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_rewards,
                &virtual_state.mergeset_non_daa,
                fs,
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
            // MISAKA VLT §6 audit fee, from the unspent remainder of the §E validator pool. Placed
            // before the drip so both paths append in one order. Inert below the VLT fence.
            let (audit_outputs, _) = self.compute_audit_fee_outputs(
                dns_params,
                virtual_state.daa_score,
                virtual_state.ghostdag_data.selected_parent,
                &template_bond_view.records(),
                self.genesis.hash.as_byte_slice(),
                validator_pool.saturating_sub(validator_reward_outputs.iter().fold(0u64, |a, o| a.saturating_add(o.value))),
            );
            validator_reward_outputs.extend(audit_outputs);
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
        // ADR-0033 (B14): PALW credit outputs, appended after the drip in BOTH paths so the
        // output order is pinned. Dormant (`None`) on every shipped network.
        if let Some(credit) = self.palw_credit_params.as_ref() {
            let credit_outputs = self.compute_palw_credit_outputs(
                credit,
                virtual_state.daa_score,
                virtual_state.ghostdag_data.selected_parent,
                &template_bond_view.records(),
                &self.initial_palw_class_state_view(),
            );
            validator_reward_outputs.extend(credit_outputs);
        }
        let coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                virtual_state.daa_score,
                miner_data.clone(),
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_rewards,
                &virtual_state.mergeset_non_daa,
                &validator_reward_outputs,
                carve,
                (newly_included_stake, expected_stake),
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
            kaspa_consensus_core::pow_layer0::required_algo_id(
                self.pow_palw_ollama_activation.is_active(virtual_state.daa_score),
                self.pow_palw_activation.is_active(virtual_state.daa_score),
                self.pow_blake2b_sha3_activation.is_active(virtual_state.daa_score),
            ),
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
        );

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
        #[cfg(feature = "evm")]
        {
            use crate::model::stores::evm::{EvmCodeStoreReader, EvmStateCheckpointStoreReader, EvmStateDiffStoreReader};
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
        let mut verified_against = None;
        let children: Vec<BlockHash> = RelationsStoreReader::get_children(&self.relations_service, pruning_point)
            .map(|c| c.read().iter().copied().collect())
            .unwrap_or_default();
        {
            for child in children {
                let Ok(header) = self.headers_store.get_header(child) else { continue };
                if self.ghostdag_store.get_selected_parent(child).ok() != Some(pruning_point) {
                    continue; // commits to a different parent's snapshot — not this one
                }
                if header.overlay_commitment_root != got {
                    return Err(PruningImportError::ImportedOverlayCommitmentMismatch(
                        pruning_point,
                        got,
                        header.overlay_commitment_root,
                    ));
                }
                verified_against = Some(child);
                break;
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
    Accepted { increase_size: u64 },
    Rejected { new_candidate: BlockHash },
}

/// How many per-certificate credit skips one recompute may name before the diagnostic stops
/// listing them. A wide credit window on a busy network holds a lot of certificates, and a
/// diagnostic that can print all of them is one that can fill a disk.
const MAX_REPORTED_CREDIT_SKIPS: usize = 16;

/// One capability declaration, accepted only if it is bond-bound, names a registered profile under
/// its registered class, and its validator's ML-DSA-87 signature verifies.
///
/// The same checks `walk_compute_overlay` applies, so a stored row can only hold a declaration the
/// credit walk would itself have accepted. Expiry is capped at `max_capability_validity_blocks`
/// past the accepting block, so a stale declaration cannot name a far-future expiry and squat in
/// committees.
fn verified_capability(
    cap: kaspa_consensus_core::vlt::ComputeCapabilityPayload,
    bonds: &[StakeBondRecord],
    net_id: &[u8],
    declaration_block: BlockHash,
    accepted_daa_score: u64,
    vlt: &kaspa_consensus_core::vlt::VltParams,
) -> Option<ComputeCapabilityRecord> {
    let bond = bonds.iter().find(|b| b.bond_outpoint == cap.bond_outpoint)?;
    if cap.validator_id != bond.validator_pubkey_hash {
        return None;
    }
    let entry = vlt.model_cost_table.lookup(cap.model_weights_hash, cap.runtime_hash)?;
    if cap.runtime_class_id != entry.runtime_class_id {
        return None;
    }
    let digest = compute_capability_message(
        net_id,
        cap.validator_id,
        cap.bond_outpoint,
        cap.model_weights_hash,
        cap.runtime_hash,
        cap.runtime_class_id,
        cap.expiry_daa_score,
    )
    .as_bytes();
    if !matches!(
        verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &cap.signature, COMPUTE_CAPABILITY_MLDSA87_CONTEXT),
        Ok(true)
    ) {
        return None;
    }
    Some(ComputeCapabilityRecord {
        declaration_block,
        accepted_daa_score,
        validator_id: cap.validator_id,
        bond_outpoint: cap.bond_outpoint,
        model_weights_hash: cap.model_weights_hash,
        runtime_hash: cap.runtime_hash,
        runtime_class_id: cap.runtime_class_id,
        expiry_daa_score: cap.expiry_daa_score.min(accepted_daa_score.saturating_add(vlt.max_capability_validity_blocks)),
    })
}

/// One phase-1 commitment, accepted only if it is bond-bound and its executor's ML-DSA-87 signature
/// verifies.
///
/// Shared by both passes of `walk_compute_overlay`. Two copies of these checks would eventually
/// differ, and the direction that matters is the lenient one: a commitment admitted by the
/// dependency pass but rejected by the primary pass would let a certificate resolve against
/// evidence the main walk does not accept.
fn verified_commitment(
    commit: kaspa_consensus_core::vlt::ComputeCommitmentPayload,
    bonds: &[StakeBondRecord],
    net_id: &[u8],
    accepted_blue_score: u64,
    accepted_daa_score: u64,
) -> Option<ComputeCommitmentRecord> {
    let bond = bonds.iter().find(|b| b.bond_outpoint == commit.executor_bond_outpoint)?;
    if commit.executor_id != bond.validator_pubkey_hash {
        return None;
    }
    let input_commitment = job_input_commitment(&commit.input);
    let digest = compute_commitment_message(net_id, commit.job_id, input_commitment, commit.executor_bond_outpoint).as_bytes();
    if !matches!(
        verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &commit.signature, COMPUTE_COMMITMENT_MLDSA87_CONTEXT),
        Ok(true)
    ) {
        return None;
    }
    Some(ComputeCommitmentRecord {
        job_id: commit.job_id,
        executor_id: commit.executor_id,
        bond_outpoint: commit.executor_bond_outpoint,
        input: commit.input,
        accepted_blue_score,
        accepted_daa_score,
    })
}

/// The derivation loop behind [`VirtualStateProcessor::palw_equivocation_slashes`], as a free
/// function so it can be exercised without standing up a processor.
///
/// Every rejection is a silent skip rather than an error: a certificate that fails to prove an
/// equivocation is simply not evidence, and the transaction carrying it is still a valid
/// transaction. Nothing here can reject a block.
///
/// Unlike the VLT half it runs beside, this does not derive-then-filter. There, mutations come
/// from payload decoding and a second pass drops the ones whose evidence was never proved —
/// necessary because evidence arriving by MERGE skips the own-body genuineness gates. Here the
/// adjudication IS the proof and it runs per certificate, so the only mutation that can be
/// emitted is one already proved against the accused bond's own key.
pub(super) fn palw_equivocation_slashes_v1<F>(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    accepted_daa_score: u64,
    chain_network_id: &[u8],
    fence_active: bool,
    verify: F,
) -> Vec<BondMutation>
where
    F: Fn(&[u8], &kaspa_hashes::Hash, &[u8]) -> bool,
{
    use kaspa_consensus_core::palw_carriage::{
        PALW_CARRIAGE_KIND_EQUIVOCATION, PalwCarriageV1, adjudicate_equivocation_carriage_v1, decode_palw_stage1_body,
        palw_carriage_tx_kind,
    };
    if !fence_active {
        return Vec::new();
    }
    let mut out = Vec::new();
    for tx in txs {
        if palw_carriage_tx_kind(&tx.subnetwork_id) != Some(PALW_CARRIAGE_KIND_EQUIVOCATION) {
            continue;
        }
        let Ok(PalwCarriageV1::Equivocation(carriage)) = decode_palw_stage1_body(PALW_CARRIAGE_KIND_EQUIVOCATION, &tx.payload) else {
            continue;
        };
        // The accused bond must exist in THIS view. An absent bond is not a bond this node may
        // take, and it is never an error: the certificate may name a bond that was pruned, or one
        // this chain never had.
        let Some(bond) = bond_view.get(&carriage.accused_bond_outpoint) else {
            continue;
        };
        if let Ok(slashed) = adjudicate_equivocation_carriage_v1(&carriage, bond, accepted_daa_score, chain_network_id, &verify) {
            out.push(BondMutation::Slash(slashed, accepted_daa_score));
        }
    }
    out
}

/// A candidate tip ranked by the ADR-0039 W4′ seam.
///
/// The rule rides every element so `Ord` stays a total order on the type rather than depending on
/// ambient state — a `BinaryHeap` may compare any two elements at any time, and an `Ord` that
/// consulted something outside the values would not be one. Every element in a given heap carries
/// the same rule, because it comes from the same `Params`.
///
/// `palw` is `None` until a resolver assembles the weight facts from chain state. With
/// `BlueWorkOnly` that field is not read at all, so the ordering is exactly `SortableBlock`'s.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RankedTip {
    block: SortableBlock,
    palw: Option<kaspa_consensus_core::palw_chain_weight::PalwChainWeightsV1>,
    rule: kaspa_consensus_core::palw_chain_weight::PalwTipOrderV1,
}

impl Ord for RankedTip {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        kaspa_consensus_core::palw_chain_weight::order_tips_v1(
            self.rule,
            (self.palw.as_ref(), &self.block),
            (other.palw.as_ref(), &other.block),
        )
    }
}

impl PartialOrd for RankedTip {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The weight oracle a node without the model artifact can offer: none.
///
/// Every step conviction then adjudicates `Unadjudicable`, which convicts nobody — the safe
/// direction, and the honest one: a node that cannot recompute the step has not established that
/// the step is wrong. Replacing this with a real oracle is the Track-D step that turns arithmetic
/// conviction on; leaving it here keeps the path structurally present and derives nothing.
struct NoStepWeights;
impl kaspa_consensus_core::palw_step_refute::PalwWeightOracleV1 for NoStepWeights {
    fn weight_row(&self, _tensor: &str, _layer: Option<u16>, _row_start: u32, _elements: u32) -> Option<Vec<u8>> {
        None
    }
}

/// Slashes proved by PALW arithmetic convictions — ADR-0028 §6's Stage-2 prerequisite, and the
/// second offence that can reach a bond.
///
/// Adjudicating one costs a single kernel step recomputed from opened tiles: a bounded CPU
/// primitive, never a model run, which is what lets a full node convict without the LLM the whole
/// design exists to keep off the validation path.
///
/// `Unadjudicable` — this build's catalog cannot decide the step — is not a conviction and
/// derives nothing. It is a fact about the accused CLASS's coverage rather than about the
/// accused, which is why ADR-0039 gates weight on coverage instead of treating gaps as noise.
pub(super) fn palw_step_conviction_slashes_v1<F>(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    accepted_daa_score: u64,
    chain_network_id: &[u8],
    fence_active: bool,
    weights: &dyn kaspa_consensus_core::palw_step_refute::PalwWeightOracleV1,
    verify: F,
) -> Vec<BondMutation>
where
    F: Fn(&[u8], &kaspa_hashes::Hash, &[u8]) -> bool,
{
    use kaspa_consensus_core::palw_carriage::{
        PALW_CARRIAGE_KIND_STEP_CONVICTION, PalwCarriageV1, adjudicate_step_conviction_carriage_v1, decode_palw_stage1_body,
        palw_carriage_tx_kind,
    };
    if !fence_active {
        return Vec::new();
    }
    let mut out = Vec::new();
    for tx in txs {
        if palw_carriage_tx_kind(&tx.subnetwork_id) != Some(PALW_CARRIAGE_KIND_STEP_CONVICTION) {
            continue;
        }
        let Ok(PalwCarriageV1::StepConviction(carriage)) = decode_palw_stage1_body(PALW_CARRIAGE_KIND_STEP_CONVICTION, &tx.payload)
        else {
            continue;
        };
        let Some(bond) = bond_view.get(&carriage.accused_bond_outpoint) else {
            continue;
        };
        if let Ok(slashed) =
            adjudicate_step_conviction_carriage_v1(&carriage, bond, accepted_daa_score, chain_network_id, weights, &verify)
        {
            out.push(BondMutation::Slash(slashed, accepted_daa_score));
        }
    }
    out
}

#[cfg(test)]
mod palw_equivocation_wiring_tests {
    /// The chain identity these slash tests adjudicate under — it must equal the network the
    /// fixtures' job context names, because a foreign-network certificate is refused now.
    const SLASH_NET: &[u8] = b"misaka-devnet";

    use super::*;
    use kaspa_consensus_core::dns_finality::{BondStatus, StakeBondRecord};
    use kaspa_consensus_core::palw_carriage::{
        PALW_CARRIAGE_VERSION_V1, PalwCarriageV1, PalwEquivocationCarriageV1, encode_palw_carriage_v1,
    };
    use kaspa_consensus_core::palw_slash::{
        PALW_S_OBJECT_VERSION_V2, PalwClassContradictionCertificateV1, PalwExecutionAttestationV1,
    };
    use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, trace_scheme_id_v2};
    use kaspa_consensus_core::subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_EQUIVOCATION};
    use kaspa_consensus_core::tx::{Transaction, TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;

    fn h(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }
    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_bytes([seed; 64]), index: 0 }
    }
    fn mock_key(signer: Hash64) -> Vec<u8> {
        signer.as_byte_slice().to_vec()
    }
    fn mock_sign(key: &[u8], digest: &kaspa_hashes::Hash) -> Vec<u8> {
        let mut s = key.to_vec();
        s.extend_from_slice(digest.as_bytes().as_slice());
        s
    }
    fn mock_verify(key: &[u8], digest: &kaspa_hashes::Hash, signature: &[u8]) -> bool {
        signature == mock_sign(key, digest).as_slice()
    }

    fn context() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: SLASH_NET.to_vec(),
            job_id: h(0x11),
            job_nullifier: h(0x12),
            assignment_id: h(0x13),
            execution_seed: [0x22; 32],
            model_profile_id: h(0x31),
            runtime_manifest_hash: h(0x32),
            runtime_class_id: h(0x33),
            shape_profile_id: h(0x34),
            trace_scheme_id: trace_scheme_id_v2(),
            cu_ruleset_id: h(0x36),
            tokenizer_id: h(0x37),
            prompt_token_ids_hash: h(0x38),
            exact_decode_tokens: 16,
            declared_prefill_tokens: 8,
            max_context_tokens: 4_096,
        }
    }

    fn certificate(signer: Hash64, accused: TransactionOutpoint) -> PalwEquivocationCarriageV1 {
        let ctx = context();
        let att = |root: Hash64| {
            let mut a = PalwExecutionAttestationV1 {
                version: PALW_S_OBJECT_VERSION_V2,
                executor_id: signer,
                job_context_hash: ctx.context_hash(),
                full_logits_trace_root: root,
                // A bare-v2 shape: the committed object IS the logits root.
                committed_root: root,
                signature: vec![],
            };
            a.signature = mock_sign(&mock_key(signer), &a.message(&ctx.network_id));
            a
        };
        PalwEquivocationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: accused,
            certificate: PalwClassContradictionCertificateV1 {
                version: PALW_S_OBJECT_VERSION_V2,
                attestation_a: att(h(0x01)),
                attestation_b: att(h(0x02)),
                job_context: ctx,
            },
        }
    }

    /// A Stage-1 carriage tx: the body rides its own subnetwork id, no magic+kind prefix.
    fn carriage_tx(c: &PalwEquivocationCarriageV1) -> Transaction {
        let stage0 = encode_palw_carriage_v1(&PalwCarriageV1::Equivocation(c.clone()));
        let body = stage0[7..].to_vec();
        Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_EQUIVOCATION, 0, body)
    }

    fn bond(signer: Hash64, outpoint: TransactionOutpoint) -> StakeBondRecord {
        StakeBondRecord {
            version: 1,
            bond_outpoint: outpoint,
            owner_pubkey_hash: h(0x0A0A),
            validator_pubkey_hash: signer,
            validator_pubkey: mock_key(signer),
            amount: 20_000_00000000,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbonding_period_blocks: 1_000,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: BondStatus::Active,
        }
    }

    fn view(signer: Hash64, outpoint: TransactionOutpoint) -> ActiveBondView {
        ActiveBondView::from_records([(outpoint, bond(signer, outpoint))])
    }

    /// **Fence OFF is not "approximately nothing" — it is nothing.** Every shipped preset runs
    /// this arm, so a proven certificate against a live bond must still produce no mutation.
    #[test]
    fn the_fence_off_path_derives_nothing_at_all() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let txs = vec![carriage_tx(&certificate(signer, accused))];
        assert!(palw_equivocation_slashes_v1(&txs, &view(signer, accused), 100, SLASH_NET, false, mock_verify).is_empty());
        // ...and the same input DOES produce a slash once the fence is on, so the test above is
        // measuring the fence rather than a broken fixture.
        assert_eq!(
            palw_equivocation_slashes_v1(&txs, &view(signer, accused), 100, SLASH_NET, true, mock_verify),
            vec![BondMutation::Slash(accused, 100)]
        );
    }

    /// Only the equivocation subnetwork is read, and only a decodable body counts. A native tx, a
    /// tx on another PALW band id, and a garbage body all pass through without a mutation and
    /// without an error.
    #[test]
    fn foreign_and_undecodable_transactions_are_skipped_not_failed() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let native = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![0xAB; 32]);
        let garbage = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_EQUIVOCATION, 0, vec![0xFF; 16]);
        let txs = vec![native, garbage];
        assert!(palw_equivocation_slashes_v1(&txs, &view(signer, accused), 100, SLASH_NET, true, mock_verify).is_empty());
    }

    /// A certificate naming a bond this view does not hold is skipped, not an error — it may name
    /// a bond that was pruned, or one this chain never had.
    #[test]
    fn a_certificate_against_an_unknown_bond_is_skipped() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let txs = vec![carriage_tx(&certificate(signer, accused))];
        let elsewhere = ActiveBondView::from_records([(op(0xB2), bond(signer, op(0xB2)))]);
        assert!(palw_equivocation_slashes_v1(&txs, &elsewhere, 100, SLASH_NET, true, mock_verify).is_empty());
    }

    /// The innocent-bond attack, at the wiring layer: a genuine certificate pointed at somebody
    /// else's bond derives no mutation, because the accused bond's own key is not the signer's.
    #[test]
    fn a_genuine_certificate_against_an_innocent_bond_derives_nothing() {
        let (signer, victim_outpoint) = (h(0xE1), op(0xB9));
        let txs = vec![carriage_tx(&certificate(signer, victim_outpoint))];
        let victim_view = view(h(0x00CE), victim_outpoint); // a DIFFERENT validator's bond
        assert!(palw_equivocation_slashes_v1(&txs, &victim_view, 100, SLASH_NET, true, mock_verify).is_empty());
    }

    /// A forged signature derives nothing: the real verifier is the only thing that decides, and
    /// a certificate that does not verify is not evidence.
    #[test]
    fn a_forged_certificate_derives_nothing() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let mut forged = certificate(signer, accused);
        forged.certificate.attestation_b.signature = vec![0xFF; 64];
        let txs = vec![carriage_tx(&forged)];
        assert!(palw_equivocation_slashes_v1(&txs, &view(signer, accused), 100, SLASH_NET, true, mock_verify).is_empty());
    }
}
