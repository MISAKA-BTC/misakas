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
                DbEvmCanonicalHeadsStore, DbEvmHeaderStore, DbEvmPayloadStore, DbEvmStateStore, EvmCanonicalHeadsStoreReader,
                EvmHeaderStore, EvmHeaderStoreReader, EvmStateStore, EvmStateStoreReader,
            },
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            palw::{DbPalwStore, PalwStoreBatchStage, PalwStoreReader},
            palw_beacon::{DbPalwBeaconStore, PalwBeaconAccumViewV1},
            palw_da::{DbPalwDaStore, PalwDaStoreReader},
            palw_lane_bits::DbPalwLaneBitsStore,
            palw_nullifier::{DbPalwNullifierStore, PalwNullifierStoreReader},
            palw_overlay_view::DbPalwOverlayViewStore,
            palw_paid_work::{DbPalwPaidWorkStore, PalwPaidWorkIds, PalwPaidWorkStoreReader},
            palw_provider_bonds::{DbPalwProviderBondsStore, PalwProviderBondsStoreReader},
            palw_pruned_frontier::{DbPalwPrunedFrontierStore, PalwPrunedFrontierStoreReader},
            palw_search_availability::{DbPalwSearchAvailabilityStore, PalwSearchAvailabilityStoreReader},
            palw_spam::{
                DbPalwSpamAccumulatorStore, PalwSpamAccumulatorStoreReader, PalwSpamAccumulatorV1, PalwSpamLaneDelta,
                palw_spam_derive_child, palw_spam_retained_path,
            },
            past_pruning_points::DbPastPruningPointsStore,
            pruning::{DbPruningStore, PruningStoreReader},
            pruning_meta::PruningMetaStores,
            pruning_overlay_snapshot::{DbPruningPointOverlaySnapshotStore, PruningPointOverlaySnapshotStoreReader},
            pruning_samples::DbPruningSamplesStore,
            reachability::DbReachabilityStore,
            relations::{DbRelationsStore, RelationsStoreReader},
            rewarded_epochs::{DbRewardedEpochsStore, RewardedEpochKeys, RewardedEpochsStoreReader},
            selected_chain::{DbSelectedChainStore, SelectedChainStore},
            stake_bonds::{DbStakeBondsStore, StakeBondsStoreReader},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::{DbUtxoDiffsStore, UtxoDiffsStoreReader},
            utxo_multisets::{DbUtxoMultisetsStore, UtxoMultisetsStoreReader},
            utxo_set::UtxoSetStoreReader,
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
        transaction_validator::{
            TransactionValidator,
            errors::{TxResult, TxRuleError},
            tx_validation_in_utxo_context::TxValidationFlags,
        },
        window::WindowManager,
    },
};
use kaspa_consensus_core::{
    BlockHash, BlockHashSet, BlueWorkType, ChainPath,
    acceptance_data::AcceptanceData,
    api::args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    block::{
        BlockTemplate, EvmClaimStaleKind, MutableBlock, TemplateBuildMode, TemplateTransactionSelector,
        TemplateTransactionSelectorFactory,
    },
    blockstatus::{
        BlockStatus,
        BlockStatus::{StatusDisqualifiedFromChain, StatusUTXOValid},
    },
    coinbase::MinerData,
    config::genesis::GenesisBlock,
    dns_finality::{
        ATTESTATION_MLDSA87_CONTEXT, ActiveBondView, AttestationContribution, BlockEpochContribution, BlockOverlayContribution,
        BondMutation, BondStatus, CanonicalLaggedEpochAnchor, DnsParams, DnsReorgMode, DnsRolloutStage,
        MandatoryAttestationContributionKey, MandatoryAttestationDeficit, MandatoryAttestationValidator, OverlaySnapshot,
        PruningPointOverlaySnapshot, StakeBondRecord, StakeScore, advance_dns_confirmation, aggregate_epoch_tallies,
        anchor_cutoff_blue_score, apply_dormancy_round, attestations_from_accepted_txs, bond_mutations_from_accepted_txs,
        canonical_lagged_epoch_anchor, check_dns_reorg_rule, compute_stake_score, derive_dns_health, dns_finality_fresh_for_bridge,
        dormancy_revival_ready, effective_bond_status, epoch_meets_quality_floor, is_bond_active_at, is_dns_confirmed,
        mandatory_attestation_mass_capacity, ready_epoch_from_tip_blue_score, recompute_epoch_tallies,
        reorg_inputs_since_common_ancestor, required_stake_for_quality_floor, stake_attestation_message, total_active_stake_by_epoch,
    },
    header::Header,
    merkle::calc_hash_merkle_root,
    mining_rules::MiningRules,
    palw::{
        PalwActiveNullifierSet, PalwBatchViewV1, PalwBeaconStateV1, PalwLaneBitsV1, PalwProviderBondMutation, PalwProviderBondRecord,
        PalwPrunedFrontierV1, ProviderBondView,
        da::{
            PalwBuriedBeaconV1, PalwDaChallengeStatusV1, PalwDaPolicyV1, PalwDaPruningSnapshotV1, PalwDaStateV1,
            PalwReceiptDaCommitmentV1,
        },
        palw_provider_bond_mutations_from_accepted_txs, provider_bond_lock_spk, provider_bond_release_daa_score,
        search_snapshot::{PALW_SEARCH_SNAPSHOT_STATE_VERSION_V1, PalwSearchAvailabilityStateV1, PalwSearchPruningSnapshotV1},
    },
    palw_pruned_frontier::{
        MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS, PALW_PRUNING_SNAPSHOT_VERSION, PalwChainDerivedAuthBundleV1, PalwLocalPaidWorkFactV1,
        PalwPrunedActiveBatchV1, PalwPrunedBeaconAccumulatorV1, PalwPrunedPaidWorkBlockV1, PalwPrunedSpamAccumulatorV1,
        PalwPrunedSpamFrontierV1, PalwPrunedSpamSupportRowV1, PalwPruningPointSnapshotPayloadV1, PalwPruningPointSnapshotV1,
        PalwPruningSnapshotCheckpoint, PalwPruningSnapshotImportAuth, PalwPruningSnapshotImportProvenance, PalwSelectedParentStateV2,
        palw_active_batch_ref_root, palw_pruned_ibd_snapshot_import_allowed,
    },
    pruning::PruningPointsList,
    tx::{MutableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
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
use kaspa_core::{debug, error, info, time::unix_now, trace, warn};
use kaspa_database::prelude::{StoreError, StoreResultExt, StoreResultUnitExt};
use kaspa_hashes::{Hash64, ZERO_HASH64};
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
    sync::{Arc, atomic::Ordering},
};

/// A missing body is a normal pruning artifact only for the current pruning point. Acceptance data
/// for every other block was derived from that block's body, so losing it before virtual commit is a
/// local consistency failure rather than an alternative block-validity result.
fn palw_body_may_be_pruned(block: BlockHash, pruning_point: BlockHash) -> bool {
    block == pruning_point
}

/// Validate one imported provider-registry row against the pruning-point UTXO set. Slashing never
/// consumes the locked output: it makes that output permanently unreleasable, so a slashed row must
/// still resolve even if an earlier unbond delay would otherwise have elapsed.
fn validate_imported_palw_provider_utxo(
    record: &PalwProviderBondRecord,
    entry: Option<&UtxoEntry>,
    pruning_point_daa_score: u64,
    epoch_length_daa: u64,
) -> PruningImportResult<()> {
    let release = provider_bond_release_daa_score(record, epoch_length_daa);
    let must_still_be_locked = record.slashed_at_daa_score.is_some() || release.is_none_or(|daa| pruning_point_daa_score < daa);
    let Some(entry) = entry else {
        if must_still_be_locked {
            return Err(PruningImportError::ImportedPalwProviderBondMissingUtxo(record.bond_outpoint));
        }
        return Ok(());
    };
    if entry.amount != record.amount_sompi || entry.script_public_key != provider_bond_lock_spk(&record.owner_public_key) {
        return Err(PruningImportError::ImportedPalwProviderBondUtxoMismatch(record.bond_outpoint));
    }
    Ok(())
}

struct PrunedSpamClosureStore {
    rows: HashMap<BlockHash, Arc<PalwSpamAccumulatorV1>>,
}

impl PalwSpamAccumulatorStoreReader for PrunedSpamClosureStore {
    fn get_optional(&self, hash: BlockHash) -> Result<Option<Arc<PalwSpamAccumulatorV1>>, StoreError> {
        Ok(self.rows.get(&hash).cloned())
    }
}

/// Validate the exact below-PP rows needed by the anti-spam window and by every deterministic child
/// skip target until the DAA horizon has moved above the boundary. This is deliberately stricter than
/// structural Borsh validation: a digest-valid but incomplete pointer graph cannot be imported.
fn validate_pruned_spam_closure(
    pruning_point: BlockHash,
    pruning_point_daa: u64,
    window_daa: u64,
    frontier: &PalwPrunedSpamFrontierV1,
) -> Result<(), String> {
    let span = kaspa_consensus_core::palw_antispam::palw_spam_checkpoint_span(window_daa);
    if span == 0 || span > MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS as u64 {
        return Err("spam window exceeds the pruning transport support bound".to_string());
    }
    if frontier.pruning_point_state.daa_score != pruning_point_daa {
        return Err("spam pruning-point row has the wrong DAA score".to_string());
    }
    let to_store = |state: &PalwPrunedSpamAccumulatorV1| {
        Arc::new(PalwSpamAccumulatorV1 {
            version: state.version,
            daa_score: state.daa_score,
            selected_height: state.selected_height,
            total_hash_blues: state.total_hash_blues,
            total_replica_blues: state.total_replica_blues,
            selected_parent: state.selected_parent,
            skip: state.skip,
        })
    };
    let mut rows = HashMap::with_capacity(frontier.support_rows.len() + 1);
    rows.insert(pruning_point, to_store(&frontier.pruning_point_state));
    for row in &frontier.support_rows {
        if rows.insert(row.block_hash, to_store(&row.state)).is_some() {
            return Err("spam closure repeats a block hash".to_string());
        }
    }
    let store = PrunedSpamClosureStore { rows };
    let required_path =
        palw_spam_retained_path(&store, pruning_point, window_daa).map_err(|err| format!("invalid bounded spam closure: {err}"))?;
    let required: HashSet<BlockHash> = required_path.iter().map(|(hash, _)| *hash).collect();
    if frontier.support_rows.len() + 1 != required.len() || store.rows.keys().any(|hash| !required.contains(hash)) {
        return Err("spam closure contains missing or non-canonical support rows".to_string());
    }
    Ok(())
}

fn palw_da_boundary_matches(embedded: &PalwDaPruningSnapshotV1, standalone: Option<&PalwDaPruningSnapshotV1>) -> bool {
    standalone == Some(embedded)
}

fn palw_search_boundary_matches(embedded: &PalwSearchPruningSnapshotV1, standalone: Option<&PalwSearchPruningSnapshotV1>) -> bool {
    standalone == Some(embedded)
}

/// Classify a completed Header-v4 direct parent. Non-selected side parents may legitimately be
/// disqualified by UTXO/DNS rules; they contribute no accepted lifecycle delta and must not make an
/// otherwise valid DAG child body-invalid. The GHOSTDAG selected parent is different: body ticket and
/// accepted-view construction both read its exact lifecycle row.
fn palw_parent_terminal_status(status: BlockStatus, selected: bool, has_accepted_view: bool) -> Result<bool, &'static str> {
    match status {
        BlockStatus::StatusUTXOValid if !selected || has_accepted_view => Ok(true),
        BlockStatus::StatusUTXOValid => Err("selected parent has no accepted lifecycle provenance"),
        BlockStatus::StatusDisqualifiedFromChain if !selected => Ok(true),
        BlockStatus::StatusDisqualifiedFromChain => Err("selected parent is disqualified"),
        BlockStatus::StatusUTXOPendingVerification => Ok(false),
        BlockStatus::StatusHeaderOnly => Err("parent body is missing"),
        BlockStatus::StatusInvalid => Err("parent is invalid"),
    }
}

#[inline]
fn palw_v4_all_pass_summary(passed_leaf_count: u32, rejected_leaf_bitmap_root: Hash64, manifest_leaf_count: u32) -> bool {
    passed_leaf_count == manifest_leaf_count && rejected_leaf_bitmap_root == ZERO_HASH64
}

fn palw_v4_parent_allows_leaf(view: &PalwBatchViewV1, batch_id: &Hash64) -> bool {
    view.entry(batch_id).is_some_and(|entry| entry.status == kaspa_consensus_core::palw::PalwBatchStatus::Registering)
}

fn palw_v4_parent_allows_certificate(view: &PalwBatchViewV1, batch_id: &Hash64) -> bool {
    view.entry(batch_id).is_some_and(|entry| {
        matches!(
            entry.status,
            kaspa_consensus_core::palw::PalwBatchStatus::Committed
                | kaspa_consensus_core::palw::PalwBatchStatus::Auditing
                | kaspa_consensus_core::palw::PalwBatchStatus::Certified
                | kaspa_consensus_core::palw::PalwBatchStatus::Active
        )
    })
}

fn fail_palw_parent_waiters_on_shutdown(messages: &[VirtualStateProcessingMessage]) {
    for message in messages {
        if let VirtualStateProcessingMessage::EnsurePalwParents { result, .. } = message {
            let _ = result.send(Err("virtual worker is shutting down".to_string()));
        }
    }
}

/// PALW accepted blobs and lifecycle provenance are staged into the UTXO `WriteBatch`. Cache-backed
/// writers update caches while staging, so either a staging error or the final RocksDB write error must
/// terminate the process; unwinding only the virtual thread could expose cache state which never became
/// durable and reintroduce arrival-order dependence after restart.
#[cold]
#[inline(never)]
fn palw_overlay_commit_fail_stop(message: String) -> ! {
    error!("{message}");
    std::process::abort()
}

/// Selected-parent DA state plus this chain block's deterministic accepted-transaction delta. The
/// parent snapshot remains separate because every effect in a block is past-relative: a certificate
/// cannot consume a response, nor a response consume a challenge, carried by the same acceptance set.
#[derive(Clone, Debug, Default)]
struct PalwDaStaged {
    /// The stored selected-parent state (shared, never cloned) — both the past-relative
    /// certificate view and the equality baseline for the unchanged-link commit fast path.
    certificate_state: Arc<PalwDaStateV1>,
    /// Block owning the parent's full durable row (`None` when the parent state is the
    /// synthetic pre-activation/genesis default that has no durable row yet).
    parent_anchor: Option<BlockHash>,
    state: PalwDaStateV1,
    slash_mutations: Vec<PalwProviderBondMutation>,
}

/// Selected-parent search-availability state plus this chain block's deterministic accepted 0x3d-0x3f
/// delta — `PalwDaStaged`'s sibling. Response/timeout effects are past-relative (they may target only
/// challenges already open in the parent state); a registering challenge is self-contained (its
/// JobSpec is validated against the parent-relative bonded scheduler registry inside the same tx),
/// so no intra-mergeset ordering can change the outcome.
#[derive(Clone, Debug, Default)]
struct PalwSearchStaged {
    /// The stored selected-parent state (shared, never cloned) — the equality baseline for the
    /// unchanged-link commit fast path.
    parent_state: Arc<PalwSearchAvailabilityStateV1>,
    /// Block owning the parent's full durable row (`None` when the parent state is the synthetic
    /// pre-activation/genesis default that has no durable row yet).
    parent_anchor: Option<BlockHash>,
    state: PalwSearchAvailabilityStateV1,
    slash_mutations: Vec<PalwProviderBondMutation>,
}

/// Read-only preflight result for a pruning-boundary import. Constructing this value performs every
/// snapshot/context/collision check and does not touch a cache-backed writer. Once staging begins,
/// any store or RocksDB failure is process-fatal because the database helpers update caches before the
/// caller commits its `WriteBatch`.
pub(crate) struct PreparedPalwPruningPointSnapshotImport {
    pruning_point: BlockHash,
    snapshot: PalwPruningPointSnapshotV1,
    existing_provider_outpoints: Vec<TransactionOutpoint>,
    beacon_state_write: Option<Arc<PalwBeaconStateV1>>,
    beacon_accumulator_write: Option<Arc<PalwBeaconAccumViewV1>>,
    overlay_view_write: Option<Arc<PalwBatchViewV1>>,
    lane_bits_write: Option<PalwLaneBitsV1>,
    nullifier_write: Option<Arc<PalwActiveNullifierSet>>,
    spam_writes: Vec<(BlockHash, Arc<PalwSpamAccumulatorV1>)>,
    da_state_write: Option<Arc<PalwDaStateV1>>,
    search_availability_state_write: Option<Arc<PalwSearchAvailabilityStateV1>>,
    compute_registry_view_write: Option<Arc<kaspa_consensus_core::palw_compute_set::PalwComputeRegistryViewV1>>,
}

/// The header version this network is configured to import a PALW boundary for. Shared by the
/// operator-pinned and chain-derived contexts so both refuse a caller-supplied version downgrade the
/// same way.
fn palw_configured_import_header_version(palw_spam_is_inert: bool) -> u16 {
    if palw_spam_is_inert {
        kaspa_consensus_core::constants::PALW_HEADER_VERSION
    } else {
        kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION
    }
}

/// Operator-authority import context: the Header-v3 closed-network path and the Header-v4
/// operator-pinned-checkpoint path.
///
/// `permissionless_enabled` is the node-local `Config::palw_permissionless_snapshot_auth` lever,
/// threaded only so the single admission expression
/// [`palw_pruned_ibd_snapshot_import_allowed`] keeps the lever and the version fence in one place.
/// With the lever off this function is byte-identical to its pre-ADR-0042 form: the chain-derived
/// arm of the admission matrix is `false` and the explicit refusal below is unreachable.
fn validate_palw_snapshot_import_auth_context(
    pruning_point_daa_score: u64,
    header_version: u16,
    pruning_point: BlockHash,
    import_auth: &PalwPruningSnapshotImportAuth,
    operator_checkpoints: &[PalwPruningSnapshotCheckpoint],
    palw_activation_daa_score: u64,
    palw_spam_is_inert: bool,
    permissionless_enabled: bool,
) -> Result<(), String> {
    if pruning_point_daa_score < palw_activation_daa_score {
        return Err(format!(
            "PALW pruning snapshot import is not allowed before PALW activation DAA score {palw_activation_daa_score}"
        ));
    }
    let configured_header_version = palw_configured_import_header_version(palw_spam_is_inert);
    if header_version != configured_header_version {
        return Err(format!(
            "PALW pruning snapshot header version {header_version} does not match this network's configured version {configured_header_version} at DAA score {pruning_point_daa_score}"
        ));
    }
    if !palw_pruned_ibd_snapshot_import_allowed(header_version, import_auth, permissionless_enabled) {
        return Err(format!(
            "PALW pruned-IBD import authentication provenance {:?} is not allowed for exact header version {}",
            import_auth.provenance, header_version
        ));
    }
    // ADR-0042. Reaching the operator-authority context with chain-derived provenance means the
    // importer did NOT take the chain-derived branch: either the lever is off (in which case the
    // admission check above already refused, so this is unreachable) or no authentication bundle was
    // supplied. A chain-derived auth carries the all-zero sentinel digest instead of an operator
    // claim, so there is nothing here that could authenticate it — refuse explicitly rather than
    // falling through to an operator-checkpoint membership error that would read as a mis-pinned
    // checkpoint. This never widens anything: it can only turn an accept into a reject.
    if import_auth.provenance == PalwPruningSnapshotImportProvenance::ChainDerivedHeaderBundle {
        return Err(
            "chain-derived PALW pruning-snapshot provenance reached the operator-authority import context without an authentication bundle"
                .to_string(),
        );
    }
    if import_auth.checkpoint.pruning_point != pruning_point {
        return Err(format!(
            "snapshot import authentication is pinned to another pruning point {}",
            import_auth.checkpoint.pruning_point
        ));
    }
    if header_version == kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION
        && !operator_checkpoints.contains(&import_auth.checkpoint)
    {
        return Err(format!(
            "Header-v4 snapshot import authentication is not present in this node's configured operator checkpoints: {}",
            import_auth.checkpoint
        ));
    }
    Ok(())
}

/// ADR-0042 chain-derived import context — the sibling of
/// [`validate_palw_snapshot_import_auth_context`] for the one provenance that carries no operator
/// authority. It is reachable only when the node-local lever is set AND the caller supplied an
/// authentication bundle, so with the lever off nothing below can execute.
///
/// It reproduces every context check the operator path applies *except* checkpoint membership (there
/// is no checkpoint to be a member of), and adds the two the sentinel shape demands. Deliberately
/// absent: any relaxation. The payload-digest equality branch is not skipped here for convenience —
/// it is replaced, strictly before the durable write, by
/// `verify_chain_derived_pruning_boundary_from_payload`, which is a stronger statement about the same
/// bytes (it reconstructs the boundary state and folds it into a chain-authenticated commitment).
fn validate_palw_chain_derived_import_auth_context(
    pruning_point_daa_score: u64,
    header_version: u16,
    pruning_point: BlockHash,
    import_auth: &PalwPruningSnapshotImportAuth,
    palw_activation_daa_score: u64,
    palw_spam_is_inert: bool,
    permissionless_enabled: bool,
) -> Result<(), String> {
    // This context authenticates exactly one provenance. `palw_pruned_ibd_snapshot_import_allowed`
    // below would still admit `(v4, OperatorPinnedCheckpoint)` — that pair is unconditionally true —
    // so without this guard an operator-pinned auth reaching here would skip its own checkpoint
    // membership and digest equality. The importer's branch already guarantees the provenance; this
    // makes the function safe to call and to test in isolation.
    if import_auth.provenance != PalwPruningSnapshotImportProvenance::ChainDerivedHeaderBundle {
        return Err(format!(
            "chain-derived import context reached with {:?} provenance, which must be authenticated by its own path",
            import_auth.provenance
        ));
    }
    // Same pre-activation guard the operator path enforces.
    if pruning_point_daa_score < palw_activation_daa_score {
        return Err(format!(
            "PALW pruning snapshot import is not allowed before PALW activation DAA score {palw_activation_daa_score}"
        ));
    }
    // Same network-version guard: an inert (Header-v3) network has no anti-spam support rows, so it
    // has nothing a chain-derived boundary could be bound to.
    let configured_header_version = palw_configured_import_header_version(palw_spam_is_inert);
    if header_version != configured_header_version {
        return Err(format!(
            "PALW pruning snapshot header version {header_version} does not match this network's configured version {configured_header_version} at DAA score {pruning_point_daa_score}"
        ));
    }
    // The single admission expression, lever-gated and Header-v4 only.
    if !palw_pruned_ibd_snapshot_import_allowed(header_version, import_auth, permissionless_enabled) {
        return Err(format!(
            "PALW pruned-IBD import authentication provenance {:?} is not allowed for exact header version {}",
            import_auth.provenance, header_version
        ));
    }
    // Same pinning guard the operator path enforces: the auth token must name this pruning point.
    if import_auth.checkpoint.pruning_point != pruning_point {
        return Err(format!(
            "snapshot import authentication is pinned to another pruning point {}",
            import_auth.checkpoint.pruning_point
        ));
    }
    // The sentinel must be the sentinel. `PalwPruningSnapshotImportAuth` is checkpoint-shaped, and a
    // chain-derived auth that smuggled a non-zero digest would be indistinguishable from an operator
    // claim at any later comparison site.
    if import_auth.checkpoint.payload_digest != Hash64::default() {
        return Err(format!(
            "chain-derived snapshot import authentication must carry the all-zero sentinel digest, found {}",
            import_auth.checkpoint.payload_digest
        ));
    }
    Ok(())
}

fn palw_da_certificate_effect_allowed(state: &PalwDaStateV1, effect: &crate::processes::palw::PalwOverlayEffect) -> bool {
    match effect {
        crate::processes::palw::PalwOverlayEffect::Certificate(certificate) => {
            palw_da_certificate_batch_allowed(state, &certificate.batch_id)
        }
        _ => true,
    }
}

#[inline]
fn palw_da_certificate_batch_allowed(state: &PalwDaStateV1, batch_id: &Hash64) -> bool {
    state.certificate_allowed(batch_id)
}

/// Apply a selected-chain reorg to an in-memory prefix-241 working view and return every outpoint
/// whose final persisted row must be rewritten/deleted. `ChainPath::removed` is already ordered from
/// the old tip back toward the common ancestor; reversing that vector would attempt to delete an
/// Insert before reverting its later Unbond/Slash. Each individual block is still reverted in reverse
/// transaction order. `added` is ancestor-to-tip and is applied in forward transaction order.
///
/// Keeping this working view is essential: RocksDB `WriteBatch` updates are not visible through
/// `store.get`, so reading the store between staged mutations loses multi-block dependencies.
fn reconcile_palw_provider_registry(
    working: &mut HashMap<TransactionOutpoint, PalwProviderBondRecord>,
    removed: &[(BlockHash, Vec<PalwProviderBondMutation>)],
    added: &[(BlockHash, Vec<PalwProviderBondMutation>)],
) -> Result<HashSet<TransactionOutpoint>, String> {
    let mut touched = HashSet::new();

    for (block, mutations) in removed {
        for mutation in mutations.iter().rev() {
            match mutation {
                PalwProviderBondMutation::Insert(outpoint, inserted) => {
                    let actual = working.remove(outpoint).ok_or_else(|| {
                        format!("cannot revert provider insert {outpoint} from detached block {block}: registry row is missing")
                    })?;
                    if actual != *inserted {
                        return Err(format!(
                            "cannot revert provider insert {outpoint} from detached block {block}: live row differs from the inserted record after later mutations were reverted"
                        ));
                    }
                    touched.insert(*outpoint);
                }
                PalwProviderBondMutation::Unbond(outpoint, daa) => {
                    let record = working.get_mut(outpoint).ok_or_else(|| {
                        format!("cannot revert provider unbond {outpoint} from detached block {block}: registry row is missing")
                    })?;
                    if record.unbond_request_daa_score != Some(*daa) {
                        return Err(format!(
                            "cannot revert provider unbond {outpoint} from detached block {block}: expected stamp {daa}, found {:?}",
                            record.unbond_request_daa_score
                        ));
                    }
                    record.unbond_request_daa_score = None;
                    touched.insert(*outpoint);
                }
                PalwProviderBondMutation::Slash(outpoint, daa) => {
                    let record = working.get_mut(outpoint).ok_or_else(|| {
                        format!("cannot revert provider slash {outpoint} from detached block {block}: registry row is missing")
                    })?;
                    if record.slashed_at_daa_score != Some(*daa) {
                        return Err(format!(
                            "cannot revert provider slash {outpoint} from detached block {block}: expected stamp {daa}, found {:?}",
                            record.slashed_at_daa_score
                        ));
                    }
                    record.slashed_at_daa_score = None;
                    touched.insert(*outpoint);
                }
            }
        }
    }

    for (block, mutations) in added {
        for mutation in mutations {
            match mutation {
                PalwProviderBondMutation::Insert(outpoint, record) => {
                    if working.insert(*outpoint, record.clone()).is_some() {
                        return Err(format!(
                            "cannot apply provider insert {outpoint} from attached block {block}: registry row already exists"
                        ));
                    }
                    touched.insert(*outpoint);
                }
                PalwProviderBondMutation::Unbond(outpoint, daa) => {
                    let record = working.get_mut(outpoint).ok_or_else(|| {
                        format!("cannot apply provider unbond {outpoint} from attached block {block}: registry row is missing")
                    })?;
                    if record.unbond_request_daa_score.is_some() {
                        return Err(format!(
                            "cannot apply provider unbond {outpoint} from attached block {block}: row is already unbonding at {:?}",
                            record.unbond_request_daa_score
                        ));
                    }
                    record.unbond_request_daa_score = Some(*daa);
                    touched.insert(*outpoint);
                }
                PalwProviderBondMutation::Slash(outpoint, daa) => {
                    let record = working.get_mut(outpoint).ok_or_else(|| {
                        format!("cannot apply provider slash {outpoint} from attached block {block}: registry row is missing")
                    })?;
                    if record.slashed_at_daa_score.is_some() {
                        return Err(format!(
                            "cannot apply provider slash {outpoint} from attached block {block}: row is already slashed at {:?}",
                            record.slashed_at_daa_score
                        ));
                    }
                    record.slashed_at_daa_score = Some(*daa);
                    touched.insert(*outpoint);
                }
            }
        }
    }

    Ok(touched)
}

/// DNS counterpart of [`reconcile_palw_provider_registry`]. It performs the full selected-chain
/// transition against one in-memory view because RocksDB reads cannot observe writes already queued
/// in the same [`WriteBatch`]. `removed` is consumed in its native tip-to-ancestor order and each
/// block's mutations are inverted in reverse transaction order; `added` is applied ancestor-to-tip.
///
/// The cached `StakeBondRecord::status` is refreshed from the authoritative DAA stamps after each
/// reversible state change. In particular, reverting a slash restores an underlying Unbonding,
/// Dormant, Active, or Pending state instead of guessing `Active`.
fn reconcile_dns_bond_registry(
    working: &mut HashMap<TransactionOutpoint, StakeBondRecord>,
    removed: &[(BlockHash, Vec<BondMutation>)],
    added: &[(BlockHash, Vec<BondMutation>)],
) -> Result<HashSet<TransactionOutpoint>, String> {
    let mut touched = HashSet::new();

    for (block, mutations) in removed {
        for mutation in mutations.iter().rev() {
            match mutation {
                BondMutation::Insert(outpoint, _) => {
                    // Ancillary buried/dormancy fields may have changed after insertion, so row
                    // identity is intentionally not compared with the original Insert record.
                    working.remove(outpoint).ok_or_else(|| {
                        format!("cannot revert DNS bond insert {outpoint} from detached block {block}: registry row is missing")
                    })?;
                    touched.insert(*outpoint);
                }
                BondMutation::Slash(outpoint, daa) => {
                    let record = working.get_mut(outpoint).ok_or_else(|| {
                        format!("cannot revert DNS bond slash {outpoint} from detached block {block}: registry row is missing")
                    })?;
                    if record.slashed_at_daa_score != Some(*daa) {
                        return Err(format!(
                            "cannot revert DNS bond slash {outpoint} from detached block {block}: expected stamp {daa}, found {:?}",
                            record.slashed_at_daa_score
                        ));
                    }
                    record.slashed_at_daa_score = None;
                    record.status = effective_bond_status(record, *daa);
                    touched.insert(*outpoint);
                }
                BondMutation::Unbond(outpoint, daa) => {
                    let record = working.get_mut(outpoint).ok_or_else(|| {
                        format!("cannot revert DNS bond unbond {outpoint} from detached block {block}: registry row is missing")
                    })?;
                    if record.unbond_request_daa_score != Some(*daa) {
                        return Err(format!(
                            "cannot revert DNS bond unbond {outpoint} from detached block {block}: expected stamp {daa}, found {:?}",
                            record.unbond_request_daa_score
                        ));
                    }
                    record.unbond_request_daa_score = None;
                    record.status = effective_bond_status(record, *daa);
                    touched.insert(*outpoint);
                }
            }
        }
    }

    for (block, mutations) in added {
        for mutation in mutations {
            match mutation {
                BondMutation::Insert(outpoint, record) => {
                    if working.insert(*outpoint, record.clone()).is_some() {
                        return Err(format!(
                            "cannot apply DNS bond insert {outpoint} from attached block {block}: registry row already exists"
                        ));
                    }
                    touched.insert(*outpoint);
                }
                BondMutation::Slash(outpoint, daa) => {
                    let record = working.get_mut(outpoint).ok_or_else(|| {
                        format!("cannot apply DNS bond slash {outpoint} from attached block {block}: registry row is missing")
                    })?;
                    if record.slashed_at_daa_score.is_some() {
                        return Err(format!(
                            "cannot apply DNS bond slash {outpoint} from attached block {block}: row is already slashed at {:?}",
                            record.slashed_at_daa_score
                        ));
                    }
                    record.slashed_at_daa_score = Some(*daa);
                    record.status = effective_bond_status(record, *daa);
                    touched.insert(*outpoint);
                }
                BondMutation::Unbond(outpoint, daa) => {
                    let record = working.get_mut(outpoint).ok_or_else(|| {
                        format!("cannot apply DNS bond unbond {outpoint} from attached block {block}: registry row is missing")
                    })?;
                    if record.unbond_request_daa_score.is_some() {
                        return Err(format!(
                            "cannot apply DNS bond unbond {outpoint} from attached block {block}: row is already unbonding at {:?}",
                            record.unbond_request_daa_score
                        ));
                    }
                    record.unbond_request_daa_score = Some(*daa);
                    record.status = effective_bond_status(record, *daa);
                    touched.insert(*outpoint);
                }
            }
        }
    }

    Ok(touched)
}

/// Normalize the legacy cached status of every DNS bond at one canonical point of view. Activation
/// advances with DAA without writing a row, so mutation-local normalization alone is insufficient:
/// a node that once applied/reverted a later Slash could otherwise retain `Active` while a node that
/// never visited that branch still retained the insertion-time `Pending` byte. The DAA stamps are
/// authoritative; this makes the persisted cache a deterministic projection of them at the new sink.
fn canonicalize_dns_bond_statuses(
    working: &mut HashMap<TransactionOutpoint, StakeBondRecord>,
    sink_daa_score: u64,
) -> HashSet<TransactionOutpoint> {
    let mut touched = HashSet::new();
    for (outpoint, record) in working {
        let effective = effective_bond_status(record, sink_daa_score);
        if record.status != effective {
            record.status = effective;
            touched.insert(*outpoint);
        }
    }
    touched
}

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

/// The gate can reject hundreds of candidate ancestors during one sink search. Log the
/// first rejection and at most one aggregate warning every 30 seconds so a wedge is
/// visible without turning the diagnostic into another source of resolve pressure.
fn dns_reorg_rejection_warning_due() -> Option<u64> {
    use std::sync::atomic::{AtomicU64, Ordering};

    static LAST_WARNING_UNIX_SECS: AtomicU64 = AtomicU64::new(0);
    static REJECTIONS_SINCE_WARNING: AtomicU64 = AtomicU64::new(0);
    const WARNING_INTERVAL_SECS: u64 = 30;

    REJECTIONS_SINCE_WARNING.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let mut previous = LAST_WARNING_UNIX_SECS.load(Ordering::Relaxed);
    loop {
        if previous != 0 && now.saturating_sub(previous) < WARNING_INTERVAL_SECS {
            return None;
        }
        match LAST_WARNING_UNIX_SECS.compare_exchange_weak(previous, now, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Some(REJECTIONS_SINCE_WARNING.swap(0, Ordering::Relaxed).saturating_sub(1)),
            Err(observed) => previous = observed,
        }
    }
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
    /// kaspa-pq **ADR-0040 ECON-03 (THE WIRE)** — the PALW provider-bond registry (prefix 241).
    /// Written by [`Self::stage_palw_provider_bond_mutations`] at virtual commit and read only to
    /// seed [`Self::initial_palw_provider_bond_view`]; every consensus decision reads the walked
    /// VIEW, never this store, for the same point-of-view reason the DNS bond set does.
    pub(super) palw_provider_bonds_store: Arc<RwLock<DbPalwProviderBondsStore>>,
    pub(super) dns_state_store: Arc<RwLock<DbDnsStateStore>>,
    // kaspa-pq ADR-0022: overlay snapshot as-of the pruning point (serve + below-pp window consult).
    pub(super) pruning_overlay_snapshot_store: Arc<RwLock<DbPruningPointOverlaySnapshotStore>>,
    pub(super) dns_params: Option<DnsParams>,

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
    // Read only by the `evm`-gated anchor and seed paths; a non-EVM build carries
    // the fields so the struct shape does not fork on a feature.
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_state_checkpoint_store: Arc<crate::model::stores::evm::DbEvmStateCheckpointStore>,
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_state_checkpoint_v2_store: Arc<crate::model::stores::evm::DbEvmStateCheckpointV2Store>,
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_checkpoint_meta_store: Arc<parking_lot::RwLock<crate::model::stores::evm::DbEvmCheckpointMetaStore>>,
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
    /// §12.3 v2: the state-anchor cadence and retention bound.
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_checkpoint_policy: kaspa_consensus_core::evm::EvmCheckpointPolicy,
    /// Per-segment retention. Read on the write path too: a segment the node keeps
    /// nothing of is not written in the first place.
    pub(super) evm_retention_policy: kaspa_consensus_core::evm::EvmRetentionPolicy,
    pub(super) evm_activation_daa_score: u64,
    // ADR-0039 PALW: the audited-compute lane's activation fence + overlay-state store. The four
    // ordinary presets keep it at `u64::MAX`, while the explicit testnet-110/devnet-111 PALW presets
    // activate at DAA 0. Read only at virtual commit to advance the §9.5 batch state machine from
    // accepted PALW overlay txs.
    pub(super) palw_activation_daa_score: u64,
    /// ADR-MA: the Compute Set registry activation fence (`u64::MAX` on every shipped preset —
    /// the registry fold below returns before touching acceptance data).
    pub(super) palw_compute_registry_activation_daa_score: u64,
    pub(super) palw_store: Arc<DbPalwStore>,
    pub(super) palw_beacon_store: Arc<DbPalwBeaconStore>,
    pub(super) palw_lane_bits_store: Arc<DbPalwLaneBitsStore>,
    pub(super) palw_nullifier_store: Arc<DbPalwNullifierStore>,
    pub(super) palw_overlay_view_store: Arc<DbPalwOverlayViewStore>,
    pub(super) palw_da_store: Arc<RwLock<DbPalwDaStore>>,
    /// Search-availability fork-local state rows (0x3d-0x3f), the DA store's sibling.
    pub(super) palw_search_availability_store: Arc<RwLock<DbPalwSearchAvailabilityStore>>,
    pub(super) palw_pruned_frontier_store: Arc<RwLock<DbPalwPrunedFrontierStore>>,
    pub(super) palw_epoch_length_daa: u64,
    /// kaspa-pq ADR-0040 §5.15.13 (G16): the batch-admission windows that DERIVE the paid-work walk
    /// bound. Held here (not re-read from params) so `palw_paid_work_window` and the body-coordinate
    /// admission check that enforces the windows read the identical values.
    pub(super) palw_batch_admission: kaspa_consensus_core::palw::PalwBatchAdmissionParams,
    /// kaspa-pq ADR-0039 §16.3: the per-lane retarget params. Held here so the algo-4 TEMPLATE derives
    /// `bits` through the same [`crate::processes::difficulty::lane_bits_from_window`] the header stage
    /// runs — `bits` is a consensus-derived field, and a template that computes it differently produces
    /// blocks its own node rejects with `UnexpectedDifficulty`.
    pub(super) palw_lane_difficulty: kaspa_consensus_core::palw::LaneDifficultyParams,
    /// Header-v4 re-genesis-only objective stamp/accumulator parameters and immutable state store.
    pub(super) palw_spam: kaspa_consensus_core::palw_antispam::PalwSpamParams,
    pub(super) palw_spam_store: Arc<DbPalwSpamAccumulatorStore>,
    /// Node-local Header-v4 snapshot import capabilities. Consensus independently checks typed
    /// operator provenance against this configured set before any cache-backed staging.
    pub(super) palw_pruning_snapshot_checkpoints: Arc<[PalwPruningSnapshotCheckpoint]>,
    /// Node-local lever admitting chain-derived (permissionless) Header-v4 snapshot import. Default
    /// false; fenced. See `docs/adr-permissionless-snapshot-authentication.md`.
    pub(super) palw_permissionless_snapshot_auth: bool,
    pub(super) palw_beacon_grace_epochs: u64,
    pub(super) palw_beacon_quorum_num: u16,
    pub(super) palw_beacon_quorum_den: u16,
    /// ADR-0040 P1-3 (CERT-01): the batch-certificate auditor quorum fraction (§10.2).
    pub(super) palw_audit_quorum_num: u16,
    pub(super) palw_audit_quorum_den: u16,
    /// kaspa-pq ADR-0040 §5.17.4 (AUTHSET-01): the beacon-selected auditor committee size, and §5.17.6
    /// (SAMPLE-01): the leaf sample size — the two config cardinalities the certificate re-derivations at
    /// `verify_certificate_attestation` consume. Held here (like the quorum fraction) because that
    /// re-derivation runs in this processor, which only sees `Params`.
    pub(super) palw_audit_committee_size: u16,
    pub(super) palw_audit_sample_size: u16,
    /// PALW's frozen `u32` network discriminator (the dedicated testnet suffix, e.g. 110).
    /// Only read after the PALW activation fence; non-suffixed, PALW-inert networks use zero.
    pub(super) palw_network_id: u32,
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
    // kaspa-pq ADR-0040 §5.15.13 (gate G16 / P1-9-RELAND): per-chain-block paid `job_nullifier`s,
    // the delta the bounded reward-coordinate duplicate-work walk reads. Empty on every preset.
    pub(super) palw_paid_work_store: Arc<DbPalwPaidWorkStore>,
    /// ADR-MA §21: content-addressed registry record tiers + the block-keyed fork-local view.
    pub(super) palw_compute_registry_store: Arc<crate::model::stores::palw_compute_registry::DbPalwComputeRegistryStore>,
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
        evm_checkpoint_policy: kaspa_consensus_core::evm::EvmCheckpointPolicy,
        evm_retention_policy: kaspa_consensus_core::evm::EvmRetentionPolicy,
        evm_shadow_state_backend: bool,
        evm_flat_authoritative: bool,
        evm_retire_206: bool,
        palw_pruning_snapshot_checkpoints: Vec<PalwPruningSnapshotCheckpoint>,
        palw_permissionless_snapshot_auth: bool,
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
        Self {
            receiver,
            pruning_sender,
            pruning_receiver,
            thread_pool,

            genesis: params.genesis.clone(),
            pow_blake2b_sha3_activation: params.pow_blake2b_sha3_activation,
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
            palw_provider_bonds_store: storage.palw_provider_bonds_store.clone(),
            dns_state_store: storage.dns_state_store.clone(),
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
            evm_state_checkpoint_v2_store: storage.evm_state_checkpoint_v2_store.clone(),
            evm_checkpoint_meta_store: storage.evm_checkpoint_meta_store.clone(),
            evm_code_store: storage.evm_code_store.clone(),
            evm_flat_account_store: storage.evm_flat_account_store.clone(),
            evm_block_state_root_store: storage.evm_block_state_root_store.clone(),
            evm_latest_state_ptr_store: storage.evm_latest_state_ptr_store.clone(),
            evm_shadow_state_backend,
            evm_flat_authoritative,
            evm_retire_206,
            evm_history_mode,
            evm_checkpoint_policy,
            evm_retention_policy,
            evm_activation_daa_score: params.evm_activation_daa_score,
            palw_activation_daa_score: params.palw_activation_daa_score,
            palw_compute_registry_activation_daa_score: params.palw_compute_registry_activation_daa_score,
            palw_store: storage.palw_store.clone(),
            palw_beacon_store: storage.palw_beacon_store.clone(),
            palw_lane_bits_store: storage.palw_lane_bits_store.clone(),
            palw_nullifier_store: storage.palw_nullifier_store.clone(),
            palw_overlay_view_store: storage.palw_overlay_view_store.clone(),
            palw_da_store: storage.palw_da_store.clone(),
            palw_search_availability_store: storage.palw_search_availability_store.clone(),
            palw_pruned_frontier_store: storage.palw_pruned_frontier_store.clone(),
            palw_epoch_length_daa: params.palw_epoch_length_daa,
            palw_batch_admission: params.palw_batch_admission,
            palw_lane_difficulty: params.palw_lane_difficulty.clone(),
            palw_spam: params.palw_spam,
            palw_spam_store: storage.palw_spam_store.clone(),
            palw_pruning_snapshot_checkpoints: palw_pruning_snapshot_checkpoints.into(),
            palw_permissionless_snapshot_auth,
            palw_beacon_grace_epochs: params.palw_beacon_grace_epochs,
            palw_beacon_quorum_num: params.palw_beacon_quorum_num,
            palw_beacon_quorum_den: params.palw_beacon_quorum_den,
            palw_audit_quorum_num: params.palw_audit_quorum_num,
            palw_audit_quorum_den: params.palw_audit_quorum_den,
            palw_audit_committee_size: params.palw_audit_committee_size,
            palw_audit_sample_size: params.palw_audit_sample_size,
            palw_network_id: params.net.suffix().unwrap_or(0),
            evm_gas_pool_v2_activation_daa_score: params.evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score: params.evm_f002_withdraw_cap_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score: params.evm_f003_mldsa_verify_activation_daa_score,
            evm_typed_receipt_root_activation_daa_score: params.evm_typed_receipt_root_activation_daa_score,
            evm_lane_kpi: EvmLaneKpi::default(),
            dns_params: params.dns_params.clone(),
            utxo_diffs_store: storage.utxo_diffs_store.clone(),
            rewarded_epochs_store: storage.rewarded_epochs_store.clone(),
            palw_paid_work_store: storage.palw_paid_work_store.clone(),
            palw_compute_registry_store: storage.palw_compute_registry_store.clone(),
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

            // Exit is ordered after the body processor has drained its workers, but keep the protocol
            // total even for tests/abnormal shutdown: every queued parent waiter is explicitly woken
            // with an error instead of being left blocked on a dropped worker.
            if messages.iter().any(VirtualStateProcessingMessage::is_exit_message) {
                fail_palw_parent_waiters_on_shutdown(&messages);
                break 'outer;
            }

            self.resolve_virtual();

            // Complete all provenance requests first. A batch may also contain the ordinary Process
            // messages which made those parents visible; their result senders below then observe the
            // terminal status written by this pass.
            for msg in &messages {
                if let VirtualStateProcessingMessage::EnsurePalwParents { parents, selected_parent, result } = msg {
                    let _ = result.send(self.ensure_palw_parent_provenance(parents, *selected_parent));
                }
            }
            let statuses_read = self.statuses_store.read();
            for msg in messages {
                match msg {
                    VirtualStateProcessingMessage::Exit => unreachable!("exit batch handled above"),
                    VirtualStateProcessingMessage::Process(task, virtual_state_result_transmitter) => {
                        // We don't care if receivers were dropped
                        let _ = virtual_state_result_transmitter.send(Ok(statuses_read.get(task.block().hash()).unwrap()));
                    }
                    VirtualStateProcessingMessage::EnsurePalwParents { .. } => {}
                };
            }
        }

        // Pass the exit signal on to the following processor
        self.pruning_sender.send(PruningProcessingMessage::Exit).unwrap();
    }

    /// Finish UTXO classification and accepted-lifecycle persistence for each Header-v4 parent from
    /// the current virtual point of view. This runs only on the single virtual worker, so concurrent
    /// body requests serialize here; the second request observes the first one's committed row.
    fn ensure_palw_parent_provenance(&self, parents: &[BlockHash], selected_parent: BlockHash) -> Result<(), String> {
        if !parents.contains(&selected_parent) {
            return Err(format!("selected parent {selected_parent} is absent from the direct-parent set"));
        }
        for &parent in parents {
            let status = self
                .statuses_store
                .read()
                .get(parent)
                .optional()
                .map_err(|err| format!("parent {parent} status read failed: {err}"))?
                .ok_or_else(|| format!("parent {parent} is unknown"))?;
            let has_view = if parent == selected_parent && status == StatusUTXOValid {
                self.palw_overlay_view_store
                    .view(parent)
                    .map_err(|err| format!("parent {parent} accepted lifecycle read failed: {err}"))?
                    .is_some()
            } else {
                false
            };
            match palw_parent_terminal_status(status, parent == selected_parent, has_view) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(reason) => return Err(format!("parent {parent} cannot provide accepted lifecycle provenance: {reason}")),
            }

            let pruning_point = self
                .pruning_point_store
                .read()
                .pruning_point()
                .map_err(|err| format!("pruning point read failed while completing parent {parent}: {err}"))?;
            let virtual_read = self.virtual_stores.upgradable_read();
            let previous = virtual_read
                .state
                .get()
                .map_err(|err| format!("virtual state read failed while completing parent {parent}: {err}"))?;
            let from = previous.ghostdag_data.selected_parent;
            let finality_point = self.virtual_finality_point(&previous.ghostdag_data, pruning_point);
            let prune_guard = self.pruning_lock.blocking_read();
            let above_finality = self
                .reachability_service
                .try_is_chain_ancestor_of(finality_point, parent)
                .map_err(|err| format!("parent {parent} reachability check failed: {err}"))?;
            drop(prune_guard);
            if !above_finality {
                return Err(format!("parent {parent} is below the virtual finality point {finality_point}"));
            }

            let mut diff = previous.utxo_diff.clone().to_reversed();
            let mut bond_view = self.initial_active_bond_view();
            let mut provider_bond_view = self.initial_palw_provider_bond_view();
            let completed =
                self.calculate_utxo_state_relatively(&virtual_read, &mut diff, &mut bond_view, &mut provider_bond_view, from, parent);
            let status = self.statuses_store.read().get(parent).unwrap();
            let has_view = if parent == selected_parent && status == StatusUTXOValid {
                self.palw_overlay_view_store
                    .view(parent)
                    .map_err(|err| format!("parent {parent} accepted lifecycle read failed after completion: {err}"))?
                    .is_some()
            } else {
                false
            };
            match palw_parent_terminal_status(status, parent == selected_parent, has_view) {
                Ok(true) => {
                    if status == StatusDisqualifiedFromChain {
                        debug!(
                            "Header-v4 non-selected direct parent {parent} completed as disqualified (walk stopped at {completed})"
                        );
                    }
                }
                Ok(false) => return Err(format!("parent {parent} remained UTXO-pending after completion walk")),
                Err(reason) => return Err(format!("parent {parent} failed UTXO/provenance completion: {reason}")),
            }
        }
        Ok(())
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
        // ADR-0040 ECON-03 (THE WIRE): the PALW provider-bond view, walked in the SAME lockstep so
        // that at each chain-block UTXO verification it equals the registry as-of that block's
        // selected parent — the point of view `palw_work_reward_class` resolves a leaf's
        // `provider_{a,b}_bond` against. Empty + untouched while PALW is fenced.
        let mut accumulated_provider_bond_view = self.initial_palw_provider_bond_view();

        let (new_sink, virtual_parent_candidates) = self.sink_search_algorithm(
            &virtual_read,
            &mut accumulated_diff,
            &mut accumulated_bond_view,
            &mut accumulated_provider_bond_view,
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
                // Likewise the provider-bond registry as-of the new sink.
                &accumulated_provider_bond_view,
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
        // ADR-0040 ECON-03 (THE WIRE): walked in lockstep with `bond_view`, on the PALW fence rather
        // than the DNS one. See `initial_palw_provider_bond_view` for why resolution lives here.
        provider_bond_view: &mut ProviderBondView,
        from: BlockHash,
        to: BlockHash,
    ) -> BlockHash {
        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.1): walk the active-bond
        // view in lockstep with `diff` so it always equals the bond set as-of
        // the block whose UTXO state `diff` represents. No-op on networks
        // without the overlay. No consumer yet (b2a) — the view is passed to
        // `verify_expected_utxo_state` inert.
        let track_bonds = self.dns_params.is_some();
        let track_provider_bonds = self.palw_activation_daa_score != u64::MAX;

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
                bond_view.revert(&self.dns_bond_mutations_for_chain_block(current));
            }
            if track_provider_bonds {
                provider_bond_view.revert(&self.palw_provider_bond_mutations_for_chain_block(current));
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
                        bond_view.apply(&self.dns_bond_mutations_for_chain_block(current));
                    }
                    if track_provider_bonds {
                        provider_bond_view.apply(&self.palw_provider_bond_mutations_for_chain_block(current));
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
                    self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, &*bond_view, &*provider_bond_view, pov_daa_score);

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

                    // Header-v4 additionally commits this exact selected-parent provider view as part
                    // of the complete PALW state root. The same walked view already fed acceptance-time
                    // collateral/DA gates above, so validation cannot read a tip-global generation.
                    let res = self.verify_expected_utxo_state(
                        &mut ctx,
                        &selected_parent_utxo_view,
                        &*bond_view,
                        &*provider_bond_view,
                        &header,
                    );

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
                        // Snapshot THIS block's DNS mutations while the acceptance data is still
                        // in memory, but keep `bond_view` at the selected parent through the commit:
                        // PALW beacon stake/signature checks are deliberately past-relative and may
                        // not see a bond created/slashed/unbonded by the block being committed.
                        let bond_muts =
                            track_bonds.then(|| self.dns_bond_mutations_from_acceptance(&ctx.mergeset_acceptance_data, pov_daa_score));
                        // ADR-0040 ECON-03: same treatment for the provider registry — snapshot now,
                        // advance only after every selected-parent-relative commitment is derived, so
                        // this block's own bonds are never visible to its own reward classification.
                        let palw_da_staged = self.stage_palw_da_effects(
                            selected_parent,
                            pov_daa_score,
                            &ctx.mergeset_acceptance_data,
                            &*provider_bond_view,
                        );
                        // Search-availability dispatch runs SECOND, deterministically: it consumes
                        // the DA delta's slash set (void sweep + fresh-registration refusal) and
                        // contributes its own scheduler-bond slashes to the same registry walk.
                        let palw_search_staged = self.stage_palw_search_availability_effects(
                            selected_parent,
                            pov_daa_score,
                            &ctx.mergeset_acceptance_data,
                            &*provider_bond_view,
                            palw_da_staged.as_ref().map(|staged| staged.slash_mutations.as_slice()).unwrap_or(&[]),
                        );
                        let mut provider_bond_muts = track_provider_bonds
                            .then(|| self.palw_provider_bond_mutations_from_acceptance(&ctx.mergeset_acceptance_data, pov_daa_score));
                        if let (Some(mutations), Some(staged)) = (provider_bond_muts.as_mut(), palw_da_staged.as_ref()) {
                            mutations.extend(staged.slash_mutations.iter().cloned());
                        }
                        if let (Some(mutations), Some(staged)) = (provider_bond_muts.as_mut(), palw_search_staged.as_ref()) {
                            mutations.extend(staged.slash_mutations.iter().cloned());
                        }
                        // Commit UTXO data for current chain block
                        self.commit_utxo_state(
                            current,
                            ctx.mergeset_diff,
                            ctx.multiset_hash,
                            ctx.mergeset_acceptance_data,
                            ctx.pruning_sample_from_pov.expect("verified"),
                            ctx.validator_rewarded_keys,
                            ctx.palw_paid_work_ids,
                            ctx.validator_quality_subpool,
                            ctx.reserve_balance_after,
                            evm_staged,
                            &*bond_view,
                            // ADR-0040 §5.17: the provider-bond view at THIS block's selected parent —
                            // snapshotted before this block's own provider mutations are applied below.
                            &*provider_bond_view,
                            palw_da_staged,
                            palw_search_staged,
                        );
                        if let Some(bond_muts) = bond_muts {
                            // Advance the in-memory selected-chain walk only after every
                            // selected-parent-relative commitment has been derived and persisted.
                            bond_view.apply(&bond_muts);
                        }
                        if let Some(provider_bond_muts) = provider_bond_muts {
                            provider_bond_view.apply(&provider_bond_muts);
                        }
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
            &self.headers_store,
            self.evm_activation_daa_score,
            &self.evm_state_checkpoint_v2_store,
            &self.evm_state_checkpoint_store,
            &self.evm_state_diff_store,
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
        // items chain in-memory). With 206 retired the worker cannot seed from the 206 store —
        // but instead of disabling the pipeline (which made the whole IBD run execute EVM inline
        // and serially), gap seeds are resolved HERE at spawn time from the validated flat store —
        // the exact seed the inline path uses — and carried inside the item. A gap whose
        // EVM-ACTIVE parent yields no flat seed leaves the entire run to the inline path (which
        // HALTs per design §7) by not pipelining; pre-activation parents need no seed (empty
        // genesis). Correctness is identical either way (I-3 invariant).
        let seeded_mode = self.evm_retire_206;
        if seeded_mode && !(self.evm_flat_authoritative && self.evm_shadow_state_backend) {
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
            let seed = if seeded_mode && !chain_from_prev {
                let seed = self.validated_flat_parent_seed(selected_parent);
                if seed.is_none() && self.evm_header_store.has(selected_parent).unwrap_or(true) {
                    // EVM-active (or unprovable) parent without a flat seed: never risk the
                    // worker's 206 path — leave the whole run to the inline HALT semantics.
                    return None;
                }
                seed
            } else {
                None
            };
            pending.push(EvmPipelineItem { block: current, selected_parent, chain_from_prev, seed });
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

    /// Load the exact selected-parent DA state. Once PALW is active, absence anywhere except the
    /// genesis/pre-activation boundary is a local consistency fault; defaulting there would let two
    /// nodes apply the same challenge against different histories.
    pub(crate) fn palw_da_parent_state(
        &self,
        selected_parent: BlockHash,
        current_daa_score: u64,
    ) -> (Arc<PalwDaStateV1>, Option<BlockHash>) {
        if self.palw_activation_daa_score == u64::MAX || current_daa_score < self.palw_activation_daa_score {
            return (Arc::new(PalwDaStateV1::default()), None);
        }
        match self.palw_da_store.read().state_and_anchor(selected_parent) {
            Ok((state, anchor)) if state.validate_structure() => (state, Some(anchor)),
            Ok(_) => {
                palw_overlay_commit_fail_stop(format!("PALW DA selected-parent state {selected_parent} failed structural validation"))
            }
            Err(StoreError::KeyNotFound(_)) if selected_parent == self.genesis.hash => (Arc::new(PalwDaStateV1::default()), None),
            Err(StoreError::KeyNotFound(_)) => {
                let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW DA could not classify missing selected-parent state {selected_parent}: {store_error}"
                    ))
                });
                if parent_daa < self.palw_activation_daa_score {
                    (Arc::new(PalwDaStateV1::default()), None)
                } else {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW DA state is missing for active selected parent {selected_parent} at DAA {parent_daa}"
                    ))
                }
            }
            Err(store_error) => palw_overlay_commit_fail_stop(format!(
                "PALW DA selected-parent state read failed for {selected_parent}: {store_error}"
            )),
        }
    }

    /// Resolve the first selected-parent ancestor buried by the fixed DA policy and carrying a
    /// fork-local beacon state. Both acceptance reservation and committed obligation creation call
    /// this helper, so an accepted Header-v4 leaf cannot be admitted against a different anchor than
    /// the one later persisted.
    pub(super) fn palw_da_buried_beacon(&self, selected_parent: BlockHash, current_daa_score: u64) -> Option<PalwBuriedBeaconV1> {
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        let cutoff = current_daa_score.saturating_sub(policy.min_beacon_burial_daa);
        std::iter::once(selected_parent)
            .chain(self.reachability_service.default_backward_chain_iterator(selected_parent))
            .find_map(|anchor_hash| {
                let anchor_daa = self.headers_store.get_daa_score(anchor_hash).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW DA buried-beacon header read failed at {anchor_hash} below selected parent {selected_parent}: {store_error}"
                    ))
                });
                if anchor_daa > cutoff {
                    return None;
                }
                let beacon = self.palw_beacon_store.beacon_state(anchor_hash).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW DA buried-beacon state read failed at {anchor_hash} below selected parent {selected_parent}: {store_error}"
                    ))
                })?;
                Some(PalwBuriedBeaconV1 {
                    epoch: beacon.epoch,
                    seed: beacon.seed,
                    anchor_hash,
                    anchor_daa_score: anchor_daa,
                    observed_daa_score: current_daa_score,
                })
            })
    }

    /// Apply accepted 0x3a-0x3c transactions in consensus acceptance order to a clone of the
    /// selected-parent state. Responses/timeouts may target only challenges already open in that
    /// parent, and challenges may target only parent obligations, enforcing a full carrier gap.
    fn stage_palw_da_effects(
        &self,
        selected_parent: BlockHash,
        current_daa_score: u64,
        acceptance_data: &AcceptanceData,
        selected_parent_provider_bond_view: &ProviderBondView,
    ) -> Option<PalwDaStaged> {
        if self.palw_activation_daa_score == u64::MAX || current_daa_score < self.palw_activation_daa_score {
            return None;
        }
        let (parent, parent_anchor) = self.palw_da_parent_state(selected_parent, current_daa_score);
        let parent_obligations: HashSet<Hash64> = parent.obligations.keys().copied().collect();
        let parent_open_challenges: HashSet<Hash64> = parent
            .challenges
            .iter()
            .filter_map(|(id, challenge)| matches!(challenge.status, PalwDaChallengeStatusV1::Open).then_some(*id))
            .collect();
        let mut state = (*parent).clone();
        state.begin_child_block();
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        let context = crate::processes::palw_da::PalwDaApplyContext {
            network_id: self.palw_network_id,
            current_daa_score,
            current_epoch: current_daa_score / self.palw_epoch_length_daa.max(1),
            policy: &policy,
            provider_bonds: selected_parent_provider_bond_view,
        };
        let mut slash_mutations = Vec::new();
        for tx in self.accepted_txs_from_acceptance_data(acceptance_data) {
            let Some(kind) = tx.subnetwork_id.palw_tx_kind() else { continue };
            if !matches!(kind, 0x3a..=0x3c) {
                continue;
            }
            let Ok(effect) = crate::processes::palw_da::parse_palw_da_effect(kind, &tx.payload) else { continue };
            let past_relative = match &effect {
                crate::processes::palw_da::PalwDaOverlayEffect::Challenge(challenge) => {
                    parent_obligations.contains(&challenge.obligation_id)
                }
                crate::processes::palw_da::PalwDaOverlayEffect::Response(response) => {
                    parent_open_challenges.contains(&response.challenge_id)
                }
                crate::processes::palw_da::PalwDaOverlayEffect::Timeout(evidence) => {
                    parent_open_challenges.contains(&evidence.challenge_id)
                }
            };
            if !past_relative {
                continue;
            }
            if let Ok((Some(PalwProviderBondMutation::Slash(provider_bond, daa_score)), _undo)) =
                crate::processes::palw_da::apply_palw_da_effect(&mut state, effect, &context)
            {
                let already_recorded = state.block_slashed_providers.contains(&provider_bond);
                state.record_block_slash(provider_bond).unwrap_or_else(|error| {
                    palw_overlay_commit_fail_stop(format!("PALW DA block slash delta exceeded its bound: {error}"))
                });
                if !already_recorded {
                    slash_mutations.push(PalwProviderBondMutation::Slash(provider_bond, daa_score));
                }
            }
        }
        if !state.validate_structure() {
            palw_overlay_commit_fail_stop("PALW DA accepted-effect staging produced invalid state".to_string());
        }
        Some(PalwDaStaged { certificate_state: parent, parent_anchor, state, slash_mutations })
    }

    /// Shared fork-local loader for the search-availability state at a chain block's selected
    /// parent — `palw_da_parent_state`'s sibling, with identical fail-stop semantics: a missing row
    /// for an ACTIVE parent is a hard inconsistency, never a silent empty default.
    pub(super) fn palw_search_parent_state(
        &self,
        selected_parent: BlockHash,
        current_daa_score: u64,
    ) -> (Arc<PalwSearchAvailabilityStateV1>, Option<BlockHash>) {
        if self.palw_activation_daa_score == u64::MAX || current_daa_score < self.palw_activation_daa_score {
            return (Arc::new(PalwSearchAvailabilityStateV1::default()), None);
        }
        match self.palw_search_availability_store.read().state_and_anchor(selected_parent) {
            Ok((state, anchor)) if state.validate_structure() => (state, Some(anchor)),
            Ok(_) => palw_overlay_commit_fail_stop(format!(
                "PALW search selected-parent state {selected_parent} failed structural validation"
            )),
            Err(StoreError::KeyNotFound(_)) if selected_parent == self.genesis.hash => {
                (Arc::new(PalwSearchAvailabilityStateV1::default()), None)
            }
            Err(StoreError::KeyNotFound(_)) => {
                let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW search could not classify missing selected-parent state {selected_parent}: {store_error}"
                    ))
                });
                if parent_daa < self.palw_activation_daa_score {
                    (Arc::new(PalwSearchAvailabilityStateV1::default()), None)
                } else {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW search state is missing for active selected parent {selected_parent} at DAA {parent_daa}"
                    ))
                }
            }
            Err(store_error) => palw_overlay_commit_fail_stop(format!(
                "PALW search selected-parent state read failed for {selected_parent}: {store_error}"
            )),
        }
    }

    /// Apply accepted 0x3d-0x3f transactions in consensus acceptance order to a clone of the
    /// selected-parent search-availability state (ADR node-anchored-web-search-da dispatch).
    ///
    /// Determinism contract, mirroring `stage_palw_da_effects`:
    /// * Responses/timeouts may target only challenges already OPEN in the parent state (full
    ///   carrier gap); plain challenges may target only parent obligations. A challenge carrying a
    ///   registration proof is self-contained — its JobSpec signatures and the scheduler's bond are
    ///   validated against the SAME parent-relative registry every node holds, so it registers and
    ///   challenges atomically without any ordering freedom.
    /// * Scheduler authorization is exclusively the on-chain bonded registry (`scheduler_is_bonded`);
    ///   the node-local admission allowlist deliberately plays no role here — with it, the same
    ///   accepted tx would be valid on one node and invalid on another (consensus split).
    /// * Slash linkage both ways: a search timeout emits the scheduler-bond slash mutation and
    ///   immediately voids that scheduler's remaining obligations; a bond already slashed by THIS
    ///   block's DA effects is voided up front and refused as a fresh registration authority, and
    ///   its slash is never double-emitted.
    /// * Every failing effect is skipped deterministically (fee-paying no-op), never a fail-stop:
    ///   all gates are pure functions of (parent state, registry view, tx bytes).
    fn stage_palw_search_availability_effects(
        &self,
        selected_parent: BlockHash,
        current_daa_score: u64,
        acceptance_data: &AcceptanceData,
        selected_parent_provider_bond_view: &ProviderBondView,
        da_slash_mutations: &[PalwProviderBondMutation],
    ) -> Option<PalwSearchStaged> {
        if self.palw_activation_daa_score == u64::MAX || current_daa_score < self.palw_activation_daa_score {
            return None;
        }
        let (parent, parent_anchor) = self.palw_search_parent_state(selected_parent, current_daa_score);
        let parent_obligations: HashSet<Hash64> = parent.obligations.keys().copied().collect();
        let parent_open_challenges: HashSet<Hash64> = parent
            .obligations
            .iter()
            .filter_map(|(root, obligation)| {
                matches!(
                    obligation.status,
                    kaspa_consensus_core::palw::search_snapshot::PalwSearchObligationStatusV1::Challenged { .. }
                )
                .then_some(*root)
            })
            .collect();
        let mut state = (*parent).clone();
        state.begin_child_block();
        let mut slash_mutations = Vec::new();
        // Bonds slashed by THIS block, in deterministic order: DA effects staged first (their
        // mutation order), then search timeouts in acceptance order. Seeding the set from the DA
        // delta voids a doubly-active scheduler's obligations and blocks it from anchoring fresh
        // ones within the same block, without ever emitting a second Slash for the same bond.
        let mut slashed_this_block: HashSet<TransactionOutpoint> = HashSet::new();
        for mutation in da_slash_mutations {
            if let PalwProviderBondMutation::Slash(bond, daa) = mutation {
                slashed_this_block.insert(*bond);
                state.void_by_scheduler_bond(*bond, *daa);
            }
        }
        let context = crate::processes::palw_search::PalwSearchApplyContext {
            network_id: self.palw_network_id,
            // Same genesis binding the snapshot/assignment admission path pins
            // (`palw_admit_search_snapshot_impl` binds `config.genesis.hash`).
            genesis_hash: self.genesis.hash,
            current_daa_score,
            provider_bonds: selected_parent_provider_bond_view,
        };
        for tx in self.accepted_txs_from_acceptance_data(acceptance_data) {
            let Some(kind) = tx.subnetwork_id.palw_tx_kind() else { continue };
            if !matches!(kind, 0x3d..=0x3f) {
                continue;
            }
            let Ok(effect) = crate::processes::palw_search::parse_palw_search_effect(kind, &tx.payload) else { continue };
            let past_relative = match &effect {
                crate::processes::palw_search::PalwSearchOverlayEffect::Challenge(challenge) => {
                    // A registering challenge is self-contained; a plain one must hit a parent
                    // obligation. Refuse a registration whose scheduler bond this block just
                    // slashed — the parent registry still shows it Active, but every node derives
                    // the same slashed set from the same accepted effects.
                    match &challenge.registration {
                        Some(registration) => !slashed_this_block.contains(&registration.assignment.scheduler_bond),
                        None => parent_obligations.contains(&challenge.object_root),
                    }
                }
                crate::processes::palw_search::PalwSearchOverlayEffect::Response(response) => {
                    parent_open_challenges.contains(&response.object_root)
                }
                crate::processes::palw_search::PalwSearchOverlayEffect::Timeout(timeout) => {
                    parent_open_challenges.contains(&timeout.object_root)
                }
            };
            if !past_relative {
                continue;
            }
            match crate::processes::palw_search::apply_palw_search_effect(&mut state, &effect, &context) {
                Ok((Some(slashed_scheduler_bond), undos)) => {
                    if slashed_this_block.contains(&slashed_scheduler_bond) {
                        // The bond is already slashed this block (DA effect or an earlier search
                        // timeout); the obligation transition stands, no second registry mutation
                        // and no second block-delta entry.
                        state.void_by_scheduler_bond(slashed_scheduler_bond, current_daa_score);
                        continue;
                    }
                    if state.record_block_slash(slashed_scheduler_bond).is_err() {
                        // Delta capacity/byte budget exhausted: revert the timeout transition so the
                        // obligation state and the slash delta never diverge; the tx becomes a
                        // deterministic no-op on every node.
                        for undo in undos.into_iter().rev() {
                            state.revert(undo).unwrap_or_else(|error| {
                                palw_overlay_commit_fail_stop(format!(
                                    "PALW search staging failed to revert its own transition: {error:?}"
                                ))
                            });
                        }
                        continue;
                    }
                    slashed_this_block.insert(slashed_scheduler_bond);
                    slash_mutations.push(PalwProviderBondMutation::Slash(slashed_scheduler_bond, current_daa_score));
                    state.void_by_scheduler_bond(slashed_scheduler_bond, current_daa_score);
                }
                Ok((None, _undos)) => {}
                Err(_) => continue,
            }
        }
        if !state.validate_structure() {
            palw_overlay_commit_fail_stop("PALW search accepted-effect staging produced invalid state".to_string());
        }
        Some(PalwSearchStaged { parent_state: parent, parent_anchor, state, slash_mutations })
    }

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
        // ADR-0040 §5.15.13: `job_nullifier`s paid by this block's coinbase. Persisted only when
        // non-empty.
        palw_paid_work_ids: PalwPaidWorkIds,
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
        // ADR-0039 §11.2: DNS bonds exactly as-of this block's selected parent.
        // Beacon commits/reveals must never observe this block's own bond mutations.
        selected_parent_bond_view: &ActiveBondView,
        // kaspa-pq ADR-0040 §5.17: PALW provider bonds exactly as-of this block's selected parent — the
        // auditor candidate pool + vote-key/stake source the certificate re-derivations read. Walked in
        // the same lockstep as the DNS view, so a certificate's committee is resolved against the frozen
        // audit snapshot, not this block's own provider mutations.
        selected_parent_provider_bond_view: &ProviderBondView,
        // DA-01: selected-parent state plus this block's accepted 0x3a-0x3c delta. `None` is allowed
        // only while the PALW fence is inactive.
        mut palw_da_staged: Option<PalwDaStaged>,
        // Search-availability: selected-parent state plus this block's accepted 0x3d-0x3f delta.
        // Same fence contract as the DA staging above.
        palw_search_staged: Option<PalwSearchStaged>,
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
                &self.evm_retention_policy,
                &self.evm_receipts_store,
                &self.evm_tx_index_store,
                &self.evm_log_index_store,
                &self.evm_trace_store,
                &self.evm_state_diff_store,
                &self.evm_code_store,
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
        // ADR-0039 §9.3/§9.5: advance the PALW batch state machine from this chain block's accepted
        // overlay txs, keyed to acceptance (a selected-chain property) exactly like the DNS overlays
        // above. The `palw_activation_daa_score == u64::MAX` guard returns before touching
        // `acceptance_data` on mainnet, testnet-10, simnet and devnet. The three PALW presets use an
        // activation score of 0 and process overlay transactions on subnetworks 0x30-0x33.
        if let Some(staged) = palw_da_staged.as_mut() {
            if current != self.genesis.hash {
                self.commit_palw_overlay_effects(
                    &mut batch,
                    current,
                    &acceptance_data,
                    selected_parent_provider_bond_view,
                    staged.certificate_state.as_ref(),
                    &mut staged.state,
                );
            }
            if !staged.state.validate_structure() {
                palw_overlay_commit_fail_stop(format!("PALW DA state for {current} failed pre-commit validation"));
            }
            // IBD/steady-state write reduction: with an idle DA state machine the child state is
            // bit-identical to the stored parent row, so a 64-byte anchor link replaces the
            // duplicated full-state row (and the full clone + serialize that produced it). The
            // equality baseline is the EXACT stored parent state; `begin_child_block` clearing a
            // non-empty per-block slash set or any accepted 0x3a-0x3c effect makes them differ and
            // falls back to the full row.
            let unchanged = staged.parent_anchor.is_some() && staged.state == *staged.certificate_state;
            if let (true, Some(anchor)) = (unchanged, staged.parent_anchor) {
                self.palw_da_store.write().set_state_link_batch(&mut batch, current, anchor).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!("PALW DA state link staging failed for {current}: {store_error}"))
                });
            } else {
                self.palw_da_store.write().set_state_batch(&mut batch, current, Arc::new(staged.state.clone())).unwrap_or_else(
                    |store_error| palw_overlay_commit_fail_stop(format!("PALW DA state staging failed for {current}: {store_error}")),
                );
            }
        } else {
            let current_daa = self.headers_store.get_daa_score(current).unwrap_or_else(|store_error| {
                palw_overlay_commit_fail_stop(format!("PALW DA activation check failed for {current}: {store_error}"))
            });
            if self.palw_activation_daa_score != u64::MAX && current_daa >= self.palw_activation_daa_score {
                palw_overlay_commit_fail_stop(format!("active PALW block {current} reached commit without staged DA state"));
            }
        }
        // ADR-MA §21.2: advance the Compute Set registry view from this chain block's accepted
        // 0x40-0x44 registry transactions — the same acceptance coordinate as the PALW overlays
        // above, behind its own activation fence (`u64::MAX` on every shipped preset, so this
        // returns immediately everywhere today).
        if current != self.genesis.hash {
            self.commit_palw_compute_registry_view(&mut batch, current, &acceptance_data, selected_parent_bond_view);
        }
        // Search-availability rows commit in the SAME batch, with the identical link-vs-full write
        // reduction: an idle search state machine costs 64 bytes per chain block, never a duplicated
        // full row.
        if let Some(staged) = palw_search_staged {
            if !staged.state.validate_structure() {
                palw_overlay_commit_fail_stop(format!("PALW search state for {current} failed pre-commit validation"));
            }
            let unchanged = staged.parent_anchor.is_some() && staged.state == *staged.parent_state;
            if let (true, Some(anchor)) = (unchanged, staged.parent_anchor) {
                self.palw_search_availability_store.write().set_state_link_batch(&mut batch, current, anchor).unwrap_or_else(
                    |store_error| {
                        palw_overlay_commit_fail_stop(format!("PALW search state link staging failed for {current}: {store_error}"))
                    },
                );
            } else {
                self.palw_search_availability_store
                    .write()
                    .set_state_batch(&mut batch, current, Arc::new(staged.state))
                    .unwrap_or_else(|store_error| {
                        palw_overlay_commit_fail_stop(format!("PALW search state staging failed for {current}: {store_error}"))
                    });
            }
        } else {
            let current_daa = self.headers_store.get_daa_score(current).unwrap_or_else(|store_error| {
                palw_overlay_commit_fail_stop(format!("PALW search activation check failed for {current}: {store_error}"))
            });
            if self.palw_activation_daa_score != u64::MAX && current_daa >= self.palw_activation_daa_score {
                palw_overlay_commit_fail_stop(format!("active PALW block {current} reached commit without staged search state"));
            }
        }
        // ADR-0039 §11.2: derive/carry this block's active beacon seed R_E (block-keyed recurrence,
        // read via selected parent). Written into THIS batch (atomic with the UTXO diff). The fast-path
        // return fires on mainnet / testnet-10 / simnet / devnet only; on testnet-palw-110 /
        // devnet-palw-111 (fence = 0) beacon state + accumulator rows ARE written per chain block.
        self.commit_palw_beacon_state(&mut batch, current, &acceptance_data, selected_parent_bond_view);
        self.acceptance_data_store.insert_batch(&mut batch, current, Arc::new(acceptance_data)).unwrap();
        if !rewarded_keys.is_empty() {
            self.rewarded_epochs_store.insert_batch(&mut batch, current, Arc::new(rewarded_keys)).unwrap();
        }
        // kaspa-pq ADR-0040 §5.15.13 (G16): this chain block's paid-`job_nullifier` delta, written into
        // the SAME atomic batch as the UTXO diff — so a block's payout and the evidence its descendants
        // dedup against can never be persisted apart. Non-empty only when a `ReplicaPalw` class was
        // actually paid, i.e. never on any shipped preset.
        if !palw_paid_work_ids.is_empty() {
            self.palw_paid_work_store.insert_batch(&mut batch, current, Arc::new(palw_paid_work_ids)).unwrap();
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
        self.db.write(batch).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!(
                "UTXO/PALW accepted provenance RocksDB commit failed for {current} after cache-backed staging: {store_error}"
            ))
        });
        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(write_guard);
    }

    /// ADR-0039 §9.3/§9.5 — apply the batch-lifecycle transitions carried by this chain block's
    /// accepted PALW overlay txs (subnetwork `0x30`–`0x33`) to the [`PalwStore`]. Runs at virtual
    /// commit, keyed to acceptance (a selected-chain property) so construction and validation see the
    /// same transitions, mirroring the DNS attestation/slashing overlays.
    ///
    /// The fast-path guard leaves the store unwritten on mainnet, testnet-10, simnet and devnet. The
    /// three PALW presets use `palw_activation_daa_score = 0`, so this runs from genesis. Overlay
    /// transitions are carried by ordinary transactions on subnetworks `0x30`–`0x33` and are
    /// independent of the algo-4 header-admission lever.
    ///
    /// Production apply writes no mutable batch status. Header-v3 retains the body-built lifecycle.
    /// Header-v4 instead clones the
    /// selected parent's accepted view and applies only this acceptance set's contextually valid effects.
    /// Blobs, that accepted view, DA state, UTXO diff and StatusUTXOValid are staged in one WriteBatch;
    /// staging or final-write failure is process-fatal because cache-backed writers may already be ahead
    /// of durable RocksDB. Certificate validity stays fork-scoped through the block-keyed exact hash:
    /// a globally cached blob cannot be named unless the selected-parent accepted view chose it after
    /// this fork's attestation and DA gates.
    /// ADR-MA §21.2 — fold this chain block's ACCEPTED Compute Set registry transactions
    /// (subnetworks 0x40-0x44) into the fork-local registry view:
    /// `view(B) = view(SP(B)) ⊕ Δ(accepted registry txs)`, persisted per block in the same commit
    /// batch — the `PalwBatchViewV1` lifecycle (block-keyed ⇒ reorg-reversible; the content
    /// record tiers are content-addressed write-once, so a losing branch leaves nothing to
    /// revert, §21.2).
    ///
    /// Failure posture: payload SHAPE is already a transaction-validity rule
    /// (`check_transaction_subnetwork` decodes the band strictly), so a semantic fold rejection
    /// here — sequence rollback, illegal lifecycle transition, ramp violation — means the tx was
    /// accepted but has NO registry effect: warn + skip, never block-invalidating (§9/§10.3 are
    /// registry admission rules, not block validity rules — the same posture as the batch
    /// overlay fold above). Store faults fail-stop.
    ///
    /// Activation certificates are currently SKIPPED with a warning: §17.3 quorum verification
    /// (the `verify_certificate_attestation` analogue over the validator set) lands in its own
    /// slice, and folding an UNVERIFIED certificate would let a single operator key activate a
    /// set — exactly what §17.3 forbids. Until then no set can leave `Proposed`/get share, which
    /// fails closed in the safe direction.
    fn commit_palw_compute_registry_view(
        &self,
        batch: &mut WriteBatch,
        current: BlockHash,
        acceptance_data: &AcceptanceData,
        selected_parent_bond_view: &ActiveBondView,
    ) {
        use crate::model::stores::palw_compute_registry::PalwComputeRegistryStoreReader;
        use kaspa_consensus_core::palw_compute_set::{
            AllocationGovernanceLimitsV1, PalwComputeRegistryEffect, PolicyGovernanceCapsV1, parse_palw_compute_registry,
        };
        if self.palw_compute_registry_activation_daa_score == u64::MAX {
            return;
        }
        let current_daa = self.headers_store.get_daa_score(current).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!("compute registry commit could not read DAA of {current}: {store_error}"))
        });
        if current_daa < self.palw_compute_registry_activation_daa_score {
            return;
        }
        let selected_parent = self.ghostdag_store.get_selected_parent(current).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!("compute registry commit could not read SP of {current}: {store_error}"))
        });
        // Seed from the selected parent's carried view. An absent row is legal exactly when the
        // parent sits below the activation fence (or is genesis); a missing row ABOVE the fence
        // is a local inconsistency, not a semantic state.
        let mut view = match self.palw_compute_registry_store.view(selected_parent) {
            Ok(Some(parent_view)) => (*parent_view).clone(),
            Ok(None) => {
                let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!(
                        "compute registry commit could not read SP DAA of {selected_parent}: {store_error}"
                    ))
                });
                if selected_parent != self.genesis.hash && parent_daa >= self.palw_compute_registry_activation_daa_score {
                    palw_overlay_commit_fail_stop(format!(
                        "compute registry view missing for post-activation selected parent {selected_parent} of {current}"
                    ));
                }
                kaspa_consensus_core::palw_compute_set::PalwComputeRegistryViewV1::new()
            }
            Err(store_error) => palw_overlay_commit_fail_stop(format!(
                "compute registry view read failed for {selected_parent} while committing {current}: {store_error}"
            )),
        };
        // Governed numbers: v0.1 initial values (§22.1/§22.2). Promoting them to Params fields is
        // part of the governance slice; they are consensus-inert until the fence opens anywhere.
        let caps = PolicyGovernanceCapsV1::default();
        let limits = AllocationGovernanceLimitsV1::default();
        for merged in acceptance_data.iter() {
            let txs = match self.block_transactions_store.get(merged.block_hash) {
                Ok(txs) => txs,
                Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => {
                    // Same tolerance as the overlay fold: a bodyless carrier is legal only if it
                    // is prunable history; anything else is an acceptance/body inconsistency.
                    let pruning_point = self.pruning_point_store.read().pruning_point().unwrap_or_else(|store_error| {
                        palw_overlay_commit_fail_stop(format!(
                            "compute registry commit could not read pruning point while committing {current}: {store_error}"
                        ))
                    });
                    if palw_body_may_be_pruned(merged.block_hash, pruning_point) {
                        continue;
                    }
                    palw_overlay_commit_fail_stop(format!(
                        "compute registry commit acceptance/body inconsistency while committing {current}: merged block {} is missing its body",
                        merged.block_hash
                    ));
                }
                Err(store_error) => palw_overlay_commit_fail_stop(format!(
                    "compute registry commit body read failed for merged block {} while committing {current}: {store_error}",
                    merged.block_hash
                )),
            };
            for entry in merged.accepted_transactions.iter() {
                let Some(tx) = txs.get(entry.index_within_block as usize) else {
                    palw_overlay_commit_fail_stop(format!(
                        "compute registry commit acceptance/body index inconsistency while committing {current}: merged block {} names index {}",
                        merged.block_hash, entry.index_within_block
                    ));
                };
                let Some(kind) = tx.subnetwork_id.palw_compute_registry_tx_kind() else { continue };
                let effect = match parse_palw_compute_registry(kind, &tx.payload) {
                    Ok(effect) => effect,
                    Err(decode_error) => {
                        // Shape was screened at tx validity; reaching here means rules drifted
                        // between admission and fold. Skip loudly rather than split.
                        warn!("compute registry: accepted tx {} no longer parses at fold ({decode_error}); skipped", entry.transaction_id);
                        continue;
                    }
                };
                let outcome = match effect {
                    PalwComputeRegistryEffect::Proposal(proposal) => view.apply_proposal(&proposal).map(|outcome| {
                        if outcome == kaspa_consensus_core::palw_compute_set::DescriptorRegistrationOutcome::Inserted {
                            let set_id = proposal.descriptor.compute_set_id();
                            self.palw_compute_registry_store
                                .insert_descriptor_batch(batch, set_id, &proposal.descriptor)
                                .unwrap_or_else(|store_error| {
                                    palw_overlay_commit_fail_stop(format!(
                                        "compute registry descriptor staging failed for {set_id}: {store_error}"
                                    ))
                                });
                        }
                    }),
                    PalwComputeRegistryEffect::ActivationCertificate(certificate) => {
                        // §17.3 — verify the embedded ML-DSA-87 validator attestation against the
                        // frozen selected-parent DNS bond view (the beacon quorum fraction) BEFORE
                        // the view learns of the certificate. A failed attestation is an admission
                        // rejection (warn + skip, set stays un-certified); only a verified
                        // certificate reaches `apply_certificate(_, true)`.
                        crate::processes::palw::verify_compute_set_activation_certificate(
                            &certificate,
                            selected_parent_bond_view,
                            self.palw_network_id,
                            current_daa,
                            self.palw_beacon_quorum_num,
                            self.palw_beacon_quorum_den,
                        )
                        .and_then(|()| view.apply_certificate(&certificate, true))
                        .map(|()| {
                            self.palw_compute_registry_store.insert_certificate_batch(batch, &certificate).unwrap_or_else(
                                |store_error| {
                                    palw_overlay_commit_fail_stop(format!(
                                        "compute registry certificate staging failed for {}: {store_error}",
                                        certificate.compute_set_id
                                    ))
                                },
                            );
                        })
                    }
                    PalwComputeRegistryEffect::PolicyUpdate(update) => {
                        view.apply_policy(&update.policy, &caps, current_daa).map(|outcome| {
                            if outcome == kaspa_consensus_core::palw_compute_set::PolicyRegistrationOutcome::Inserted {
                                let policy_id = update.policy.policy_id();
                                self.palw_compute_registry_store
                                    .insert_policy_batch(batch, policy_id, &update.policy)
                                    .unwrap_or_else(|store_error| {
                                        palw_overlay_commit_fail_stop(format!(
                                            "compute registry policy staging failed for {policy_id}: {store_error}"
                                        ))
                                    });
                            }
                        })
                    }
                    PalwComputeRegistryEffect::AllocationPlan(plan) => view.apply_plan(&plan, &limits, current_daa).map(|outcome| {
                        if outcome == kaspa_consensus_core::palw_compute_set::PlanRegistrationOutcome::Inserted {
                            self.palw_compute_registry_store.insert_plan_batch(batch, plan.plan_id, &plan).unwrap_or_else(
                                |store_error| {
                                    palw_overlay_commit_fail_stop(format!(
                                        "compute registry plan staging failed for {}: {store_error}",
                                        plan.plan_id
                                    ))
                                },
                            );
                        }
                    }),
                    PalwComputeRegistryEffect::EmergencyHalt(halt) => view.apply_halt(&halt),
                };
                if let Err(rejection) = outcome {
                    warn!(
                        "compute registry: accepted tx {} rejected at fold while committing {current}: {rejection}",
                        entry.transaction_id
                    );
                }
            }
        }
        self.palw_compute_registry_store.set_view_batch(batch, current, std::sync::Arc::new(view)).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!("compute registry view staging failed for {current}: {store_error}"))
        });
    }

    fn commit_palw_overlay_effects(
        &self,
        batch: &mut WriteBatch,
        current: BlockHash,
        acceptance_data: &AcceptanceData,
        selected_parent_provider_bond_view: &ProviderBondView,
        certificate_da_state: &PalwDaStateV1,
        da_state: &mut PalwDaStateV1,
    ) {
        if self.palw_activation_daa_score == u64::MAX {
            return; // inert fast path — no header read, no acceptance-data walk.
        }
        let current_header = self.headers_store.get_header(current).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!("PALW overlay commit could not read chain block {current}: {store_error}"))
        });
        let cur_daa = current_header.daa_score;
        if cur_daa < self.palw_activation_daa_score {
            return;
        }
        // kaspa-pq ADR-0040 §5.17.3 — the selected parent anchors the buried-walk audit-epoch seed
        // resolution below; it is strictly in `current`'s past, so every node reaching `current` walks
        // the identical selected-parent chain (order-independent).
        let selected_parent = self.ghostdag_store.get_selected_parent(current).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!(
                "PALW overlay commit could not read selected parent for chain block {current}: {store_error}"
            ))
        });
        let epoch_len = self.palw_epoch_length_daa.max(1);
        let inclusion_epoch = cur_daa / epoch_len;
        let inclusion_window_epochs = kaspa_consensus_core::palw::palw_audit_epoch_inclusion_window_epochs(&self.palw_batch_admission);
        let accepted_v4 = current_header.version >= kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION;
        let mut accepted_view = if accepted_v4 {
            Some(
                self.palw_overlay_view_store
                    .view(selected_parent)
                    .unwrap_or_else(|store_error| {
                        palw_overlay_commit_fail_stop(format!(
                            "PALW accepted lifecycle could not read selected-parent view {selected_parent} while committing {current}: {store_error}"
                        ))
                    })
                    .map(|view| (*view).clone())
                    .unwrap_or_else(|| {
                        palw_overlay_commit_fail_stop(format!(
                            "Header-v4 block {current} reached virtual commit without selected-parent accepted lifecycle {selected_parent}"
                        ))
                    }),
            )
        } else {
            None
        };
        let parent_accepted_view = accepted_view.clone();
        let content_stage = PalwStoreBatchStage::new(&self.palw_store);
        let mut accepted_leaves = Vec::new();
        let mut certified_batches = Vec::new();
        for merged in acceptance_data.iter() {
            // A pruning-point proof intentionally arrives without the pruning point's body. That exact
            // KeyNotFound is the sole normal skip. Acceptance data for any other merged block could only
            // have been produced from a body, so a missing/corrupt body here is a local store invariant
            // failure. Do not silently turn it into an inert PALW effect: two nodes observing the same
            // accepted transaction in different persistence states would then materialise different
            // content-addressed blobs.
            let txs = match self.block_transactions_store.get(merged.block_hash) {
                Ok(txs) => txs,
                Err(StoreError::KeyNotFound(_)) => {
                    let pruning_point = self.pruning_point_store.read().pruning_point().unwrap_or_else(|store_error| {
                        palw_overlay_commit_fail_stop(format!(
                            "PALW overlay commit could not classify a missing body for merged block {} while committing {current}: pruning-point read failed: {store_error}",
                            merged.block_hash
                        ))
                    });
                    if palw_body_may_be_pruned(merged.block_hash, pruning_point) {
                        trace!(
                            "PALW overlay commit: skipping bodyless pruning point {} while committing {current}",
                            merged.block_hash
                        );
                        continue;
                    }
                    palw_overlay_commit_fail_stop(format!(
                        "PALW overlay commit acceptance/body inconsistency while committing {current}: merged block {} is missing its body but current pruning point is {pruning_point}",
                        merged.block_hash
                    ));
                }
                Err(store_error) => palw_overlay_commit_fail_stop(format!(
                    "PALW overlay commit body read failed for merged block {} while committing {current}: {store_error}",
                    merged.block_hash
                )),
            };
            let carrier_daa = self.headers_store.get_daa_score(merged.block_hash).unwrap_or_else(|store_error| {
                palw_overlay_commit_fail_stop(format!(
                    "PALW accepted lifecycle could not read carrier DAA {} while committing {current}: {store_error}",
                    merged.block_hash
                ))
            });
            let carrier_epoch = carrier_daa / epoch_len;
            for entry in merged.accepted_transactions.iter() {
                let Some(tx) = txs.get(entry.index_within_block as usize) else {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW overlay commit acceptance/body index inconsistency while committing {current}: merged block {} has {} transactions but accepted transaction {} names index {}",
                        merged.block_hash,
                        txs.len(),
                        entry.transaction_id,
                        entry.index_within_block
                    ));
                };
                if tx.id() != entry.transaction_id {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW overlay commit acceptance/body id inconsistency while committing {current}: merged block {} index {} resolves to {} but acceptance data names {}",
                        merged.block_hash,
                        entry.index_within_block,
                        tx.id(),
                        entry.transaction_id
                    ));
                }
                let Some(kind) = tx.subnetwork_id.palw_tx_kind() else { continue };
                // Malformed/unhandled payloads and rejected §9.5 transitions are dropped: a PALW tx
                // that fails payload validity or the batch-state guard has no consensus effect here
                // (body-processing already screened well-formedness; this is the state-application
                // step). `parse`+`apply` are the same units exercised by `processes::palw` tests.
                if let Ok(effect) = crate::processes::palw::parse_palw_overlay(kind, &tx.payload) {
                    // Beacon effects use the fork-local, block-keyed accumulator below. Never
                    // dual-write them into the legacy epoch-global store: a side branch processed
                    // first could otherwise contaminate the selected chain's R_E.
                    if matches!(
                        &effect,
                        crate::processes::palw::PalwOverlayEffect::BeaconCommit(_)
                            | crate::processes::palw::PalwOverlayEffect::BeaconReveal(_)
                    ) {
                        continue;
                    }
                    // DA is a second, independent certificate prerequisite. It is evaluated against
                    // the selected-parent snapshot, never against challenges/responses or leaf chunks
                    // carried by this same acceptance set.
                    if !palw_da_certificate_effect_allowed(certificate_da_state, &effect) {
                        continue;
                    }
                    // V2 votes authenticate only an aggregate summary; they carry no per-ticket proof
                    // that a named leaf was not rejected. Until a future certificate version carries a
                    // bitmap witness, Header-v4 accepts only the canonical all-pass summary. V3 keeps
                    // its legacy partial-pass behavior unchanged.
                    if accepted_v4
                        && matches!(
                            &effect,
                            crate::processes::palw::PalwOverlayEffect::Certificate(certificate)
                                if !parent_accepted_view
                                    .as_ref()
                                    .and_then(|view| view.entry(&certificate.batch_id))
                                    .is_some_and(|entry| palw_v4_all_pass_summary(
                                        certificate.passed_leaf_count,
                                        certificate.rejected_leaf_bitmap_root,
                                        entry.leaf_count,
                                    ))
                        )
                    {
                        continue;
                    }
                    // Public/value Header-v4 admits only the self-contained DA object-v2 leaf schema.
                    // Object-v1 remains decodable for pre-v4 legacy networks, but it cannot enter a
                    // v4 fork's content-addressed leaf store or create DA obligations.
                    if current_header.version >= kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION
                        && matches!(
                            &effect,
                            crate::processes::palw::PalwOverlayEffect::LeafChunk(chunk)
                                if chunk.leaves.iter().any(|leaf| {
                                    leaf.receipt_da_object_version
                                        != kaspa_consensus_core::palw::da::PALW_RECEIPT_DA_OBJECT_VERSION_V2
                                })
                        )
                    {
                        continue;
                    }
                    // On v4, lifecycle and immutable content share one accepted-effect gate. Prepare
                    // the pure view transition first, but publish it only if the contextual content
                    // validation below also succeeds. Raw/non-accepted manifests, chunks and certs can
                    // therefore neither consume a slot nor pin a hash.
                    let mut transitioned_view = accepted_view.clone();
                    if let Some(view) = transitioned_view.as_mut() {
                        let lifecycle_allowed = match &effect {
                            crate::processes::palw::PalwOverlayEffect::Manifest(manifest) => view.apply_manifest(
                                manifest,
                                carrier_epoch,
                                self.palw_batch_admission.max_batch_leaves,
                                self.palw_batch_admission.max_leaf_chunk_leaves,
                                self.palw_batch_admission.registration_lead_epochs,
                                self.palw_batch_admission.active_window_epochs,
                                self.palw_batch_admission.audit_window_epochs,
                                self.palw_batch_admission.min_leaf_bond_sompi,
                                self.palw_batch_admission.max_view_batches,
                            ),
                            crate::processes::palw::PalwOverlayEffect::LeafChunk(chunk) => {
                                parent_accepted_view.as_ref().is_some_and(|parent| {
                                    palw_v4_parent_allows_leaf(parent, &chunk.batch_id)
                                        && parent.entry(&chunk.batch_id).is_some_and(|lifecycle| {
                                            super::utxo_validation::palw_v4_leaf_chunk_matches_canonical_span(
                                                chunk,
                                                lifecycle,
                                                self.palw_batch_admission.max_leaf_chunk_leaves,
                                            )
                                        })
                                }) && view.apply_leaf_chunk(&chunk.batch_id, chunk.chunk_index)
                            }
                            crate::processes::palw::PalwOverlayEffect::Certificate(certificate) => parent_accepted_view
                                .as_ref()
                                .is_some_and(|parent| palw_v4_parent_allows_certificate(parent, &certificate.batch_id)),
                            _ => true,
                        };
                        if !lifecycle_allowed {
                            continue;
                        }
                    }
                    let accepted_leaf_chunk = match &effect {
                        crate::processes::palw::PalwOverlayEffect::LeafChunk(chunk) => Some(chunk.leaves.clone()),
                        _ => None,
                    };
                    let certified_batch = match &effect {
                        crate::processes::palw::PalwOverlayEffect::Certificate(certificate) => {
                            Some((certificate.batch_id, certificate.hash(), certificate.approving_stake))
                        }
                        _ => None,
                    };
                    // kaspa-pq ADR-0040 §5.17 (AUTHSET-01 / SAMPLE-01 / SEL-01): a batch certificate is
                    // checked against the BEACON-SELECTED auditor committee (re-derived by the SEL-01
                    // weighted sampler over the selected-parent PROVIDER-bond view) and the beacon-selected
                    // on-chain leaf sample (re-derived `audit_sample_root`), plus real ML-DSA-87 vote
                    // signatures and stake-weighted quorum, before its blob may be persisted. Votes resolve
                    // against provider bonds, not DNS stake bonds — see the ctx doc for why this refines
                    // §5.17.2's scaffold wording.
                    //
                    // **Evaluated at the AUDIT-BEACON EPOCH's snapshot, not at inclusion time** (§5.17.2 /
                    // §12′). Eligibility freezes at selection: evaluating at this block's DAA would let an
                    // attacker holding a certificate include it just after an honest auditor's bond lapses,
                    // invalidating that vote. The epoch is the certificate's committed field, covered by
                    // every vote's `signing_hash`, so it cannot be re-aimed after the votes are collected.
                    //
                    // **The audit-epoch seed** `R_{audit_beacon_epoch − 1}` is resolved by the buried
                    // selected-parent walk (§5.17.3), a pure function of `(headers, reachability)`;
                    // unresolvable ⇒ the verifier FAILS CLOSED, kept sound by the bounded inclusion window.
                    // Resolved only for a Certificate (the only effect that carries `audit_beacon_epoch`).
                    let (snapshot_daa, prev_seed) = match &effect {
                        crate::processes::palw::PalwOverlayEffect::Certificate(c) => (
                            c.audit_beacon_epoch.saturating_mul(epoch_len),
                            crate::processes::palw::resolve_palw_audit_epoch_seed(
                                &self.headers_store,
                                &self.reachability_service,
                                selected_parent,
                                self.palw_activation_daa_score,
                                self.palw_epoch_length_daa,
                                c.audit_beacon_epoch,
                            ),
                        ),
                        _ => (cur_daa, None),
                    };
                    let attest = crate::processes::palw::PalwCertificateAttestationCtx {
                        network_id: self.palw_network_id,
                        pov_daa_score: snapshot_daa,
                        provider_bond_view: selected_parent_provider_bond_view,
                        prev_seed,
                        // The inclusion coordinate is the block that actually CARRIED the accepted
                        // certificate transaction. `current` may accept it from an older mergeset
                        // block after an epoch boundary; using `current` would make validity depend on
                        // merge delay rather than on immutable carrier provenance.
                        inclusion_epoch: carrier_epoch,
                        inclusion_window_epochs,
                        committee_size: self.palw_audit_committee_size as usize,
                        sample_size: self.palw_audit_sample_size as u32,
                        quorum_num: self.palw_audit_quorum_num,
                        quorum_den: self.palw_audit_quorum_den,
                    };
                    let apply_result = crate::processes::palw::apply_palw_overlay_effect(
                        effect.clone(),
                        &content_stage,
                        &self.palw_beacon_store,
                        Some(&attest),
                    );
                    // Payload/state-transition rejection is an inert overlay effect by design, but an
                    // infrastructure write failure is not a validity opinion. Continuing would commit
                    // StatusUTXOValid below while leaving this node without the blob a descendant may
                    // resolve. Fail-stop instead of silently manufacturing an arrival-order-dependent
                    // local state. This does not make the direct write crash-atomic with `batch` (the
                    // documented remaining blocker), but it closes the more dangerous "write failed,
                    // UTXO result committed anyway" half.
                    if matches!(apply_result, Err(crate::processes::palw::PalwOverlayError::StoreError)) {
                        palw_overlay_commit_fail_stop(format!(
                            "PALW overlay store operation failed while committing chain block {current} from merged block {} transaction {} at index {}",
                            merged.block_hash, entry.transaction_id, entry.index_within_block
                        ));
                    }
                    if apply_result.is_ok() {
                        if accepted_v4 {
                            match &effect {
                                crate::processes::palw::PalwOverlayEffect::Manifest(_)
                                | crate::processes::palw::PalwOverlayEffect::LeafChunk(_) => {
                                    accepted_view = transitioned_view;
                                }
                                crate::processes::palw::PalwOverlayEffect::Certificate(certificate) => {
                                    accepted_view.as_mut().expect("v4 view initialized").apply_verified_certificate(
                                        &certificate.batch_id,
                                        certificate.hash(),
                                        certificate.approving_stake,
                                        carrier_daa,
                                    );
                                }
                                _ => {}
                            }
                        }
                        if let Some(leaves) = accepted_leaf_chunk {
                            accepted_leaves.extend(leaves);
                        }
                        if let Some((batch_id, _, _)) = certified_batch {
                            certified_batches.push(batch_id);
                        }
                    }
                }
            }
        }

        for batch_id in certified_batches {
            da_state.remove_certified_batch(&batch_id);
        }

        // Register provider-specific sample obligations only for leaf chunks which passed the exact
        // same contextual/store gate above. The anchor is the first selected-parent ancestor buried
        // by the fixed policy and carrying a fork-local beacon state.
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        let buried_beacon = self.palw_da_buried_beacon(selected_parent, cur_daa);
        if let Some(beacon) = buried_beacon {
            for leaf in accepted_leaves {
                let commitment = PalwReceiptDaCommitmentV1 {
                    object_version: leaf.receipt_da_object_version,
                    object_len: leaf.receipt_da_object_len,
                    chunk_count: leaf.receipt_da_chunk_count,
                    root: leaf.receipt_da_root,
                };
                match da_state.register_leaf_obligations(&leaf, commitment, &beacon, &policy, cur_daa) {
                    Ok(_) | Err(kaspa_consensus_core::palw::da::PalwDaError::ObligationState) => {}
                    Err(kaspa_consensus_core::palw::da::PalwDaError::Capacity) => palw_overlay_commit_fail_stop(format!(
                        "PALW DA acceptance reservation diverged while committing leaf {}/{}: capacity was exhausted after acceptance",
                        leaf.batch_id, leaf.leaf_index
                    )),
                    Err(error) => palw_overlay_commit_fail_stop(format!(
                        "PALW DA obligation registration failed for accepted leaf {}/{}: {error}",
                        leaf.batch_id, leaf.leaf_index
                    )),
                }
            }
        }

        content_stage.stage_into(batch).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!("PALW accepted content staging failed for {current}: {store_error}"))
        });

        // Header-v4's authoritative lifecycle is derived solely from UTXO-accepted effects and is
        // persisted in this same commit as the accepted blobs. Legacy headers keep reading the raw
        // body-stage view, which is left byte-for-byte unchanged.
        let referenceable: BTreeSet<Hash64> = if let Some(mut view) = accepted_view {
            let could_activate = view.batches.values().any(|entry| {
                entry.status == kaspa_consensus_core::palw::PalwBatchStatus::Certified
                    && inclusion_epoch >= entry.activation_not_before_epoch
            });
            let activation_open = could_activate
                && self
                    .dns_params
                    .as_ref()
                    .and_then(|dns| {
                        crate::processes::palw::resolve_palw_lagged_anchor(
                            &self.headers_store,
                            &self.reachability_service,
                            dns,
                            selected_parent,
                        )
                    })
                    .map(|anchor| {
                        let samples = crate::processes::palw::resolve_palw_buried_epoch_seeds(
                            &self.headers_store,
                            &self.reachability_service,
                            anchor.anchor_hash,
                            self.palw_activation_daa_score,
                            self.palw_epoch_length_daa,
                            self.palw_beacon_grace_epochs.saturating_add(2),
                        );
                        kaspa_consensus_core::palw::palw_lagged_activation_open(&samples)
                    })
                    .unwrap_or(false);
            view.advance_epoch_gated(
                inclusion_epoch,
                self.palw_batch_admission.registration_lead_epochs,
                self.palw_batch_admission.audit_window_epochs,
                activation_open,
            );
            view.retain(
                inclusion_epoch,
                cur_daa,
                self.palw_batch_admission.registration_lead_epochs,
                self.palw_batch_admission.audit_window_epochs,
            );
            let referenceable = view.batches.keys().copied().collect();
            self.palw_overlay_view_store.set_batch(batch, current, Arc::new(view)).unwrap_or_else(|store_error| {
                palw_overlay_commit_fail_stop(format!("PALW accepted lifecycle staging failed for {current}: {store_error}"))
            });
            referenceable
        } else {
            // Lifecycle eviction is the terminal-history compaction authority for legacy views too.
            let view = self.palw_overlay_view_store.view(current).unwrap_or_else(|store_error| {
                palw_overlay_commit_fail_stop(format!("PALW DA could not read lifecycle view for {current}: {store_error}"))
            });
            let Some(view) = view else {
                palw_overlay_commit_fail_stop(format!("PALW DA lifecycle view is missing for active block {current}"));
            };
            view.batches.keys().copied().collect()
        };
        da_state.retain_referenceable_batches(&referenceable, inclusion_epoch);
    }

    /// ADR-0039 §11.2 — derive (or carry) this chain block's active beacon seed `R_E` and persist it
    /// keyed by `current` (block-keyed recurrence, like `reserve_balance`). Every block carries its
    /// selected parent's active state; the FIRST block of a new PALW epoch (its DAA epoch exceeds the
    /// parent's) re-derives the seed from the epoch's accumulated commits/reveals via
    /// [`derive_beacon_epoch_state`]. The write goes into the commit `WriteBatch` (atomic with the UTXO
    /// diff).
    ///
    /// **Inert on every currently activated preset** (`palw_activation_daa_score == u64::MAX`): the
    /// fast-path returns before any store read. On a PALW hard-fork/re-genesis network, commits and
    /// reveals are accumulated in a block-keyed selected-parent view, commit-time DNS stake is frozen,
    /// DNS health is recomputed from the parent's canonical attestation window, and the full resulting
    /// state is committed by Header-v3's `overlay_commitment_root` in descendants.
    /// ADR-0039 C6 SLICE 2 — derive this block's OWN beacon state `R_E`, as a pure, deterministic
    /// function reused by both the commit path (which persists it + advances the accumulator) and the
    /// UTXO-validation path (which authenticates `header.palw_beacon_seed` against it). Returns `None`
    /// when there is no state to derive (inert / pre-activation / the genesis block, whose accumulator
    /// the commit path seeds empty). Identical `(current, selected_parent_bond_view)` ⇒ identical result
    /// on every node — this is what makes the retained `palw_beacon_seed` field trustworthy, so a
    /// descendant may read a buried anchor's `R_E` from its header for the clause-9 draw. Reads the
    /// selected parent's accumulator + carried state, both present here (virtual stage, chain block).
    pub(super) fn derive_palw_beacon_state_value(
        &self,
        current: BlockHash,
        selected_parent_bond_view: &ActiveBondView,
    ) -> Option<kaspa_consensus_core::palw::PalwBeaconStateV1> {
        if self.palw_activation_daa_score == u64::MAX {
            return None; // inert fast path
        }
        let cur_daa = self.headers_store.get_daa_score(current).unwrap();
        // The genesis block seeds only an empty accumulator (no carried state to derive from).
        if cur_daa < self.palw_activation_daa_score || current == self.genesis.hash {
            return None;
        }
        let selected_parent = self.ghostdag_store.get_selected_parent(current).unwrap();
        self.derive_palw_beacon_state_core(cur_daa, selected_parent, current, selected_parent_bond_view)
    }

    /// The pure derivation body of [`Self::derive_palw_beacon_state_value`], with `(cur_daa,
    /// selected_parent)` supplied by the caller instead of resolved from a stored `current` block.
    /// Two callers share it, which is what makes `palw_beacon_seed` construction == validation:
    ///   - the validation/commit path calls `derive_palw_beacon_state_value(current, …)`, which reads
    ///     `cur_daa`/`selected_parent` off the already-stored block, then delegates here;
    ///   - the mining template (`build_block_template`) calls this directly with `virtual_state.daa_score`
    ///     and `virtual_state.ghostdag_data.selected_parent` — both known BEFORE the block (hence its
    ///     hash) exists, so it can stamp the derived seed into the header it is assembling.
    /// A block mined from that template has exactly those `(daa_score, selected_parent)` (GHOSTDAG is
    /// deterministic over the parent set), and the same selected-parent bond view, so the seed the
    /// template stamps equals the seed S2 validation re-derives. `current_label` is used only for panic
    /// messages. The sole entry points are both gated on `palw_activation_daa_score` (the template call
    /// is additionally behind `version >= PALW_HEADER_VERSION`), which makes this INERT on mainnet /
    /// testnet-10 / simnet / devnet — but NOT on `testnet-palw-110` / `devnet-palw-111`, whose fence is
    /// 0 (`config/params.rs:1403`, `:1454`), where it is reached on every chain block.
    pub(super) fn derive_palw_beacon_state_core(
        &self,
        cur_daa: u64,
        selected_parent: BlockHash,
        current_label: BlockHash,
        selected_parent_bond_view: &ActiveBondView,
    ) -> Option<kaspa_consensus_core::palw::PalwBeaconStateV1> {
        use crate::model::stores::palw_beacon::PalwBeaconAccumViewV1;
        use kaspa_consensus_core::palw::derive_beacon_epoch_state;
        let epoch_len = self.palw_epoch_length_daa.max(1);
        let sp_daa = self.headers_store.get_daa_score(selected_parent).unwrap_or(0);
        let epoch_cur = cur_daa / epoch_len;

        let parent_view = match self.palw_beacon_store.accum_view(selected_parent).unwrap() {
            Some(view) => (*view).clone(),
            None if sp_daa < self.palw_activation_daa_score || selected_parent == self.genesis.hash => PalwBeaconAccumViewV1::new(),
            None => panic!("missing fork-local PALW beacon accumulator for active selected parent {selected_parent}"),
        };
        let prev = self.palw_beacon_store.beacon_state(selected_parent).unwrap();
        if prev.as_ref().is_some_and(|s| s.epoch > epoch_cur) {
            panic!("PALW beacon epoch regressed at {current_label}: parent state is ahead of current DAA epoch");
        }

        // Derive on an epoch boundary (or the first PALW block, which has no carried parent state); else
        // carry the parent's active state forward unchanged. Crucially, derivation reads the PARENT
        // view before this block's effects are applied, so an R_E boundary block cannot influence R_E.
        //
        // A wide mergeset can advance DAA by more than one PALW epoch in a single block. The seed is a
        // hash chain (`R_E = f(R_{E-1}, …)`), so every skipped epoch MUST be replayed in ascending order
        // rather than jumping straight to `epoch_cur` — otherwise the intermediate epochs' accumulated
        // commits/reveals are silently dropped by `retain_future_of` below without ever entering the
        // chain, and the recurrence no longer matches §11.2. The replay reads each epoch's own inputs
        // and stake snapshot from the parent view (still intact here — `retain_future_of` runs only
        // after), and threads `seed`/`degraded_epochs` through each step so the grace recurrence is
        // likewise exact. The loop is bounded: a block's DAA increment is bounded by its mergeset, so
        // `epoch_cur - prev.epoch` is small.
        let state = if prev.as_ref().is_none_or(|s| epoch_cur > s.epoch) {
            let (newly_confirmed_anchor, dns_healthy) = self.palw_dns_confirmation(selected_parent, selected_parent_bond_view);
            // DNS confirmation is monotonic along this fork-local selected-parent chain. If the
            // newest lag-ready candidate has not accumulated both depths yet, retain the parent's
            // previously confirmed anchor rather than feeding an unconfirmed candidate into R_E.
            //
            // Carry rule (§12.1 clause-6 freeze, panel F3): the anchor FACTS are recomputed only when
            // the confirmed anchor ADVANCES; while it is unchanged the parent's frozen facts are
            // carried verbatim. The facts are anchor-pure (same anchor ⇒ same facts), so this is a
            // determinism-preserving optimization that also avoids re-reading the anchor header at
            // every boundary of a long-lived anchor.
            let dns_anchor = match (newly_confirmed_anchor, prev.as_ref()) {
                (Some(fresh), prev_state) if prev_state.is_none_or(|s| s.dns_anchor != fresh.hash) => fresh,
                (_, Some(s)) if s.dns_anchor != kaspa_hashes::Hash64::default() => s.anchor(),
                _ => kaspa_consensus_core::palw::BeaconDnsAnchor::UNCONFIRMED,
            };
            let dns_healthy = dns_healthy && dns_anchor.is_confirmed();
            // Every replayed epoch folds in THIS block's selected-parent DNS confirmation. A skipped
            // epoch has no distinct confirmation snapshot to recover (the anchor is only resolvable from
            // a block's own POV), and reusing one deterministic, already-lagged+confirmed anchor keeps
            // the chain identical on every node while granting no extra grinding freedom — the skipped
            // epochs' commit/reveal sets were fixed in the parent view before this block existed.
            let mut seed = prev.as_ref().map(|s| s.seed).unwrap_or_default();
            let mut degraded = prev.as_ref().map(|s| s.degraded_epochs).unwrap_or(0);
            // No carried state ⇒ this is the first PALW block: derive only its own epoch.
            let first_epoch = prev.as_ref().map(|s| s.epoch + 1).unwrap_or(epoch_cur);
            let mut replayed = None;
            for epoch in first_epoch..=epoch_cur {
                let step = derive_beacon_epoch_state(
                    epoch,
                    &seed,
                    &dns_anchor,
                    &parent_view.epoch_inputs(epoch),
                    dns_healthy,
                    degraded,
                    self.palw_beacon_grace_epochs,
                    self.palw_beacon_quorum_num,
                    self.palw_beacon_quorum_den,
                    |bond| parent_view.stake_of(epoch, bond),
                );
                seed = step.seed;
                degraded = step.degraded_epochs;
                replayed = Some(step);
            }
            replayed.expect("epoch range is non-empty: first_epoch <= epoch_cur")
        } else {
            (*prev.unwrap()).clone()
        };
        Some(state)
    }

    fn commit_palw_beacon_state(
        &self,
        batch: &mut WriteBatch,
        current: BlockHash,
        acceptance_data: &AcceptanceData,
        selected_parent_bond_view: &ActiveBondView,
    ) {
        use crate::model::stores::palw_beacon::PalwBeaconAccumViewV1;
        if self.palw_activation_daa_score == u64::MAX {
            return; // inert fast path
        }
        let cur_daa = self.headers_store.get_daa_score(current).unwrap();
        if cur_daa < self.palw_activation_daa_score {
            return;
        }

        // A genesis-active PALW re-genesis has no selected parent. Seed only the fork-local
        // accumulator here; the first child derives the initial epoch state from this empty view.
        if current == self.genesis.hash {
            self.palw_beacon_store.set_accum_view_batch(batch, current, Arc::new(PalwBeaconAccumViewV1::new())).unwrap();
            return;
        }

        let selected_parent = self.ghostdag_store.get_selected_parent(current).unwrap();
        let epoch_len = self.palw_epoch_length_daa.max(1);
        let sp_daa = self.headers_store.get_daa_score(selected_parent).unwrap_or(0);
        let epoch_cur = cur_daa / epoch_len;

        // The block's OWN beacon state (the same derivation UTXO validation authenticates the header
        // `palw_beacon_seed` against — construction == validation).
        let state = self
            .derive_palw_beacon_state_value(current, selected_parent_bond_view)
            .expect("a non-genesis active block has a derivable beacon state");
        self.palw_beacon_store.set_state_batch(batch, current, Arc::new(state)).unwrap();

        // Only after R_E is frozen do this block's accepted E-2/E-1 operations enter the child view.
        let mut next_view = match self.palw_beacon_store.accum_view(selected_parent).unwrap() {
            Some(view) => (*view).clone(),
            None if sp_daa < self.palw_activation_daa_score || selected_parent == self.genesis.hash => PalwBeaconAccumViewV1::new(),
            None => panic!("missing fork-local PALW beacon accumulator for active selected parent {selected_parent}"),
        };
        next_view.retain_future_of(epoch_cur);
        self.apply_palw_beacon_effects(&mut next_view, acceptance_data, selected_parent_bond_view, cur_daa);
        self.palw_beacon_store.set_accum_view_batch(batch, current, Arc::new(next_view)).unwrap();
    }

    /// Apply accepted beacon operations to a selected-parent-carried accumulator. Invalid contextual
    /// operations are fee-paying no-ops: they never enter R_E, but also do not invalidate an otherwise
    /// valid UTXO transaction. Every decision is past-relative to `selected_parent_bond_view`.
    fn apply_palw_beacon_effects(
        &self,
        view: &mut crate::model::stores::palw_beacon::PalwBeaconAccumViewV1,
        acceptance_data: &AcceptanceData,
        selected_parent_bond_view: &ActiveBondView,
        current_daa: u64,
    ) {
        use crate::processes::palw::PalwOverlayEffect;
        use kaspa_consensus_core::palw::PALW_BEACON_MLDSA87_CONTEXT;

        // §11.2 phase coordinate — FROZEN: the commit/reveal lead (`E-2` / `E-1`) is measured against the
        // UTXO **acceptance** epoch (this chain block's own DAA epoch), never the carrier block's.
        // Rationale, and why the alternative is not merely unimplemented but unsafe from here:
        //  * Determinism / c==v: the acceptance epoch is a function of THIS block's DAA score, so the
        //    template and validation paths derive it identically from the one selected-parent POV.
        //  * Carrier-block semantics would require a per-mergeset-source, block-keyed bond view AND
        //    outcome to validate each source's signature/bond at ITS own epoch — not obtainable from a
        //    single POV, so it cannot be made deterministic here.
        //  * Security is unaffected: `is_in_phase` pins `target == accept_epoch + lead` EXACTLY, so a
        //    withheld or early tx is dropped rather than retargeted. A miner choosing when to include a
        //    commit can therefore only censor it (a pre-existing, general property) — never re-aim it at
        //    a different epoch, so no grinding freedom is gained.
        //  * Consistency: identical coordinate to every other `acceptance_data`-driven overlay (DNS
        //    attestations, slashing).
        let current_epoch = current_daa / self.palw_epoch_length_daa.max(1);
        for merged in acceptance_data {
            let txs = self
                .block_transactions_store
                .get(merged.block_hash)
                .unwrap_or_else(|e| panic!("accepted PALW beacon body {} is unavailable: {e}", merged.block_hash));
            for entry in &merged.accepted_transactions {
                let tx = txs.get(entry.index_within_block as usize).unwrap_or_else(|| {
                    panic!("accepted PALW transaction index {} is outside block {}", entry.index_within_block, merged.block_hash)
                });
                let Some(kind) = tx.subnetwork_id.palw_tx_kind() else { continue };
                // Kinds outside this walker's jurisdiction (revocation, unbond, DA 0x3a-0x3c and
                // search 0x3d-0x3f lifecycles) have their own dedicated acceptance walkers — they are
                // SKIPPED here, never a decode panic: an accepted in-band tx of a foreign kind must
                // not be able to crash the beacon accumulator. A handled kind that fails to decode is
                // still a hard invariant violation (isolation admitted it by decoding it).
                let effect = match crate::processes::palw::parse_palw_overlay(kind, &tx.payload) {
                    Ok(effect) => effect,
                    Err(crate::processes::palw::PalwOverlayError::UnhandledSubnet(_)) => continue,
                    Err(e) => panic!("isolation-admitted PALW payload failed contextual decode: {e:?}"),
                };
                match effect {
                    PalwOverlayEffect::BeaconCommit(commit) => {
                        if !commit.is_in_phase(current_epoch) {
                            continue;
                        }
                        let Some(bond) = selected_parent_bond_view.active_bond_at(&commit.bond_outpoint, current_daa) else {
                            continue;
                        };
                        let digest = commit.signing_hash(self.palw_network_id);
                        if !matches!(
                            verify_mldsa87_with_context(
                                &bond.validator_pubkey,
                                &digest.as_bytes(),
                                &commit.signature,
                                PALW_BEACON_MLDSA87_CONTEXT,
                            ),
                            Ok(true)
                        ) {
                            continue;
                        }
                        view.record_commit(commit.epoch, commit.bond_outpoint, commit.commitment, bond.amount);
                    }
                    PalwOverlayEffect::BeaconReveal(reveal) => {
                        if !reveal.is_in_phase(current_epoch) {
                            continue;
                        }
                        let Some(bond) = selected_parent_bond_view.active_bond_at(&reveal.bond_outpoint, current_daa) else {
                            continue;
                        };
                        let Some(commitment) = view.commitment_of(reveal.epoch, &reveal.bond_outpoint) else {
                            continue;
                        };
                        if !reveal.matches_commit(&commitment) {
                            continue;
                        }
                        let digest = reveal.signing_hash(self.palw_network_id);
                        if !matches!(
                            verify_mldsa87_with_context(
                                &bond.validator_pubkey,
                                &digest.as_bytes(),
                                &reveal.signature,
                                PALW_BEACON_MLDSA87_CONTEXT,
                            ),
                            Ok(true)
                        ) {
                            continue;
                        }
                        view.record_valid_reveal(reveal.epoch, reveal.bond_outpoint, reveal.entropy_digest());
                    }
                    _ => {}
                }
            }
        }
    }

    /// Resolve the newest DNS-confirmed anchor and current health from the selected parent's own
    /// chain window and bond view. This deliberately does not read the virtual-tip `DnsState`
    /// singleton: both work depth and stake depth are re-derived at this block POV, and a lag-ready
    /// candidate is returned only after it clears the same two confirmation thresholds.
    fn palw_dns_confirmation(
        &self,
        selected_parent: BlockHash,
        selected_parent_bond_view: &ActiveBondView,
    ) -> (Option<kaspa_consensus_core::palw::BeaconDnsAnchor>, bool) {
        use kaspa_consensus_core::dns_finality::DnsHealth;

        let Some(params) = self.dns_params.as_ref() else { return (None, false) };
        let Some(candidate) = self.palw_lagged_dns_anchor_candidate(selected_parent) else { return (None, false) };
        let dns_anchor = candidate.anchor_hash;
        let sp_daa = self.headers_store.get_daa_score(selected_parent).unwrap();
        let bonds = selected_parent_bond_view.records();
        let active_stakes: Vec<u64> = bonds.iter().filter(|b| is_bond_active_at(b, sp_daa)).map(|b| b.amount).collect();
        let total_active = active_stakes.iter().fold(0u64, |sum, stake| sum.saturating_add(*stake));
        let active_validators = active_stakes.len() as u32;
        let hard_mandatory_active = sp_daa >= params.mandatory_attestation_inclusion_daa_score;
        let capacity = mandatory_attestation_mass_capacity(
            active_stakes.iter().copied(),
            total_active,
            0,
            params.stake_event_quality_floor_bps,
            self.max_block_mass,
            params.max_attestation_shard_mass,
        );
        let overlay_active = sp_daa >= params.dns_activation_daa_score
            && total_active >= params.min_active_stake_sompi
            && active_validators >= params.min_active_validators
            && params.dns_v3_params_consistent()
            && (!hard_mandatory_active || capacity.fits);
        if !overlay_active {
            return (None, false);
        }

        let (contributions, epoch_anchor_daa, _) =
            self.collect_stake_contributions_v2(selected_parent, None, &bonds, self.genesis.hash.as_byte_slice(), params);
        let totals = total_active_stake_by_epoch(&bonds, &epoch_anchor_daa);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        let stake_depth = compute_stake_score(&per_epoch, params.stake_event_quality_floor_bps);
        // Both hashes are reachable selected-chain blocks. Missing/corrupt work is a consensus DB
        // failure, never zero: defaulting the anchor to zero would inflate work depth and let one
        // node alone classify an unconfirmed candidate as DNS-confirmed inside the v3 commitment.
        let selected_parent_work = self
            .ghostdag_store
            .get_blue_work(selected_parent)
            .unwrap_or_else(|err| panic!("failed reading blue work for PALW selected parent {selected_parent}: {err}"));
        let anchor_work = self
            .ghostdag_store
            .get_blue_work(dns_anchor)
            .unwrap_or_else(|err| panic!("failed reading blue work for PALW DNS anchor {dns_anchor}: {err}"));
        let work_depth = selected_parent_work.saturating_sub(anchor_work);
        let confirmed = is_dns_confirmed(work_depth, stake_depth, params.required_work_depth, params.required_stake_depth);
        let healthy = derive_dns_health(
            &per_epoch,
            params.stake_event_quality_floor_bps,
            params.stake_censorship_floor_bps,
            params.degraded_stake_quality_epochs,
            true,
        ) == DnsHealth::Active;
        // §12.1 clause-6 facts: every field is a frozen property of the ANCHOR BLOCK itself (its own
        // header-committed coordinates + overlay root), never of the boundary-time view — so the
        // certificate digest cannot be ground by boundary producers (panel F1/F2). The anchor header
        // is a confirmed selected-chain ancestor within the lag window, so it is retained.
        let facts = confirmed.then(|| {
            let anchor_header = self
                .headers_store
                .get_header(dns_anchor)
                .unwrap_or_else(|err| panic!("failed reading header for confirmed PALW DNS anchor {dns_anchor}: {err}"));
            kaspa_consensus_core::palw::BeaconDnsAnchor {
                hash: dns_anchor,
                blue_score: candidate.anchor_blue_score,
                daa_score: candidate.anchor_daa_score,
                overlay_root: anchor_header.overlay_commitment_root,
            }
        });
        (facts, healthy)
    }

    /// The newest lag-ready DNS anchor candidate as-of `selected_parent`. This is not itself a
    /// confirmation decision; [`Self::palw_dns_confirmation`] additionally requires both work and
    /// stake depth before the candidate may enter the PALW beacon recurrence.
    fn palw_lagged_dns_anchor_candidate(&self, selected_parent: BlockHash) -> Option<CanonicalLaggedEpochAnchor> {
        let dns_params = self.dns_params.as_ref()?;
        let sp_blue = self.headers_store.get_blue_score(selected_parent).ok()?;
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let lag = dns_params.attestation_lag_blue_score;
        let dns_epoch = kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score(sp_blue, epoch_len, lag)?;
        // Forward the FULL anchor record — clause 6's certificate digest needs the anchor's own
        // coordinates, not just its hash (panel Q3/storage-(ii) resolution).
        self.canonical_anchor_by_blue_score(dns_epoch, selected_parent, dns_params)
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
        // ADR-0040 ECON-03 (THE WIRE): the provider-bond registry as-of the virtual selected parent.
        selected_parent_provider_bond_view: &ProviderBondView,
        chain_path: &ChainPath,
    ) -> Result<Arc<VirtualState>, RuleError> {
        let new_virtual_state = self.calculate_virtual_state(
            &virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            selected_parent_multiset,
            accumulated_diff,
            selected_parent_bond_view,
            selected_parent_provider_bond_view,
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
        // ADR-0040 ECON-03 (THE WIRE): forwarded to `calculate_utxo_state` so a virtual-state recompute
        // classifies provider collateral from the identical view a block validation does.
        selected_parent_provider_bond_view: &ProviderBondView,
    ) -> Result<Arc<VirtualState>, RuleError> {
        let selected_parent_utxo_view = (&virtual_stores.utxo_set).compose(&*accumulated_diff);
        let mut ctx = UtxoProcessingContext::new((&virtual_ghostdag_data).into(), selected_parent_multiset);

        // Calc virtual DAA score, difficulty bits and past median time
        let virtual_daa_window = self.window_manager.block_daa_window(&virtual_ghostdag_data)?;
        let virtual_bits = self.window_manager.calculate_difficulty_bits(&virtual_ghostdag_data, &virtual_daa_window);
        let virtual_past_median_time = self.window_manager.calc_past_median_time(&virtual_ghostdag_data)?.0;

        // Calc virtual UTXO state relative to selected parent
        self.calculate_utxo_state(
            &mut ctx,
            &selected_parent_utxo_view,
            selected_parent_bond_view,
            selected_parent_provider_bond_view,
            virtual_daa_window.daa_score,
        );

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

    /// Keep the §8 log posting index (prefix 205) equal to the CANONICAL chain.
    ///
    /// Postings used to be written for every UTXO-valid block, with the query
    /// canonical-filtering each one at read time. Three costs followed: the index
    /// was strictly larger than the chain it serves (one log yields up to five
    /// postings, so it is the lane's biggest multiplier), every range query paid a
    /// filter, and a side-branch posting was unreachable from the canonical number
    /// map the pruner walks — so it could never be reclaimed at all.
    ///
    /// Detach before attach, mirroring the number map: a block both removed and
    /// re-added in one reorg ends attached, because the batch applies the deletes
    /// and then the writes.
    ///
    /// Postings are derived from the block's receipts, exactly as the writer used
    /// to derive them, so attach and detach are inverses by construction rather
    /// than by two code paths agreeing.
    fn update_evm_canonical_log_index(&self, batch: &mut WriteBatch, chain_path: &ChainPath) {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmReceiptsStoreReader};
        use kaspa_consensus_core::evm::EvmPruneSegment;

        if self.evm_activation_daa_score == u64::MAX || !self.evm_retention_policy.writes(EvmPruneSegment::LogPostings) {
            return;
        }

        let postings_of = |block: BlockHash| {
            let header = self.evm_header_store.get(block).optional().ok().flatten()?;
            let receipts = self.evm_receipts_store.get(block).ok()?;
            Some(crate::processes::evm::pruner::derive_log_postings(&receipts, header.evm_number, block))
        };

        for removed in chain_path.removed.iter().rev().copied() {
            let Some(postings) = postings_of(removed) else { continue };
            for (kind, selector, loc) in &postings {
                if let Err(e) = self.evm_log_index_store.delete_posting_batch(batch, *kind, selector, loc) {
                    warn!("[evm-log-index] detaching postings of {removed} failed: {e}");
                }
            }
        }
        for added in chain_path.added.iter().copied() {
            let Some(postings) = postings_of(added) else { continue };
            for (kind, selector, loc) in &postings {
                if let Err(e) = self.evm_log_index_store.write_posting_batch(batch, *kind, selector, loc) {
                    warn!("[evm-log-index] attaching postings of {added} failed: {e}");
                }
            }
            // The completeness floor is a property of the CANONICAL index, so it
            // moves here rather than at per-block commit.
            if let Ok(header) = self.evm_header_store.get(added)
                && let Err(e) = self.evm_log_index_store.set_floor_batch(batch, header.evm_number)
            {
                warn!("[evm-log-index] floor update for {added} failed: {e}");
            }
        }
    }

    /// §12.3 v2: write a sparse, compressed, code-free reconstruction ANCHOR at the
    /// new sink when the cadence is due.
    ///
    /// Three deliberate differences from the v1 checkpoint this replaces:
    ///
    /// * **Canonical only.** Called from `commit_virtual_state` with the selected
    ///   sink, so a UTXO-valid block that loses the sink search is never anchored.
    /// * **Paced in time, capped in blocks.** `evm_number % 2048` is 3.4 minutes at
    ///   10 BPS and 34 at 1 BPS; the same constant meant wildly different things as
    ///   the BPS moved, and at 10 BPS it reproduced the whole state hundreds of
    ///   times a day.
    /// * **Bounded.** Old anchors are evicted, so the store plateaus instead of
    ///   growing with the chain.
    ///
    /// Never fatal: an anchor is reconstruction convenience, and failing to write
    /// one must not stop the virtual state from committing. A missed anchor only
    /// lengthens the diff replay for a later historical query.
    #[cfg(feature = "evm")]
    fn stage_evm_checkpoint_anchor(&self, batch: &mut WriteBatch, sink: BlockHash) {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmStateCheckpointV2StoreReader, EvmStateStoreReader};
        use kaspa_consensus_core::evm::EvmStateCheckpointV2;

        if self.evm_activation_daa_score == u64::MAX || !self.evm_history_mode.writes_state_history() {
            return;
        }
        let policy = self.evm_checkpoint_policy;
        let Some(header) = self.evm_header_store.get(sink).optional().unwrap_or(None) else {
            return; // pre-activation sink: nothing to anchor
        };
        let timestamp_ms = match self.headers_store.get_timestamp(sink) {
            Ok(t) => t,
            Err(e) => {
                warn!("[evm-anchor] header timestamp read failed for {sink}: {e}; skipping this anchor");
                return;
            }
        };
        let mut meta = match self.evm_checkpoint_meta_store.read().get() {
            Ok(m) => m,
            Err(e) => {
                warn!("[evm-anchor] cadence state read failed: {e}; skipping this anchor");
                return;
            }
        };
        // NOT a pruning-point anchor. The sink is the chain tip and the pruning point
        // is far behind it, so comparing them here — as an earlier version did — is
        // dead code that never fires. The pruning-point anchor is written by the
        // pruning processor immediately before it deletes, which is the last moment
        // the material to build one exists; this path only handles the time-paced
        // cadence at the tip.
        if !policy.is_due(&meta, header.evm_number, timestamp_ms, false) {
            return;
        }
        if EvmStateCheckpointV2StoreReader::has(&*self.evm_state_checkpoint_v2_store, sink).unwrap_or(false) {
            return; // already anchored (a reorg returning to a previous sink)
        }

        // Where the state comes from is decided by WHICH BACKEND IS AUTHORITATIVE,
        // not by whether the result looks empty. An EVM lane with no accounts yet has
        // an empty state, and that is a state worth anchoring — treating "empty" as
        // "no source" would silently skip every anchor on a quiet chain and only show
        // up much later as an unreconstructable range.
        let snapshot = if self.evm_flat_authoritative {
            match crate::processes::evm::materialize_snapshot(&self.evm_flat_account_store, &self.evm_code_store) {
                Ok(snap) => snap,
                Err(e) => {
                    warn!("[evm-anchor] flat materialize failed at {sink}: {e}; skipping this anchor");
                    return;
                }
            }
        } else {
            // 206 is authoritative here. Absent means this block committed no state
            // row (pre-activation, or already pruned), so there is nothing to anchor.
            match self.evm_state_store.get(sink).optional() {
                Ok(Some(snap)) => snap,
                Ok(None) => return,
                Err(e) => {
                    warn!("[evm-anchor] 206 read failed at {sink}: {e}; skipping this anchor");
                    return;
                }
            }
        };

        let checkpoint = EvmStateCheckpointV2::build(sink, header.evm_number, header.state_root, &snapshot, policy.codec);
        let stored = checkpoint.stored_len();
        let raw = checkpoint.uncompressed_len;
        if let Err(e) = self.evm_state_checkpoint_v2_store.insert_batch(batch, sink, checkpoint) {
            warn!("[evm-anchor] anchor write failed for {sink}: {e}");
            return;
        }
        // Evicting is what makes this a bounded store rather than a slower-growing one.
        // The PRUNING POINT's anchor is exempt: it is the floor every future
        // reconstruction and every IBD we serve starts from, and the cadence knows
        // nothing about that role.
        let pruning_point = self.pruning_point_store.read().pruning_point().ok();
        for evicted in policy.record(&mut meta, sink, header.evm_number, timestamp_ms) {
            if Some(evicted) == pruning_point {
                continue;
            }
            if let Err(e) = self.evm_state_checkpoint_v2_store.delete_batch(batch, evicted) {
                warn!("[evm-anchor] evicting anchor {evicted} failed: {e}");
            }
        }
        if let Err(e) = self.evm_checkpoint_meta_store.write().set_batch(batch, meta) {
            warn!("[evm-anchor] cadence state write failed: {e}");
            return;
        }
        info!(
            "[evm-anchor] state anchor at EVM #{} ({sink}): {stored} B stored / {raw} B raw, codec={}",
            header.evm_number,
            policy.codec.as_str()
        );
    }

    /// Non-`evm` builds have no EVM state to anchor.
    #[cfg(not(feature = "evm"))]
    fn stage_evm_checkpoint_anchor(&self, _batch: &mut WriteBatch, _sink: BlockHash) {}

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
        let staged_dns_bonds = self.stage_dns_bond_mutations(&mut batch, chain_path, dns_sink);
        // ADR-0040 ECON-03 (THE WIRE): the PALW provider-bond registry moves in the SAME batch and by
        // the same detach-before-attach rule, so the persisted registry and the selected chain can
        // never be committed out of step. Inert while PALW is fenced.
        self.stage_palw_provider_bond_mutations(&mut batch, chain_path);

        // kaspa-pq Phase 10 (ADR-0009 A.5): recompute the DNS StakeScore over
        // the bounded recent epoch window and stage the updated DnsState into
        // the same batch. Inert unless the overlay is configured.
        self.update_dns_state(&mut batch, dns_sink, staged_dns_bonds.as_deref());

        // kaspa-pq EVM Lane v0.4 (§10 / invariant I3): a virtual change only
        // MOVES the canonical EVM head pointers — no execution happens here.
        self.update_evm_canonical_heads(&mut batch, dns_sink);

        // kaspa-pq EVM Lane v0.4 (§16 RPC / canonical-index fix): the canonical
        // `evm_number → L1 hash` map follows the selected chain (detach/attach),
        // not per-block result-commit — so a sink-search loser can't shadow it.
        self.update_evm_canonical_number_map(&mut batch, chain_path);

        // The log posting index follows the same detach/attach discipline, for the
        // same reason: written per-block it covered side branches too, which made
        // it larger than the chain it indexes and left postings no pruner could
        // reach.
        self.update_evm_canonical_log_index(&mut batch, chain_path);

        // §12.3 v2: write a state-history ANCHOR if the cadence says one is due.
        // Here, not in `commit_utxo_state`, for the same reason as the number map
        // above: that path also runs for sink-search losers, so anchoring there
        // reproduced the entire EVM state for side branches.
        self.stage_evm_checkpoint_anchor(&mut batch, dns_sink);

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

    /// kaspa-pq **ADR-0040 ECON-03 (THE WIRE) — the registry WRITER for prefix 241.**
    ///
    /// Stages the `PalwProviderBond`-store mutations implied by this selected-chain change into
    /// `batch`, so they commit atomically with the virtual state. Transposed from
    /// [`Self::stage_dns_bond_mutations`], with the same detach-before-attach order: blocks LEAVING
    /// the selected chain are reverted (most-recent first, and within a block in reverse tx order)
    /// BEFORE blocks joining it are applied.
    ///
    /// **Order independence.** Apply and revert are exact inverses here for the same structural reason
    /// they are in [`ProviderBondView`]: `Insert` reverts by delete, `Unbond`/`Slash` revert by
    /// clearing the single DAA stamp they set, and there is no mutable `status` field to restore to a
    /// guessed value (the DNS registry now re-derives its legacy cached status after each mutation,
    /// while this record type needs no cache at all — see `PalwProviderBondRecord`). Two nodes
    /// reaching the same sink by different reorg paths therefore hold byte-identical rows, which is
    /// what makes it safe for the reward path to read a view seeded from here.
    ///
    /// Inert while PALW is fenced (`palw_activation_daa_score == u64::MAX` on mainnet, testnet-10,
    /// simnet and devnet). The three PALW presets use an activation score of 0 and process
    /// provider-bond (`0x30`) and provider-unbond (`0x37`) transactions.
    fn stage_palw_provider_bond_mutations(&self, batch: &mut WriteBatch, chain_path: &ChainPath) {
        if self.palw_activation_daa_score == u64::MAX {
            return;
        }
        let mut store = self.palw_provider_bonds_store.write();
        let mut working = HashMap::new();
        for result in store.iterator() {
            let (outpoint, record) = result.unwrap_or_else(|store_error| {
                palw_overlay_commit_fail_stop(format!(
                    "PALW provider-registry read failed while staging a selected-chain change: {store_error}"
                ))
            });
            if working.insert(outpoint, (*record).clone()).is_some() {
                palw_overlay_commit_fail_stop(format!(
                    "PALW provider-registry iterator returned duplicate outpoint {outpoint} while staging a selected-chain change"
                ));
            }
        }

        let removed: Vec<_> = chain_path
            .removed
            .iter()
            .copied()
            .map(|block| (block, self.palw_provider_bond_mutations_for_chain_block(block)))
            .collect();
        let added: Vec<_> =
            chain_path.added.iter().copied().map(|block| (block, self.palw_provider_bond_mutations_for_chain_block(block))).collect();
        let touched = reconcile_palw_provider_registry(&mut working, &removed, &added).unwrap_or_else(|invariant_error| {
            palw_overlay_commit_fail_stop(format!("PALW provider-registry selected-chain reconciliation failed: {invariant_error}"))
        });

        // Write only each touched outpoint's FINAL row. This avoids depending on whether the store
        // cache exposes earlier operations queued in this same RocksDB WriteBatch.
        for outpoint in touched {
            if let Some(record) = working.remove(&outpoint) {
                store.insert_batch(batch, outpoint, Arc::new(record)).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW provider-registry could not stage final row for {outpoint}: {store_error}"
                    ))
                });
            } else {
                store.delete_batch(batch, outpoint).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!(
                        "PALW provider-registry could not stage final deletion for {outpoint}: {store_error}"
                    ))
                });
            }
        }
    }

    fn stage_dns_bond_mutations(
        &self,
        batch: &mut WriteBatch,
        chain_path: &ChainPath,
        new_sink: BlockHash,
    ) -> Option<Vec<StakeBondRecord>> {
        self.dns_params.as_ref()?;
        let mut store = self.stake_bonds_store.write();
        let mut working = HashMap::new();
        for result in store.iterator() {
            let (outpoint, record) = result.unwrap_or_else(|store_error| {
                palw_overlay_commit_fail_stop(format!(
                    "DNS bond-registry read failed while staging a selected-chain change: {store_error}"
                ))
            });
            if working.insert(outpoint, (*record).clone()).is_some() {
                palw_overlay_commit_fail_stop(format!(
                    "DNS bond-registry iterator returned duplicate outpoint {outpoint} while staging a selected-chain change"
                ));
            }
        }

        let removed: Vec<_> =
            chain_path.removed.iter().copied().map(|block| (block, self.dns_bond_mutations_for_chain_block(block))).collect();
        let added: Vec<_> =
            chain_path.added.iter().copied().map(|block| (block, self.dns_bond_mutations_for_chain_block(block))).collect();
        let mut touched = reconcile_dns_bond_registry(&mut working, &removed, &added).unwrap_or_else(|invariant_error| {
            palw_overlay_commit_fail_stop(format!("DNS bond-registry selected-chain reconciliation failed: {invariant_error}"))
        });
        let sink_daa_score = self.headers_store.get_daa_score(new_sink).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!(
                "DNS bond-registry could not resolve new sink {new_sink} while canonicalizing status: {store_error}"
            ))
        });
        touched.extend(canonicalize_dns_bond_statuses(&mut working, sink_daa_score));

        for outpoint in touched {
            if let Some(record) = working.get(&outpoint) {
                store.insert_batch(batch, outpoint, Arc::new(record.clone())).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!("DNS bond-registry could not stage final row for {outpoint}: {store_error}"))
                });
            } else {
                store.delete_batch(batch, outpoint).unwrap_or_else(|store_error| {
                    palw_overlay_commit_fail_stop(format!(
                        "DNS bond-registry could not stage final deletion for {outpoint}: {store_error}"
                    ))
                });
            }
        }

        // The DNS recompute runs before this WriteBatch commits. Return the exact post-reorg view so
        // it never re-reads stale RocksDB rows and overwrites the mutations staged above.
        let mut records: Vec<_> = working.into_values().collect();
        records.sort_by(|a, b| {
            (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
        });
        Some(records)
    }

    /// Seeds the per-block [`ActiveBondView`] walk (ADR-0009 Addendum B §B.1)
    /// from the `StakeBonds` store snapshot — which, at the start of
    /// `resolve_virtual`, reflects the bond set as-of the previous sink (the
    /// same anchor `accumulated_diff` starts from). Returns an empty view on
    /// networks without the overlay (`dns_params` is `None`), so the bond-view
    /// walk is a no-op there.
    pub(crate) fn initial_active_bond_view(&self) -> ActiveBondView {
        if self.dns_params.is_none() {
            return ActiveBondView::new();
        }
        let mut records = Vec::new();
        for result in self.stake_bonds_store.read().iterator() {
            let (_, record) = result.unwrap_or_else(|store_error| {
                palw_overlay_commit_fail_stop(format!("DNS bond-registry read failed while building the active view: {store_error}"))
            });
            records.push((record.bond_outpoint, (*record).clone()));
        }
        ActiveBondView::from_records(records)
    }

    /// kaspa-pq **ADR-0040 ECON-03 (THE WIRE) — where provider-collateral resolution lives, and why.**
    ///
    /// Seeds the per-block [`ProviderBondView`] walk from the `PalwProviderBond` store (prefix 241),
    /// which at the start of `resolve_virtual` reflects the registry as-of the previous sink — the
    /// same anchor `accumulated_diff` and the DNS `ActiveBondView` start from.
    ///
    /// ## The coordinate decision (stated, per the ECON-03 brief)
    ///
    /// Resolution needs a POINT OF VIEW: "is this bond active?" is a question about a DAA score along
    /// a particular chain. There were two candidate coordinates and only one is admissible:
    ///
    /// * **Body / mergeset** (`validate_public_leaf`, where `provider_a_bond != provider_b_bond` is
    ///   checked today) — REJECTED. That validator is context-free by construction; BIND-03 settled
    ///   that the batch view stays at the body coordinate and that body validity must not read
    ///   point-of-view state. Resolving there would also make leaf admission depend on a bond's live
    ///   status, so an unbond timed by a third party could invalidate an already-accepted batch.
    /// * **Reward / virtual** (`palw_work_reward_class`, in `calculate_utxo_state`) — CHOSEN. It
    ///   already reads the leaf, it already has `pov_daa_score`, acceptance data lives here, and it is
    ///   the single seam shared by coinbase construction and validation, so c == v holds structurally
    ///   rather than by two matching copies. It is also the coordinate where the failure mode is
    ///   proportionate: withholding a payout, not bricking a block.
    ///
    /// So `validate_public_leaf` KEEPS its distinctness check — a pure shape rule, and still the only
    /// property it can assert; resolution is layered on top at the reward coordinate.
    /// The distinctness check is no longer load-bearing for the economics; it merely stops a leaf from
    /// naming one bond twice, which would otherwise let a single bond back both halves of a pair.
    ///
    /// Returns an empty view while PALW is fenced (`palw_activation_daa_score == u64::MAX`: mainnet,
    /// testnet-10, simnet, devnet), so the walk is a no-op and every reward is byte-identical there.
    pub(crate) fn initial_palw_provider_bond_view(&self) -> ProviderBondView {
        if self.palw_activation_daa_score == u64::MAX {
            return ProviderBondView::new();
        }
        let store = self.palw_provider_bonds_store.read();
        ProviderBondView::from_records(store.iterator().map(|result| match result {
            Ok((op, record)) => (op, (*record).clone()),
            Err(store_error) => palw_overlay_commit_fail_stop(format!(
                "PALW provider-registry read failed while seeding the virtual view: {store_error}"
            )),
        }))
    }

    /// The per-provider-bond acceptance floors — `(min_provider_bond_sompi, provider_unbond_floor_epochs)`
    /// from `palw_batch_admission`. Both are `is_consistent_for_activation`-enforced non-zero on every
    /// activated preset, so neither floor is vacuous where it runs.
    pub(super) fn palw_provider_bond_floors(&self) -> (u64, u64) {
        (self.palw_batch_admission.min_provider_bond_sompi, self.palw_batch_admission.provider_unbond_floor_epochs)
    }

    /// Re-derives the [`PalwProviderBondMutation`]s a chain block contributed, from its retained
    /// acceptance data. Deterministic, so it serves both apply (added) and revert (removed) — the
    /// exact shape of [`Self::dns_bond_mutations_for_chain_block`].
    fn palw_provider_bond_mutations_for_chain_block(&self, chain_block: BlockHash) -> Vec<PalwProviderBondMutation> {
        let accepted_daa_score = self.headers_store.get_header(chain_block).unwrap().daa_score;
        // Per-block fence, matching the leg-5 authorizer's call site EXACTLY. Without it a net with a
        // FINITE, non-zero `palw_activation_daa_score` would write registry rows for blocks below the
        // fence while the authorizer declined to check their `0x37` transactions — i.e. unauthenticated
        // unbonds in the pre-activation window. Unreachable on the six shipped presets (each is either
        // `u64::MAX`, where the caller already returned, or 0, where every block is at/above), which is
        // exactly why it would have gone unnoticed. The writer and the verifier must share one gate.
        if accepted_daa_score < self.palw_activation_daa_score {
            return Vec::new();
        }
        let (min_bond, unbond_floor) = self.palw_provider_bond_floors();
        let mut mutations = palw_provider_bond_mutations_from_accepted_txs(
            &self.accepted_txs_of_chain_block_for_registry(chain_block, "PALW provider-bond"),
            accepted_daa_score,
            min_bond,
            unbond_floor,
        );
        let da_state = self.palw_da_store.read().state(chain_block).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!(
                "PALW provider-registry could not read DA slash delta for chain block {chain_block}: {store_error}"
            ))
        });
        if !da_state.validate_structure() {
            palw_overlay_commit_fail_stop(format!("PALW DA state is invalid while deriving registry mutations for {chain_block}"));
        }
        mutations.extend(
            da_state
                .block_slashed_providers
                .iter()
                .copied()
                .map(|provider_bond| PalwProviderBondMutation::Slash(provider_bond, accepted_daa_score)),
        );
        // Search-availability timeout slashes ride the identical per-block delta contract, from the
        // sibling store. Staging guarantees the two deltas never name the same bond in one block.
        let search_state = self.palw_search_availability_store.read().state(chain_block).unwrap_or_else(|store_error| {
            palw_overlay_commit_fail_stop(format!(
                "PALW provider-registry could not read search slash delta for chain block {chain_block}: {store_error}"
            ))
        });
        if !search_state.validate_structure() {
            palw_overlay_commit_fail_stop(format!("PALW search state is invalid while deriving registry mutations for {chain_block}"));
        }
        mutations.extend(
            search_state
                .block_slashed_schedulers
                .iter()
                .copied()
                .map(|scheduler_bond| PalwProviderBondMutation::Slash(scheduler_bond, accepted_daa_score)),
        );
        mutations
    }

    /// [`PalwProviderBondMutation`]s for a block whose acceptance data is still in memory (the block
    /// currently being UTXO-validated, before its `acceptance_data_store` entry is committed).
    fn palw_provider_bond_mutations_from_acceptance(
        &self,
        acceptance_data: &AcceptanceData,
        accepted_daa_score: u64,
    ) -> Vec<PalwProviderBondMutation> {
        // Same per-block fence as `palw_provider_bond_mutations_for_chain_block`, so the in-memory walk
        // and the persisted registry cannot disagree about which blocks contribute.
        if accepted_daa_score < self.palw_activation_daa_score {
            return Vec::new();
        }
        let (min_bond, unbond_floor) = self.palw_provider_bond_floors();
        palw_provider_bond_mutations_from_accepted_txs(
            &self.accepted_txs_from_acceptance_data(acceptance_data),
            accepted_daa_score,
            min_bond,
            unbond_floor,
        )
    }

    /// Re-derives the [`BondMutation`]s a chain block contributed, from its
    /// retained acceptance data (ADR-0009 Addendum A.4). Deterministic, so it
    /// serves both apply (added) and revert (removed).
    fn dns_bond_mutations_for_chain_block(&self, chain_block: BlockHash) -> Vec<BondMutation> {
        let accepted_daa_score = self.headers_store.get_header(chain_block).unwrap().daa_score;
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        bond_mutations_from_accepted_txs(
            &self.accepted_txs_of_chain_block_for_registry(chain_block, "DNS bond"),
            accepted_daa_score,
            min_bond,
            unbonding_floor,
        )
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
    fn accepted_txs_of_chain_block(&self, chain_block: BlockHash) -> Vec<Transaction> {
        match self.acceptance_data_store.get(chain_block) {
            // TOLERANT: a chain block being walked here may be at or below the
            // pruning point, so its mergeset can reference blocks whose bodies were
            // pruned. Skip those rather than panicking — the sibling already
            // tolerates the acceptance data itself being absent, and this closes the
            // same gap one level down (the block-transactions store). Pruning walks
            // pruned blocks by construction, so a pruned body means "no accepted txs
            // resolvable here", never a fault.
            Ok(ad) => self.accepted_txs_from_acceptance_data_impl(&ad, true),
            Err(StoreError::KeyNotFound(_)) => {
                trace!(
                    "accepted_txs_of_chain_block: no acceptance data for {chain_block} (pruning point / pruned) — treating as no accepted txs"
                );
                Vec::new()
            }
            Err(e) => panic!("accepted_txs_of_chain_block: acceptance_data_store.get({chain_block}) failed: {e}"),
        }
    }

    /// Registry-writer variant of [`Self::accepted_txs_of_chain_block`]. Missing acceptance data is
    /// expected only for the current imported pruning point; treating any other missing row as an
    /// empty mutation set would silently commit an incomplete collateral registry. Store failures
    /// therefore terminate the process before the selected-chain WriteBatch can commit.
    fn accepted_txs_of_chain_block_for_registry(&self, chain_block: BlockHash, registry: &str) -> Vec<Transaction> {
        match self.acceptance_data_store.get(chain_block) {
            Ok(acceptance_data) => self.accepted_txs_from_acceptance_data(&acceptance_data),
            Err(StoreError::KeyNotFound(_)) => {
                let pruning_point = self
                    .pruning_point_store
                    .read()
                    .pruning_point()
                    .optional()
                    .unwrap_or_else(|store_error| {
                        palw_overlay_commit_fail_stop(format!(
                            "{registry} registry could not resolve the pruning point after missing acceptance data for {chain_block}: {store_error}"
                        ))
                    });
                if pruning_point == Some(chain_block) {
                    Vec::new()
                } else {
                    palw_overlay_commit_fail_stop(format!(
                        "{registry} registry acceptance data is missing for non-pruning-point block {chain_block} (current pruning point {pruning_point:?})"
                    ))
                }
            }
            Err(store_error) => palw_overlay_commit_fail_stop(format!(
                "{registry} registry acceptance-data read failed for block {chain_block}: {store_error}"
            )),
        }
    }

    /// Resolves accepted transactions from already-loaded acceptance data
    /// (`block_transactions_store[index_within_block]`). Split out so the
    /// per-block bond-view walk (ADR-0009 Addendum B) can derive a *not-yet-
    /// committed* block's mutations from the in-memory `ctx.mergeset_acceptance_data`,
    /// whose `acceptance_data_store` entry does not exist until `commit_utxo_state`.
    pub(super) fn accepted_txs_from_acceptance_data(&self, acceptance_data: &AcceptanceData) -> Vec<Transaction> {
        // STRICT by default: every caller except the pruning-facing chain-block walk
        // operates on RECENT, unpruned acceptance data (a block being validated, its
        // in-memory mergeset), where a missing body is genuine corruption to fail on.
        self.accepted_txs_from_acceptance_data_impl(acceptance_data, false)
    }

    /// Resolve accepted transactions from acceptance data. `tolerate_pruned_bodies`
    /// skips a mergeset block whose transactions have been pruned instead of
    /// panicking — set only on the pruning-facing path (`accepted_txs_of_chain_block`),
    /// where walking a pruned block is expected; strict everywhere else so a missing
    /// RECENT body still fails fast.
    fn accepted_txs_from_acceptance_data_impl(
        &self,
        acceptance_data: &AcceptanceData,
        tolerate_pruned_bodies: bool,
    ) -> Vec<Transaction> {
        let mut txs = Vec::new();
        for mergeset in acceptance_data.iter() {
            let block_txs = match self.block_transactions_store.get(mergeset.block_hash) {
                Ok(t) => t,
                Err(StoreError::KeyNotFound(_)) if tolerate_pruned_bodies => {
                    trace!(
                        "accepted_txs_from_acceptance_data: mergeset block {} bodies are pruned — contributing no accepted txs",
                        mergeset.block_hash
                    );
                    continue;
                }
                Err(e) => {
                    panic!("accepted_txs_from_acceptance_data: block_transactions_store.get({}) failed: {e}", mergeset.block_hash)
                }
            };
            for entry in mergeset.accepted_transactions.iter() {
                let tx = block_txs.get(entry.index_within_block as usize).unwrap_or_else(|| {
                    palw_overlay_commit_fail_stop(format!(
                        "acceptance data for block {} references transaction index {} but the body has only {} transactions",
                        mergeset.block_hash,
                        entry.index_within_block,
                        block_txs.len()
                    ))
                });
                txs.push(tx.clone());
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
    fn dns_bond_mutations_from_acceptance(&self, acceptance_data: &AcceptanceData, accepted_daa_score: u64) -> Vec<BondMutation> {
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        bond_mutations_from_accepted_txs(
            &self.accepted_txs_from_acceptance_data(acceptance_data),
            accepted_daa_score,
            min_bond,
            unbonding_floor,
        )
    }

    /// kaspa-pq DNS Dormancy Fence (design v0.1 §4.3 / §7.3, PR-D2/D3/D4): the
    /// per-epoch dormancy pass, folded into [`Self::update_dns_state`].
    ///
    /// **Fenced inert**: returns immediately unless
    /// `sink_daa >= dormancy_activation_daa_score` (`u64::MAX` on every shipped
    /// preset), so this is byte-identical below the fence.
    ///
    /// ## Reorg-safety (adversarial review + buried-only redesign 2026-07-07)
    /// The persisted dormancy fields (`last_attested_epoch`, `dormant_at_daa_score`,
    /// `dormant_at_epoch`) enter `overlay_commitment_root` and drive the finality
    /// denominator, so they MUST be a pure deterministic function of the canonical
    /// chain or two honest nodes at the same tip fork (`BadOverlayCommitment`).
    /// **FIXED (real-time path):** every transition here keys off `buried_epoch`
    /// (finalized past `max(attestation_lag, max_reorg_horizon)`), the eviction stamp
    /// is the buried round's canonical anchor DAA (never the path-dependent `sink_daa`),
    /// and revival compares in the blue-epoch coordinate (`dormant_at_epoch`, closing
    /// the D4-2 unit bug). Buried data cannot reorg ⇒ the state is reorg-invariant.
    ///
    /// ## Blocker 1 — multi-round catch-up (skip determinism): **CLOSED (PR-D4 checkpoint)**
    /// A `last_evicted_round_epoch` cursor is carried in `DnsState` (recompute-derived, NOT in
    /// the overlay commitment, needs no reorg revert — it is set to the deterministic
    /// `buried_epoch`). The pass replays every eviction round in `(last_evicted_round_epoch,
    /// buried_epoch]` exactly once, ascending, each against its own as-of-`r` buried state via
    /// the pure [`kaspa_consensus_core::dns_finality::apply_dormancy_round`] kernel — so a
    /// virtual commit that JUMPS several epochs lands on the identical dormant set as a node
    /// that advanced one epoch at a time (proven by `dormancy_catch_up_rate_limits_across_rounds`:
    /// jump replay == incremental replay). A fresh `DnsState` (genesis / post-IBD import) seeds
    /// the cursor at the pruning point's buried epoch (the imported bonds already carry the
    /// dormant transitions through `pp`), so the catch-up replays only the `(pp, sink]` rounds.
    ///
    /// ## ⚠️ Blocker 2 — pruned-IBD as-of-pp `last_attested`: **REMAINING (fence stays inert)**
    /// [`Self::bonds_as_of`] nulls dormant stamps `> pp_buried` EXACTLY (a discrete event), but
    /// `last_attested_epoch` is an overwrite-with-latest field, so its as-of-`pp` value is not
    /// recoverable from the current state + post-`pp` data (the `max` is lossy). Because a
    /// bond's committed **Dormant status** is downstream of `last_attested` (via eviction), an
    /// importer that starts from a wrong `last_attested` can evict in a different round → a later
    /// `c != v`. **Exact fix (specified, not yet wired):** unify `last_attested` for *Active*
    /// bonds onto the committed, pruning-survivable **rewarded-epoch overlay window**
    /// (`rewarded_epochs_store`, carried in the snapshot as `BlockOverlayContribution.rewarded_keys
    /// = (outpoint, epoch)`) — reconstructable byte-exactly by a pruned importer — plus a new
    /// consistency invariant I7 (`overlay_window_walk_bound` ≥ the dormancy inactivity horizon,
    /// so every *Active* bond's last attestation is inside the window; fail-safe → dormancy stays
    /// inert if violated). *Dormant* bonds need no exact value (revival requires a post-`pp`
    /// attestation, which the importer replays live). Edge cases to resolve in that change:
    /// rewarded ⊊ credited (pool-cap / zero-reward lag) and the just-revived-bond transition.
    ///
    /// **Release gate (independent of the fence):** the three appended `StakeBondRecord`
    /// fields grow the borsh overlay-commitment preimage, so this binary MUST ship only
    /// on a coordinated **re-genesis** — never rolled onto an existing overlay-active net.
    ///
    /// Effects staged into the atomic `batch` (buried-only, per catch-up round then once at
    /// `buried_epoch`): touch_last_attested (D2), the eviction round `Active -> Dormant` (D3),
    /// then revival `Dormant -> Active` (D4). Returns the new `last_evicted_round_epoch`.
    #[allow(clippy::too_many_arguments)] // buried-only inputs threaded explicitly (all pure)
    fn stage_dormancy_transitions(
        &self,
        batch: &mut WriteBatch,
        sink: BlockHash,
        bonds: &[StakeBondRecord],
        contributions: &[AttestationContribution],
        revival_signals: &[(TransactionOutpoint, u64)],
        epoch_anchor_daa: &BTreeMap<u64, u64>,
        prev_last_evicted: u64,
        sink_daa: u64,
        sink_blue: u64,
        dns_params: &DnsParams,
    ) -> u64 {
        // Master fence: inert until governance activates it under a re-genesis.
        if sink_daa < dns_params.dormancy_activation_daa_score {
            return prev_last_evicted;
        }
        let epoch_len_blue = dns_params.attestation_epoch_length_blue_score.max(1);
        // BURIED-ONLY + CATCH-UP (PR-D4 checkpoint): `buried_epoch` = the latest epoch finalized
        // past BOTH the attestation lag AND the reorg horizon. Every dormancy transition is
        // driven only by buried data, so the persisted state (last_attested_epoch,
        // dormant_at_daa_score, dormant_at_epoch) is a pure function of the canonical chain —
        // reorg-invariant — hence safe in the overlay commitment and the finality denominator.
        // Each eviction ROUND is replayed exactly once against its AS-OF-r buried state via the
        // `(prev_last_evicted, buried_epoch]` catch-up below, so a virtual commit that jumps
        // several epochs cannot skip a round and desync from a node that advanced one at a time.
        let bury_blue = dns_params.attestation_lag_blue_score.max(dns_params.max_reorg_horizon_blocks);
        let Some(buried_epoch) = ready_epoch_from_tip_blue_score(sink_blue, epoch_len_blue, bury_blue) else {
            return prev_last_evicted; // no epoch buried past the horizon yet (early chain)
        };

        // This recompute's BURIED attestation epochs per bond (sorted), so the per-round replay
        // can reconstruct `max attested epoch <= r` — the as-of-r inactivity signal — for any
        // round r. Contributions (Active/credited) + revival signals (Dormant/uncredited) both
        // count for recency. Epochs newer than `buried_epoch` are excluded (not yet finalized).
        let mut att_by_bond: std::collections::HashMap<TransactionOutpoint, Vec<u64>> = std::collections::HashMap::new();
        for c in contributions {
            if c.epoch <= buried_epoch {
                att_by_bond.entry(c.bond_outpoint).or_default().push(c.epoch);
            }
        }
        for &(op, e) in revival_signals {
            if e <= buried_epoch {
                att_by_bond.entry(op).or_default().push(e);
            }
        }
        // (att_by_bond stays unsorted; the kernel takes max<=r directly.)

        // Working copy in the pre-pass snapshot's order, so staging can diff by index (records
        // are never reordered). The catch-up mutates it via the pure kernel; only records that
        // actually changed are staged into the batch.
        let mut work: Vec<StakeBondRecord> = bonds.to_vec();

        let period = dns_params.dormancy_evict_period_epochs.max(1);
        let revival_delay = dns_params.dormancy_revival_delay_epochs.max(1) as u64;

        // CATCH-UP: replay each eviction round r in (prev_last_evicted, buried_epoch] once,
        // ascending, against its own as-of-r buried state (the deterministic kernel).
        let mut last_evicted = prev_last_evicted;
        let mut r = (prev_last_evicted / period + 1) * period;
        while r <= buried_epoch {
            // Deterministic anchor DAA for round r (in-window map, else derive). Unavailable
            // (r far below the window after an extreme catch-up gap) ⇒ stop and retry next
            // recompute rather than skip the round.
            let Some(round_anchor_daa) = epoch_anchor_daa
                .get(&r)
                .copied()
                .or_else(|| self.canonical_anchor_by_blue_score(r, sink, dns_params).map(|a| a.anchor_daa_score))
            else {
                break;
            };
            apply_dormancy_round(&mut work, &att_by_bond, r, round_anchor_daa, sink_daa, epoch_len_blue, dns_params);
            last_evicted = r;
            r += period;
        }

        // Final touch up to buried_epoch (past the last round) so revival + future rounds see it.
        for rec in work.iter_mut() {
            if let Some(m) = att_by_bond.get(&rec.bond_outpoint).and_then(|v| v.iter().copied().filter(|&e| e <= buried_epoch).max())
                && rec.last_attested_epoch.is_none_or(|le| m > le)
            {
                rec.last_attested_epoch = Some(m);
            }
        }

        // REVIVAL at buried_epoch (responsive; not round-gated). Unbonding/Slashed outrank
        // Dormant (skipped). The touch above means a freshly-attested bond is never an eviction
        // candidate, so revival-after-eviction has no conflict.
        for rec in work.iter_mut() {
            if effective_bond_status(rec, sink_daa) != BondStatus::Dormant {
                continue;
            }
            let Some(dormant_epoch) = rec.dormant_at_epoch else {
                continue;
            };
            let last = rec.last_attested_epoch.unwrap_or(0);
            if dormancy_revival_ready(dormant_epoch, last, buried_epoch, revival_delay) {
                rec.dormant_at_daa_score = None;
                rec.dormant_at_epoch = None;
                rec.status = effective_bond_status(rec, sink_daa);
            }
        }

        // Stage only the records that actually changed (diff by index vs the pre-pass snapshot).
        let mut store = self.stake_bonds_store.write();
        for (i, rec) in work.iter().enumerate() {
            if bonds[i] != *rec {
                store.insert_batch(batch, rec.bond_outpoint, Arc::new(rec.clone())).unwrap();
            }
        }
        last_evicted
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
    fn update_dns_state(&self, batch: &mut WriteBatch, sink: BlockHash, staged_bonds: Option<&[StakeBondRecord]>) {
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

        // Snapshot the bond set (bounded by the active validator count). The selected-chain writer
        // already staged its changes into this same RocksDB WriteBatch, which a store iterator cannot
        // observe, so consume its exact post-reorg snapshot. The fallback is kept for direct/internal
        // callers and fails closed on any store read error.
        let bonds: Vec<StakeBondRecord> = if let Some(staged) = staged_bonds {
            staged.to_vec()
        } else {
            self.stake_bonds_store
                .read()
                .iterator()
                .map(|result| {
                    result.map(|(_, rec)| (*rec).clone()).unwrap_or_else(|store_error| {
                        palw_overlay_commit_fail_stop(format!(
                            "DNS bond-registry read failed while recomputing DNS state: {store_error}"
                        ))
                    })
                })
                .collect()
        };

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
        let (contributions, epoch_anchor_daa, revival_signals) =
            self.collect_stake_contributions_v2(sink, None, &bonds, net_id.as_byte_slice(), dns_params);

        // kaspa-pq DNS Dormancy Fence (design v0.1, PR-D2/D3/D4): record each attested
        // bond's latest epoch, revive Dormant bonds that attested (§4.5), then (on a
        // round boundary) evict Active bonds inactive past the window to Dormant so dead
        // stake self-heals out of the finality denominator. Fenced inert
        // (dormancy_activation_daa_score = u64::MAX on every shipped preset) so this
        // returns immediately below the fence.
        // Round cursor for the eviction catch-up. Carried in DnsState (recompute-derived, NOT
        // in the overlay commitment). On a fresh DnsState (genesis or post-IBD import) the
        // imported bonds already carry dormant transitions through the pruning point, so seed
        // the cursor at the pp's buried epoch — the catch-up then replays only the (pp, sink]
        // rounds not yet reflected in the imported set (genesis pp ⇒ blue 0 ⇒ seed 0).
        let prev_last_evicted = match prev_dns_state.as_ref() {
            Some(p) => p.last_evicted_round_epoch,
            None => {
                let bury_blue = dns_params.attestation_lag_blue_score.max(dns_params.max_reorg_horizon_blocks);
                self.pruning_point_store
                    .read()
                    .pruning_point()
                    .optional()
                    .ok()
                    .flatten()
                    .and_then(|pp| self.headers_store.get_blue_score(pp).ok())
                    .and_then(|pp_blue| ready_epoch_from_tip_blue_score(pp_blue, epoch_len_blue, bury_blue))
                    .unwrap_or(0)
            }
        };
        let new_last_evicted = self.stage_dormancy_transitions(
            batch,
            sink,
            &bonds,
            &contributions,
            &revival_signals,
            &epoch_anchor_daa,
            prev_last_evicted,
            sink_daa,
            sink_blue,
            dns_params,
        );

        let totals = total_active_stake_by_epoch(&bonds, &epoch_anchor_daa);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        let stake_depth = compute_stake_score(&per_epoch, dns_params.stake_event_quality_floor_bps);

        // kaspa-pq Phase 13 (ADR-0018 §C): derive the read-only DnsHealth liveness signal
        // from the same per-epoch tallies that fed the StakeScore. `overlay_active` iff the
        // reorg gate is engaged (`Active`); in Bootstrap there is no DNS finality to judge,
        // so health stays `DisabledBeforeActivation`. Purely a signal — never a
        // block-validity input, so this is inert wherever the gate is dormant.
        let health = derive_dns_health(
            &per_epoch,
            dns_params.stake_event_quality_floor_bps,
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
                    "[stake-score] credited epoch={} bond={} stake={} validator_id={}",
                    c.epoch, c.bond_outpoint.transaction_id, c.signed_stake_sompi, c.validator_id
                );
            }
        }

        // audit #3: the canonical lagged anchor of the latest ready epoch — a fixed,
        // blue_score-coordinated selected-chain point every node derives identically. THIS (not
        // the POV-dependent `sink`) is what gets DNS-confirmed and protected by the reorg gate, so
        // nodes that recompute at different boundary sinks still protect the same anchor. `None`
        // until an epoch's anchor is buried and lag-ready (early chain / not yet ready).
        let confirmable_anchor = ready_epoch_from_tip_blue_score(sink_blue, epoch_len_blue, dns_params.attestation_lag_blue_score)
            .and_then(|epoch| self.canonical_anchor_by_blue_score(epoch, sink, dns_params))
            .map(|a| (a.anchor_hash, a.anchor_daa_score));

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
            new_last_evicted,
        );
        let previous_confirmed = prev_dns_state.as_ref().map(|s| s.last_dns_confirmed_anchor).unwrap_or_default();
        if new_state.last_dns_confirmed_anchor != BlockHash::default() && new_state.last_dns_confirmed_anchor != previous_confirmed {
            info!(
                "[dns-finality] confirmed anchor updated: {} -> {} at anchor_daa={}, sink={}, sink_daa={}, work_depth={:?}, \
                 stake_depth={}, health={:?}",
                previous_confirmed,
                new_state.last_dns_confirmed_anchor,
                new_state.last_dns_confirmed_anchor_daa_score,
                sink,
                sink_daa,
                work_depth,
                stake_depth.0,
                health,
            );
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

        // Normalize the stored status to the EFFECTIVE status at the anchor. Mutation writers also
        // refresh this legacy cache, but time-based activation can advance without a row write;
        // `effective_bond_status` over the canonical timing fields remains the single source of truth
        // for the committed snapshot.
        let mut bonds = selected_parent_bond_view.records();
        for b in bonds.iter_mut() {
            b.status = effective_bond_status(b, anchor_daa);
        }
        let reserve_balance = self.reserve_balance_store.get(selected_parent).unwrap_or(0);

        let walk_bound = self.overlay_window_walk_bound(dns_params);
        let window = self.selected_chain_overlay_window(selected_parent, anchor_daa, walk_bound);

        OverlaySnapshot { bonds, reserve_balance, window }
    }

    /// Compute the overlay commitment required by `header_version` from the already-built legacy DNS
    /// snapshot and the selected parent's carried PALW state.
    ///
    /// Pre-v3 returns the legacy snapshot root without touching the PALW store,
    /// preserving existing-network behavior exactly. Header-v3 reads the
    /// block-keyed state (`Ok(None)` is valid only when the selected parent is
    /// genesis or still pre-activation) and commits the full record through
    /// [`OverlaySnapshot::versioned_commitment_root`]. Header-v4 instead folds a disjoint v2 digest
    /// over the complete selected-parent execution frontier, beacon accumulator, provider view,
    /// active paid-work window, and DA state. Thus the first post-pruning-point `c == v` check
    /// independently authenticates every consensus-relevant sidecar component.
    pub(super) fn versioned_overlay_commitment_root(
        &self,
        header_version: u16,
        selected_parent: BlockHash,
        snapshot: &OverlaySnapshot,
        selected_parent_provider_bond_view: &ProviderBondView,
    ) -> kaspa_hashes::Hash64 {
        if header_version >= kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION {
            let selected_parent_daa_score = self
                .headers_store
                .get_daa_score(selected_parent)
                .unwrap_or_else(|err| panic!("failed reading DAA for Header-v4 selected parent {selected_parent}: {err}"));
            let active_non_genesis =
                selected_parent != self.genesis.hash && selected_parent_daa_score >= self.palw_activation_daa_score;

            let beacon_state = self
                .palw_beacon_store
                .beacon_state(selected_parent)
                .unwrap_or_else(|err| panic!("failed reading PALW beacon state for selected parent {selected_parent}: {err}"))
                .map(|state| (*state).clone());
            let mut overlay_view = self
                .palw_overlay_view_store
                .view(selected_parent)
                .unwrap_or_else(|err| panic!("failed reading PALW batch view for selected parent {selected_parent}: {err}"))
                .map(|view| (*view).clone());
            let mut lane_bits = self
                .palw_lane_bits_store
                .lane_bits(selected_parent)
                .unwrap_or_else(|err| panic!("failed reading PALW lane bits for selected parent {selected_parent}: {err}"));
            let active_nullifiers = self
                .palw_nullifier_store
                .get(selected_parent)
                .optional()
                .unwrap_or_else(|err| panic!("failed reading PALW nullifiers for selected parent {selected_parent}: {err}"));
            let mut beacon_accumulator = self
                .palw_beacon_store
                .accum_view(selected_parent)
                .unwrap_or_else(|err| panic!("failed reading PALW beacon accumulator for selected parent {selected_parent}: {err}"))
                .map(|view| PalwPrunedBeaconAccumulatorV1 {
                    version: view.version,
                    epochs: view.epochs.clone(),
                    stake_by_epoch: view.stake_by_epoch.clone(),
                });

            // Genesis (or a finite-fence pre-activation parent) has no block-keyed rows. Commit its
            // exact semantic defaults rather than an ambiguous absence: the first v4 child seeds the
            // empty batch/accumulator and the re-genesis lane pair from these same values.
            if !active_non_genesis {
                overlay_view.get_or_insert_with(kaspa_consensus_core::palw::PalwBatchViewV1::new);
                lane_bits.get_or_insert(kaspa_consensus_core::palw::PalwLaneBitsV1 {
                    hash_bits: self.palw_lane_difficulty.genesis_hash_bits,
                    replica_bits: self.palw_lane_difficulty.genesis_replica_bits,
                });
                beacon_accumulator.get_or_insert_with(PalwPrunedBeaconAccumulatorV1::new);
            }

            if active_non_genesis
                && (beacon_state.is_none()
                    || overlay_view.is_none()
                    || lane_bits.is_none()
                    || active_nullifiers.is_none()
                    || beacon_accumulator.is_none())
            {
                panic!(
                    "active Header-v4 selected parent {selected_parent} is missing a required beacon/batch/lane/nullifier/accumulator row"
                );
            }

            let frontier = PalwPrunedFrontierV1 {
                beacon_state,
                overlay_view,
                lane_bits,
                active_nullifiers: active_nullifiers.map(|set| (*set).clone()).unwrap_or_default(),
            };
            // Header-v4 commits only the selected parent's immutable, block-keyed content references.
            // Never enumerate the mutable global blob store here: late availability must not change an
            // already accepted parent's commitment.
            let active_batch_ref_root = palw_active_batch_ref_root(frontier.overlay_view.as_ref());
            let mut paid_work_nullifiers: Vec<Hash64> =
                self.palw_paid_work_window(selected_parent, selected_parent_daa_score).into_iter().collect();
            paid_work_nullifiers.sort_by_key(|hash| hash.as_bytes());
            let da_state_root = self.palw_da_parent_state(selected_parent, selected_parent_daa_score).0.state_root();
            let search_availability_state_root =
                self.palw_search_parent_state(selected_parent, selected_parent_daa_score).0.state_root();
            let selected_parent_state = PalwSelectedParentStateV2::try_new(
                selected_parent,
                selected_parent_daa_score,
                frontier,
                beacon_accumulator,
                selected_parent_provider_bond_view.records(),
                paid_work_nullifiers,
                da_state_root,
                search_availability_state_root,
                active_batch_ref_root,
            )
            .unwrap_or_else(|err| panic!("invalid Header-v4 selected-parent state at {selected_parent}: {err}"));
            let selected_parent_state_root = selected_parent_state.state_root();
            return snapshot.versioned_commitment_root(header_version, None, Some(&selected_parent_state_root));
        }

        let beacon_state = if header_version >= kaspa_consensus_core::constants::PALW_HEADER_VERSION {
            let state = self
                .palw_beacon_store
                .beacon_state(selected_parent)
                .unwrap_or_else(|err| panic!("failed reading PALW beacon state for selected parent {selected_parent}: {err}"));
            if state.is_none()
                && selected_parent != self.genesis.hash
                && self.headers_store.get_daa_score(selected_parent).unwrap() >= self.palw_activation_daa_score
            {
                panic!("missing PALW beacon state for active selected parent {selected_parent}");
            }
            state
        } else {
            None
        };
        snapshot.versioned_commitment_root(header_version, beacon_state.as_deref(), None)
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

    /// kaspa-pq **ADR-0040 §5.15.13 — gate G16 (P1-9-RELAND)**: the `job_nullifier`s already PAID on
    /// `anchor`'s selected chain within [`PalwBatchAdmissionParams::paid_work_walk_bound_daa`].
    ///
    /// This is the paid-set the reward coordinate deduplicates against. Three properties, each with a
    /// line below that enforces it:
    ///
    /// * **Chain-relative, hence reorg-clean.** The set is a pure function of `(anchor's selected
    ///   chain, walk_bound)`. A block paid on a branch that loses a reorg is simply not on the new
    ///   chain, so its nullifiers are unpaid there — which is correct, because its payout was undone
    ///   with it. Nothing is carried, so nothing has to be un-carried.
    /// * **Order-independent.** Two nodes evaluating the SAME block walk the same selected chain from
    ///   the same anchor with the same bound and read the same block-keyed rows, so they cannot
    ///   disagree. There is no arrival-order input anywhere in this function.
    /// * **Bounded.** `walk_bound` is derived from the batch-admission windows, which
    ///   `PalwBatchManifestV1::admission_valid` enforces — see that method's doc for the derivation.
    ///
    /// The fast path returns an empty set while PALW is gated. PALW networks walk persisted paid-work
    /// rows from the selected chain.
    ///
    /// Pruned-IBD boundary: like every selected-chain walk here, this one
    /// cannot traverse below the pruning point. Unlike the DNS overlay window it is deliberately NOT
    /// merged with `pruning_overlay_snapshot_store`: that snapshot's borsh encoding is the preimage of
    /// `Header::overlay_commitment_root` and adding a field to it would move that commitment on every
    /// net, including mainnet. So a pruned joiner validating the first `walk_bound` DAA above its
    /// pruning point sees a short prefix. `palw_paid_work_walk_stays_above_the_pruning_point` pins the
    /// parameter relation that keeps this to the bootstrap band only; closing the band itself is an
    /// activation-blocking item, not something a comment covers.
    pub(crate) fn palw_paid_work_window(&self, anchor: BlockHash, anchor_daa: u64) -> std::collections::HashSet<Hash64> {
        let mut paid = std::collections::HashSet::new();
        if self.palw_activation_daa_score == u64::MAX {
            return paid; // inert fast path — no algo-4 source can be accepted, so nothing was ever paid
        }
        let walk_bound = self.palw_batch_admission.paid_work_walk_bound_daa(self.palw_epoch_length_daa);
        // A complete PALW sidecar carries the part of this bounded selected-chain walk at/below the
        // pruning point. Validate it on every use: silently ignoring a corrupt singleton would reopen
        // duplicate payment exactly when historical rows are no longer available to recover it.
        let persisted = match self.palw_pruned_frontier_store.read().get() {
            Ok(snapshot) => {
                if let Err(err) = self.validate_palw_pruning_snapshot_context(&snapshot) {
                    panic!("invalid persisted PALW pruning snapshot while reading paid-work window: {err}");
                }
                Some(snapshot)
            }
            Err(StoreError::KeyNotFound(_)) => None,
            Err(err) => panic!("PALW pruning snapshot store is unreadable: {err}"),
        };
        let current_pp = self.pruning_point_store.read().pruning_point().unwrap();
        if persisted.is_none() && current_pp != self.genesis.hash {
            panic!("missing PALW pruning snapshot for non-genesis pruning point {current_pp}");
        }
        if let Some(snapshot) = persisted.as_ref() {
            for row in &snapshot.payload.paid_work {
                if anchor_daa.saturating_sub(row.block_daa_score) <= walk_bound {
                    paid.extend(row.job_nullifiers.iter().copied());
                }
            }
        }
        let stop_at = persisted.as_ref().map(|s| s.payload.pruning_point);
        for ancestor in std::iter::once(anchor).chain(self.reachability_service.default_backward_chain_iterator(anchor)) {
            if Some(ancestor) == stop_at {
                break;
            }
            let ancestor_daa = self.headers_store.get_daa_score(ancestor).unwrap();
            if anchor_daa.saturating_sub(ancestor_daa) > walk_bound {
                break;
            }
            // FAIL CLOSED on anything that is not a genuine absence. A block that paid no PALW work
            // writes no row, so `KeyNotFound` is the normal case and means "this ancestor paid nothing".
            // Every OTHER StoreError is an IO/corruption fault, and swallowing it would silently drop
            // that ancestor's paid nullifiers from the set — i.e. re-open the duplicate-work hole this
            // walk exists to close, as a transient double-PAYMENT rather than a loud failure. Panicking
            // matches `headers_store.get_daa_score(...).unwrap()` three lines above: inside the reward
            // path, a store we cannot read is not a condition we can safely continue through.
            match self.palw_paid_work_store.get(ancestor) {
                Ok(ids) => paid.extend(ids.iter().copied()),
                Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => {}
                Err(e) => panic!("PALW paid-work store unreadable for ancestor {ancestor}: {e}"),
            }
        }
        paid
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
        let (contributions, epoch_anchor_daa, _) = self.collect_stake_contributions_v2(tip, Some(ancestor), bonds, net_id, dns_params);
        let totals = total_active_stake_by_epoch(bonds, &epoch_anchor_daa);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        compute_stake_score(&per_epoch, dns_params.stake_event_quality_floor_bps)
    }

    /// StakeScore over the complete configured window at `tip`. Used when the common
    /// ancestor is provably older than that window: in that case the since-ancestor
    /// score is exactly the full-window score, without an unbounded selected-chain walk.
    fn stake_score_over_window(&self, tip: BlockHash, bonds: &[StakeBondRecord], dns_params: &DnsParams, net_id: &[u8]) -> StakeScore {
        let (contributions, epoch_anchor_daa, _) = self.collect_stake_contributions_v2(tip, None, bonds, net_id, dns_params);
        let totals = total_active_stake_by_epoch(bonds, &epoch_anchor_daa);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        compute_stake_score(&per_epoch, dns_params.stake_event_quality_floor_bps)
    }

    /// kaspa-pq Phase 13 (ADR-0018 §H): the selected-chain common ancestor of `a` and `b`
    /// — the first block on `a`'s selected chain (from `a` inclusive, walking back) that is
    /// also a chain-ancestor of `b`. Returns the ancestor plus steps walked, or `None`
    /// when it is older than `max_walk`.
    fn selected_chain_common_ancestor(&self, a: BlockHash, b: BlockHash, max_walk: u64) -> Option<(BlockHash, u64)> {
        for (walked, block) in (0_u64..).zip(std::iter::once(a).chain(self.reachability_service.default_backward_chain_iterator(a))) {
            if walked > max_walk {
                return None;
            }
            if self.reachability_service.is_chain_ancestor_of(block, b) {
                return Some((block, walked));
            }
        }
        None
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

    /// kaspa-pq DNS v3: the canonical anchors for every **creditable** epoch within the
    /// stake-score window ending at `tip`, computed in ONE selected-parent-chain walk.
    /// "Creditable" = ready (buried by `attestation_lag_blue_score`), non-duplicate
    /// (`anchor(E) != anchor(E-1)`; a sparse chain that reused the previous anchor earns no
    /// new credit), and recent enough that both `anchor_cutoff(E)` and `anchor_cutoff(E-1)`
    /// fall inside the collected window (so the duplicate flag is reliable). Older / unready
    /// / duplicate epochs are simply absent. Position comes from header-committed
    /// `blue_score`, never the store index, so archival and IBD-synced nodes agree.
    pub(crate) fn canonical_anchors_in_window(
        &self,
        tip: BlockHash,
        dns_params: &DnsParams,
    ) -> BTreeMap<u64, CanonicalLaggedEpochAnchor> {
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let backoff = dns_params.attestation_anchor_backoff_blue_score;
        let lag = dns_params.attestation_lag_blue_score;
        let window = dns_params.stake_score_window_blue_score;

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
    ) -> (Vec<AttestationContribution>, BTreeMap<u64, u64>, Vec<(TransactionOutpoint, u64)>) {
        // Canonical anchors for the creditable epoch window, computed from THIS chain's tip.
        let anchors = self.canonical_anchors_in_window(tip, dns_params);
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
        // Dormancy Fence (PR-D4): signature-verified, canonical attestations naming
        // a *Dormant* bond — the accepted-but-uncredited revival signal. Always empty
        // when the fence is inert (no bond is ever Dormant), so credit is unchanged.
        let mut revival_signals: Vec<(TransactionOutpoint, u64)> = Vec::new();
        let Ok(tip_blue) = self.headers_store.get_blue_score(tip) else {
            return (contributions, epoch_anchor_daa, revival_signals);
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
                // The bond must be Active OR Dormant at the CANONICAL anchor DAA (==
                // att.target_daa_score by the gate above), not a self-reported / current
                // value. Active bonds are credited; Dormant bonds (Dormancy Fence, D4)
                // yield only a revival signal — never credit. Pending/Unbonding/Slashed skip.
                let status = effective_bond_status(bond, anchor.anchor_daa_score);
                if !matches!(status, BondStatus::Active | BondStatus::Dormant) {
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
                    if status == BondStatus::Active {
                        contributions.push(AttestationContribution {
                            epoch: att.epoch,
                            validator_id: att.validator_id,
                            bond_outpoint: att.bond_outpoint,
                            signed_stake_sompi: bond.amount,
                        });
                    } else {
                        // Dormant: registry-only revival signal (design §4.5), no credit.
                        revival_signals.push((att.bond_outpoint, att.epoch));
                    }
                }
            }
        }
        (contributions, epoch_anchor_daa, revival_signals)
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
    fn dns_reorg_allows(&self, candidate: BlockHash, prev_sink: BlockHash, candidate_bond_view: &ActiveBondView) -> bool {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return true;
        };
        let Ok(state) = self.dns_state_store.read().get() else {
            return true; // no DnsState written yet
        };
        if state.rollout_stage != DnsRolloutStage::Active {
            return true; // gate dormant outside the Active stage
        }
        let confirmed = state.last_dns_confirmed_anchor;
        if confirmed == BlockHash::default() {
            return true; // nothing confirmed yet
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

        // The heavy two-dimensional inputs are needed only when the candidate abandons
        // the confirmed prefix under the dominance rule.
        let mut common_ancestor = None;
        let mut horizon_exceeded = false;
        let inputs = if dns_params.reorg_mode == DnsReorgMode::TwoDimensionalDominance && !includes {
            // Do not short-circuit at the ordinary horizon: that made a temporary local
            // fork permanent. Search through the whole StakeScore window. If I is still
            // older, full-window StakeScore is exactly the since-I score; cumulative tip
            // work is equivalent because the same I cancels from both sides. The recovery
            // path therefore remains bounded while every depth reaches emergency dominance.
            let common_ancestor_walk = dns_params.max_reorg_horizon_blocks.max(dns_params.stake_score_window_blue_score);
            let ancestor = self.selected_chain_common_ancestor(candidate, prev_sink, common_ancestor_walk);
            horizon_exceeded = ancestor.is_none_or(|(_, walked)| walked > dns_params.max_reorg_horizon_blocks);
            common_ancestor = ancestor.map(|(hash, _)| hash);

            let net_id_hash = self.genesis.hash;
            let net_id = net_id_hash.as_byte_slice();
            // Per-branch bond sets (safety — each branch under its OWN view; see doc comment).
            let candidate_bonds = candidate_bond_view.records();
            let canonical_bonds: Vec<StakeBondRecord> =
                self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
            let (common_work, candidate_stake, canonical_stake) = match common_ancestor {
                Some(ancestor) => (
                    self.ghostdag_store.get_blue_work(ancestor).unwrap_or_default(),
                    self.stake_score_since_ancestor(candidate, ancestor, &candidate_bonds, dns_params, net_id),
                    self.stake_score_since_ancestor(prev_sink, ancestor, &canonical_bonds, dns_params, net_id),
                ),
                None => (
                    BlueWorkType::from_u64(0),
                    self.stake_score_over_window(candidate, &candidate_bonds, dns_params, net_id),
                    self.stake_score_over_window(prev_sink, &canonical_bonds, dns_params, net_id),
                ),
            };
            reorg_inputs_since_common_ancestor(
                state.rollout_stage,
                dns_params.reorg_mode,
                includes,
                self.ghostdag_store.get_blue_work(candidate).unwrap_or_default(),
                self.ghostdag_store.get_blue_work(prev_sink).unwrap_or_default(),
                common_work,
                candidate_stake,
                canonical_stake,
                dns_params.emergency_work_margin,
                dns_params.emergency_stake_margin,
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
            )
        };
        let outcome = check_dns_reorg_rule(&inputs);
        if !outcome.is_accept()
            && let Some(suppressed) = dns_reorg_rejection_warning_due()
        {
            warn!(
                "[dns-reorg-gate] candidate {candidate} rejected: reason={outcome:?}, confirmed_anchor={confirmed}, \
                 previous_sink={prev_sink}, common_ancestor={common_ancestor:?}, horizon_exceeded={horizon_exceeded}, \
                 candidate_work={:?}, canonical_work={:?}, work_margin={:?}, candidate_stake={}, canonical_stake={}, \
                 stake_margin={}, suppressed_since_previous_warning={suppressed}. The node is intentionally holding its \
                 current virtual sink; persistent warnings indicate a possible self-wedge.",
                inputs.candidate_work_after,
                inputs.canonical_work_after,
                inputs.emergency_work_margin,
                inputs.candidate_stake_after.0,
                inputs.canonical_stake_after.0,
                inputs.emergency_stake_margin.0,
            );
        }
        outcome.is_accept()
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
    pub(super) fn sink_search_algorithm(
        &self,
        stores: &VirtualStores,
        diff: &mut UtxoDiff,
        bond_view: &mut ActiveBondView,
        provider_bond_view: &mut ProviderBondView,
        prev_sink: BlockHash,
        tips: Vec<BlockHash>,
        finality_point: BlockHash,
        pruning_point: BlockHash,
    ) -> (BlockHash, VecDeque<BlockHash>) {
        // TODO (relaxed): additional tests

        let mut heap = tips
            .into_iter()
            .map(|block| SortableBlock { hash: block, blue_work: self.ghostdag_store.get_blue_work(block).unwrap() })
            .collect::<BinaryHeap<_>>();

        // The initial diff point is the previous sink
        let mut diff_point = prev_sink;

        // We maintain the following invariant: `heap` is an antichain.
        // It holds at step 0 since tips are an antichain, and remains through the loop
        // since we check that every pushed block is not in the past of current heap
        // (and it can't be in the future by induction)
        loop {
            let candidate = heap.pop().expect("valid sink must exist").hash;
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
                diff_point = self.calculate_utxo_state_relatively(stores, diff, bond_view, provider_bond_view, diff_point, candidate);
                if diff_point == candidate {
                    // This indicates that candidate has valid UTXO state and that `diff` represents its diff from virtual

                    // kaspa-pq Phase 10 (ADR-0009): the DNS finality reorg gate. Inert
                    // unless the overlay is configured and in the Active stage; it then
                    // rejects a candidate that would abandon a DNS-confirmed anchor. The
                    // rejection is soft — we fall through to push the candidate's parents
                    // and continue, converging on a DNS-valid sink (mirrors the
                    // invalid-UTXO handling below).
                    if self.dns_reorg_allows(candidate, prev_sink, bond_view) {
                        // All blocks with lower blue work than filtering_root are:
                        // 1. not in its future (bcs blue work is monotonic),
                        // 2. will be removed eventually by the bounded merge check.
                        // Hence as an optimization we prefer removing such blocks in advance to allow valid tips to be considered.
                        let filtering_root = self.depth_store.merge_depth_root(candidate).unwrap();
                        let filtering_blue_work = self.ghostdag_store.get_blue_work(filtering_root).unwrap_or_default();
                        return (
                            candidate,
                            heap.into_sorted_iter().take_while(|s| s.blue_work >= filtering_blue_work).map(|s| s.hash).collect(),
                        );
                    }
                    debug!("Block candidate {} rejected by the DNS finality reorg gate; ignored from Virtual chain.", candidate);
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
        self.validate_palw_overlay_activation(&mutable_tx.tx, virtual_daa_score)?;
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
        self.validate_palw_overlay_activation(tx, virtual_state.daa_score)?;
        self.transaction_validator.validate_tx_in_header_context_with_args(
            tx,
            virtual_state.daa_score,
            virtual_state.past_median_time,
        )?;
        let ValidatedTransaction { calculated_fee, .. } =
            // `None`, `None`, `None`: mempool/template single-tx context, not mergeset acceptance (the
            // bond spend-gate, the provider-unbond authorization filter, and the provider-bond spend
            // gate are all acceptance-time only, inert here).
            self.validate_transaction_in_utxo_context(tx, utxo_view, virtual_state.daa_score, TxValidationFlags::Full, None, None, None)?;
        Ok(calculated_fee)
    }

    /// Isolation can decode the future PALW wire format without a chain POV, but neither the mempool
    /// nor template construction may admit those reserved subnetworks before the hard fork. Keep this
    /// predicate shared by both paths so a locally constructed template cannot fail its own body
    /// contextual validation.
    fn validate_palw_overlay_activation(&self, tx: &Transaction, pov_daa_score: u64) -> TxResult<()> {
        if pov_daa_score < self.palw_activation_daa_score && tx.subnetwork_id.palw_tx_kind().is_some() {
            return Err(TxRuleError::SubnetworksDisabled(tx.subnetwork_id.clone()));
        }
        Ok(())
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

        let anchors = self.canonical_anchors_in_window(selected_parent, dns_params);
        if anchors.is_empty() {
            return Vec::new();
        }

        let bonds = selected_parent_bond_view.records();
        let (parent_contributions, _, _) =
            self.collect_stake_contributions_v2(selected_parent, None, &bonds, self.genesis.hash.as_byte_slice(), dns_params);
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
            *entry = entry.saturating_add(c.signed_stake_sompi);
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
        // Header-v4 commits this PALW provider registry. Capture it under the same virtual read
        // generation as `virtual_state` and the DNS view so the template never mixes a newer tip
        // registry with an older selected parent.
        let template_provider_bond_view = self.initial_palw_provider_bond_view();
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
            template_provider_bond_view,
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

    /// kaspa-pq ADR-0040 — the PALW lane `bits` an algo-4 template must stamp.
    ///
    /// Derived from the SAME window the header stage will use: `block_daa_window` over the virtual
    /// ghostdag data. A block built from this template has the virtual's parents, so its own DAA window
    /// is this window — the identical argument that makes today's `virtual_bits` the correct template
    /// `bits` for the hash lane. The lane filter and retarget then run through the shared
    /// [`crate::processes::difficulty::lane_bits_from_window`], so construction and validation cannot
    /// drift.
    ///
    /// Below the lane's `min_samples` this HOLDs at `genesis_bits`, which is why stamping
    /// `genesis_replica_bits` directly appears to work on a lane with no blocks yet — and stops working
    /// at the `min_samples`-th algo-4 block.
    pub(crate) fn palw_lane_bits_for_template(
        &self,
        lane_algo_id: u8,
        per_set: Option<crate::processes::difficulty::PalwSetSublane>,
    ) -> Result<u32, RuleError> {
        let virtual_state = self.lkg_virtual_state.load();
        // ADR-MA §12 — the sublane restriction must MATCH the fence, both ways: a registry-active
        // algo-4 template without a resolved (set, share) would stamp flat-lane bits its own
        // node's per-set header check rejects, and a closed-fence template passing a sublane
        // would stamp per-set bits nothing validates. Either mismatch is a caller bug — fail
        // loud rather than emit self-rejecting templates. Unreachable on every shipped preset
        // (fence = u64::MAX, callers pass None).
        let registry_active = self.palw_compute_registry_activation_daa_score != u64::MAX
            && virtual_state.daa_score >= self.palw_compute_registry_activation_daa_score
            && lane_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA;
        if registry_active != per_set.is_some() {
            return Err(RuleError::PalwComputeSetResolution(
                virtual_state.ghostdag_data.selected_parent,
                if registry_active {
                    "registry-active algo-4 template built without a resolved Compute Set sublane (ADR-MA §12)".into()
                } else {
                    "per-set sublane supplied while the registry fence is closed for this template".into()
                },
            ));
        }
        let daa_window = self.window_manager.block_daa_window(&virtual_state.ghostdag_data)?;
        Ok(crate::processes::difficulty::lane_bits_from_window(
            self.headers_store.as_ref(),
            &daa_window.window,
            lane_algo_id,
            &self.palw_lane_difficulty,
            per_set,
        ))
    }

    /// Derive the same Header-v4 candidate transition the header validator derives. The event delta
    /// is bounded by the consensus mergeset limit; the horizon baseline follows authenticated,
    /// deterministic skip pointers under the consensus lookup-hop cap.
    fn derive_palw_spam_template_fields(
        &self,
        virtual_state: &VirtualState,
        child_is_replica: bool,
    ) -> Result<(Hash64, u16), RuleError> {
        if self.palw_spam.is_inert() {
            return Ok((Hash64::default(), 0));
        }
        let mut delta = PalwSpamLaneDelta::default();
        for &blue in virtual_state.ghostdag_data.mergeset_blues.iter().skip(1) {
            let merged = self
                .headers_store
                .get_header(blue)
                .map_err(|e| RuleError::PalwSpamAccumulatorInvalid(format!("template merged-blue header {blue}: {e}")))?;
            if merged.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA {
                delta.replica_blues = delta
                    .replica_blues
                    .checked_add(1)
                    .ok_or_else(|| RuleError::PalwSpamAccumulatorInvalid("template replica delta overflow".into()))?;
            } else {
                delta.hash_blues = delta
                    .hash_blues
                    .checked_add(1)
                    .ok_or_else(|| RuleError::PalwSpamAccumulatorInvalid("template hash delta overflow".into()))?;
            }
        }
        let (state, counts) = palw_spam_derive_child(
            self.palw_spam_store.as_ref(),
            virtual_state.ghostdag_data.selected_parent,
            virtual_state.daa_score,
            self.palw_spam.window_daa,
            delta,
            child_is_replica,
        )
        .map_err(|e| RuleError::PalwSpamAccumulatorInvalid(e.to_string()))?;
        let required_bits = if child_is_replica {
            kaspa_consensus_core::palw_antispam::palw_spam_target(self.palw_spam, counts)
                .map_err(|error| match error {
                    kaspa_consensus_core::palw_antispam::PalwSpamError::RateExceeded { prospective, capacity } => {
                        RuleError::PalwSpamRateExceeded { prospective, capacity }
                    }
                    other => RuleError::PalwSpamAccumulatorInvalid(other.to_string()),
                })?
                .required_stamp_bits
        } else {
            0
        };
        Ok((state.commitment(), required_bits))
    }

    /// Re-derive replica fields for the exact ordinary template being converted. A virtual-generation
    /// move (including a side tip that changes parents while the selected sink stays fixed) fails
    /// closed instead of mixing candidate state from two generations.
    pub(crate) fn palw_spam_replica_fields_for_template(&self, header: &Header) -> Result<(Hash64, u16), RuleError> {
        let virtual_state = self.lkg_virtual_state.load();
        if header.daa_score != virtual_state.daa_score || header.direct_parents() != virtual_state.parents.as_slice() {
            return Err(RuleError::PalwSpamAccumulatorInvalid(
                "virtual generation moved while converting the hash template to a replica template".into(),
            ));
        }
        self.derive_palw_spam_template_fields(&virtual_state, true)
    }

    pub(crate) fn build_block_template_from_virtual_state(
        &self,
        virtual_state: Arc<VirtualState>,
        // kaspa-pq narrow P0-1: the bond view + deposit-claim snapshot, both
        // captured in the SAME virtual generation as `virtual_state` by the caller
        // (under one read lock) — so the reward fan-out, the overlay commitment and
        // the EVM claim payload all reference one coherent generation.
        template_bond_view: ActiveBondView,
        template_provider_bond_view: ProviderBondView,
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
        // Declare the highest active header schema, exactly mirroring
        // `HeaderProcessor::check_header_version`: Compute Set v5 > PALW anti-spam v4 > PALW v3 >
        // EVM v2 > base v1. (The v5 rung was landed with the registry fence — a template on a
        // registry-active net must declare v5 or its own header check rejects it with
        // WrongBlockVersion.)
        let version = if virtual_state.daa_score >= self.palw_activation_daa_score
            && virtual_state.daa_score >= self.palw_compute_registry_activation_daa_score
        {
            kaspa_consensus_core::constants::PALW_COMPUTE_SET_HEADER_VERSION
        } else if virtual_state.daa_score >= self.palw_activation_daa_score && !self.palw_spam.is_inert() {
            kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION
        } else if virtual_state.daa_score >= self.palw_activation_daa_score {
            kaspa_consensus_core::constants::PALW_HEADER_VERSION
        } else if virtual_state.daa_score >= self.evm_activation_daa_score {
            kaspa_consensus_core::constants::EVM_HEADER_VERSION
        } else {
            BLOCK_VERSION
        };
        let (palw_spam_accumulator_commitment, _) = self
            .derive_palw_spam_template_fields(&virtual_state, false)
            .map_err(|err| err.with_attestation_template_drops(&dropped_attestation_shards))?;
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
            // kaspa-pq Phase 3 (ADR-0007): the template declares the network-correct Layer-1 algo
            // for this DAA score — BLAKE2b-512 ∥ SHA3-512 (algo_id = 3) once activated, else kHeavyHash (1).
            kaspa_consensus_core::pow_layer0::required_algo_id(self.pow_blake2b_sha3_activation.is_active(virtual_state.daa_score)),
            virtual_state.daa_score,
            virtual_state.ghostdag_data.blue_work,
            virtual_state.ghostdag_data.blue_score,
            header_pruning_point,
        );
        // Header-v3 commits the exact GHOSTDAG-derived component decomposition even for the
        // permanent algo-3 hash lane; ticket fields remain zero on a hash-lane template.
        let header = if version >= kaspa_consensus_core::constants::PALW_HEADER_VERSION {
            // ADR-0039 C6 SLICE 2: stamp this block's OWN beacon state R_E into the header, computed by
            // the SAME derivation the S2 UTXO-validation check (`check_palw_beacon_seed` →
            // `derive_palw_beacon_state_value`) re-runs — so a block mined from this template
            // authenticates (construction == validation). The template knows `(daa_score,
            // selected_parent)` before the block hash exists, so it calls the shared core directly; the
            // selected-parent bond view is the same `template_bond_view` the overlay-commitment root is
            // built from (validation resolves the identical view for the mined block). `unwrap_or_default`
            // covers the never-taken pre-activation branch (this arm is behind `version >= v3`).
            let derived_beacon = self.derive_palw_beacon_state_core(
                virtual_state.daa_score,
                virtual_state.ghostdag_data.selected_parent,
                virtual_state.ghostdag_data.selected_parent,
                &template_bond_view,
            );
            let palw_beacon_seed = derived_beacon.as_ref().map(|s| s.seed).unwrap_or_default();
            // K5 (ADR-0039 §11.3) template contract, c==v twin of the S2 `PalwLaneHalted` rule + the
            // body-stage clause 10: a FUTURE algo-4 candidate constructor MUST suppress emission unless
            // `palw_template_lane_open(derived.mode, buried_carry_run, grace)` — i.e. the block's own
            // mode is not Halted AND the lagged buried seed-carry run does not exceed grace (the second
            // conjunct prevents post-recovery self-bricking). Today the template is ALWAYS algo-3
            // (`required_algo_id` above never returns id 4), so no ticket is emitted and the guard is a
            // documented invariant + debug assert, not a live gate.
            debug_assert!(
                virtual_state.daa_score < self.palw_activation_daa_score
                    || header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA
                    || derived_beacon.as_ref().is_none_or(|d| {
                        kaspa_consensus_core::palw::palw_template_lane_open(d.mode, 0, self.palw_beacon_grace_epochs)
                    }),
                "K5: an algo-4 template must consult palw_template_lane_open (mode not Halted + buried carry <= grace)"
            );
            header.with_palw_fields(kaspa_consensus_core::header::PalwHeaderFields {
                blue_hash_work: virtual_state.ghostdag_data.blue_hash_work,
                blue_compute_work: virtual_state.ghostdag_data.blue_compute_work,
                palw_beacon_seed,
                palw_spam_accumulator_commitment,
                ..Default::default()
            })
        } else {
            header
        };
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
        // `overlay_commitment_root` byte-for-byte (construction == validation). A pre-v3
        // header stays unchanged when DNS is absent; Header-v3 always commits the versioned
        // root because R_E is part of that schema. Appended after the EVM fields;
        // `with_overlay_commitment` re-finalizes over the full preimage.
        let header = if self.dns_params.is_some() || header.version >= kaspa_consensus_core::constants::PALW_HEADER_VERSION {
            let selected_parent = virtual_state.ghostdag_data.selected_parent;
            let overlay_snapshot = self.compute_overlay_snapshot(selected_parent, &template_bond_view);
            let overlay_root = self.versioned_overlay_commitment_root(
                header.version,
                selected_parent,
                &overlay_snapshot,
                &template_provider_bond_view,
            );
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
            Vec::new(), // kaspa-pq ADR-0040 §5.15.13 (G16): genesis pays no PALW work.
            0,          // kaspa-pq ADR-0018 "本格版": genesis has no validator quality sub-pool.
            0,          // kaspa-pq ADR-0018 "本格版" (Phase 4): genesis reserve balance is 0.
            None,       // kaspa-pq ADR-0020 v0.4: genesis is EVM-inert (v0 header).
            &ActiveBondView::new(),
            &ProviderBondView::new(), // kaspa-pq ADR-0040 §5.17: genesis has no provider bonds.
            (self.palw_activation_daa_score != u64::MAX).then(PalwDaStaged::default),
            (self.palw_activation_daa_score != u64::MAX).then(PalwSearchStaged::default),
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
        let new_pruning_point_header = self.headers_store.get_header(new_pruning_point).unwrap();
        // The provider registry and every fork-local PALW boundary component must already have been
        // installed by the complete sidecar before virtual is seeded. Genesis is the sole exception:
        // it has no below-boundary state and must still have an empty registry.
        if self.palw_is_active_at(new_pruning_point_header.daa_score) {
            if new_pruning_point == self.genesis.hash {
                let registry_has_rows = self.palw_provider_bonds_store.read().iterator().next().transpose().map_err(|err| {
                    PruningImportError::ImportedPalwSnapshotInvalid(
                        new_pruning_point,
                        format!("provider-registry read at genesis reset: {err}"),
                    )
                })?;
                if registry_has_rows.is_some() {
                    return Err(PruningImportError::PalwProviderRegistrySnapshotUnavailable(new_pruning_point));
                }
            } else {
                self.validate_imported_palw_provider_utxos(new_pruning_point)?;
            }
        }
        info!("Importing the UTXO set of the pruning point {}", new_pruning_point);
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
            // ADR-0040 ECON-03: likewise the provider-bond registry as-of the imported pruning point.
            // The ordinary presets are PALW-fenced and therefore empty. On testnet-110/devnet-111,
            // however, pruning-point transport of this registry is NOT solved: this initial view is
            // empty and late/pruned IBD must remain outside the closed-testnet operating envelope.
            &self.initial_palw_provider_bond_view(),
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
            use crate::model::stores::evm::{EvmCodeStoreReader, EvmStateDiffStoreReader};
            let flat_head = self.evm_latest_state_ptr_store.read().get().ok().flatten().map(|p| p.canonical_head);

            // (2) The flat head IS the pruning point (a freshly pruned-IBD-imported
            // node pins the pointer to the pp) ⇒ materialize it directly.
            if flat_head == Some(pruning_point) {
                return match crate::processes::evm::materialize_snapshot(&self.evm_flat_account_store, &self.evm_code_store) {
                    Ok(snapshot) => Some((header, snapshot)),
                    Err(e) => {
                        warn!("[evm] pruning-point flat materialize failed for {pruning_point}: {e}");
                        None
                    }
                };
            }

            // (3) Forward §12 reconstruct: seed from an anchor at/below pp and replay
            // diffs upward. Cheap when an anchor is dense; yields nothing when the
            // anchor material below pp has been reclaimed (the broken-node case).
            let forward = crate::processes::evm::gather_reconstruction_inputs(
                pruning_point,
                |b| {
                    crate::processes::evm::resolve_anchor_snapshot(
                        b,
                        &self.evm_state_checkpoint_v2_store,
                        &self.evm_state_checkpoint_store,
                        &self.evm_code_store,
                    )
                },
                |b| self.evm_state_diff_store.get(b),
                |b| {
                    crate::processes::evm::classify_parent_for_anchor(
                        b,
                        self.evm_activation_daa_score,
                        |h| self.evm_header_store.get(h).optional().unwrap().is_some(),
                        |h| self.headers_store.get_daa_score(h).ok(),
                    )
                },
            )
            .ok()
            .and_then(|(seed, forward_diffs)| {
                kaspa_evm::reconstruct::reconstruct_evm_state(
                    &seed,
                    &forward_diffs,
                    |h| self.evm_code_store.get(*h).ok().flatten(),
                    header.state_root,
                )
                .ok()
            });
            if let Some(snapshot) = forward {
                return Some((header, snapshot));
            }

            // (4) DERIVATION — the path that makes serving self-healing. The forward
            // walk needs an anchor at/below pp; this one needs none. The flat head
            // sits at the tip, the retained (pp, tip] diffs are still on disk, and
            // `apply_inverse_state_diff` reverts them one block at a time down to pp,
            // root-verified against the committed header. So a node with no anchor
            // at all still serves the correct pp state instead of returning nothing
            // (the live-net serveability break) or, worse, the empty state. On
            // success the derived state is CACHED as an anchor, so the next serve
            // takes the fast forward path — the anchor is an optimization, not a
            // precondition.
            let Some(fh) = flat_head else {
                warn!("[evm] pruning-point {pruning_point} cannot be served: no anchor and no flat backend to derive from");
                return None;
            };
            match crate::processes::evm::reconstruct_pruning_point_snapshot_verified(
                pruning_point,
                header.state_root,
                fh,
                &self.evm_flat_account_store,
                &self.evm_code_store,
                &self.evm_state_diff_store,
                Self::PP_DERIVE_MAX_STEPS,
            ) {
                Ok(snapshot) => {
                    self.cache_pruning_point_anchor(pruning_point, header.evm_number, header.state_root, &snapshot);
                    Some((header, snapshot))
                }
                Err(e) => {
                    warn!(
                        "[evm] pruning-point {pruning_point} state could not be derived from the flat head ({e}); \
                         this node cannot serve pruned IBD for the EVM lane"
                    );
                    None
                }
            }
        }
        #[cfg(not(feature = "evm"))]
        None
    }

    /// Max blocks the on-demand serving derivation reverts before giving up —
    /// generous over the pruning depth so a legitimately deep range serves, bounded
    /// so a broken diff chain still terminates.
    #[cfg(feature = "evm")]
    const PP_DERIVE_MAX_STEPS: usize = 4_000_000;

    /// Cache a derived pruning-point state as a §12 v2 anchor.
    ///
    /// Best-effort and never on the critical path: the serve has already succeeded
    /// with the derived state, and this only makes the NEXT serve take the fast
    /// forward path. A write failure is logged and swallowed — a node that cannot
    /// cache the anchor keeps serving by deriving each time, slower but correct.
    #[cfg(feature = "evm")]
    fn cache_pruning_point_anchor(
        &self,
        pruning_point: BlockHash,
        evm_number: u64,
        state_root: kaspa_hashes::EvmH256,
        snapshot: &kaspa_consensus_core::evm::EvmStateSnapshot,
    ) {
        use crate::model::stores::evm::EvmStateCheckpointV2StoreReader;
        if EvmStateCheckpointV2StoreReader::has(&*self.evm_state_checkpoint_v2_store, pruning_point).unwrap_or(false) {
            return;
        }
        let checkpoint = kaspa_consensus_core::evm::EvmStateCheckpointV2::build(
            pruning_point,
            evm_number,
            state_root,
            snapshot,
            self.evm_checkpoint_policy.codec,
        );
        let mut batch = WriteBatch::default();
        if self.evm_state_checkpoint_v2_store.insert_batch(&mut batch, pruning_point, checkpoint).is_ok()
            && self.db.write(batch).is_ok()
        {
            info!("[evm] cached the derived pruning-point {pruning_point} state as an anchor (subsequent serves take the fast path)");
        }
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
    fn bonds_as_of(&self, pp_daa: u64, pp_blue: u64) -> Vec<StakeBondRecord> {
        // Dormancy Fence (PR-D4): the as-of-pp buried epoch (same bury depth as the live
        // transition), so a discrete dormancy event (an eviction round) stamped AFTER pp is
        // nulled — exactly like slash/unbond. pp is deeply buried (pruning_depth ≫ horizon),
        // so this equals what pp's finalized region implied.
        let pp_buried_epoch = self.dns_params.as_ref().and_then(|p| {
            let epoch_len = p.attestation_epoch_length_blue_score.max(1);
            let bury_blue = p.attestation_lag_blue_score.max(p.max_reorg_horizon_blocks);
            ready_epoch_from_tip_blue_score(pp_blue, epoch_len, bury_blue)
        });
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
                // Dormancy Fence: null a dormancy that was stamped AFTER pp (its buried round
                // epoch is past pp's buried epoch) — as-of pp the bond was not yet Dormant. This
                // is exact (a discrete event). `status` is re-normalized by compute_overlay_snapshot.
                //
                // ⚠️ NOT YET EXACT (Blocker 2, fence-gated): `last_attested_epoch` is an
                // overwrite-with-latest field, so its as-of-pp value is NOT recoverable by a
                // clamp here — `min(e, pp_buried_epoch)` OVER-estimates for a bond whose last
                // pre-pp attestation predates pp_buried_epoch, still diverging a pruned importer's
                // root. Specified fix (see `stage_dormancy_transitions` doc): source Active bonds'
                // `last_attested` from the committed, pruning-survivable rewarded-epoch overlay
                // window (`rewarded_epochs_store`, reconstructable byte-exactly here from the
                // snapshot window) under invariant I7, not a prune-time clamp — left untouched
                // rather than clamped wrongly. Dormant bonds need no exact value (revival replays
                // a post-pp attestation live).
                if let Some(cap) = pp_buried_epoch
                    && rec.dormant_at_epoch.is_some_and(|e| e > cap)
                {
                    rec.dormant_at_daa_score = None;
                    rec.dormant_at_epoch = None;
                    rec.status = BondStatus::Active;
                }
                rec
            })
            .collect()
    }

    /// Build the overlay snapshot as-of `pruning_point` while below-boundary source rows still
    /// exist. The pruning processor stages this value in the SAME batch as the new pruning-point
    /// pointer and PALW/DA sidecars, so a crash cannot expose a new point with an old DNS snapshot.
    pub fn build_pruning_point_overlay_snapshot(&self, pruning_point: BlockHash) -> Option<PruningPointOverlaySnapshot> {
        self.dns_params.as_ref()?;
        let pp_daa = self.headers_store.get_daa_score(pruning_point).unwrap();
        let pp_blue = self.headers_store.get_blue_score(pruning_point).unwrap();
        let view = ActiveBondView::from_records(self.bonds_as_of(pp_daa, pp_blue).into_iter().map(|r| (r.bond_outpoint, r)));
        let snapshot = self.compute_overlay_snapshot(pruning_point, &view);
        Some(PruningPointOverlaySnapshot { pruning_point, snapshot })
    }

    /// Direct capture helper retained for explicit maintenance callers. Periodic pruning uses
    /// [`Self::build_pruning_point_overlay_snapshot`] and batches the result with the pointer move.
    pub fn capture_pruning_point_overlay_snapshot(&self, pruning_point: BlockHash) {
        let Some(snapshot) = self.build_pruning_point_overlay_snapshot(pruning_point) else {
            return;
        };
        let mut batch = WriteBatch::default();
        self.pruning_overlay_snapshot_store.write().set_batch(&mut batch, snapshot).unwrap();
        self.db.write(batch).unwrap();
    }

    fn palw_is_active_at(&self, daa_score: u64) -> bool {
        self.palw_activation_daa_score != u64::MAX && daa_score >= self.palw_activation_daa_score
    }

    /// Materialize the canonical pruning transport for a compact lifecycle view. See
    /// [`DbPalwStore::pruning_active_batches`] for the fail-closed raw-body/accepted-content seam.
    fn palw_active_batch_bundles(
        &self,
        view: Option<&kaspa_consensus_core::palw::PalwBatchViewV1>,
    ) -> Result<Vec<PalwPrunedActiveBatchV1>, String> {
        self.palw_store.pruning_active_batches(view)
    }

    fn provider_bonds_as_of(&self, pruning_point_daa_score: u64) -> Result<Vec<PalwProviderBondRecord>, String> {
        let store = self.palw_provider_bonds_store.read();
        store
            .iterator()
            .map(|row| {
                let (_, record) = row.map_err(|err| format!("provider-registry iteration failed: {err}"))?;
                Ok((*record).clone())
            })
            .filter_map(|row: Result<PalwProviderBondRecord, String>| match row {
                Ok(record) if record.created_daa_score > pruning_point_daa_score => None,
                Ok(mut record) => {
                    if record.unbond_request_daa_score.is_some_and(|daa| daa > pruning_point_daa_score) {
                        record.unbond_request_daa_score = None;
                    }
                    if record.slashed_at_daa_score.is_some_and(|daa| daa > pruning_point_daa_score) {
                        record.slashed_at_daa_score = None;
                    }
                    Some(Ok(record))
                }
                Err(err) => Some(Err(err)),
            })
            .collect()
    }

    /// Deterministically materialize every PALW component needed at a pruning boundary. The caller
    /// writes the returned value in the SAME RocksDB batch as the new pruning-point pointer, before
    /// any below-boundary row can be reclaimed.
    /// ADR-MA §21.3 — the Compute Set registry component of the pruning snapshot: the FULL
    /// content-addressed record tiers plus the fork-local view at the pruning point. `None`
    /// while the registry fence is closed at the boundary (every shipped preset). Full history
    /// is the sound minimum, not archival generosity: §21.4 resolves every retained header's
    /// committed record ids against these exact bytes, and admission is proposal-bonded
    /// (§23.2), so the tiers stay small.
    fn build_palw_compute_registry_pruning_snapshot(
        &self,
        pruning_point: BlockHash,
        pruning_point_daa_score: u64,
    ) -> Result<Option<kaspa_consensus_core::palw_pruned_frontier::PalwComputeRegistryPruningSnapshotV1>, String> {
        use kaspa_consensus_core::palw_pruned_frontier::{
            PALW_COMPUTE_REGISTRY_PRUNING_SNAPSHOT_VERSION, PalwComputeRegistryPruningSnapshotV1,
        };
        if self.palw_compute_registry_activation_daa_score == u64::MAX
            || pruning_point_daa_score < self.palw_compute_registry_activation_daa_score
        {
            return Ok(None);
        }
        let store = self.palw_compute_registry_store.as_ref();
        let view = {
            use crate::model::stores::palw_compute_registry::PalwComputeRegistryStoreReader;
            store
                .view(pruning_point)
                .map_err(|err| format!("compute registry view read failed at {pruning_point}: {err}"))?
                .map(|view| (*view).clone())
                .unwrap_or_default()
        };
        let mut snapshot = PalwComputeRegistryPruningSnapshotV1 {
            version: PALW_COMPUTE_REGISTRY_PRUNING_SNAPSHOT_VERSION,
            pruning_point,
            descriptors: store.all_descriptors().map_err(|err| format!("descriptor tier scan at {pruning_point}: {err}"))?,
            policies: store.all_policies().map_err(|err| format!("policy tier scan at {pruning_point}: {err}"))?,
            plans: store.all_plans().map_err(|err| format!("plan tier scan at {pruning_point}: {err}"))?,
            activation_certificates: store
                .all_certificates()
                .map_err(|err| format!("certificate tier scan at {pruning_point}: {err}"))?,
            view,
        };
        snapshot.canonicalize();
        Ok(Some(snapshot))
    }

    pub fn build_palw_pruning_point_snapshot(&self, pruning_point: BlockHash) -> Result<PalwPruningPointSnapshotV1, String> {
        let pruning_point_header = self
            .headers_store
            .get_header(pruning_point)
            .map_err(|err| format!("missing pruning-point header {pruning_point}: {err}"))?;
        let pruning_point_daa_score = pruning_point_header.daa_score;
        let paid_work_window_daa = self.palw_batch_admission.paid_work_walk_bound_daa(self.palw_epoch_length_daa);
        let active = self.palw_is_active_at(pruning_point_daa_score);

        let beacon_state = self
            .palw_beacon_store
            .beacon_state(pruning_point)
            .map_err(|err| format!("beacon-state read failed at {pruning_point}: {err}"))?
            .map(|state| (*state).clone());
        let overlay_view = self
            .palw_overlay_view_store
            .view(pruning_point)
            .map_err(|err| format!("overlay-view read failed at {pruning_point}: {err}"))?
            .map(|view| (*view).clone());
        let lane_bits = self
            .palw_lane_bits_store
            .lane_bits(pruning_point)
            .map_err(|err| format!("lane-bits read failed at {pruning_point}: {err}"))?;
        let active_nullifiers = self
            .palw_nullifier_store
            .get(pruning_point)
            .optional()
            .map_err(|err| format!("active-nullifier read failed at {pruning_point}: {err}"))?
            .map(|set| (*set).clone())
            .unwrap_or_default();
        let beacon_accumulator = self
            .palw_beacon_store
            .accum_view(pruning_point)
            .map_err(|err| format!("beacon-accumulator read failed at {pruning_point}: {err}"))?
            .map(|view| PalwPrunedBeaconAccumulatorV1 {
                version: view.version,
                epochs: view.epochs.clone(),
                stake_by_epoch: view.stake_by_epoch.clone(),
            });
        let active_batches = self.palw_active_batch_bundles(overlay_view.as_ref())?;
        let spam_active = !self.palw_spam.is_inert()
            && pruning_point_header.version >= kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION;
        let spam_accumulator = if spam_active {
            let mut retained = palw_spam_retained_path(self.palw_spam_store.as_ref(), pruning_point, self.palw_spam.window_daa)
                .map_err(|err| format!("cannot capture bounded spam closure at {pruning_point}: {err}"))?
                .into_iter();
            let (captured_pp, pp_state) =
                retained.next().ok_or_else(|| format!("active Header-v4 pruning point {pruning_point} has an empty spam closure"))?;
            if captured_pp != pruning_point {
                return Err("bounded spam closure did not start at the pruning point".to_string());
            }
            if pp_state.commitment() != pruning_point_header.palw_spam_accumulator_commitment {
                return Err(format!("spam accumulator at {pruning_point} disagrees with the PP header commitment"));
            }
            let to_transport = |state: &PalwSpamAccumulatorV1| PalwPrunedSpamAccumulatorV1 {
                version: state.version,
                daa_score: state.daa_score,
                selected_height: state.selected_height,
                total_hash_blues: state.total_hash_blues,
                total_replica_blues: state.total_replica_blues,
                selected_parent: state.selected_parent,
                skip: state.skip,
            };
            let support_rows: Vec<PalwPrunedSpamSupportRowV1> =
                retained.map(|(block_hash, state)| PalwPrunedSpamSupportRowV1 { block_hash, state: to_transport(&state) }).collect();
            if support_rows.len() > MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS {
                return Err(format!(
                    "spam pruning support has {} rows, exceeding the {}-row transport bound",
                    support_rows.len(),
                    MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS
                ));
            }
            Some(PalwPrunedSpamFrontierV1 { pruning_point_state: to_transport(&pp_state), support_rows })
        } else {
            None
        };

        if active && pruning_point != self.genesis.hash {
            if beacon_state.is_none() || overlay_view.is_none() || lane_bits.is_none() || beacon_accumulator.is_none() {
                return Err(format!(
                    "active PALW pruning point {pruning_point} is missing a required beacon/view/lane/accumulator component"
                ));
            }
            if !self.palw_nullifier_store.has(pruning_point).map_err(|err| format!("nullifier presence read failed: {err}"))? {
                return Err(format!("active PALW pruning point {pruning_point} is missing its nullifier row"));
            }
        }

        // DA has no independent activation fence in the re-genesis release. A non-genesis PALW
        // boundary therefore always carries even an empty v1 DA state; omission would otherwise
        // silently forget live obligations, challenges and timeout-dedup state after IBD.
        let da_snapshot = if active && pruning_point != self.genesis.hash {
            let state = self
                .palw_da_store
                .read()
                .state(pruning_point)
                .map_err(|err| format!("DA state read failed at active pruning point {pruning_point}: {err}"))?;
            let snapshot = PalwDaPruningSnapshotV1 {
                version: kaspa_consensus_core::palw::da::PALW_DA_SNAPSHOT_VERSION_V1,
                pruning_point,
                state: (*state).clone(),
            };
            if !snapshot.validate() {
                return Err(format!("DA state at active pruning point {pruning_point} is not a valid pruning snapshot"));
            }
            Some(snapshot)
        } else {
            None
        };

        // Search availability rides the same rule as DA: a non-genesis PALW boundary always carries
        // even an empty v1 state, or open search obligations/deadlines would be forgotten after IBD.
        let search_availability_snapshot = if active && pruning_point != self.genesis.hash {
            let state = self
                .palw_search_availability_store
                .read()
                .state(pruning_point)
                .map_err(|err| format!("search state read failed at active pruning point {pruning_point}: {err}"))?;
            let snapshot =
                PalwSearchPruningSnapshotV1 { version: PALW_SEARCH_SNAPSHOT_STATE_VERSION_V1, pruning_point, state: (*state).clone() };
            if !snapshot.validate() {
                return Err(format!("search state at active pruning point {pruning_point} is not a valid pruning snapshot"));
            }
            Some(snapshot)
        } else {
            None
        };

        let mut paid_work = Vec::new();
        if active {
            // `default_backward_chain_iterator` yields its start INCLUSIVELY — a prepended
            // `once(pruning_point)` double-counted the pruning point's own row, which the snapshot
            // writer's dup-block coherence check then rejected on every capture (the second
            // pruning-point-pinning defect found by the ADR-0044 long-chain harness).
            for block in self.reachability_service.default_backward_chain_iterator(pruning_point) {
                let block_daa_score = self
                    .headers_store
                    .get_daa_score(block)
                    .map_err(|err| format!("paid-work boundary header {block} is unavailable: {err}"))?;
                if pruning_point_daa_score.saturating_sub(block_daa_score) > paid_work_window_daa {
                    break;
                }
                // Presence of acceptance data is the recovery proof that this source row has not
                // already been pruned. `PalwPaidWork` intentionally omits empty rows, so absence in
                // that store alone cannot distinguish "paid nothing" from "was reclaimed".
                self.acceptance_data_store
                    .get(block)
                    .map_err(|err| format!("paid-work source acceptance data {block} is unavailable: {err}"))?;
                let job_nullifiers = self
                    .palw_paid_work_store
                    .get(block)
                    .optional()
                    .map_err(|err| format!("paid-work row read failed at {block}: {err}"))?
                    .map(|ids| (*ids).clone())
                    .unwrap_or_default();
                paid_work.push(PalwPrunedPaidWorkBlockV1 { block_hash: block, block_daa_score, job_nullifiers });
            }
        }

        let compute_registry_snapshot = self.build_palw_compute_registry_pruning_snapshot(pruning_point, pruning_point_daa_score)?;
        let snapshot = PalwPruningPointSnapshotV1::try_new(PalwPruningPointSnapshotPayloadV1 {
            version: PALW_PRUNING_SNAPSHOT_VERSION,
            pruning_point,
            pruning_point_daa_score,
            paid_work_window_daa,
            frontier: PalwPrunedFrontierV1 { beacon_state, overlay_view, lane_bits, active_nullifiers },
            beacon_accumulator,
            spam_accumulator,
            da_snapshot,
            search_availability_snapshot,
            compute_registry_snapshot,
            active_batches,
            provider_bonds: if active { self.provider_bonds_as_of(pruning_point_daa_score)? } else { Vec::new() },
            paid_work,
        })
        .map_err(|err| format!("PALW pruning snapshot writer rejected {pruning_point}: {err}"))?;
        self.validate_palw_pruning_snapshot(
            &snapshot,
            pruning_point,
            pruning_point_daa_score,
            pruning_point_header.version,
            pruning_point_header.palw_spam_accumulator_commitment,
        )
        .map_err(|err| err.to_string())?;
        Ok(snapshot)
    }

    fn validate_palw_pruning_snapshot(
        &self,
        snapshot: &PalwPruningPointSnapshotV1,
        expected_pruning_point: BlockHash,
        expected_daa_score: u64,
        expected_header_version: u16,
        expected_spam_commitment: Hash64,
    ) -> PruningImportResult<()> {
        snapshot
            .validate_canonical()
            .map_err(|err| PruningImportError::ImportedPalwSnapshotInvalid(expected_pruning_point, err.to_string()))?;
        let payload = &snapshot.payload;
        if payload.pruning_point != expected_pruning_point {
            return Err(PruningImportError::ImportedPalwSnapshotPointMismatch(expected_pruning_point, payload.pruning_point));
        }
        if payload.pruning_point_daa_score != expected_daa_score {
            return Err(PruningImportError::ImportedPalwSnapshotDaaMismatch(
                expected_pruning_point,
                expected_daa_score,
                payload.pruning_point_daa_score,
            ));
        }
        let expected_bound = self.palw_batch_admission.paid_work_walk_bound_daa(self.palw_epoch_length_daa);
        if payload.paid_work_window_daa != expected_bound {
            return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                expected_pruning_point,
                format!("paid-work bound {} does not match local bound {expected_bound}", payload.paid_work_window_daa),
            ));
        }
        let active_non_genesis = self.palw_is_active_at(expected_daa_score) && expected_pruning_point != self.genesis.hash;
        if active_non_genesis
            && (payload.frontier.beacon_state.is_none()
                || payload.frontier.overlay_view.is_none()
                || payload.frontier.lane_bits.is_none()
                || payload.beacon_accumulator.is_none()
                || payload.da_snapshot.is_none()
                || payload.search_availability_snapshot.is_none())
        {
            return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                expected_pruning_point,
                "active snapshot is missing a required frontier, DA or search-availability component".to_string(),
            ));
        }
        if !active_non_genesis && payload.da_snapshot.is_some() {
            return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                expected_pruning_point,
                "snapshot carries DA state before PALW activation or at genesis".to_string(),
            ));
        }
        if !active_non_genesis && payload.search_availability_snapshot.is_some() {
            return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                expected_pruning_point,
                "snapshot carries search-availability state before PALW activation or at genesis".to_string(),
            ));
        }
        let spam_required =
            !self.palw_spam.is_inert() && expected_header_version >= kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION;
        match (&payload.spam_accumulator, spam_required) {
            (Some(spam), true) if spam.pruning_point_state.commitment() == expected_spam_commitment => {
                validate_pruned_spam_closure(expected_pruning_point, expected_daa_score, self.palw_spam.window_daa, spam)
                    .map_err(|err| PruningImportError::ImportedPalwSnapshotInvalid(expected_pruning_point, err))?;
            }
            (Some(_), true) => {
                return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                    expected_pruning_point,
                    "Header-v4 spam frontier does not match the pruning-point header commitment".to_string(),
                ));
            }
            (None, true) => {
                return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                    expected_pruning_point,
                    "active Header-v4 snapshot is missing its spam frontier".to_string(),
                ));
            }
            (Some(_), false) => {
                return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                    expected_pruning_point,
                    "snapshot carries a spam frontier before Header-v4 anti-spam activation".to_string(),
                ));
            }
            (None, false) => {}
        }
        Ok(())
    }

    fn validate_palw_pruning_snapshot_context(&self, snapshot: &PalwPruningPointSnapshotV1) -> PruningImportResult<()> {
        let pp = snapshot.payload.pruning_point;
        let header = self
            .headers_store
            .get_header(pp)
            .map_err(|err| PruningImportError::ImportedPalwSnapshotInvalid(pp, format!("pruning-point header unavailable: {err}")))?;
        self.validate_palw_pruning_snapshot(snapshot, pp, header.daa_score, header.version, header.palw_spam_accumulator_commitment)
    }

    /// Serve only a fully validated current snapshot. The embedded DA boundary and the standalone DA
    /// singleton are one durability unit and must match exactly. A stale/corrupt/partial boundary is
    /// unavailable, not downgraded to the old partial `PalwPrunedFrontierV1`.
    pub fn pruning_point_palw_snapshot(&self) -> Option<PalwPruningPointSnapshotV1> {
        let snapshot = self.palw_pruned_frontier_store.read().get().ok()?;
        let current_pp = self.pruning_point_store.read().pruning_point().ok()?;
        if snapshot.payload.pruning_point != current_pp || self.validate_palw_pruning_snapshot_context(&snapshot).is_err() {
            return None;
        }
        if let Some(embedded_da) = snapshot.payload.da_snapshot.as_ref() {
            let standalone_da = self.palw_da_store.read().pruning_snapshot().ok();
            if !palw_da_boundary_matches(embedded_da, standalone_da.as_ref()) {
                return None;
            }
        }
        if let Some(embedded_search) = snapshot.payload.search_availability_snapshot.as_ref() {
            let standalone_search = self.palw_search_availability_store.read().pruning_snapshot().ok();
            if !palw_search_boundary_matches(embedded_search, standalone_search.as_ref()) {
                return None;
            }
        }
        Some(snapshot)
    }

    /// ADR-0042, chain-derived provenance ONLY: bind every transported paid-work row's ATTRIBUTION —
    /// `block_hash`, `block_daa_score`, and which nullifiers sit in that row — to this node's own
    /// stores, because no commitment covers it.
    ///
    /// `verify_chain_derived_pruning_boundary_from_payload` authenticates the payload against the
    /// descendant's `overlay_commitment_root`, and that fold sees only the deduplicated UNION of the
    /// in-window nullifiers. A peer can hold the union byte-identical while re-dating a row or moving
    /// a nullifier between rows; the fold at the pruning point still matches exactly, but
    /// [`Self::palw_paid_work_window`] re-runs `anchor_daa - row.block_daa_score <= walk_bound`
    /// against an ADVANCING anchor, so the installed window diverges from the network's a few epochs
    /// later and the node then rejects HONEST blocks with `BadOverlayCommitment`. Committing the
    /// attribution instead is not an option: the live counterpart builds its `paid_work_nullifiers`
    /// from an attribution-free `HashSet<Hash64>`, so folding attribution would move the value every
    /// Header-v4 block already commits.
    ///
    /// Resolution rules, all fail-closed:
    /// * a header this node does not have resolves to `None`, which the verifier turns into a
    ///   REFUSAL — an unverifiable row is exactly the row an attacker would fabricate, and there is
    ///   deliberately no "skip when unavailable" branch;
    /// * `KeyNotFound` from the paid-work store is a genuine absence and means "this block paid
    ///   nothing" (the store omits empty rows), so it resolves to the empty set;
    /// * every OTHER store error is an IO/corruption fault and is surfaced as a rejection rather than
    ///   laundered into either of the above.
    ///
    /// The rule itself, and the argument that it plus the fold is complete, live in
    /// `kaspa_consensus_core::palw_pruned_frontier::verify_chain_derived_paid_work_attribution`.
    fn bind_chain_derived_paid_work_attribution(
        &self,
        pruning_point: BlockHash,
        payload: &PalwPruningPointSnapshotPayloadV1,
    ) -> PruningImportResult<()> {
        // R2 (DoS) — bound the attacker-directed store work BEFORE doing any of it. Each row costs two
        // RocksDB point lookups on a key the peer chose, and until this check the only ceiling was
        // MAX_PALW_PRUNING_PAID_BLOCKS (1_000_001) inside validate_paid_work, i.e. ~2M lookups per import
        // attempt. `prepare`'s only store access used to be the pruning-point header, so this is new
        // attacker-directed work on the pre-write path; refuse the oversized payload up front rather than
        // discovering it row by row. (Same class as the cardinality fence moved earlier in ibd/flow.rs.)
        // The capacity reservation is deliberately AFTER this check — allocating from a peer-supplied
        // length is itself the first unit of attacker-directed work.
        if payload.paid_work.len() > kaspa_consensus_core::palw_pruned_frontier::MAX_PALW_PRUNING_PAID_BLOCKS {
            return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                pruning_point,
                format!(
                    "paid-work row count {} exceeds the {} ceiling — refusing before any store lookup",
                    payload.paid_work.len(),
                    kaspa_consensus_core::palw_pruned_frontier::MAX_PALW_PRUNING_PAID_BLOCKS
                ),
            ));
        }
        // R1 — bind the ROW SET to this node's selected chain.
        //
        // Every check below this point is per-row: it asks "is what you claim about block X true?".
        // Nothing asked "should block X be here at all?", so a peer could pad the payload with rows
        // for blocks that are merely in the DAA window and have a header — off-chain blocks, or
        // on-chain blocks it chose to include or omit. Such a row passes: the header resolves, the
        // DAA matches, and `palw_paid_work_store` legitimately holds nothing for a block that paid
        // nothing, so the peer's empty claim matches our empty answer. The union the
        // `overlay_commitment_root` fold covers is unchanged (empty rows contribute nothing), so the
        // fold cannot see it either. The damage is not a fork: the installed window is unaffected,
        // but the snapshot this node PERSISTS and re-serves is no longer canonical, so its
        // `payload_digest` differs and operator-pinned peers refuse to sync from it — the node is
        // poisoned as an IBD source.
        //
        // The canonical set is not a filter over the window, it is a WALK: the builder starts at the
        // pruning point (inclusively) and follows selected parents while the row stays inside
        // `paid_work_walk_bound_daa`, emitting one row per block — INCLUDING blocks that paid
        // nothing, which is why "reject empty rows" is not the fix and would break honest imports.
        //
        // So reproduce that walk locally and require an exact sequence match. The walk uses the
        // ghostdag selected parent rather than `default_backward_chain_iterator`: both yield the
        // same chain, but ghostdag data is exactly what the IBD trusted-data package carries
        // (`TrustedHeader { header, ghostdag }`), so this holds at import time without depending on
        // reachability having been rebuilt. Coverage is sufficient by construction — trusted data
        // spans this preset's sampled difficulty window around the pruning point while the walk needs
        // `paid_work_walk_bound_daa`; `trusted_data_covers_the_paid_work_walk` pins that relationship.
        //
        // Bounds are LOCAL on purpose. `pruning_point_daa_score` and `paid_work_window_daa` in the
        // payload are peer-supplied; using them here would let the peer choose the very walk it is
        // being checked against.
        let local_pp_daa_score = self.headers_store.get_daa_score(pruning_point).map_err(|err| {
            PruningImportError::ImportedPalwSnapshotInvalid(
                pruning_point,
                format!("pruning-point header {pruning_point} is unreadable: {err}"),
            )
        })?;
        let local_walk_bound = self.palw_batch_admission.paid_work_walk_bound_daa(self.palw_epoch_length_daa);
        // Collected as (daa, hash) because the comparison happens in CANONICAL order, not walk
        // order: `validate_canonical` sorts `paid_work` by (block_daa_score, block_hash), so a
        // walk-ordered (newest-first) expectation would mismatch an honest payload on row 0.
        let mut expected_chain: Vec<(u64, BlockHash)> = Vec::new();
        let mut cursor = pruning_point;
        while cursor != kaspa_consensus_core::blockhash::ORIGIN {
            let daa_score = match self.headers_store.get_daa_score(cursor) {
                Ok(daa_score) => daa_score,
                // The walk ran past what this node holds. Fail closed: a truncated expectation would
                // silently accept whatever the peer sent beyond that point, which is the hole itself.
                Err(err) => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        format!(
                            "cannot rebuild the paid-work chain: header {cursor} is unavailable ({err}); \
                             refusing rather than accepting an unverifiable row set"
                        ),
                    ));
                }
            };
            if local_pp_daa_score.saturating_sub(daa_score) > local_walk_bound {
                break;
            }
            expected_chain.push((daa_score, cursor));
            if expected_chain.len() > kaspa_consensus_core::palw_pruned_frontier::MAX_PALW_PRUNING_PAID_BLOCKS {
                return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                    pruning_point,
                    "the local paid-work chain walk exceeded the row ceiling".to_owned(),
                ));
            }
            let selected_parent = match self.ghostdag_store.get_selected_parent(cursor) {
                Ok(parent) => parent,
                Err(err) => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        format!(
                            "cannot rebuild the paid-work chain: ghostdag data for {cursor} is unavailable ({err}); \
                             refusing rather than accepting an unverifiable row set"
                        ),
                    ));
                }
            };
            // Genesis's selected parent is ORIGIN, a sentinel with no header; the loop condition
            // ends the walk there rather than looking the sentinel up and calling it "unavailable",
            // which would refuse every honest short-chain import. The self-reference guard is
            // belt-and-braces against a malformed store looping forever.
            if selected_parent == cursor {
                break;
            }
            cursor = selected_parent;
        }
        let mut local_facts: Vec<Option<PalwLocalPaidWorkFactV1>> = Vec::with_capacity(payload.paid_work.len());
        for row in &payload.paid_work {
            let block_daa_score = match self.headers_store.get_daa_score(row.block_hash) {
                Ok(daa_score) => daa_score,
                Err(StoreError::KeyNotFound(_)) => {
                    local_facts.push(None);
                    continue;
                }
                Err(err) => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        format!("paid-work source header {} is unreadable: {err}", row.block_hash),
                    ));
                }
            };
            // R3 — do NOT conflate "this node pruned its own copy" with "the block paid nothing".
            // `palw_paid_work_store` rows are deleted by the pruning processor while the header can
            // survive, so `.unwrap_or_default()` would compare an HONEST non-empty row against a locally
            // empty set and reject it as "does not match the locally recorded nullifiers" — reading as
            // peer misbehaviour when the truth is local unavailability. That is the first failure a soak
            // hits and the one most likely to be misdiagnosed. Absence is only equivalent to empty when
            // the row itself is empty (the store omits empty rows); otherwise it is OUR gap, and we say so.
            let stored = self.palw_paid_work_store.get(row.block_hash).optional().map_err(|err| {
                PruningImportError::ImportedPalwSnapshotInvalid(
                    pruning_point,
                    format!("paid-work row for {} is unreadable: {err}", row.block_hash),
                )
            })?;
            let job_nullifiers = match stored {
                Some(ids) => (*ids).clone(),
                // Store miss + non-empty claim => we cannot adjudicate it. Still fail-closed (we refuse
                // the import), but with an accurate cause so the operator does not hunt a malicious peer.
                None if !row.job_nullifiers.is_empty() => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        format!(
                            "cannot verify the paid-work attribution of {}: this node no longer holds its \
                             paid-work record (pruned locally), so the peer's non-empty claim is \
                             unadjudicable here — this is a LOCAL data gap, not evidence of peer misbehaviour; \
                             import via the operator-pinned checkpoint instead",
                            row.block_hash
                        ),
                    ));
                }
                None => Vec::new(),
            };
            local_facts.push(Some(PalwLocalPaidWorkFactV1 { block_daa_score, job_nullifiers }));
        }
        kaspa_consensus_core::palw_pruned_frontier::verify_chain_derived_paid_work_attribution(payload, &local_facts)
            .map_err(|err| PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, err.to_string()))?;

        // Same comparator `validate_canonical` applies to the payload.
        expected_chain.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.as_bytes().cmp(&b.1.as_bytes())));
        let expected_chain: Vec<BlockHash> = expected_chain.into_iter().map(|(_, hash)| hash).collect();
        let transported: Vec<BlockHash> = payload.paid_work.iter().map(|row| row.block_hash).collect();
        if transported != expected_chain {
            // Name the first divergence: "7 vs 7" alone cannot distinguish a reordering from a
            // substitution, and this is the message an operator gets during a failed IBD.
            let at = transported.iter().zip(expected_chain.iter()).position(|(t, e)| t != e);
            let detail = match at {
                Some(i) => format!("first divergence at row {i}: got {}, expected {}", transported[i], expected_chain[i]),
                None => "one is a prefix of the other".to_owned(),
            };
            return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                pruning_point,
                format!(
                    "paid-work rows are not this node's selected chain from the pruning point: expected {} row(s) \
                     walking selected parents within {local_walk_bound} DAA, got {} — {detail}",
                    expected_chain.len(),
                    transported.len()
                ),
            ));
        }

        Ok(())
    }

    /// Canonically validate and atomically install the PALW boundary state. The pruning-point header
    /// must already be present from the validated pruning proof/header sync, and every caller-supplied
    /// scalar is independently compared with that stored header before sidecar staging.
    pub(crate) fn prepare_pruning_point_palw_snapshot_import(
        &self,
        pruning_point: BlockHash,
        expected_daa_score: u64,
        expected_header_version: u16,
        expected_spam_commitment: Hash64,
        import_auth: PalwPruningSnapshotImportAuth,
        snapshot: PalwPruningPointSnapshotV1,
        chain_derived: Option<&PalwChainDerivedAuthBundleV1>,
    ) -> PruningImportResult<PreparedPalwPruningPointSnapshotImport> {
        // Fenced permissionless path (ADR-0042). It is entered only when ALL THREE hold:
        //   1. the node-local `Config::palw_permissionless_snapshot_auth` lever is set — default
        //      false, set by no preset and by no CLI default;
        //   2. the caller supplied a chain-derived authentication bundle; and
        //   3. the caller's `import_auth` actually claims `ChainDerivedHeaderBundle` provenance.
        //
        // (3) is load-bearing and not redundant with (2): without it, a caller that supplied a bundle
        // alongside an operator-pinned auth would take the chain-derived branch and thereby SKIP the
        // payload-digest equality check below — a weakening of the shipped operator path. Instead a
        // bundle presented with any other provenance is refused outright.
        //
        // With the lever off `chain_derived_auth` is `None` for every input, so everything below is
        // byte-for-byte the pre-ADR-0042 behaviour.
        let chain_derived_auth = match (chain_derived.filter(|_| self.palw_permissionless_snapshot_auth), import_auth.provenance) {
            (Some(bundle), PalwPruningSnapshotImportProvenance::ChainDerivedHeaderBundle) => Some(bundle),
            (Some(_), provenance) => {
                return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                    pruning_point,
                    format!(
                        "a chain-derived authentication bundle was supplied with {provenance:?} provenance; \
                         the bundle must never displace that provenance's own authentication"
                    ),
                ));
            }
            (None, _) => None,
        };
        match chain_derived_auth {
            Some(_) => {
                // No operator pin. The gate fixes the header version to v4; enforce the same
                // pre-activation guard `validate_palw_snapshot_import_auth_context` enforces.
                if !kaspa_consensus_core::palw_pruned_frontier::palw_pruned_ibd_chain_derived_import_allowed(
                    true,
                    expected_header_version,
                ) {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        "chain-derived pruning-snapshot import requires Header-v4".to_string(),
                    ));
                }
                validate_palw_chain_derived_import_auth_context(
                    expected_daa_score,
                    expected_header_version,
                    pruning_point,
                    &import_auth,
                    self.palw_activation_daa_score,
                    self.palw_spam.is_inert(),
                    self.palw_permissionless_snapshot_auth,
                )
                .map_err(|err| PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, err))?;
            }
            None => {
                validate_palw_snapshot_import_auth_context(
                    expected_daa_score,
                    expected_header_version,
                    pruning_point,
                    &import_auth,
                    &self.palw_pruning_snapshot_checkpoints,
                    self.palw_activation_daa_score,
                    self.palw_spam.is_inert(),
                    self.palw_permissionless_snapshot_auth,
                )
                .map_err(|err| PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, err))?;
            }
        }
        let header = self.headers_store.get_header(pruning_point).map_err(|err| {
            PruningImportError::ImportedPalwSnapshotInvalid(
                pruning_point,
                format!("validated pruning-point header is not locally available: {err}"),
            )
        })?;
        if header.daa_score != expected_daa_score
            || header.version != expected_header_version
            || header.palw_spam_accumulator_commitment != expected_spam_commitment
        {
            return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                pruning_point,
                format!(
                    "caller pruning-point metadata does not match the locally stored header (DAA {}/{}, version {}/{}, spam commitment {}/{})",
                    expected_daa_score,
                    header.daa_score,
                    expected_header_version,
                    header.version,
                    expected_spam_commitment,
                    header.palw_spam_accumulator_commitment,
                ),
            ));
        }
        self.validate_palw_pruning_snapshot(
            &snapshot,
            pruning_point,
            expected_daa_score,
            expected_header_version,
            expected_spam_commitment,
        )?;
        match chain_derived_auth {
            Some(bundle) => {
                // ADR-0042 review point (c) — the support-row closure binding — is enforced HERE, and
                // nowhere later. `verify_chain_derived_pruning_boundary_from_payload` derives the
                // paid-work nullifier window and the DA state root from the transported payload, then
                // calls `verify_chain_derived_pruning_boundary`, which
                //   (1) reconstructs the selected-parent state from that same payload and requires
                //       `palw_overlay_commitment_root_v2(legacy_root, state_root)` to equal the
                //       chain-authenticated descendant's `overlay_commitment_root`, and
                //   (2) calls `verify_support_rows_against_transported_headers`, requiring every
                //       anti-spam support row — plus the pruning-point row itself — to be bound, one
                //       for one with no extras and no omissions, to a transported Header-v4 preimage
                //       whose `palw_spam_accumulator_commitment` equals that row's commitment.
                //
                // This runs strictly BEFORE `stage_prepared_pruning_point_palw_snapshot_import`: this
                // function only ever returns a read-only `PreparedPalwPruningPointSnapshotImport`, and
                // staging is the caller's next statement. Nothing may be moved after that boundary,
                // because from there on a failure is process-fatal rather than recoverable.
                //
                // This function does not establish that the descendant header is real:
                // proof of work, chain membership and burial under the adopted chain are the wiring
                // layer's obligation (review points (a′)/(b)), discharged before the bundle is handed
                // to this function. `extract_authenticated_bundle` authenticates nothing.
                let walk_bound = self.palw_batch_admission.paid_work_walk_bound_daa(self.palw_epoch_length_daa);
                kaspa_consensus_core::palw_pruned_frontier::verify_chain_derived_pruning_boundary_from_payload(
                    &snapshot.payload,
                    walk_bound,
                    bundle,
                )
                .map_err(|err| PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, err.to_string()))?;
                // The fold above authenticates every field the selected-parent `state_root` covers —
                // and the paid-work ATTRIBUTION is not one of them (see
                // `verify_chain_derived_paid_work_attribution`). Bind it against this node's own
                // stores, still strictly before staging. Chain-derived provenance ONLY: the
                // operator-pin and Header-v3 arms below authenticate these bytes verbatim through
                // their payload digest and must keep their exact shipped behaviour.
                self.bind_chain_derived_paid_work_attribution(pruning_point, &snapshot.payload)?;
            }
            None => {
                if snapshot.payload_digest != import_auth.checkpoint.payload_digest {
                    return Err(PruningImportError::ImportedPalwSnapshotDigestMismatch(
                        pruning_point,
                        import_auth.checkpoint.payload_digest,
                        snapshot.payload_digest,
                    ));
                }
            }
        }

        let payload = &snapshot.payload;
        let active = self.palw_is_active_at(expected_daa_score);

        // Preflight every collision/mismatch before mutating any store cache or staging any write.
        // A rejected sidecar must leave both RocksDB and the in-process view byte-for-byte unchanged.
        for row in &payload.active_batches {
            match self.palw_store.batch_manifest(row.batch_id).optional().map_err(|err| {
                PruningImportError::ImportedPalwSnapshotInvalid(
                    pruning_point,
                    format!("existing active-batch manifest read {}: {err}", row.batch_id),
                )
            })? {
                Some(existing) if *existing != row.manifest => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        format!("active-batch manifest {} collides with different local content", row.batch_id),
                    ));
                }
                _ => {}
            }
            for leaf in &row.leaves {
                match self.palw_store.leaf(row.batch_id, leaf.leaf_index).optional().map_err(|err| {
                    PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        format!("existing active-batch leaf read ({}, {}): {err}", row.batch_id, leaf.leaf_index),
                    )
                })? {
                    Some(existing) if *existing != *leaf => {
                        return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                            pruning_point,
                            format!("active-batch leaf ({}, {}) collides with different local content", row.batch_id, leaf.leaf_index),
                        ));
                    }
                    _ => {}
                }
            }
            if let Some(certificate) = &row.certificate {
                let cert_hash = certificate.hash();
                match self.palw_store.certificate(cert_hash).optional().map_err(|err| {
                    PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        format!("existing active-batch certificate read {cert_hash}: {err}"),
                    )
                })? {
                    Some(existing) if *existing != *certificate => {
                        return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                            pruning_point,
                            format!("active-batch certificate {cert_hash} collides with different local content"),
                        ));
                    }
                    _ => {}
                }
            }
        }
        let existing_provider_outpoints: Vec<TransactionOutpoint> = {
            let store = self.palw_provider_bonds_store.read();
            store
                .iterator()
                .map(|row| {
                    row.map(|(outpoint, _)| outpoint).map_err(|err| {
                        PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("provider store iteration: {err}"))
                    })
                })
                .collect::<PruningImportResult<_>>()?
        };

        let mut beacon_state_write = None;
        let mut beacon_accumulator_write = None;
        let mut overlay_view_write = None;
        let mut lane_bits_write = None;
        let mut nullifier_write = None;
        if active && pruning_point != self.genesis.hash {
            if let Some(state) = &payload.frontier.beacon_state {
                match self.palw_beacon_store.beacon_state(pruning_point).map_err(|err| {
                    PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing beacon read: {err}"))
                })? {
                    Some(existing) if *existing != *state => {
                        return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                            pruning_point,
                            "existing pruning-point beacon state disagrees with snapshot".to_string(),
                        ));
                    }
                    Some(_) => {}
                    None => beacon_state_write = Some(Arc::new(state.clone())),
                }
            }
            if let Some(imported) = &payload.beacon_accumulator {
                let view = PalwBeaconAccumViewV1 {
                    version: imported.version,
                    epochs: imported.epochs.clone(),
                    stake_by_epoch: imported.stake_by_epoch.clone(),
                };
                match self.palw_beacon_store.accum_view(pruning_point).map_err(|err| {
                    PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing accumulator read: {err}"))
                })? {
                    Some(existing) if *existing != view => {
                        return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                            pruning_point,
                            "existing pruning-point beacon accumulator disagrees with snapshot".to_string(),
                        ));
                    }
                    Some(_) => {}
                    None => beacon_accumulator_write = Some(Arc::new(view)),
                }
            }
            if let Some(view) = &payload.frontier.overlay_view {
                match self.palw_overlay_view_store.view(pruning_point).map_err(|err| {
                    PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing batch-view read: {err}"))
                })? {
                    Some(existing) if *existing != *view => {
                        return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                            pruning_point,
                            "existing pruning-point batch view disagrees with snapshot".to_string(),
                        ));
                    }
                    Some(_) => {}
                    None => overlay_view_write = Some(Arc::new(view.clone())),
                }
            }
            if let Some(bits) = payload.frontier.lane_bits {
                match self.palw_lane_bits_store.lane_bits(pruning_point).map_err(|err| {
                    PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing lane-bits read: {err}"))
                })? {
                    Some(existing) if existing != bits => {
                        return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                            pruning_point,
                            "existing pruning-point lane bits disagree with snapshot".to_string(),
                        ));
                    }
                    Some(_) => {}
                    None => lane_bits_write = Some(bits),
                }
            }
            match self.palw_nullifier_store.get(pruning_point).optional().map_err(|err| {
                PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing nullifier read: {err}"))
            })? {
                Some(existing) if *existing != payload.frontier.active_nullifiers => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        "existing pruning-point nullifier set disagrees with snapshot".to_string(),
                    ));
                }
                Some(_) => {}
                None => nullifier_write = Some(Arc::new(payload.frontier.active_nullifiers.clone())),
            }
        }

        let mut spam_writes: Vec<(BlockHash, Arc<PalwSpamAccumulatorV1>)> = Vec::new();
        if let Some(spam) = &payload.spam_accumulator {
            let to_store = |state: &PalwPrunedSpamAccumulatorV1| PalwSpamAccumulatorV1 {
                version: state.version,
                daa_score: state.daa_score,
                selected_height: state.selected_height,
                total_hash_blues: state.total_hash_blues,
                total_replica_blues: state.total_replica_blues,
                selected_parent: state.selected_parent,
                skip: state.skip,
            };
            let rows = std::iter::once((pruning_point, &spam.pruning_point_state))
                .chain(spam.support_rows.iter().map(|row| (row.block_hash, &row.state)));
            for (hash, transported) in rows {
                let state = to_store(transported);
                match self.palw_spam_store.get_optional(hash).map_err(|err| {
                    PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing spam-row read {hash}: {err}"))
                })? {
                    Some(existing) if *existing != state => {
                        return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                            pruning_point,
                            format!("existing spam accumulator {hash} disagrees with snapshot"),
                        ));
                    }
                    Some(_) => {}
                    None => spam_writes.push((hash, Arc::new(state))),
                }
            }
        }

        let da_state_write = if let Some(da) = &payload.da_snapshot {
            match self.palw_da_store.read().state(pruning_point).optional().map_err(|err| {
                PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing DA-state read: {err}"))
            })? {
                Some(existing) if *existing != da.state => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        "existing pruning-point DA state disagrees with snapshot".to_string(),
                    ));
                }
                Some(_) => None,
                None => Some(Arc::new(da.state.clone())),
            }
        } else {
            None
        };

        let search_availability_state_write = if let Some(search) = &payload.search_availability_snapshot {
            match self.palw_search_availability_store.read().state(pruning_point).optional().map_err(|err| {
                PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing search-state read: {err}"))
            })? {
                Some(existing) if *existing != search.state => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        "existing pruning-point search-availability state disagrees with snapshot".to_string(),
                    ));
                }
                Some(_) => None,
                None => Some(Arc::new(search.state.clone())),
            }
        } else {
            None
        };

        // ADR-MA §21.3/§21.4 — the Compute Set registry component is MANDATORY exactly when the
        // registry fence is open at the boundary: without it, retained headers' committed record
        // ids stop resolving (work re-verification and per-set difficulty fail-stop), so a
        // registry-active boundary without the component is unusable — and a closed-fence
        // boundary carrying one is a peer smuggling unaccountable state. The view row follows
        // the DA-state posture: divergence from an existing row is a recoverable rejection.
        let registry_active_at_boundary = self.palw_compute_registry_activation_daa_score != u64::MAX
            && payload.pruning_point_daa_score >= self.palw_compute_registry_activation_daa_score;
        let compute_registry_view_write = if let Some(registry) = &payload.compute_registry_snapshot {
            if !registry_active_at_boundary {
                return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                    pruning_point,
                    "compute registry snapshot present but the registry fence is closed at the boundary".to_string(),
                ));
            }
            use crate::model::stores::palw_compute_registry::PalwComputeRegistryStoreReader;
            match self.palw_compute_registry_store.view(pruning_point).map_err(|err| {
                PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("existing registry-view read: {err}"))
            })? {
                Some(existing) if *existing != registry.view => {
                    return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                        pruning_point,
                        "existing pruning-point compute registry view disagrees with snapshot".to_string(),
                    ));
                }
                Some(_) => None,
                None => Some(Arc::new(registry.view.clone())),
            }
        } else {
            if registry_active_at_boundary {
                return Err(PruningImportError::ImportedPalwSnapshotInvalid(
                    pruning_point,
                    "registry-active boundary requires the compute registry snapshot (ADR-MA §21.3/§21.4)".to_string(),
                ));
            }
            None
        };

        Ok(PreparedPalwPruningPointSnapshotImport {
            pruning_point,
            snapshot,
            existing_provider_outpoints,
            beacon_state_write,
            beacon_accumulator_write,
            overlay_view_write,
            lane_bits_write,
            nullifier_write,
            spam_writes,
            da_state_write,
            search_availability_state_write,
            compute_registry_view_write,
        })
    }

    /// Stage a fully preflighted PALW boundary in the caller's batch. No recoverable error may be
    /// returned after this method starts: cache-backed stores are updated ahead of the RocksDB commit.
    /// Callers therefore abort the process on any error from this method or from the final DB write.
    pub(crate) fn stage_prepared_pruning_point_palw_snapshot_import(
        &self,
        batch: &mut WriteBatch,
        prepared: PreparedPalwPruningPointSnapshotImport,
    ) -> Result<(), String> {
        let PreparedPalwPruningPointSnapshotImport {
            pruning_point,
            snapshot,
            existing_provider_outpoints,
            beacon_state_write,
            beacon_accumulator_write,
            overlay_view_write,
            lane_bits_write,
            nullifier_write,
            spam_writes,
            da_state_write,
            search_availability_state_write,
            compute_registry_view_write,
        } = prepared;
        let payload = &snapshot.payload;

        // All semantic checks passed. Install the selected-chain registry, fork-local boundary rows,
        // content-addressed active blobs, DA singleton and digest-bound envelope in the caller's
        // RocksDB commit.
        self.palw_store
            .import_active_batches_batch(batch, &payload.active_batches)
            .map_err(|err| format!("active batch blob write staging: {err}"))?;
        {
            let mut store = self.palw_provider_bonds_store.write();
            for outpoint in existing_provider_outpoints {
                store.delete_batch(batch, outpoint).map_err(|err| format!("provider delete staging: {err}"))?;
            }
            for record in &payload.provider_bonds {
                store
                    .insert_batch(batch, record.bond_outpoint, Arc::new(record.clone()))
                    .map_err(|err| format!("provider insert staging: {err}"))?;
            }
        }
        if let Some(state) = beacon_state_write {
            self.palw_beacon_store
                .set_state_batch(batch, pruning_point, state)
                .map_err(|err| format!("beacon write staging: {err}"))?;
        }
        if let Some(view) = beacon_accumulator_write {
            self.palw_beacon_store
                .set_accum_view_batch(batch, pruning_point, view)
                .map_err(|err| format!("accumulator write staging: {err}"))?;
        }
        if let Some(view) = overlay_view_write {
            self.palw_overlay_view_store
                .set_batch(batch, pruning_point, view)
                .map_err(|err| format!("batch-view write staging: {err}"))?;
        }
        if let Some(bits) = lane_bits_write {
            self.palw_lane_bits_store
                .set_batch(batch, pruning_point, bits)
                .map_err(|err| format!("lane-bits write staging: {err}"))?;
        }
        if let Some(nullifiers) = nullifier_write {
            self.palw_nullifier_store
                .insert_batch(batch, pruning_point, nullifiers)
                .map_err(|err| format!("nullifier write staging: {err}"))?;
        }
        for (hash, state) in spam_writes {
            self.palw_spam_store
                .insert_batch(batch, hash, state)
                .map_err(|err| format!("spam support-row write staging {hash}: {err}"))?;
        }
        if let Some(da) = &payload.da_snapshot {
            let mut store = self.palw_da_store.write();
            if let Some(state) = da_state_write {
                store.set_state_batch(batch, pruning_point, state).map_err(|err| format!("DA state write staging: {err}"))?;
            }
            store.set_pruning_snapshot_batch(batch, da).map_err(|err| format!("DA snapshot write staging: {err}"))?;
        }
        if let Some(search) = &payload.search_availability_snapshot {
            let mut store = self.palw_search_availability_store.write();
            if let Some(state) = search_availability_state_write {
                store.set_state_batch(batch, pruning_point, state).map_err(|err| format!("search state write staging: {err}"))?;
            }
            store.set_pruning_snapshot_batch(batch, search).map_err(|err| format!("search snapshot write staging: {err}"))?;
        }
        if let Some(registry) = &payload.compute_registry_snapshot {
            // §21.2 — record tiers are content-addressed write-once: pre-existing identical
            // records are idempotent no-ops, so the import merges cleanly over local state.
            let store = self.palw_compute_registry_store.as_ref();
            for descriptor in &registry.descriptors {
                store
                    .insert_descriptor_batch(batch, descriptor.compute_set_id(), descriptor)
                    .map_err(|err| format!("compute registry descriptor staging: {err}"))?;
            }
            for policy in &registry.policies {
                store
                    .insert_policy_batch(batch, policy.policy_id(), policy)
                    .map_err(|err| format!("compute registry policy staging: {err}"))?;
            }
            for plan in &registry.plans {
                store.insert_plan_batch(batch, plan.plan_id, plan).map_err(|err| format!("compute registry plan staging: {err}"))?;
            }
            for certificate in &registry.activation_certificates {
                store
                    .insert_certificate_batch(batch, certificate)
                    .map_err(|err| format!("compute registry certificate staging: {err}"))?;
            }
            if let Some(view) = compute_registry_view_write {
                store.set_view_batch(batch, pruning_point, view).map_err(|err| format!("compute registry view staging: {err}"))?;
            }
        }

        self.palw_pruned_frontier_store.write().set_batch(batch, snapshot).map_err(|err| format!("snapshot write staging: {err}"))?;
        Ok(())
    }

    /// Canonically validate and atomically install the PALW boundary state. All peer-controlled and
    /// store-collision errors are returned before staging. Once cache-backed staging starts, a local
    /// store/DB failure aborts the process instead of unwinding with caches ahead of RocksDB.
    ///
    /// `chain_derived` carries the ADR-0042 permissionless authentication bundle the IBD flow already
    /// bound to a work-authenticated descendant. It is honoured only when the node-local
    /// `Config::palw_permissionless_snapshot_auth` lever is set and `import_auth` claims chain-derived
    /// provenance; see `prepare_pruning_point_palw_snapshot_import`. Passing `None` reproduces the
    /// operator-pinned / Header-v3 behaviour exactly.
    pub fn import_pruning_point_palw_snapshot(
        &self,
        pruning_point: BlockHash,
        expected_daa_score: u64,
        expected_header_version: u16,
        expected_spam_commitment: Hash64,
        import_auth: PalwPruningSnapshotImportAuth,
        snapshot: PalwPruningPointSnapshotV1,
        chain_derived: Option<&PalwChainDerivedAuthBundleV1>,
    ) -> PruningImportResult<()> {
        // Every peer-controlled check — including the chain-derived boundary fold and the support-row
        // preimage binding (review point (c)) — completes inside `prepare_…` and returns a recoverable
        // `Err`. Staging starts only after the `?` below.
        let prepared = self.prepare_pruning_point_palw_snapshot_import(
            pruning_point,
            expected_daa_score,
            expected_header_version,
            expected_spam_commitment,
            import_auth,
            snapshot,
            chain_derived,
        )?;
        let mut batch = WriteBatch::default();
        if let Err(err) = self.stage_prepared_pruning_point_palw_snapshot_import(&mut batch, prepared) {
            palw_overlay_commit_fail_stop(format!("PALW pruning snapshot batch staging failed for {pruning_point}: {err}"));
        }
        if let Err(err) = self.db.write(batch) {
            palw_overlay_commit_fail_stop(format!("PALW pruning snapshot atomic write failed for {pruning_point}: {err}"));
        }
        Ok(())
    }

    /// Final pre-UTXO-import check: every collateral row which must still be locked at the pruning
    /// point resolves against the downloaded UTXO set, and every present row matches amount + owner
    /// script. This runs before virtual state is seeded from the imported provider registry.
    fn validate_imported_palw_provider_utxos(&self, pruning_point: BlockHash) -> PruningImportResult<()> {
        let snapshot = self.palw_pruned_frontier_store.read().get().map_err(|err| {
            PruningImportError::ImportedPalwSnapshotInvalid(pruning_point, format!("snapshot unavailable before UTXO import: {err}"))
        })?;
        self.validate_palw_pruning_snapshot_context(&snapshot)?;
        if snapshot.payload.pruning_point != pruning_point {
            return Err(PruningImportError::ImportedPalwSnapshotPointMismatch(pruning_point, snapshot.payload.pruning_point));
        }
        let pp_daa = snapshot.payload.pruning_point_daa_score;
        let pruning_meta = self.pruning_meta_stores.read();
        for record in &snapshot.payload.provider_bonds {
            let entry = UtxoSetStoreReader::get(&pruning_meta.utxo_set, &record.bond_outpoint).optional().map_err(|err| {
                PruningImportError::ImportedPalwSnapshotInvalid(
                    pruning_point,
                    format!("provider UTXO lookup {} failed: {err}", record.bond_outpoint),
                )
            })?;
            validate_imported_palw_provider_utxo(record, entry.as_deref(), pp_daa, self.palw_epoch_length_daa)?;
        }
        Ok(())
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

#[cfg(test)]
mod palw_pruning_import_auth_tests {
    use super::{
        validate_palw_chain_derived_import_auth_context,
        validate_palw_snapshot_import_auth_context as validate_auth_context_with_lever,
    };
    use kaspa_consensus_core::{
        Hash64,
        constants::{PALW_ANTISPAM_HEADER_VERSION, PALW_HEADER_VERSION},
        palw_pruned_frontier::{PalwPruningSnapshotCheckpoint, PalwPruningSnapshotImportAuth},
    };

    /// The pre-ADR-0042 seven-argument shape, pinned to the lever being OFF. Every assertion that
    /// existed before the lever was threaded goes through this wrapper with its arguments unchanged,
    /// which is precisely the "lever off ⇒ byte-identical behaviour" property.
    fn validate_palw_snapshot_import_auth_context(
        pruning_point_daa_score: u64,
        header_version: u16,
        pruning_point: Hash64,
        import_auth: &PalwPruningSnapshotImportAuth,
        operator_checkpoints: &[PalwPruningSnapshotCheckpoint],
        palw_activation_daa_score: u64,
        palw_spam_is_inert: bool,
    ) -> Result<(), String> {
        validate_auth_context_with_lever(
            pruning_point_daa_score,
            header_version,
            pruning_point,
            import_auth,
            operator_checkpoints,
            palw_activation_daa_score,
            palw_spam_is_inert,
            false,
        )
    }

    fn h(word: u64) -> Hash64 {
        Hash64::from_u64_word(word)
    }

    #[test]
    fn consensus_import_requires_exact_version_provenance_and_pruning_point() {
        let pruning_point = h(1);
        let legacy = PalwPruningSnapshotImportAuth::legacy_header_v3(pruning_point, h(2));
        let pinned =
            PalwPruningSnapshotImportAuth::operator_pinned(PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: h(2) });

        assert!(validate_palw_snapshot_import_auth_context(10, PALW_HEADER_VERSION, pruning_point, &legacy, &[], 10, true).is_ok());
        assert!(
            validate_palw_snapshot_import_auth_context(
                10,
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                &pinned,
                &[pinned.checkpoint],
                10,
                false,
            )
            .is_ok()
        );
        assert!(
            validate_palw_snapshot_import_auth_context(10, PALW_ANTISPAM_HEADER_VERSION, pruning_point, &pinned, &[], 10, false,)
                .is_err()
        );
        assert!(
            validate_palw_snapshot_import_auth_context(
                10,
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                &legacy,
                &[pinned.checkpoint],
                10,
                false,
            )
            .is_err()
        );
        assert!(
            validate_palw_snapshot_import_auth_context(
                10,
                PALW_HEADER_VERSION,
                pruning_point,
                &pinned,
                &[pinned.checkpoint],
                10,
                true,
            )
            .is_err()
        );
        assert!(
            validate_palw_snapshot_import_auth_context(
                10,
                PALW_ANTISPAM_HEADER_VERSION + 1,
                pruning_point,
                &pinned,
                &[pinned.checkpoint],
                10,
                false,
            )
            .is_err()
        );
        assert!(
            validate_palw_snapshot_import_auth_context(
                10,
                PALW_ANTISPAM_HEADER_VERSION,
                h(3),
                &pinned,
                &[pinned.checkpoint],
                10,
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn non_inert_v4_network_rejects_caller_supplied_v3_downgrade() {
        let pruning_point = h(4);
        let legacy = PalwPruningSnapshotImportAuth::legacy_header_v3(pruning_point, h(5));
        assert!(validate_palw_snapshot_import_auth_context(20, PALW_HEADER_VERSION, pruning_point, &legacy, &[], 10, false).is_err());
        assert!(validate_palw_snapshot_import_auth_context(9, PALW_HEADER_VERSION, pruning_point, &legacy, &[], 10, true).is_err());
    }

    /// The lever is threaded into the operator-authority context for exactly one reason: to keep the
    /// admission matrix expressed in one place. It must not widen anything the operator path accepts.
    #[test]
    fn the_lever_does_not_widen_the_operator_authority_context() {
        let pruning_point = h(11);
        let legacy = PalwPruningSnapshotImportAuth::legacy_header_v3(pruning_point, h(12));
        let pinned =
            PalwPruningSnapshotImportAuth::operator_pinned(PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: h(12) });

        for version in [PALW_HEADER_VERSION, PALW_ANTISPAM_HEADER_VERSION, PALW_ANTISPAM_HEADER_VERSION + 1] {
            for inert in [true, false] {
                for auth in [&legacy, &pinned] {
                    for checkpoints in [&[][..], &[pinned.checkpoint][..]] {
                        let off = validate_auth_context_with_lever(10, version, pruning_point, auth, checkpoints, 10, inert, false);
                        let on = validate_auth_context_with_lever(10, version, pruning_point, auth, checkpoints, 10, inert, true);
                        assert_eq!(off, on, "lever changed the operator-authority verdict for version {version} inert {inert}");
                    }
                }
            }
        }
    }

    /// Chain-derived provenance can only ever be authenticated by the chain-derived context, which the
    /// importer enters only with a bundle in hand. Arriving at the operator-authority context with it
    /// is refused on both lever settings — with the lever off by the admission matrix, with the lever
    /// on by the explicit guard.
    #[test]
    fn operator_authority_context_never_authenticates_chain_derived_provenance() {
        let pruning_point = h(21);
        let chain_derived = PalwPruningSnapshotImportAuth::chain_derived(pruning_point);
        for lever in [false, true] {
            for checkpoints in [&[][..], &[chain_derived.checkpoint][..]] {
                assert!(
                    validate_auth_context_with_lever(
                        10,
                        PALW_ANTISPAM_HEADER_VERSION,
                        pruning_point,
                        &chain_derived,
                        checkpoints,
                        10,
                        false,
                        lever,
                    )
                    .is_err(),
                    "operator-authority context accepted chain-derived provenance with lever {lever}"
                );
            }
        }
    }

    /// The chain-derived context is fail-closed on every axis the operator context is, minus the
    /// checkpoint membership it structurally cannot have, plus the sentinel-digest shape.
    #[test]
    fn chain_derived_context_is_fail_closed_on_every_axis() {
        let pruning_point = h(31);
        let auth = PalwPruningSnapshotImportAuth::chain_derived(pruning_point);

        // The only accepting cell: lever on, Header-v4, non-inert network, at/after activation, the
        // auth pinned to this pruning point, sentinel digest.
        assert!(
            validate_palw_chain_derived_import_auth_context(10, PALW_ANTISPAM_HEADER_VERSION, pruning_point, &auth, 10, false, true)
                .is_ok()
        );

        // Lever off.
        assert!(
            validate_palw_chain_derived_import_auth_context(10, PALW_ANTISPAM_HEADER_VERSION, pruning_point, &auth, 10, false, false)
                .is_err()
        );
        // Before PALW activation.
        assert!(
            validate_palw_chain_derived_import_auth_context(9, PALW_ANTISPAM_HEADER_VERSION, pruning_point, &auth, 10, false, true)
                .is_err()
        );
        // Inert (Header-v3) network: no anti-spam rows exist to bind a boundary to.
        assert!(
            validate_palw_chain_derived_import_auth_context(10, PALW_ANTISPAM_HEADER_VERSION, pruning_point, &auth, 10, true, true)
                .is_err()
        );
        // Header-v3 and any future version stay closed.
        for version in [PALW_HEADER_VERSION, PALW_ANTISPAM_HEADER_VERSION + 1] {
            assert!(
                validate_palw_chain_derived_import_auth_context(10, version, pruning_point, &auth, 10, false, true).is_err(),
                "chain-derived context admitted header version {version}"
            );
        }
        // Auth pinned to another pruning point.
        assert!(
            validate_palw_chain_derived_import_auth_context(10, PALW_ANTISPAM_HEADER_VERSION, h(32), &auth, 10, false, true).is_err()
        );
        // A non-sentinel digest would make a peer-advertised auth indistinguishable from an operator
        // claim at any later comparison site.
        let mut smuggled = auth;
        smuggled.checkpoint.payload_digest = h(33);
        assert!(
            validate_palw_chain_derived_import_auth_context(
                10,
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                &smuggled,
                10,
                false,
                true
            )
            .is_err()
        );
        // Any other provenance is refused by the shared admission expression.
        for other in [
            PalwPruningSnapshotImportAuth::legacy_header_v3(pruning_point, Hash64::default()),
            PalwPruningSnapshotImportAuth::operator_pinned(PalwPruningSnapshotCheckpoint {
                pruning_point,
                payload_digest: Hash64::default(),
            }),
        ] {
            assert!(
                validate_palw_chain_derived_import_auth_context(
                    10,
                    PALW_ANTISPAM_HEADER_VERSION,
                    pruning_point,
                    &other,
                    10,
                    false,
                    true
                )
                .is_err(),
                "chain-derived context accepted {:?} provenance",
                other.provenance
            );
        }
    }

    // =====================================================================================
    // ADR-0042 FAIL-CLOSED PROOF for the chain-derived (permissionless) snapshot import.
    //
    // Everything above this line is pure-function context testing. Everything below drives the
    // REAL importer — `VirtualStateProcessor::prepare_pruning_point_palw_snapshot_import` — on a
    // REAL Header-v4 chain built through `validate_and_insert_block`, and is the first place
    // anywhere that the `Some(bundle)` arm executes.
    //
    // WHY `prepare_…` IS THE RIGHT INJECTION POINT, AND HOW THE ORDERING IS OBSERVABLE.
    // `prepare_…` is the single authentication choke point shared by both durable write paths
    // (`VirtualStateProcessor::import_pruning_point_palw_snapshot` and
    // `Consensus::intrusive_pruning_point_update_with_palw_snapshot`). It returns a read-only
    // `PreparedPalwPruningPointSnapshotImport`; that value is the ONLY input
    // `stage_prepared_pruning_point_palw_snapshot_import` accepts, and staging is the only producer
    // of the `WriteBatch` those two `self.db.write(batch)` calls commit. So:
    //
    //   1. TYPE LEVEL — an `Err` from `prepare_…` means no token was ever constructed, so staging
    //      cannot be called, so no batch exists, so `db.write` is unreachable. That is the ordering
    //      guarantee, and it is what makes the "no recoverable error after staging begins" rule
    //      (staging failures call `palw_overlay_commit_fail_stop` → `abort()`) safe.
    //   2. DB LEVEL — `db.latest_sequence_number()` is RocksDB's own committed-write counter. The
    //      fixture stops every consensus processor (`tc.shutdown`) before these assertions run, so
    //      the database is quiescent and the counter is a real `db.write` spy: it is asserted equal
    //      across each call, on the accepting path as well as on every rejecting one.
    //   3. STORE LEVEL — `palw_boundary_fingerprint` reads back every store
    //      `stage_prepared_pruning_point_palw_snapshot_import` writes to (pruned-frontier envelope,
    //      provider registry, beacon state/accumulator, overlay view, lane bits, nullifiers, the
    //      anti-spam support rows, and the DA / search-availability state + boundary singletons)
    //      through their caches, and is asserted unchanged. This catches a cache-ahead-of-RocksDB
    //      mutation that (2) alone would miss.
    //
    // Scope limitation. Review points (a′) "the transported headers belong to a
    // work-authenticated set" and (b) "the descendant is a buried chain child of the pruning point"
    // are the transport layer's obligation and are not enforced by the importer at all — the
    // importer consumes an already-projected `PalwChainDerivedAuthBundleV1`. The fixture therefore
    // *sources* its descendant and support headers from the local, fully validated stores rather
    // than asserting that the importer checks their provenance. What is proven here is review point
    // (c) plus the boundary fold, which is exactly the set of checks that live in `prepare_…`.
    // =====================================================================================

    use super::{
        BlockHash, BlockStatus, GhostdagStoreReader, HeaderStoreReader, PalwChainDerivedAuthBundleV1, PalwDaStoreReader,
        PalwNullifierStoreReader, PalwProviderBondRecord, PalwProviderBondsStoreReader, PalwPrunedFrontierStoreReader,
        PalwPrunedPaidWorkBlockV1, PalwPruningPointSnapshotPayloadV1, PalwPruningPointSnapshotV1, PalwSearchAvailabilityStoreReader,
        PalwSpamAccumulatorStoreReader, Params, PreparedPalwPruningPointSnapshotImport, PruningImportError, PruningImportResult,
        StoreResultExt, TransactionOutpoint, VirtualStateProcessor,
    };
    use crate::consensus::test_consensus::TestConsensus;
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_consensus_core::block::Block;
    use kaspa_consensus_core::coinbase::MinerData;
    use kaspa_consensus_core::config::{Config, ConfigBuilder, params::STAGING_MAINNET_PALW_PARAMS};
    use kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk;
    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::palw_pruned_frontier::{
        PalwChainDerivedHeaderBundleWireV1, palw_pruning_payload_paid_work_nullifiers,
        verify_chain_derived_pruning_boundary_from_payload,
    };
    use std::sync::Arc;

    /// Linear chain length. Only the anti-spam closure below the boundary and one post-boundary
    /// child are needed, so this stays tiny — the whole fixture builds in a few tens of ms.
    const FIXTURE_CHAIN_LEN: u64 = 10;
    /// Index of the boundary inside the fixture chain (0 = genesis). The child at `+1` is the
    /// descendant whose committed `overlay_commitment_root` authenticates the boundary.
    const FIXTURE_PRUNING_POINT_INDEX: usize = 6;

    /// A real Header-v4 network, derived from the only shipped non-inert v4 preset
    /// (`STAGING_MAINNET_PALW_PARAMS`, ADR-0048). The genesis timestamp is moved into the past and
    /// the genesis hash recomputed, because the shipped staging genesis is dated in the future and
    /// the block template would otherwise mint `TimeTooFarIntoTheFuture` headers.
    ///
    /// `palw_spam` / `palw_activation_daa_score` / `genesis.version` are left exactly as shipped, so
    /// this is the real v4 fence (`PalwSpamParams::PUBLIC_REGENESIS_CANDIDATE`), not a relaxed one.
    fn v4_fixture_params() -> Params {
        let mut params = STAGING_MAINNET_PALW_PARAMS;
        params.genesis.timestamp = 1_600_000_000_000;
        params.genesis.hash = Hash64::default();
        params.genesis.hash = Header::from(&params.genesis).hash;
        assert!(!params.palw_spam.is_inert(), "the fixture must run on a genuinely Header-v4 network");
        assert_eq!(params.genesis.version, PALW_ANTISPAM_HEADER_VERSION);
        params
    }

    /// A real Header-v4 lifecycle plus the honest chain-derived bundle a correct transport layer
    /// would hand the importer.
    ///
    /// Nothing here is hand-seeded: every block goes through `validate_and_insert_block` and reaches
    /// `StatusUTXOValid`, the payload comes from the production capture path
    /// (`build_palw_pruning_point_snapshot`), and the bundle is projected by the production wire type
    /// (`PalwChainDerivedHeaderBundleWireV1::extract_authenticated_bundle`) from headers read back
    /// out of the headers store. In particular `descendant_overlay_commitment_root` is the value the
    /// descendant block actually committed and `verify_expected_utxo_state` actually accepted — so
    /// an accepting assertion below is the ADR-0042 §1c derivation equivalence, end to end.
    struct ChainDerivedFixture {
        tc: TestConsensus,
        config: Config,
        blocks: Vec<Block>,
        pruning_point: BlockHash,
        pp_daa_score: u64,
        pp_spam_commitment: Hash64,
        /// The pruning point plus every anti-spam support row — the exact key set the boundary
        /// installs anti-spam rows for, used by the store fingerprint.
        spam_closure: Vec<BlockHash>,
        snapshot: PalwPruningPointSnapshotV1,
        wire: PalwChainDerivedHeaderBundleWireV1,
        bundle: PalwChainDerivedAuthBundleV1,
    }

    impl ChainDerivedFixture {
        async fn build() -> Self {
            let config = ConfigBuilder::new(v4_fixture_params()).skip_proof_of_work().set_palw_permissionless_snapshot_auth().build();
            assert!(config.palw_permissionless_snapshot_auth, "this fixture exists to exercise the lever-on path");
            let (tc, blocks) = mine_v4_chain(&config).await;

            let chain: Vec<BlockHash> =
                std::iter::once(config.params.genesis.hash).chain(blocks.iter().map(|b| b.header.hash)).collect();
            let pruning_point = chain[FIXTURE_PRUNING_POINT_INDEX];
            let descendant = chain[FIXTURE_PRUNING_POINT_INDEX + 1];

            let pp_header = tc.headers_store().get_header(pruning_point).unwrap();
            let d_header = tc.headers_store().get_header(descendant).unwrap();
            assert_eq!(pp_header.version, PALW_ANTISPAM_HEADER_VERSION, "the fixture boundary must be a Header-v4 block");
            assert_eq!(d_header.version, PALW_ANTISPAM_HEADER_VERSION);
            // The linear chain makes the descendant's GHOSTDAG selected parent the pruning point,
            // which is the structural precondition the transport layer owns (review point (b)).
            assert_eq!(
                tc.ghostdag_store().get_selected_parent(descendant).unwrap(),
                pruning_point,
                "the fixture descendant must be a chain child of the boundary"
            );

            let vp = tc.virtual_processor();
            let snapshot = vp.build_palw_pruning_point_snapshot(pruning_point).expect("the capture path must build a v4 boundary");
            let spam = snapshot.payload.spam_accumulator.as_ref().expect("a Header-v4 boundary carries an anti-spam frontier");
            assert!(!spam.support_rows.is_empty(), "the fixture must exercise a non-empty support-row closure");

            let mut spam_closure = vec![pruning_point];
            let mut support_headers = vec![(*pp_header).clone()];
            for row in &spam.support_rows {
                spam_closure.push(row.block_hash);
                support_headers.push((*tc.headers_store().get_header(row.block_hash).unwrap()).clone());
            }

            // The legacy DNS/EVM overlay snapshot as-of the boundary — the same value the descendant
            // folded. It needs no independent authentication: a substituted snapshot simply changes
            // `palw_overlay_commitment_root_v2` and fails the comparison (see the tamper below).
            let overlay = vp.compute_overlay_snapshot(pruning_point, &vp.initial_active_bond_view());
            let wire = PalwChainDerivedHeaderBundleWireV1 {
                descendant_header: (*d_header).clone(),
                support_headers,
                dns_overlay_snapshot: overlay,
            };
            let bundle = wire.extract_authenticated_bundle().expect("honest transported headers must project");
            assert_eq!(
                bundle.descendant_overlay_commitment_root, d_header.overlay_commitment_root,
                "the honest bundle must restate the descendant's own committed root, never invent one"
            );

            Self {
                tc,
                config,
                blocks,
                pruning_point,
                pp_daa_score: pp_header.daa_score,
                pp_spam_commitment: pp_header.palw_spam_accumulator_commitment,
                spam_closure,
                snapshot,
                wire,
                bundle,
            }
        }

        fn vp(&self) -> &Arc<VirtualStateProcessor> {
            self.tc.virtual_processor()
        }

        /// Run the importer's single authentication choke point with this fixture's boundary
        /// metadata, and assert — around the call itself — that nothing durable moved.
        ///
        /// The returned `Result` is the verdict under test; the write-freedom of the call is
        /// asserted here so no individual test can forget it.
        fn prepare_observing_no_durable_write(
            &self,
            header_version: u16,
            import_auth: PalwPruningSnapshotImportAuth,
            snapshot: PalwPruningPointSnapshotV1,
            chain_derived: Option<&PalwChainDerivedAuthBundleV1>,
        ) -> PruningImportResult<PreparedPalwPruningPointSnapshotImport> {
            let vp = self.vp();
            let seq_before = vp.db.latest_sequence_number();
            let fingerprint_before = palw_boundary_fingerprint(vp, self.pruning_point, &self.spam_closure);
            let verdict = vp.prepare_pruning_point_palw_snapshot_import(
                self.pruning_point,
                self.pp_daa_score,
                header_version,
                self.pp_spam_commitment,
                import_auth,
                snapshot,
                chain_derived,
            );
            assert_eq!(
                seq_before,
                vp.db.latest_sequence_number(),
                "prepare_pruning_point_palw_snapshot_import committed a RocksDB write; authentication must stay strictly \
                 before the durable write"
            );
            assert_eq!(
                fingerprint_before,
                palw_boundary_fingerprint(vp, self.pruning_point, &self.spam_closure),
                "prepare_pruning_point_palw_snapshot_import mutated a store the staging step owns"
            );
            verdict
        }

        /// A second node running the SAME chain with the lever OFF and the honest boundary pinned as
        /// an operator checkpoint — the byte-identical-behaviour baseline for the fence test. The
        /// checkpoint digest is only knowable after the first node has captured the snapshot, which
        /// is why this cannot be folded into `build`.
        async fn spawn_lever_off_peer(&self, checkpoints: Vec<PalwPruningSnapshotCheckpoint>) -> ChainDerivedFixture {
            let mut config = self.config.clone();
            config.palw_permissionless_snapshot_auth = false;
            config.palw_pruning_snapshot_checkpoints = checkpoints;
            let tc = TestConsensus::new(&config);
            let handles = tc.init();
            for block in &self.blocks {
                let status = tc
                    .validate_and_insert_block(block.clone())
                    .virtual_state_task
                    .await
                    .expect("the lever-off peer must accept the identical chain");
                assert_eq!(status, BlockStatus::StatusUTXOValid);
            }
            tc.shutdown(handles);
            ChainDerivedFixture {
                tc,
                config,
                blocks: self.blocks.clone(),
                pruning_point: self.pruning_point,
                pp_daa_score: self.pp_daa_score,
                pp_spam_commitment: self.pp_spam_commitment,
                spam_closure: self.spam_closure.clone(),
                snapshot: self.snapshot.clone(),
                wire: self.wire.clone(),
                bundle: self.bundle.clone(),
            }
        }
    }

    /// Mine a linear Header-v4 chain through the real template + `validate_and_insert_block`, then
    /// stop every processor so the database is quiescent for the write-freedom assertions.
    ///
    /// `header.finalize()` replaces the harness's precomputed placeholder hash with the header's own
    /// canonical preimage hash. That is mandatory here and not cosmetic: `extract_authenticated_bundle`
    /// re-derives every transported header's identity (the F6 trust boundary), so headers carrying a
    /// harness-assigned hash cannot be transported at all.
    async fn mine_v4_chain(config: &Config) -> (TestConsensus, Vec<Block>) {
        let tc = TestConsensus::new(config);
        let handles = tc.init();
        let mut tip = config.params.genesis.hash;
        let mut blocks = Vec::with_capacity(FIXTURE_CHAIN_LEN as usize);
        for i in 1..=FIXTURE_CHAIN_LEN {
            let miner = MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]);
            let mut mutable = tc.build_utxo_valid_block_with_parents(Hash64::from_u64_word(i), vec![tip], miner, vec![]);
            mutable.header.finalize();
            let block = mutable.to_immutable();
            let status = tc
                .validate_and_insert_block(block.clone())
                .virtual_state_task
                .await
                .unwrap_or_else(|err| panic!("fixture block {i} rejected: {err:?}"));
            assert_eq!(status, BlockStatus::StatusUTXOValid, "fixture block {i} must be UTXO-valid");
            tip = block.header.hash;
            blocks.push(block);
        }
        tc.shutdown(handles);
        (tc, blocks)
    }

    /// Read back every store `stage_prepared_pruning_point_palw_snapshot_import` writes to. Debug
    /// rendering is used deliberately: it is total over these types and compares the full contents,
    /// not a hash or a presence flag.
    fn palw_boundary_fingerprint(vp: &VirtualStateProcessor, pruning_point: BlockHash, spam_closure: &[BlockHash]) -> Vec<String> {
        let mut rows = Vec::new();
        rows.push(format!("pruned-frontier={:?}", vp.palw_pruned_frontier_store.read().get().ok()));
        rows.push(format!(
            "provider-registry={:?}",
            vp.palw_provider_bonds_store
                .read()
                .iterator()
                .map(|row| row.expect("provider registry iteration"))
                .collect::<Vec<(TransactionOutpoint, Arc<PalwProviderBondRecord>)>>()
        ));
        rows.push(format!("beacon-state={:?}", vp.palw_beacon_store.beacon_state(pruning_point).unwrap()));
        rows.push(format!("beacon-accumulator={:?}", vp.palw_beacon_store.accum_view(pruning_point).unwrap()));
        rows.push(format!("overlay-view={:?}", vp.palw_overlay_view_store.view(pruning_point).unwrap()));
        rows.push(format!("lane-bits={:?}", vp.palw_lane_bits_store.lane_bits(pruning_point).unwrap()));
        rows.push(format!("nullifiers={:?}", vp.palw_nullifier_store.get(pruning_point).optional().unwrap()));
        for block in spam_closure {
            rows.push(format!("spam[{block}]={:?}", vp.palw_spam_store.get_optional(*block).unwrap()));
        }
        {
            let da = vp.palw_da_store.read();
            rows.push(format!("da-state={:?}", da.state(pruning_point).unwrap()));
            rows.push(format!("da-boundary={:?}", da.pruning_snapshot().ok()));
        }
        {
            let search = vp.palw_search_availability_store.read();
            rows.push(format!("search-state={:?}", search.state(pruning_point).unwrap()));
            rows.push(format!("search-boundary={:?}", search.pruning_snapshot().ok()));
        }
        rows
    }

    fn expect_invalid(verdict: PruningImportResult<PreparedPalwPruningPointSnapshotImport>, needle: &str, case: &str) {
        match verdict {
            Err(PruningImportError::ImportedPalwSnapshotInvalid(_, message)) => {
                assert!(message.contains(needle), "{case}: expected a rejection mentioning {needle:?}, got {message:?}");
            }
            Err(other) => panic!("{case}: expected ImportedPalwSnapshotInvalid, got {other:?}"),
            Ok(_) => panic!("{case}: a tampered chain-derived import was ACCEPTED"),
        }
    }

    /// Re-seal a tampered payload the way a hostile peer would: `PalwPruningPointSnapshotV1::new`
    /// canonicalizes and recomputes `payload_digest`, so the envelope stays self-consistent and the
    /// tamper has to be caught by the chain-derived authentication rather than by a stale digest.
    fn reseal(payload: PalwPruningPointSnapshotPayloadV1) -> PalwPruningPointSnapshotV1 {
        PalwPruningPointSnapshotV1::new(payload)
    }

    /// (1) The accepting case — the first exercise anywhere of the `Some(bundle)` arm.
    ///
    /// Asserts that with the lever ON, chain-derived provenance and the honest bundle projected from
    /// the real descendant header, `prepare_…` returns the `PreparedPalwPruningPointSnapshotImport`
    /// token, and that producing that token wrote nothing (so the negatives below are not vacuous and
    /// the accepting path is itself verify-before-write).
    ///
    /// Because `bundle.descendant_overlay_commitment_root` is the root the descendant block actually
    /// committed and consensus actually validated, acceptance here IS the ADR-0042 §1c statement: the
    /// state reconstructed from the transported payload folds to the same commitment as the state the
    /// live stores produced.
    #[tokio::test]
    async fn chain_derived_honest_bundle_is_accepted_and_prepare_writes_nothing() {
        let fixture = ChainDerivedFixture::build().await;
        let prepared = fixture.prepare_observing_no_durable_write(
            PALW_ANTISPAM_HEADER_VERSION,
            PalwPruningSnapshotImportAuth::chain_derived(fixture.pruning_point),
            fixture.snapshot.clone(),
            Some(&fixture.bundle),
        );
        let prepared = prepared.expect("the honest chain-derived boundary must authenticate");
        assert_eq!(prepared.pruning_point, fixture.pruning_point);
        assert_eq!(prepared.snapshot.payload_digest, fixture.snapshot.payload_digest);
        assert_eq!(
            fixture.bundle.support_row_headers.len(),
            fixture.spam_closure.len(),
            "the accepted bundle must bind the pruning-point row plus every anti-spam support row"
        );
        // The acceptance is caused by the bundle, not by something else on this fixture: chain-derived
        // provenance carries the all-zero sentinel instead of an operator digest, so the very same call
        // with no bundle cannot authenticate anything.
        assert!(
            fixture
                .prepare_observing_no_durable_write(
                    PALW_ANTISPAM_HEADER_VERSION,
                    PalwPruningSnapshotImportAuth::chain_derived(fixture.pruning_point),
                    fixture.snapshot.clone(),
                    None,
                )
                .is_err(),
            "chain-derived provenance without a bundle must fall through to the operator-authority context and be refused"
        );
    }

    /// (2) Tamper matrix. Each row asserts a specific rejection, and
    /// `prepare_observing_no_durable_write` asserts around every call that the RocksDB write counter
    /// and the full staging write set are unchanged before any durable write.
    ///
    /// The cases split into the two halves of the chain-derived check:
    ///   * the boundary fold (`palw_overlay_commitment_root_v2(legacy_root, state_root)` vs. the
    ///     descendant's committed root) catches every payload-side and root-side tamper;
    ///   * `verify_support_rows_against_transported_headers` catches every support-set tamper.
    #[tokio::test]
    async fn chain_derived_tampered_bundle_is_rejected_before_any_durable_write() {
        let fixture = ChainDerivedFixture::build().await;
        let auth = PalwPruningSnapshotImportAuth::chain_derived(fixture.pruning_point);
        let honest = &fixture.bundle;
        const FOLD: &str = "descendant overlay commitment does not authenticate the transported PALW state";

        // ---- root-side tampers: the two values the transport layer is trusted to have authenticated ----
        let mut wrong_descendant = honest.clone();
        wrong_descendant.descendant_overlay_commitment_root = h(0xdead);
        expect_invalid(
            fixture.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                auth,
                fixture.snapshot.clone(),
                Some(&wrong_descendant),
            ),
            FOLD,
            "wrong descendant overlay commitment",
        );

        let mut wrong_legacy = honest.clone();
        wrong_legacy.legacy_overlay_root = h(0xbeef);
        expect_invalid(
            fixture.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                auth,
                fixture.snapshot.clone(),
                Some(&wrong_legacy),
            ),
            FOLD,
            "substituted DNS/EVM overlay snapshot",
        );

        // ---- support-set tampers (ADR-0042 review point (c)) ----
        let mut substituted_commitment = honest.clone();
        substituted_commitment.support_row_headers[1].spam_accumulator_commitment = h(0xfeed);
        expect_invalid(
            fixture.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                auth,
                fixture.snapshot.clone(),
                Some(&substituted_commitment),
            ),
            "transported spam header commitment does not bind its row",
            "substituted support-row header body",
        );

        // A forged identity: the (block_hash, commitment) pair a peer would obtain by Borsh-forging
        // `Header::hash` if the projection did NOT re-derive it. Constructed directly, bypassing
        // `extract_authenticated_bundle`, so this asserts the importer is fail-closed even against a
        // transport layer that skipped the F6 check.
        let mut forged_identity = honest.clone();
        forged_identity.support_row_headers[1].block_hash = h(0xf0f0);
        forged_identity.support_row_headers.sort_by(|a, b| a.block_hash.as_bytes().cmp(&b.block_hash.as_bytes()));
        expect_invalid(
            fixture.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                auth,
                fixture.snapshot.clone(),
                Some(&forged_identity),
            ),
            "transported spam header has no matching support row",
            "forged support-row block identity",
        );

        let mut dropped = honest.clone();
        dropped.support_row_headers.remove(1);
        expect_invalid(
            fixture.prepare_observing_no_durable_write(PALW_ANTISPAM_HEADER_VERSION, auth, fixture.snapshot.clone(), Some(&dropped)),
            "transported spam header set size does not match the support rows",
            "dropped support-row header",
        );

        let mut duplicated = honest.clone();
        duplicated.support_row_headers[1] = duplicated.support_row_headers[0];
        expect_invalid(
            fixture.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                auth,
                fixture.snapshot.clone(),
                Some(&duplicated),
            ),
            "not canonical: transported spam headers",
            "duplicated support-row header padding out an omitted row",
        );

        // ---- payload-side tampers: both derived roots the importer computes from the payload ----
        let mut da_tampered = fixture.snapshot.payload.clone();
        da_tampered
            .da_snapshot
            .as_mut()
            .expect("an active v4 boundary carries DA state")
            .state
            .block_slashed_providers
            .push(TransactionOutpoint::new(h(0x0da0), 0));
        expect_invalid(
            fixture.prepare_observing_no_durable_write(PALW_ANTISPAM_HEADER_VERSION, auth, reseal(da_tampered), Some(honest)),
            FOLD,
            "tampered DA-derived state root",
        );

        let mut paid_work_tampered = fixture.snapshot.payload.clone();
        assert!(!paid_work_tampered.paid_work.is_empty(), "the fixture must carry a paid-work window to tamper with");
        paid_work_tampered.paid_work[0].job_nullifiers.push(h(0x9a1d));
        expect_invalid(
            fixture.prepare_observing_no_durable_write(PALW_ANTISPAM_HEADER_VERSION, auth, reseal(paid_work_tampered), Some(honest)),
            FOLD,
            "tampered paid-work nullifier set",
        );

        // The search-availability state root is folded into the same commitment. The e2e fixture has
        // no store-equality assertion for it, so pin it here: a tampered search state must not import.
        let mut search_tampered = fixture.snapshot.payload.clone();
        search_tampered
            .search_availability_snapshot
            .as_mut()
            .expect("an active v4 boundary carries search-availability state")
            .state
            .block_slashed_schedulers
            .push(TransactionOutpoint::new(h(0x5ea4), 0));
        expect_invalid(
            fixture.prepare_observing_no_durable_write(PALW_ANTISPAM_HEADER_VERSION, auth, reseal(search_tampered), Some(honest)),
            FOLD,
            "tampered search-availability state root",
        );

        // Finally: the honest inputs still authenticate on this very fixture, so none of the above is
        // passing for an incidental reason.
        assert!(
            fixture
                .prepare_observing_no_durable_write(PALW_ANTISPAM_HEADER_VERSION, auth, fixture.snapshot.clone(), Some(honest))
                .is_ok(),
            "the negatives are vacuous unless the honest bundle still passes"
        );
    }

    /// FINDING 1 — the paid-work ATTRIBUTION gap, on the real importer.
    ///
    /// Both tampers below keep the transported nullifier UNION byte-identical, so each is first shown
    /// to pass `verify_chain_derived_pruning_boundary_from_payload` — the boundary fold that was, until
    /// this check existed, the whole of the chain-derived authentication. That assertion IS the gap:
    /// a payload the fold accepts installs a paid-work window whose per-row DAA attribution differs
    /// from the network's, and `palw_paid_work_window` re-filters those rows against an advancing
    /// anchor, so the node would derive a different `selected_parent_palw_state_root` a few epochs
    /// later and start rejecting HONEST blocks with `BadOverlayCommitment`.
    ///
    /// ADR-0042 **R1** — the row SET is bound to this node's selected chain, not just each row's
    /// contents.
    ///
    /// Every other check here is per-row: "is what you claim about block X true?". None asked
    /// "should block X be in this payload at all?". A row with `job_nullifiers == []` therefore went
    /// unconstrained — the header resolves, the DAA matches, and `palw_paid_work_store` legitimately
    /// holds nothing for a block that paid nothing, so an empty claim matches an empty answer. The
    /// `overlay_commitment_root` fold is blind to it too: empty rows contribute nothing to the
    /// nullifier union the fold covers. The node then PERSISTED and re-served a non-canonical
    /// snapshot, so its `payload_digest` diverged and operator-pinned peers refused to sync from it —
    /// poisoning it as an IBD source (availability, not a fork).
    ///
    /// Dropping an empty row is the cheapest form of that — no fabricated block — and is precisely
    /// what the fold cannot see. If this import is ACCEPTED, R1 is open.
    ///
    /// The fix is NOT "reject empty rows": the canonical builder emits one row per selected-chain
    /// block in the window, including blocks that paid nothing, so that rule would refuse honest
    /// payloads. The binding rebuilds the walk instead.
    #[tokio::test]
    async fn chain_derived_paid_work_row_set_is_bound_to_the_selected_chain() {
        let fixture = ChainDerivedFixture::build().await;
        let mut payload = fixture.snapshot.payload.clone();
        let dropped = payload
            .paid_work
            .iter()
            .position(|row| row.job_nullifiers.is_empty())
            .expect("the fixture chain must contain a block that paid nothing — otherwise R1 is untested");
        let removed = payload.paid_work.remove(dropped);
        let verdict = fixture.prepare_observing_no_durable_write(
            PALW_ANTISPAM_HEADER_VERSION,
            PalwPruningSnapshotImportAuth::chain_derived(fixture.pruning_point),
            reseal(payload),
            Some(&fixture.bundle),
        );
        expect_invalid(
            verdict,
            "not this node's selected chain",
            &format!("dropped the empty paid-work row for {}", removed.block_hash),
        );
    }

    /// `prepare_observing_no_durable_write` asserts around every call that neither the RocksDB write
    /// counter nor the staging write set moved, so the refusal lands strictly before any durable write.
    #[tokio::test]
    async fn chain_derived_binds_paid_work_row_attribution_to_the_local_stores() {
        let fixture = ChainDerivedFixture::build().await;
        let auth = PalwPruningSnapshotImportAuth::chain_derived(fixture.pruning_point);
        let walk_bound =
            fixture.config.params.palw_batch_admission.paid_work_walk_bound_daa(fixture.config.params.palw_epoch_length_daa);
        let honest_union = palw_pruning_payload_paid_work_nullifiers(&fixture.snapshot.payload, walk_bound);
        assert!(
            fixture.snapshot.payload.paid_work.len() > 1,
            "the fixture must carry a real below-boundary paid-work window to re-attribute"
        );

        // ---- (1) a re-dated row: same block hashes, same nullifier union, different attribution ----
        let mut redated = fixture.snapshot.payload.clone();
        assert_ne!(
            redated.paid_work[0].block_daa_score, redated.pruning_point_daa_score,
            "the re-dating tamper needs a row that is not already at the boundary DAA score"
        );
        redated.paid_work[0].block_daa_score = redated.pruning_point_daa_score;
        let redated = reseal(redated);
        assert_eq!(
            palw_pruning_payload_paid_work_nullifiers(&redated.payload, walk_bound),
            honest_union,
            "the tamper must leave the folded union byte-identical, or it is not the attack under test"
        );
        assert!(
            verify_chain_derived_pruning_boundary_from_payload(&redated.payload, walk_bound, &fixture.bundle).is_ok(),
            "THE GAP: the boundary fold accepts a re-dated paid-work window, so only a store cross-check can reject it"
        );
        expect_invalid(
            fixture.prepare_observing_no_durable_write(PALW_ANTISPAM_HEADER_VERSION, auth, redated, Some(&fixture.bundle)),
            "paid-work row re-dates its source block",
            "re-dated paid-work row",
        );

        // ---- (2) a row naming a block this node has no header for ----
        let mut unknown = fixture.snapshot.payload.clone();
        unknown.paid_work.push(PalwPrunedPaidWorkBlockV1 {
            block_hash: h(0xc0de),
            block_daa_score: unknown.pruning_point_daa_score,
            job_nullifiers: vec![],
        });
        let unknown = reseal(unknown);
        assert_eq!(
            palw_pruning_payload_paid_work_nullifiers(&unknown.payload, walk_bound),
            honest_union,
            "an empty invented row leaves the folded union byte-identical"
        );
        assert!(
            verify_chain_derived_pruning_boundary_from_payload(&unknown.payload, walk_bound, &fixture.bundle).is_ok(),
            "THE GAP: the boundary fold accepts an invented empty paid-work row"
        );
        expect_invalid(
            fixture.prepare_observing_no_durable_write(PALW_ANTISPAM_HEADER_VERSION, auth, unknown, Some(&fixture.bundle)),
            "paid-work row names a block with no locally available header",
            "paid-work row naming an unknown block",
        );

        // ---- the honest window still authenticates, so neither negative is vacuous ----
        assert!(
            fixture
                .prepare_observing_no_durable_write(
                    PALW_ANTISPAM_HEADER_VERSION,
                    auth,
                    fixture.snapshot.clone(),
                    Some(&fixture.bundle)
                )
                .is_ok(),
            "the honest chain-derived boundary must still authenticate against this node's own stores"
        );
    }

    /// The attribution cross-check is CHAIN-DERIVED ONLY, and the lever-off path is untouched.
    ///
    /// The operator-pin provenance authenticates the payload bytes verbatim through its digest, so an
    /// operator who pins a boundary keeps exactly the shipped semantics — including for a payload whose
    /// paid-work attribution the chain-derived rule would refuse. Asserting acceptance here is the
    /// point: it is what proves the new gate does not run on that path.
    #[tokio::test]
    async fn the_paid_work_attribution_check_never_runs_on_the_operator_pin_path() {
        let lever_on = ChainDerivedFixture::build().await;
        let mut redated = lever_on.snapshot.payload.clone();
        redated.paid_work[0].block_daa_score = redated.pruning_point_daa_score;
        let redated = reseal(redated);
        assert_ne!(redated.payload_digest, lever_on.snapshot.payload_digest, "the tamper must actually move the payload digest");

        let honest_pin =
            PalwPruningSnapshotCheckpoint { pruning_point: lever_on.pruning_point, payload_digest: lever_on.snapshot.payload_digest };
        let redated_pin =
            PalwPruningSnapshotCheckpoint { pruning_point: lever_on.pruning_point, payload_digest: redated.payload_digest };
        let peer = lever_on.spawn_lever_off_peer(vec![honest_pin, redated_pin]).await;
        assert!(!peer.config.palw_permissionless_snapshot_auth);

        assert!(
            peer.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                PalwPruningSnapshotImportAuth::operator_pinned(honest_pin),
                peer.snapshot.clone(),
                None,
            )
            .is_ok(),
            "the shipped operator-pinned path must keep working"
        );
        assert!(
            peer.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                PalwPruningSnapshotImportAuth::operator_pinned(redated_pin),
                redated.clone(),
                None,
            )
            .is_ok(),
            "the operator pin covers the paid-work attribution bytes itself; the chain-derived cross-check must not run here"
        );
        // Lever off, bundle supplied: still the operator-pinned verdict, unchanged.
        assert!(
            peer.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                PalwPruningSnapshotImportAuth::operator_pinned(redated_pin),
                redated,
                Some(&peer.bundle),
            )
            .is_ok(),
            "with the lever off a supplied bundle must not pull the import into the chain-derived branch"
        );
    }

    /// The negative observable is not vacuous: run the honest chain-derived boundary through the
    /// REAL durable write path (`import_pruning_point_palw_snapshot` → `stage_prepared_…` →
    /// `self.db.write`) and require BOTH observables the tests above assert *unchanged* to actually
    /// move. Without this, "the RocksDB write counter did not advance" would be a statement about a
    /// counter that never advances.
    ///
    /// It also pins the other half of the ordering claim: a chain-derived import that authenticates
    /// does install, so the fail-closed tests are rejecting tampering rather than rejecting the
    /// feature.
    #[tokio::test]
    async fn the_durable_write_observables_are_not_vacuous() {
        let fixture = ChainDerivedFixture::build().await;
        let vp = fixture.vp();
        let before_seq = vp.db.latest_sequence_number();
        let before_fp = palw_boundary_fingerprint(vp, fixture.pruning_point, &fixture.spam_closure);
        vp.import_pruning_point_palw_snapshot(
            fixture.pruning_point,
            fixture.pp_daa_score,
            PALW_ANTISPAM_HEADER_VERSION,
            fixture.pp_spam_commitment,
            PalwPruningSnapshotImportAuth::chain_derived(fixture.pruning_point),
            fixture.snapshot.clone(),
            Some(&fixture.bundle),
        )
        .expect("the honest chain-derived boundary must install through the real write path");
        assert_ne!(
            before_seq,
            vp.db.latest_sequence_number(),
            "the RocksDB write counter must advance on a real durable write, or the fail-closed assertions are vacuous"
        );
        assert_ne!(
            before_fp,
            palw_boundary_fingerprint(vp, fixture.pruning_point, &fixture.spam_closure),
            "the staging write set must change on a real durable write, or the fail-closed assertions are vacuous"
        );
    }

    /// The F6 trust boundary, on real chain headers rather than hand-built ones: a peer that
    /// Borsh-forges `Header::hash` cannot get a bundle out of the production projection at all. This
    /// is the transport layer's half of the same defence the previous test pins at the importer.
    #[tokio::test]
    async fn chain_derived_wire_projection_rejects_a_borsh_forged_support_header_hash() {
        let fixture = ChainDerivedFixture::build().await;
        assert!(fixture.wire.extract_authenticated_bundle().is_ok(), "the honest wire bundle must project");

        let mut forged = fixture.wire.clone();
        forged.support_headers[1].hash = h(0xf0f0);
        // Round-trip through Borsh, which is the only decoder that lets `hash` diverge from the body.
        let decoded: PalwChainDerivedHeaderBundleWireV1 = borsh::from_slice(&borsh::to_vec(&forged).unwrap()).unwrap();
        assert_eq!(decoded.support_headers[1].hash, h(0xf0f0), "Borsh must actually carry the peer-chosen hash");
        assert!(
            decoded.extract_authenticated_bundle().is_err(),
            "a Borsh-forged block identity must not project into an authentication bundle"
        );
    }

    /// (3) The fence: with the lever OFF a supplied bundle is IGNORED and behaviour is byte-identical
    /// to the pre-ADR-0042 operator-pinned path.
    ///
    /// Runs on a second node carrying the identical chain, the lever off, and the honest boundary
    /// pinned as an operator checkpoint. Four statements:
    ///   * the operator-pinned import succeeds with no bundle (the baseline);
    ///   * it produces an identical `PreparedPalwPruningPointSnapshotImport` when the honest bundle
    ///     IS supplied — the bundle changed nothing;
    ///   * a wrong operator digest still fails with `ImportedPalwSnapshotDigestMismatch` even with
    ///     the bundle present, so the bundle cannot bypass the digest equality check;
    ///   * chain-derived provenance is refused outright by the admission matrix.
    #[tokio::test]
    async fn lever_off_ignores_a_supplied_chain_derived_bundle() {
        let lever_on = ChainDerivedFixture::build().await;
        let pinned =
            PalwPruningSnapshotCheckpoint { pruning_point: lever_on.pruning_point, payload_digest: lever_on.snapshot.payload_digest };
        let mispinned = PalwPruningSnapshotCheckpoint { pruning_point: lever_on.pruning_point, payload_digest: h(0xbad1) };
        let peer = lever_on.spawn_lever_off_peer(vec![pinned, mispinned]).await;
        assert!(!peer.config.palw_permissionless_snapshot_auth);

        let without_bundle = peer
            .prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                PalwPruningSnapshotImportAuth::operator_pinned(pinned),
                peer.snapshot.clone(),
                None,
            )
            .expect("the shipped operator-pinned path must keep working");
        let with_bundle = peer
            .prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                PalwPruningSnapshotImportAuth::operator_pinned(pinned),
                peer.snapshot.clone(),
                Some(&peer.bundle),
            )
            .expect("supplying a bundle with the lever off must not change the operator-pinned verdict");
        assert_eq!(
            prepared_fingerprint(&without_bundle),
            prepared_fingerprint(&with_bundle),
            "with the lever off the chain-derived bundle must be ignored entirely"
        );

        match peer.prepare_observing_no_durable_write(
            PALW_ANTISPAM_HEADER_VERSION,
            PalwPruningSnapshotImportAuth::operator_pinned(mispinned),
            peer.snapshot.clone(),
            Some(&peer.bundle),
        ) {
            Err(PruningImportError::ImportedPalwSnapshotDigestMismatch(..)) => {}
            Err(other) => panic!("a bundle must not let a mis-pinned operator checkpoint through: {other:?}"),
            Ok(_) => panic!("a bundle must not let a mis-pinned operator checkpoint through: the import was ACCEPTED"),
        }

        expect_invalid(
            peer.prepare_observing_no_durable_write(
                PALW_ANTISPAM_HEADER_VERSION,
                PalwPruningSnapshotImportAuth::chain_derived(peer.pruning_point),
                peer.snapshot.clone(),
                Some(&peer.bundle),
            ),
            "is not allowed for exact header version",
            "chain-derived provenance with the lever off",
        );
    }

    /// The complementary refusal on the lever-ON node: a bundle supplied alongside a provenance that
    /// has its own authentication must never take the chain-derived branch, because doing so would
    /// skip that provenance's payload-digest equality check.
    #[tokio::test]
    async fn a_bundle_never_displaces_another_provenances_authentication() {
        let fixture = ChainDerivedFixture::build().await;
        for auth in [
            PalwPruningSnapshotImportAuth::operator_pinned(PalwPruningSnapshotCheckpoint {
                pruning_point: fixture.pruning_point,
                payload_digest: fixture.snapshot.payload_digest,
            }),
            PalwPruningSnapshotImportAuth::legacy_header_v3(fixture.pruning_point, fixture.snapshot.payload_digest),
        ] {
            expect_invalid(
                fixture.prepare_observing_no_durable_write(
                    PALW_ANTISPAM_HEADER_VERSION,
                    auth,
                    fixture.snapshot.clone(),
                    Some(&fixture.bundle),
                ),
                "must never displace that provenance's own authentication",
                "bundle presented with a non-chain-derived provenance",
            );
        }
    }

    /// (4) Header-v4 only, even with the lever on. The version fence fires before anything else in
    /// the chain-derived branch, so no non-v4 boundary can reach the fold.
    #[tokio::test]
    async fn chain_derived_requires_header_v4_even_with_the_lever_on() {
        let fixture = ChainDerivedFixture::build().await;
        for version in [PALW_HEADER_VERSION, PALW_ANTISPAM_HEADER_VERSION + 1] {
            expect_invalid(
                fixture.prepare_observing_no_durable_write(
                    version,
                    PalwPruningSnapshotImportAuth::chain_derived(fixture.pruning_point),
                    fixture.snapshot.clone(),
                    Some(&fixture.bundle),
                ),
                "chain-derived pruning-snapshot import requires Header-v4",
                "non-v4 header version with the lever on",
            );
        }
    }

    /// Every field of the read-only token, rendered for equality — so "the bundle changed nothing" is
    /// a statement about the whole import decision rather than a spot check.
    ///
    /// Honest caveat about what this comparison is dominated by on this fixture: the peer replays the
    /// same chain, so every boundary row already exists locally and the collision preflight resolves
    /// each optional write to `None`. The equality therefore chiefly pins the pruning point, the
    /// accepted snapshot envelope, and the fact that no write slot was filled differently. It is the
    /// complete token, but it is not a rich write plan.
    fn prepared_fingerprint(prepared: &PreparedPalwPruningPointSnapshotImport) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            prepared.pruning_point,
            prepared.snapshot,
            prepared.existing_provider_outpoints,
            prepared.beacon_state_write,
            prepared.beacon_accumulator_write,
            prepared.overlay_view_write,
            prepared.lane_bits_write,
            prepared.nullifier_write,
            prepared.spam_writes,
            prepared.da_state_write,
            prepared.search_availability_state_write,
        )
    }
}

#[cfg(test)]
mod palw_pruning_da_boundary_tests {
    use super::palw_da_boundary_matches;
    use kaspa_consensus_core::{
        Hash64,
        palw::da::{PALW_DA_SNAPSHOT_VERSION_V1, PalwDaPruningSnapshotV1, PalwDaStateV1},
    };

    fn snapshot(point_byte: u8) -> PalwDaPruningSnapshotV1 {
        PalwDaPruningSnapshotV1 {
            version: PALW_DA_SNAPSHOT_VERSION_V1,
            pruning_point: Hash64::from_bytes([point_byte; 64]),
            state: PalwDaStateV1::default(),
        }
    }

    #[test]
    fn embedded_da_requires_the_exact_standalone_boundary() {
        let embedded = snapshot(1);
        assert!(palw_da_boundary_matches(&embedded, Some(&embedded)));
        assert!(!palw_da_boundary_matches(&embedded, None));
        assert!(!palw_da_boundary_matches(&embedded, Some(&snapshot(2))));
    }
}

#[cfg(test)]
mod palw_pruning_spam_closure_tests {
    use super::{
        BlockHash, PALW_PRUNING_SNAPSHOT_VERSION, PalwPrunedFrontierV1, PalwPrunedSpamAccumulatorV1, PalwPrunedSpamFrontierV1,
        PalwPrunedSpamSupportRowV1, PalwPruningPointSnapshotPayloadV1, PalwPruningPointSnapshotV1, PalwSpamAccumulatorStoreReader,
        PalwSpamAccumulatorV1, PalwSpamLaneDelta, palw_spam_derive_child, validate_pruned_spam_closure,
    };
    use crate::model::stores::palw_spam::palw_spam_skip_height;
    use kaspa_database::prelude::StoreError;
    use kaspa_hashes::Hash64;
    use std::{collections::HashMap, sync::Arc};

    fn h(height: u64) -> BlockHash {
        let mut bytes = [0u8; 64];
        bytes[..8].copy_from_slice(&height.to_le_bytes());
        Hash64::from_bytes(bytes)
    }

    fn row(height: u64, window: u64) -> PalwPrunedSpamAccumulatorV1 {
        PalwPrunedSpamAccumulatorV1 {
            version: 1,
            daa_score: height,
            selected_height: height,
            total_hash_blues: height,
            total_replica_blues: 0,
            selected_parent: (height != 0).then(|| h(height - 1)),
            skip: (height >= 2).then(|| h(palw_spam_skip_height(height, window))),
        }
    }

    fn scrambled_h(height: u64) -> BlockHash {
        let scrambled = height.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left((height % 63) as u32);
        let mut bytes = [0u8; 64];
        bytes[..8].copy_from_slice(&scrambled.to_le_bytes());
        bytes[8..16].copy_from_slice(&height.to_le_bytes());
        Hash64::from_bytes(bytes)
    }

    fn scrambled_row(height: u64, window: u64) -> PalwPrunedSpamAccumulatorV1 {
        PalwPrunedSpamAccumulatorV1 {
            version: 1,
            daa_score: height,
            selected_height: height,
            total_hash_blues: height,
            total_replica_blues: 0,
            selected_parent: (height != 0).then(|| scrambled_h(height - 1)),
            skip: (height >= 2).then(|| scrambled_h(palw_spam_skip_height(height, window))),
        }
    }

    fn complete_frontier(tip: u64, window: u64) -> PalwPrunedSpamFrontierV1 {
        let floor = tip.saturating_sub(kaspa_consensus_core::palw_antispam::palw_spam_checkpoint_span(window));
        PalwPrunedSpamFrontierV1 {
            pruning_point_state: row(tip, window),
            support_rows: (floor..tip)
                .rev()
                .map(|height| PalwPrunedSpamSupportRowV1 { block_hash: h(height), state: row(height, window) })
                .collect(),
        }
    }

    #[derive(Default)]
    struct MemoryStore(HashMap<BlockHash, Arc<PalwSpamAccumulatorV1>>);

    impl PalwSpamAccumulatorStoreReader for MemoryStore {
        fn get_optional(&self, hash: BlockHash) -> Result<Option<Arc<PalwSpamAccumulatorV1>>, StoreError> {
            Ok(self.0.get(&hash).cloned())
        }
    }

    fn to_store(row: &PalwPrunedSpamAccumulatorV1) -> PalwSpamAccumulatorV1 {
        PalwSpamAccumulatorV1 {
            version: row.version,
            daa_score: row.daa_score,
            selected_height: row.selected_height,
            total_hash_blues: row.total_hash_blues,
            total_replica_blues: row.total_replica_blues,
            selected_parent: row.selected_parent,
            skip: row.skip,
        }
    }

    #[test]
    fn complete_checkpoint_closure_supports_children_past_the_next_checkpoint() {
        const TIP: u64 = 96;
        const WINDOW: u64 = 17;
        const FUTURE: u64 = 40;
        let frontier = complete_frontier(TIP, WINDOW);
        validate_pruned_spam_closure(h(TIP), TIP, WINDOW, &frontier).unwrap();

        let mut store = MemoryStore::default();
        store.0.insert(h(TIP), Arc::new(to_store(&frontier.pruning_point_state)));
        for support in &frontier.support_rows {
            store.0.insert(support.block_hash, Arc::new(to_store(&support.state)));
        }
        let mut full = MemoryStore::default();
        for height in 0..=TIP + FUTURE {
            full.0.insert(h(height), Arc::new(to_store(&row(height, WINDOW))));
        }
        for height in TIP + 1..=TIP + FUTURE {
            let expected = palw_spam_derive_child(&full, h(height - 1), height, WINDOW, PalwSpamLaneDelta::default(), false).unwrap();
            let actual = palw_spam_derive_child(&store, h(height - 1), height, WINDOW, PalwSpamLaneDelta::default(), false)
                .expect("one imported checkpoint must support every future Header-v4 child");
            assert_eq!(actual, expected, "child {height} diverged after pruning import");
            store.0.insert(h(height), Arc::new(actual.0));
        }
    }

    #[test]
    fn missing_window_row_and_forged_skip_fail_closed() {
        const TIP: u64 = 40;
        let mut missing = complete_frontier(TIP, TIP);
        missing.support_rows.retain(|support| support.block_hash != h(TIP - 1));
        assert!(validate_pruned_spam_closure(h(TIP), TIP, TIP, &missing).is_err());

        let mut forged = complete_frontier(TIP, TIP);
        forged.pruning_point_state.skip = Some(h(TIP - 1));
        assert!(validate_pruned_spam_closure(h(TIP), TIP, TIP, &forged).is_err());
    }

    #[test]
    fn extra_old_and_wrong_boundary_rows_fail_closed_but_graph_order_is_irrelevant() {
        const TIP: u64 = 96;
        const WINDOW: u64 = 17;

        let mut extra = complete_frontier(TIP, WINDOW);
        extra.support_rows.push(PalwPrunedSpamSupportRowV1 { block_hash: h(1), state: row(1, WINDOW) });
        assert!(validate_pruned_spam_closure(h(TIP), TIP, WINDOW, &extra).is_err());

        let mut reordered = complete_frontier(TIP, WINDOW);
        reordered.support_rows.swap(0, 1);
        validate_pruned_spam_closure(h(TIP), TIP, WINDOW, &reordered).unwrap();

        let mut wrong_daa = complete_frontier(TIP, WINDOW);
        wrong_daa.pruning_point_state.daa_score += 1;
        assert!(validate_pruned_spam_closure(h(TIP), TIP, WINDOW, &wrong_daa).is_err());
    }

    #[test]
    fn builder_try_new_hash_sort_round_trips_the_exact_nonmonotonic_graph() {
        const TIP: u64 = 96;
        const WINDOW: u64 = 17;
        let floor = TIP - kaspa_consensus_core::palw_antispam::palw_spam_checkpoint_span(WINDOW);
        let builder_frontier = PalwPrunedSpamFrontierV1 {
            pruning_point_state: scrambled_row(TIP, WINDOW),
            support_rows: (floor..TIP)
                .rev()
                .map(|height| PalwPrunedSpamSupportRowV1 { block_hash: scrambled_h(height), state: scrambled_row(height, WINDOW) })
                .collect(),
        };
        let builder_order: Vec<BlockHash> = builder_frontier.support_rows.iter().map(|row| row.block_hash).collect();
        let snapshot = PalwPruningPointSnapshotV1::try_new(PalwPruningPointSnapshotPayloadV1 {
            version: PALW_PRUNING_SNAPSHOT_VERSION,
            pruning_point: scrambled_h(TIP),
            pruning_point_daa_score: TIP,
            paid_work_window_daa: 0,
            frontier: PalwPrunedFrontierV1 {
                beacon_state: None,
                overlay_view: None,
                lane_bits: None,
                active_nullifiers: Default::default(),
            },
            beacon_accumulator: None,
            spam_accumulator: Some(builder_frontier),
            da_snapshot: None,
            compute_registry_snapshot: None,
            search_availability_snapshot: None,
            active_batches: vec![],
            provider_bonds: vec![],
            paid_work: vec![],
        })
        .unwrap();
        let canonical = snapshot.payload.spam_accumulator.as_ref().unwrap();
        let canonical_order: Vec<BlockHash> = canonical.support_rows.iter().map(|row| row.block_hash).collect();
        assert_ne!(canonical_order, builder_order, "fixture must exercise try_new's hash-order canonicalization");
        assert!(canonical.support_rows.windows(2).all(|rows| rows[0].block_hash < rows[1].block_hash));
        validate_pruned_spam_closure(scrambled_h(TIP), TIP, WINDOW, canonical).unwrap();
    }
}

#[cfg(test)]
mod palw_overlay_commit_tests {
    use super::{
        BlockHash, BondMutation, BondStatus, HashMap, PalwProviderBondMutation, PalwProviderBondRecord, StakeBondRecord,
        TransactionOutpoint, canonicalize_dns_bond_statuses, fail_palw_parent_waiters_on_shutdown, palw_body_may_be_pruned,
        palw_da_certificate_batch_allowed, palw_parent_terminal_status, palw_v4_all_pass_summary, palw_v4_parent_allows_certificate,
        palw_v4_parent_allows_leaf, reconcile_dns_bond_registry, reconcile_palw_provider_registry,
        validate_imported_palw_provider_utxo,
    };
    use crate::pipeline::deps_manager::VirtualStateProcessingMessage;
    use crate::pipeline::virtual_processor::errors::PruningImportError;
    use kaspa_consensus_core::blockstatus::BlockStatus;
    use kaspa_consensus_core::palw::{
        PalwBatchManifestV1, PalwBatchViewV1,
        da::{PalwDaObligationStatusV1, PalwDaObligationV1, PalwDaStateV1},
    };
    use kaspa_hashes::Hash64;

    fn outpoint(byte: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([byte; 64]), 0)
    }

    fn provider_record(outpoint: TransactionOutpoint) -> PalwProviderBondRecord {
        PalwProviderBondRecord {
            version: 1,
            bond_outpoint: outpoint,
            owner_pubkey_hash: Hash64::from_bytes([2; 64]),
            owner_public_key: vec![3; 32],
            operator_group_id: Hash64::from_bytes([4; 64]),
            runtime_classes: vec![Hash64::from_bytes([5; 64])],
            capacity_by_shape: vec![(6, 7)],
            reward_key_root: Hash64::from_bytes([8; 64]),
            amount_sompi: 10_000,
            activation_daa_score: 10,
            created_daa_score: 10,
            unbond_delay_epochs: 6,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        }
    }

    #[test]
    fn slashed_provider_snapshot_missing_utxo_tamper_is_rejected() {
        let op = outpoint(0x5a);
        let mut slashed = provider_record(op);
        // Even an earlier exit whose delay has elapsed cannot release a subsequently slashed bond:
        // slashing takes status precedence and burns/forfeits the still-locked principal.
        slashed.unbond_request_daa_score = Some(20);
        slashed.unbond_delay_epochs = 1;
        slashed.slashed_at_daa_score = Some(25);

        let err = validate_imported_palw_provider_utxo(&slashed, None, 200, 100).unwrap_err();
        assert!(matches!(err, PruningImportError::ImportedPalwProviderBondMissingUtxo(actual) if actual == op));
    }

    fn dns_record(outpoint: TransactionOutpoint) -> StakeBondRecord {
        StakeBondRecord {
            version: 1,
            bond_outpoint: outpoint,
            owner_pubkey_hash: Hash64::from_bytes([10; 64]),
            validator_pubkey_hash: Hash64::from_bytes([11; 64]),
            validator_pubkey: vec![12; 32],
            amount: 20_000,
            activation_daa_score: 10,
            created_daa_score: 10,
            unbonding_period_blocks: 1_000,
            owner_reward_spk_payload: [13; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: BondStatus::Active,
            last_attested_epoch: None,
            dormant_at_daa_score: None,
            dormant_at_epoch: None,
        }
    }

    #[test]
    fn only_the_current_pruning_point_may_normally_lack_a_body() {
        let pruning_point = BlockHash::from(41u64);
        assert!(palw_body_may_be_pruned(pruning_point, pruning_point));
        assert!(!palw_body_may_be_pruned(BlockHash::from(42u64), pruning_point));
    }

    #[test]
    fn v4_parent_barrier_preserves_normal_disqualified_side_parent_dags() {
        assert_eq!(palw_parent_terminal_status(BlockStatus::StatusUTXOValid, true, true), Ok(true));
        assert!(palw_parent_terminal_status(BlockStatus::StatusUTXOValid, true, false).is_err());
        assert_eq!(palw_parent_terminal_status(BlockStatus::StatusDisqualifiedFromChain, false, false), Ok(true));
        assert!(palw_parent_terminal_status(BlockStatus::StatusDisqualifiedFromChain, true, false).is_err());
        assert_eq!(palw_parent_terminal_status(BlockStatus::StatusUTXOPendingVerification, false, false), Ok(false));
        assert!(palw_parent_terminal_status(BlockStatus::StatusHeaderOnly, false, false).is_err());
        assert!(palw_parent_terminal_status(BlockStatus::StatusInvalid, false, false).is_err());
    }

    #[test]
    fn shutdown_explicitly_wakes_every_v4_parent_waiter() {
        let (first_tx, first_rx) = crossbeam_channel::bounded(1);
        let (second_tx, second_rx) = crossbeam_channel::bounded(1);
        let messages = vec![
            VirtualStateProcessingMessage::EnsurePalwParents {
                parents: vec![BlockHash::from(1u64)],
                selected_parent: BlockHash::from(1u64),
                result: first_tx,
            },
            VirtualStateProcessingMessage::Exit,
            VirtualStateProcessingMessage::EnsurePalwParents {
                parents: vec![BlockHash::from(2u64)],
                selected_parent: BlockHash::from(2u64),
                result: second_tx,
            },
        ];
        fail_palw_parent_waiters_on_shutdown(&messages);
        assert!(first_rx.recv().unwrap().is_err());
        assert!(second_rx.recv().unwrap().is_err());
    }

    #[test]
    fn v4_certificate_summary_requires_canonical_all_pass() {
        assert!(palw_v4_all_pass_summary(8, Hash64::default(), 8));
        assert!(!palw_v4_all_pass_summary(7, Hash64::default(), 8));
        assert!(!palw_v4_all_pass_summary(8, Hash64::from_bytes([1; 64]), 8));
    }

    #[test]
    fn v4_lifecycle_effects_require_a_full_parent_carrier_gap() {
        let mut manifest = PalwBatchManifestV1 {
            version: 1,
            batch_id: Hash64::default(),
            registration_epoch: 5,
            model_profile_id: Hash64::from_bytes([2; 64]),
            runtime_class_id: Hash64::from_bytes([3; 64]),
            leaf_count: 1,
            chunk_count: 1,
            leaf_root: Hash64::from_bytes([4; 64]),
            descriptor_root: Hash64::from_bytes([5; 64]),
            total_leaf_bond_sompi: 0,
            audit_policy_id: Hash64::from_bytes([6; 64]),
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
        };
        manifest.batch_id = manifest.content_id();

        let empty_parent = PalwBatchViewV1::new();
        assert!(!palw_v4_parent_allows_leaf(&empty_parent, &manifest.batch_id), "same-set manifest must not unlock a leaf");

        let mut registering_parent = PalwBatchViewV1::new();
        assert!(registering_parent.apply_manifest(&manifest, 5, 256, 64, 2, 6, 6, 0, 1_024));
        assert!(palw_v4_parent_allows_leaf(&registering_parent, &manifest.batch_id));
        assert!(registering_parent.apply_leaf_chunk(&manifest.batch_id, 0));
        // This clone represents the next block's selected-parent snapshot. A certificate in the same
        // acceptance set as the completing chunk still tests the pre-chunk parent and is refused.
        assert!(palw_v4_parent_allows_certificate(&registering_parent, &manifest.batch_id));

        let mut pre_chunk_parent = PalwBatchViewV1::new();
        assert!(pre_chunk_parent.apply_manifest(&manifest, 5, 256, 64, 2, 6, 6, 0, 1_024));
        assert!(!palw_v4_parent_allows_certificate(&pre_chunk_parent, &manifest.batch_id));
    }

    #[test]
    fn da_certificate_gate_requires_a_nonempty_all_satisfied_selected_parent_set() {
        let batch_id = Hash64::from_bytes([0x31; 64]);
        let obligation_id = Hash64::from_bytes([0x32; 64]);
        let mut state = PalwDaStateV1::default();
        assert!(!palw_da_certificate_batch_allowed(&state, &batch_id), "empty is fail-closed");
        state.obligations.insert(
            obligation_id,
            PalwDaObligationV1 {
                version: 1,
                obligation_id,
                batch_id,
                leaf_index: 0,
                leaf_hash: Hash64::from_bytes([0x33; 64]),
                object_root: Hash64::from_bytes([0x34; 64]),
                object_len: 1,
                chunk_count: 1,
                chunk_index: 0,
                provider_bond: outpoint(0x35),
                beacon_epoch: 1,
                beacon_anchor: Hash64::from_bytes([0x36; 64]),
                created_daa_score: 10,
                retention_until_daa_score: 100,
                status: PalwDaObligationStatusV1::Pending,
            },
        );
        assert!(!palw_da_certificate_batch_allowed(&state, &batch_id));
        let certificate_parent = state.clone();
        state.obligations.get_mut(&obligation_id).unwrap().status =
            PalwDaObligationStatusV1::Satisfied(Hash64::from_bytes([0x37; 64]));
        assert!(palw_da_certificate_batch_allowed(&state, &batch_id));
        assert!(
            !palw_da_certificate_batch_allowed(&certificate_parent, &batch_id),
            "a same-acceptance-set DA response must not unlock a certificate; the gate reads the selected-parent snapshot"
        );
    }

    #[test]
    fn provider_registry_writer_reconciles_multi_block_attach_and_whole_detach() {
        let op = outpoint(1);
        let base = provider_record(op);
        let insert_block = BlockHash::from(11u64);
        let unbond_block = BlockHash::from(12u64);
        let added = vec![
            (insert_block, vec![PalwProviderBondMutation::Insert(op, base.clone())]),
            (unbond_block, vec![PalwProviderBondMutation::Unbond(op, 20)]),
        ];
        let mut working = HashMap::new();
        let touched = reconcile_palw_provider_registry(&mut working, &[], &added).unwrap();
        assert!(touched.contains(&op));
        assert_eq!(working.get(&op).unwrap().unbond_request_daa_score, Some(20));

        // `ChainPath::removed` is tip -> ancestor. Reverting it in this order clears Unbond before
        // removing Insert; reversing the vector is the historical residual-row bug.
        let removed = vec![
            (unbond_block, vec![PalwProviderBondMutation::Unbond(op, 20)]),
            (insert_block, vec![PalwProviderBondMutation::Insert(op, base)]),
        ];
        reconcile_palw_provider_registry(&mut working, &removed, &[]).unwrap();
        assert!(!working.contains_key(&op));
    }

    #[test]
    fn provider_registry_writer_uses_detached_working_row_before_attached_mutation() {
        let op = outpoint(9);
        let mut old = provider_record(op);
        old.unbond_request_daa_score = Some(20);
        let mut working = HashMap::from([(op, old)]);
        let removed = vec![(BlockHash::from(21u64), vec![PalwProviderBondMutation::Unbond(op, 20)])];
        let added = vec![(BlockHash::from(22u64), vec![PalwProviderBondMutation::Slash(op, 30)])];

        reconcile_palw_provider_registry(&mut working, &removed, &added).unwrap();
        let final_record = working.get(&op).unwrap();
        assert_eq!(final_record.unbond_request_daa_score, None);
        assert_eq!(final_record.slashed_at_daa_score, Some(30));
    }

    #[test]
    fn dns_registry_writer_reconciles_multi_block_attach_and_whole_detach() {
        let op = outpoint(31);
        let base = dns_record(op);
        let insert_block = BlockHash::from(31u64);
        let unbond_block = BlockHash::from(32u64);
        let slash_block = BlockHash::from(33u64);
        let added = vec![
            (insert_block, vec![BondMutation::Insert(op, base.clone())]),
            (unbond_block, vec![BondMutation::Unbond(op, 20)]),
            (slash_block, vec![BondMutation::Slash(op, 30)]),
        ];
        let mut working = HashMap::new();
        reconcile_dns_bond_registry(&mut working, &[], &added).unwrap();
        let attached = working.get(&op).unwrap();
        assert_eq!(attached.unbond_request_daa_score, Some(20));
        assert_eq!(attached.slashed_at_daa_score, Some(30));
        assert_eq!(attached.status, BondStatus::Slashed);

        // Native ChainPath order is tip -> ancestor. Each later stamp must be cleared before the
        // creating Insert is removed.
        let removed = vec![
            (slash_block, vec![BondMutation::Slash(op, 30)]),
            (unbond_block, vec![BondMutation::Unbond(op, 20)]),
            (insert_block, vec![BondMutation::Insert(op, base)]),
        ];
        reconcile_dns_bond_registry(&mut working, &removed, &[]).unwrap();
        assert!(!working.contains_key(&op));
    }

    #[test]
    fn dns_registry_writer_restores_underlying_status_and_feeds_final_snapshot() {
        let op = outpoint(41);
        let mut dormant = dns_record(op);
        dormant.dormant_at_daa_score = Some(15);
        dormant.dormant_at_epoch = Some(2);
        dormant.status = BondStatus::Dormant;
        let original = dormant.clone();
        let mut working = HashMap::from([(op, dormant)]);

        let slash_block = BlockHash::from(41u64);
        let added = vec![(slash_block, vec![BondMutation::Slash(op, 30)])];
        reconcile_dns_bond_registry(&mut working, &[], &added).unwrap();
        assert_eq!(working.get(&op).unwrap().status, BondStatus::Slashed);

        let removed = vec![(slash_block, vec![BondMutation::Slash(op, 30)])];
        reconcile_dns_bond_registry(&mut working, &removed, &[]).unwrap();
        assert_eq!(working.get(&op), Some(&original), "apply then detach must restore the byte-identical row");

        // The map itself is the exact post-reorg snapshot passed to update_dns_state before the DB
        // batch commits; this assertion pins the row that downstream recomputation must observe.
        let snapshot: Vec<_> = working.into_values().collect();
        assert_eq!(snapshot, vec![original]);
    }

    #[test]
    fn dns_registry_writer_canonicalizes_time_advanced_cached_status() {
        let op = outpoint(51);
        let mut pending = dns_record(op);
        pending.activation_daa_score = 100;
        pending.status = BondStatus::Pending;
        let mut working = HashMap::from([(op, pending)]);

        let touched = canonicalize_dns_bond_statuses(&mut working, 200);
        assert_eq!(working.get(&op).unwrap().status, BondStatus::Active);
        assert_eq!(touched, [op].into_iter().collect());
        assert!(canonicalize_dns_bond_statuses(&mut working, 200).is_empty(), "normalization must be idempotent");
    }
}
