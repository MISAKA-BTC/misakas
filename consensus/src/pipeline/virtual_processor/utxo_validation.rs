use super::VirtualStateProcessor;
use crate::{
    errors::{
        BlockProcessResult,
        RuleError::{
            BadAcceptedIDMerkleRoot, BadCoinbaseTransaction, BadOverlayCommitment, BadPalwBeaconSeed, BadUTXOCommitment,
            IneligibleAttestationInBlock, InvalidTransactionsInUtxoContext, MissingMandatoryAttestationInBlock,
            NonReleasableBondSpendInBlock, PalwComputeSetGoverningMismatch, PalwLaneHalted, UnauthorizedUnbondRequestInBlock,
            UnverifiableSlashingEvidenceInBlock, WrongHeaderPruningPoint,
        },
    },
    model::stores::{
        block_transactions::BlockTransactionsStoreReader,
        daa::DaaStoreReader,
        ghostdag::{CompactGhostdagData, GhostdagData},
        headers::HeaderStoreReader,
        palw::PalwStoreReader,
        palw_paid_work::PalwPaidWorkIds,
        rewarded_epochs::RewardedEpochKeys,
    },
    processes::{
        pruning::PruningPointReply,
        transaction_validator::{
            errors::{TxResult, TxRuleError},
            tx_validation_in_utxo_context::TxValidationFlags,
        },
    },
};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
    acceptance_data::{AcceptedTxEntry, MergesetBlockAcceptanceData},
    api::args::TransactionValidationArgs,
    coinbase::*,
    dns_finality::{
        ATTESTATION_MLDSA87_CONTEXT, ActiveBondView, BlockEpochContribution, BondMutation, BondStatus, DnsParams, FeeSplitParams,
        OverlaySnapshot, RewardedEpochSet, SlashingSideEffect, StakeAttestation, UNBOND_REQUEST_CONTEXT,
        attestations_from_accepted_txs, bond_mutations_from_accepted_txs, bond_release_daa_score, decode_attestation_shard,
        effective_bond_status, epoch_meets_quality_floor, epochs_finalized_at, is_bond_active_at, mandatory_attestation_mass_capacity,
        recompute_epoch_tallies, resolve_slashing_side_effects, slashing_evidence_from_accepted_txs, split_validator_pool,
        stake_attestation_message, unbond_request_message, unbond_requests_from_accepted_txs, validator_id_from_pubkey,
        validator_participation_reward_outputs, validator_quality_bonus_outputs, victim_compensation_outputs,
    },
    hashing,
    header::Header,
    muhash::MuHashExtensions,
    palw::{
        PalwBatchViewV1, PalwProviderBondMutation, ProviderBondView,
        da::{
            PALW_DA_MAX_OBLIGATIONS, PALW_DA_MAX_PRUNING_SNAPSHOT_BYTES, PALW_RECEIPT_DA_OBJECT_VERSION_V2, PalwBuriedBeaconV1,
            PalwDaError, PalwDaPolicyV1, PalwDaStateV1, PalwReceiptDaCommitmentV1,
        },
        is_provider_bond_releasable_at, palw_leaf_merkle_depth, palw_provider_bond_mutations_from_accepted_txs,
        palw_verify_leaf_membership, provider_bond_lock_spk,
    },
    subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
    tx::{
        MutableTransaction, PopulatedTransaction, Transaction, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry,
        ValidatedTransaction, VerifiableTransaction,
    },
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
};
use kaspa_core::{info, trace};
use kaspa_database::prelude::StoreErrorPredicates;
use kaspa_muhash::MuHash;
use kaspa_txscript::{script_class::parse_evm_deposit_lock, verify_mldsa87_with_context};
use kaspa_utils::refs::Refs;

use rayon::prelude::*;
use smallvec::{SmallVec, smallvec};
use std::{
    collections::{HashMap, HashSet},
    iter::once,
    ops::Deref,
};

pub(crate) mod crescendo {
    use kaspa_core::{info, log::CRESCENDO_KEYWORD};
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };

    #[derive(Clone)]
    pub(crate) struct _CrescendoLogger {
        steps: Arc<AtomicU8>,
    }

    impl _CrescendoLogger {
        pub fn _new() -> Self {
            Self { steps: Arc::new(AtomicU8::new(Self::_ACTIVATE)) }
        }

        const _ACTIVATE: u8 = 0;

        pub fn _report_activation(&self) -> bool {
            if self.steps.compare_exchange(Self::_ACTIVATE, Self::_ACTIVATE + 1, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                info!(target: CRESCENDO_KEYWORD, "[Crescendo] [--------- Crescendo activated for UTXO state processing rules ---------]");
                true
            } else {
                false
            }
        }
    }
}

/// A context for processing the UTXO state of a block with respect to its selected parent.
/// Note this can also be the virtual block.
pub(super) struct UtxoProcessingContext<'a> {
    pub ghostdag_data: Refs<'a, GhostdagData>,
    pub multiset_hash: MuHash,
    pub mergeset_diff: UtxoDiff,
    pub accepted_tx_ids: Vec<TransactionId>,
    pub mergeset_acceptance_data: Vec<MergesetBlockAcceptanceData>,
    pub mergeset_rewards: BlockHashMap<BlockRewardData>,
    pub pruning_sample_from_pov: Option<BlockHash>,
    /// kaspa-pq (ADR-0009 Addendum B §B.3(c)): the `(bond, epoch)` pairs this
    /// block's coinbase rewarded, computed during `verify_expected_utxo_state`
    /// and persisted by `commit_utxo_state` for descendant uniqueness checks.
    pub validator_rewarded_keys: RewardedEpochKeys,
    /// kaspa-pq **ADR-0040 §5.15.13 (gate G16 / P1-9-RELAND)**: the `job_nullifier`s this block's
    /// coinbase actually PAID a `ReplicaPalw` provider pair for, in canonical mergeset order. Filled
    /// by `calculate_utxo_state` at the SAME seam that classifies the reward (so construction and
    /// validation record the identical list), and persisted by `commit_utxo_state` for descendants to
    /// dedup against.
    pub palw_paid_work_ids: PalwPaidWorkIds,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): this block's validator quality
    /// sub-pool (`split_validator_pool(.).1`), persisted by `commit_utxo_state` as
    /// the per-epoch accumulator's recompute input. `0` (never persisted) below
    /// `pos_v2_activation_daa_score` — i.e. on the devnet/simnet preset
    /// (`GENESIS_ACTIVE_DNS_PARAMS`, fenced at `u64::MAX`); on mainnet/testnet
    /// (`PRODUCTION_DNS_PARAMS`, fence = 0) it is populated from block 1.
    pub validator_quality_subpool: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): this block's security-reserve **accrual** — the
    /// `Σ security_reserve_sompi` of its slashing side-effects (set in `apply_slashing_side_effects`).
    /// Feeds the per-block reserve-balance recurrence. `0` below the v2 fence.
    pub reserve_accrual: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): this block's **cumulative reserve balance**
    /// (`balance_after(selected_parent) + reserve_accrual − drip`), persisted by `commit_utxo_state`
    /// when non-zero. The finalizing coinbase reads the selected parent's value for the drip. `0`
    /// (never persisted) below the v2 fence.
    pub reserve_balance_after: u64,
}

impl<'a> UtxoProcessingContext<'a> {
    pub fn new(ghostdag_data: Refs<'a, GhostdagData>, selected_parent_multiset_hash: MuHash) -> Self {
        let mergeset_size = ghostdag_data.mergeset_size();
        Self {
            ghostdag_data,
            multiset_hash: selected_parent_multiset_hash,
            mergeset_diff: UtxoDiff::default(),
            accepted_tx_ids: Vec::with_capacity(1), // We expect at least the selected parent coinbase tx
            mergeset_rewards: BlockHashMap::with_capacity(mergeset_size),
            mergeset_acceptance_data: Vec::with_capacity(mergeset_size),
            pruning_sample_from_pov: Default::default(),
            validator_rewarded_keys: Vec::new(),
            palw_paid_work_ids: Vec::new(),
            validator_quality_subpool: 0,
            reserve_accrual: 0,
            reserve_balance_after: 0,
        }
    }

    pub fn selected_parent(&self) -> BlockHash {
        self.ghostdag_data.selected_parent
    }
}

/// kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): the per-tx bond-spend SKIP filter
/// threaded into mergeset acceptance validation. When `Some`, a transaction that spends a **known
/// non-releasable** bond's locked output-0 (resolved in `bond_view` at `daa_score`) fails UTXO
/// validation — so the acceptance loop SKIPS it (treats it like an invalid tx), keeping the carrying
/// block valid and the bond UTXO locked. `None` ⇒ the check is inert (every spend allowed by it), so
/// below the activation fence behavior is byte-identical to the legacy own-body-only gate.
///
/// `bond_view` is the **post-acceptance** view (selected-parent bonds + every bond freshly inserted
/// anywhere in this block's mergeset), a deterministic function of the shared inputs, so the rule is
/// construction == validation. Cheap to `Copy` (a shared ref + a u64); `Sync` for the parallel walk.
#[derive(Clone, Copy)]
pub(crate) struct BondSpendFilter<'a> {
    bond_view: &'a ActiveBondView,
    daa_score: u64,
}

impl BondSpendFilter<'_> {
    /// `true` iff `outpoint` is a known bond that is NOT releasable at `self.daa_score` (i.e. its
    /// locked output-0 must not be spent). Mirrors the legacy [`bond_spend_gate`] releasable test.
    fn locks(&self, outpoint: &TransactionOutpoint) -> bool {
        self.bond_view.get(outpoint).is_some_and(|bond| {
            let releasable = effective_bond_status(bond, self.daa_score) == BondStatus::Unbonding
                && bond_release_daa_score(bond).is_some_and(|release| self.daa_score >= release);
            !releasable
        })
    }
}

/// kaspa-pq **ADR-0040 ECON-03 leg 5**: the per-tx provider-unbond AUTHORIZATION skip filter threaded
/// into mergeset acceptance validation — the exact shape of [`BondSpendFilter`], and for the exact
/// same reason.
///
/// When `Some`, a `0x37` transaction whose request is not owner-authorized against
/// `provider_bond_view` fails UTXO validation, so the acceptance loop SKIPS it (treats it like an
/// invalid tx). It therefore never enters `ctx.mergeset_acceptance_data`, never reaches
/// `palw_provider_bond_mutations_from_accepted_txs`, and **mutates nothing** — while the carrying and
/// merging blocks both stay valid. `None` ⇒ the check is inert (every `0x37` allowed by it), which is
/// every path that is not fence-active mergeset acceptance.
///
/// `provider_bond_view` is the **selected-parent** registry, NOT a view including this block's own
/// mutations — see the wiring note in [`VirtualStateProcessor::calculate_utxo_state`] for why that
/// point of view is load-bearing. Cheap to `Copy` (a shared ref + a u32 + a u64); `Sync` for the
/// parallel walk.
#[derive(Clone, Copy)]
pub(crate) struct ProviderUnbondAuthFilter<'a> {
    provider_bond_view: &'a ProviderBondView,
    da_state: &'a PalwDaStateV1,
    network_id: u32,
    daa_score: u64,
}

impl ProviderUnbondAuthFilter<'_> {
    /// `Some(bond_outpoint)` iff `tx` is a `0x37` provider-unbond transaction carrying a request that
    /// is NOT owner-authorized at this point of view; `None` for every other transaction and for an
    /// authorized request.
    ///
    /// Decoding routes through `palw_provider_unbond_requests_from_accepted_txs`, the SAME single
    /// decoder the registry producer uses, so the set of transactions this filter judges and the set
    /// that would mutate the registry are identical by construction rather than by two matching copies
    /// of the same three lines. Non-`0x37` traffic short-circuits on the subnetwork byte before any
    /// allocation or signature work, so the ML-DSA-87 verification is paid only on actual requests.
    fn unauthorized(&self, tx: &Transaction) -> Option<TransactionOutpoint> {
        kaspa_consensus_core::palw::palw_provider_unbond_requests_from_accepted_txs(std::slice::from_ref(tx))
            .into_iter()
            .find(|(_, req)| {
                !crate::processes::palw::palw_provider_unbond_request_authorized(
                    req,
                    self.provider_bond_view,
                    self.network_id,
                    self.daa_score,
                ) || !self.da_state.exit_allowed(&req.bond_outpoint, self.daa_score)
            })
            .map(|(_, req)| req.bond_outpoint)
    }
}

#[inline]
fn palw_da_reward_pair_allowed(
    state: &PalwDaStateV1,
    provider_a: &TransactionOutpoint,
    provider_b: &TransactionOutpoint,
    daa_score: u64,
) -> bool {
    state.reward_allowed(provider_a, daa_score) && state.reward_allowed(provider_b, daa_score)
}

/// kaspa-pq **ADR-0040 ECON-03 leg 4 — the provider-bond SPEND gate, as an acceptance-time SKIP.**
///
/// The exact shape of [`BondSpendFilter`], swapping the DNS releasability test for the provider helper
/// [`is_provider_bond_releasable_at`]. A provider bond's locked output-0 is plain P2PKH the owner could
/// otherwise spend the next block, which would make [`PalwProviderBondRecord::unbond_delay_epochs`] a
/// reward-side clock over collateral that is not actually locked — a bond could resolve `Active` at
/// payout while its coins were already gone. This filter locks that output-0 until the bond is
/// registry-releasable (Unbonding AND past its clamped release DAA) **and** the selected-parent DA
/// state permits exit. Both are required: an authorized unbond and a new leaf may share a mergeset,
/// so a later spend must not forget the obligation created after the unbond authorization snapshot.
///
/// When `Some`, a transaction spending a known non-releasable provider bond's output-0 fails UTXO
/// validation, so the acceptance loop SKIPS it (treats it like an invalid tx) — the carrying block
/// stays valid and the bond's output-0 stays in the set. It is NOT a block-reject; that shape was the
/// merge-blue DoS removed in leg 5. `None` ⇒ inert (every spend allowed by it), which is every path
/// that is not fence-active mergeset acceptance, so below the fence acceptance is byte-identical.
///
/// `provider_bond_view` is the **post-acceptance** view (selected-parent provider bonds + every
/// provider-bond Insert declared anywhere in this block's mergeset), a deterministic function of the
/// shared inputs, so the rule is construction == validation. `da_state` is the same selected-parent
/// state used by the unbond authorization and reward gates. Provider release is in EPOCHS, so the
/// filter also carries `epoch_length_daa` (the DNS gate needed only blocks). Cheap to `Copy`; `Sync`
/// for the parallel walk.
#[derive(Clone, Copy)]
pub(crate) struct ProviderBondSpendFilter<'a> {
    provider_bond_view: &'a ProviderBondView,
    da_state: &'a PalwDaStateV1,
    epoch_length_daa: u64,
    daa_score: u64,
}

impl ProviderBondSpendFilter<'_> {
    /// `true` iff `outpoint` is a known provider bond that fails either the registry release clock or
    /// the DA exit gate at `self.daa_score` (i.e. its locked output-0 must not be spent).
    fn locks(&self, outpoint: &TransactionOutpoint) -> bool {
        self.provider_bond_view.get(outpoint).is_some_and(|rec| {
            !is_provider_bond_releasable_at(rec, self.daa_score, self.epoch_length_daa)
                || !self.da_state.exit_allowed(outpoint, self.daa_score)
        })
    }
}

/// Header-v4 canonical leaf-chunk span predicate shared by acceptance-time DA reservation and the
/// accepted-view commit. Keeping one predicate prevents the later bitmap transition from accepting a
/// payload the capacity gate refused.
pub(super) fn palw_v4_leaf_chunk_matches_canonical_span(
    chunk: &kaspa_consensus_core::palw::PalwLeafChunkV1,
    lifecycle: &kaspa_consensus_core::palw::PalwBatchLifecycleV1,
    max_leaf_chunk_leaves: u16,
    v3_admitted: bool,
) -> bool {
    // Bug report #6 flag day: below `palw_leaf_chunk_v3_admission_daa_score` NO chunk version is
    // admissible — v2 died with D3-b's context-free rejection, and admitting v3 early is exactly
    // the acceptance divergence that partitioned testnet-21 (a carrier accepted by fixed nodes and
    // skipped by everyone else splits the UTXO set on the next block). `v3_admitted` is derived
    // from the POV block's own consensus daa at both call sites, never from local wall-clock state.
    if !v3_admitted {
        return false;
    }
    // Header-v4 treats each bitmap bit as the commitment that EVERY leaf in that canonical chunk
    // span was accepted. Merkle membership alone is insufficient: without this binding, the same
    // valid leaf/proof (or any proper subset) can be relabelled with another `chunk_index`, set all
    // bitmap bits, and advance the batch to Committed while required leaf slots remain absent.
    //
    // The width is the same consensus admission parameter which fixed `manifest.chunk_count`.
    // Require both the inherited lifecycle shape and this payload to agree with it before touching
    // DA capacity, accepted-chunk state, or the later lifecycle/content staging path.
    // ADR-0045 D3-b: the payload version this predicate admits MUST be the one context-free
    // validation admits. D3-b moved the chunk to v3 (witnesses, without which clauses 11/12 have
    // nothing to check) and rejects v2 outright — while this predicate was left pinned at v2. The
    // two conditions are mutually exclusive, so every leaf chunk was refused here before the PCPB
    // arm ever ran, and the batch aged out of Registering with chunks 0/N and no error anywhere.
    let chunk_width = u32::from(max_leaf_chunk_leaves);
    if chunk.version != kaspa_consensus_core::palw::PALW_LEAF_CHUNK_VERSION_V3
        || chunk.chunk_index >= lifecycle.chunk_count
        || chunk.proofs.len() != chunk.leaves.len()
        || chunk_width == 0
        || u32::from(lifecycle.chunk_count) != lifecycle.leaf_count.div_ceil(chunk_width)
    {
        return false;
    }
    let Some(span_start) = u32::from(chunk.chunk_index).checked_mul(chunk_width) else {
        return false;
    };
    let Some(unclamped_span_end) = span_start.checked_add(chunk_width) else {
        return false;
    };
    let span_end = unclamped_span_end.min(lifecycle.leaf_count);
    let Some(expected_leaf_count) = span_end.checked_sub(span_start) else {
        return false;
    };
    expected_leaf_count != 0
        && chunk.leaves.len() == expected_leaf_count as usize
        && chunk.leaves.iter().enumerate().all(|(offset, leaf)| leaf.leaf_index == span_start + offset as u32)
}

/// Header-v4 acceptance-time DA reservation. This gate runs only after ordinary UTXO validation and
/// walks those survivors in canonical mergeset/transaction order. A capacity failure therefore skips
/// the carrying transaction instead of invalidating a block that merely merged it.
///
/// Leaf context is intentionally resolved from the selected parent's block-keyed lifecycle view, not
/// from the process-global content store. The latter can contain a sink-search loser or a side-fork
/// manifest solely because it arrived first. Requiring a parent lifecycle entry also imposes a carrier
/// gap: a manifest in this same acceptance set cannot make a leaf admissible.
struct PalwDaAcceptanceGate<'a> {
    state: PalwDaStateV1,
    parent_obligations: HashSet<kaspa_hashes::Hash64>,
    parent_open_challenges: HashSet<kaspa_hashes::Hash64>,
    parent_lifecycle: PalwBatchViewV1,
    palw_store: &'a dyn PalwStoreReader,
    accepted_chunks: HashSet<(kaspa_hashes::Hash64, u16)>,
    staged_leaf_hashes: HashMap<(kaspa_hashes::Hash64, u32), kaspa_hashes::Hash64>,
    buried_beacon: Option<PalwBuriedBeaconV1>,
    provider_bonds: &'a ProviderBondView,
    network_id: u32,
    daa_score: u64,
    epoch: u64,
    max_leaf_chunk_leaves: u16,
    /// Bug report #6 flag day: the daa score from which v3 leaf chunks are admitted.
    leaf_chunk_v3_admission_daa_score: u64,
    max_obligations: usize,
    max_snapshot_bytes: usize,
}

impl<'a> PalwDaAcceptanceGate<'a> {
    fn new(
        parent: &PalwDaStateV1,
        parent_lifecycle: PalwBatchViewV1,
        palw_store: &'a dyn PalwStoreReader,
        buried_beacon: Option<PalwBuriedBeaconV1>,
        provider_bonds: &'a ProviderBondView,
        network_id: u32,
        daa_score: u64,
        epoch_length_daa: u64,
        max_leaf_chunk_leaves: u16,
        leaf_chunk_v3_admission_daa_score: u64,
    ) -> Self {
        let parent_obligations = parent.obligations.keys().copied().collect();
        let parent_open_challenges = parent
            .challenges
            .iter()
            .filter_map(|(id, challenge)| {
                matches!(challenge.status, kaspa_consensus_core::palw::da::PalwDaChallengeStatusV1::Open).then_some(*id)
            })
            .collect();
        let mut state = parent.clone();
        state.begin_child_block();
        Self {
            state,
            parent_obligations,
            parent_open_challenges,
            parent_lifecycle,
            palw_store,
            accepted_chunks: HashSet::new(),
            staged_leaf_hashes: HashMap::new(),
            buried_beacon,
            provider_bonds,
            network_id,
            daa_score,
            epoch: daa_score / epoch_length_daa.max(1),
            max_leaf_chunk_leaves,
            leaf_chunk_v3_admission_daa_score,
            max_obligations: PALW_DA_MAX_OBLIGATIONS,
            max_snapshot_bytes: PALW_DA_MAX_PRUNING_SNAPSHOT_BYTES,
        }
    }

    /// `false` means the UTXO-valid transaction is omitted from acceptance. Semantic-invalid DA
    /// challenge/response/timeout effects retain their historical inert behavior; only a transition
    /// which is otherwise valid but cannot fit is skipped. Header-v4 leaf chunks are stricter because
    /// persisting their content without the corresponding obligation is the state-split this gate
    /// closes.
    fn allows(&mut self, tx: &Transaction) -> bool {
        let Some(kind) = tx.subnetwork_id.palw_tx_kind() else { return true };
        if matches!(kind, 0x3a..=0x3c) {
            return self.allows_da_effect(kind, &tx.payload);
        }
        let Ok(crate::processes::palw::PalwOverlayEffect::LeafChunk(chunk)) =
            crate::processes::palw::parse_palw_overlay(kind, &tx.payload)
        else {
            return true;
        };
        self.allows_leaf_chunk(&chunk)
    }

    fn allows_leaf_chunk(&mut self, chunk: &kaspa_consensus_core::palw::PalwLeafChunkV1) -> bool {
        let Some(lifecycle) = self.parent_lifecycle.entry(&chunk.batch_id) else {
            return false;
        };
        if lifecycle.status != kaspa_consensus_core::palw::PalwBatchStatus::Registering
            || !palw_v4_leaf_chunk_matches_canonical_span(
                chunk,
                lifecycle,
                self.max_leaf_chunk_leaves,
                self.daa_score >= self.leaf_chunk_v3_admission_daa_score,
            )
        {
            return false;
        }
        let (chunk_word, chunk_bit) = ((chunk.chunk_index / 64) as usize, chunk.chunk_index % 64);
        let Some(chunk_word_value) = lifecycle.chunks_present.get(chunk_word) else {
            return false;
        };
        if chunk_word_value & (1u64 << chunk_bit) != 0 || self.accepted_chunks.contains(&(chunk.batch_id, chunk.chunk_index)) {
            return false;
        }
        let expected_proof_len = palw_leaf_merkle_depth(lifecycle.leaf_count);
        for (leaf, proof) in chunk.leaves.iter().zip(&chunk.proofs) {
            if leaf.batch_id != chunk.batch_id
                || leaf.leaf_index >= lifecycle.leaf_count
                || leaf.registered_epoch != lifecycle.registration_epoch
                || leaf.receipt_da_object_version != PALW_RECEIPT_DA_OBJECT_VERSION_V2
                || proof.siblings.len() as u32 != expected_proof_len
            {
                return false;
            }
            let mut projected = leaf.clone();
            projected.batch_id = kaspa_hashes::Hash64::default();
            if !palw_verify_leaf_membership(&projected.leaf_hash(), leaf.leaf_index, lifecycle.leaf_count, proof, &lifecycle.leaf_root)
            {
                return false;
            }
            if self.staged_leaf_hashes.get(&(chunk.batch_id, leaf.leaf_index)).is_some_and(|existing| *existing != leaf.leaf_hash()) {
                return false;
            }
            match self.palw_store.leaf(chunk.batch_id, leaf.leaf_index) {
                Ok(existing) if existing.leaf_hash() != leaf.leaf_hash() => return false,
                Ok(_) => {}
                Err(error) if error.is_key_not_found() => {}
                Err(error) => panic!(
                    "PALW Header-v4 DA acceptance could not preflight immutable leaf slot {}/{}: {error}",
                    chunk.batch_id, leaf.leaf_index
                ),
            }
        }
        let Some(beacon) = self.buried_beacon.as_ref() else {
            return false;
        };
        let mut candidate = self.state.clone();
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        for leaf in &chunk.leaves {
            let commitment = PalwReceiptDaCommitmentV1 {
                object_version: leaf.receipt_da_object_version,
                object_len: leaf.receipt_da_object_len,
                chunk_count: leaf.receipt_da_chunk_count,
                root: leaf.receipt_da_root,
            };
            match candidate.register_leaf_obligations(leaf, commitment, beacon, &policy, self.daa_score) {
                Ok(_) => {}
                Err(_) => return false,
            }
        }
        if candidate.obligations.len() > self.max_obligations
            || candidate.canonical_snapshot_encoded_len().is_none_or(|len| len > self.max_snapshot_bytes)
        {
            return false;
        }
        self.state = candidate;
        self.accepted_chunks.insert((chunk.batch_id, chunk.chunk_index));
        self.staged_leaf_hashes.extend(chunk.leaves.iter().map(|leaf| ((chunk.batch_id, leaf.leaf_index), leaf.leaf_hash())));
        true
    }

    fn allows_da_effect(&mut self, kind: u8, payload: &[u8]) -> bool {
        let Ok(effect) = crate::processes::palw_da::parse_palw_da_effect(kind, payload) else {
            return true;
        };
        let past_relative = match &effect {
            crate::processes::palw_da::PalwDaOverlayEffect::Challenge(challenge) => {
                self.parent_obligations.contains(&challenge.obligation_id)
            }
            crate::processes::palw_da::PalwDaOverlayEffect::Response(response) => {
                self.parent_open_challenges.contains(&response.challenge_id)
            }
            crate::processes::palw_da::PalwDaOverlayEffect::Timeout(evidence) => {
                self.parent_open_challenges.contains(&evidence.challenge_id)
            }
        };
        if !past_relative {
            return true;
        }
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        let context = crate::processes::palw_da::PalwDaApplyContext {
            network_id: self.network_id,
            current_daa_score: self.daa_score,
            current_epoch: self.epoch,
            policy: &policy,
            provider_bonds: self.provider_bonds,
        };
        let mut candidate = self.state.clone();
        match crate::processes::palw_da::apply_palw_da_effect(&mut candidate, effect, &context) {
            Ok((mutation, _)) => {
                if let Some(PalwProviderBondMutation::Slash(provider_bond, _)) = mutation
                    && candidate.record_block_slash(provider_bond).is_err()
                {
                    return false;
                }
                self.state = candidate;
                true
            }
            Err(crate::processes::palw_da::PalwDaProcessError::Core(PalwDaError::Capacity)) => false,
            Err(_) => true,
        }
    }
}

impl VirtualStateProcessor {
    /// Calculates UTXO state and transaction acceptance data relative to the selected parent state
    ///
    /// kaspa-pq Phase 10/11 (ADR-0016 §D.4): `selected_parent_bond_view` is the
    /// bond set as-of this block's selected parent — the same view the overlay
    /// block-validity rules in `verify_expected_utxo_state` read. After the
    /// mergeset is applied, [`Self::apply_slashing_side_effects`] consumes it to
    /// remove each slashed bond's locked output-0 from `ctx.mergeset_diff` +
    /// `ctx.multiset_hash` (and so the `utxo_commitment`) and mint the reporter
    /// reward at `(slashing_tx_id, 0)`. Both paths into this function (block
    /// validation and virtual recompute) pass the same view + `pov_daa_score`,
    /// so the side-effect is byte-identical across construction and validation.
    /// Gated on `dns_activation_daa_score` (= 0 on every current network), so it
    /// runs from genesis today. (The gate is retained for any net that sets it > 0.)
    pub(super) fn calculate_utxo_state<V: UtxoView + Sync>(
        &self,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        selected_parent_bond_view: &ActiveBondView,
        // kaspa-pq **ADR-0040 ECON-03 (THE WIRE)**: the PALW provider-bond registry as-of this block's
        // selected parent — the point of view `palw_work_reward_class` resolves each algo-4 source's
        // leaf bonds against. Walked in lockstep with `selected_parent_bond_view` by
        // `calculate_utxo_state_relatively`; empty while PALW is fenced.
        selected_parent_provider_bond_view: &ProviderBondView,
        pov_daa_score: u64,
    ) {
        let selected_parent_transactions = self.block_transactions_store.get(ctx.selected_parent()).unwrap();
        let validated_coinbase = ValidatedTransaction::new_coinbase(&selected_parent_transactions[0]);
        // DA reward and exit decisions are selected-parent-relative, just like provider collateral.
        // Missing active state is fail-stop inside the shared loader; pre-activation returns empty.
        let selected_parent_da_state = self.palw_da_parent_state(ctx.selected_parent(), pov_daa_score).0;
        let mut palw_da_acceptance_gate = (self.palw_activation_daa_score != u64::MAX
            && pov_daa_score >= self.palw_activation_daa_score
            && !self.palw_spam.is_inert())
        .then(|| {
            let parent_lifecycle = self
                .palw_overlay_view_store
                .view(ctx.selected_parent())
                .unwrap_or_else(|store_error| {
                    panic!(
                        "PALW Header-v4 acceptance could not read selected-parent lifecycle {}: {store_error}",
                        ctx.selected_parent()
                    )
                })
                .map(|view| (*view).clone())
                .unwrap_or_default();
            PalwDaAcceptanceGate::new(
                &selected_parent_da_state,
                parent_lifecycle,
                self.palw_store.as_ref(),
                self.palw_da_buried_beacon(ctx.selected_parent(), pov_daa_score),
                selected_parent_provider_bond_view,
                self.palw_network_id,
                pov_daa_score,
                self.palw_epoch_length_daa,
                self.palw_batch_admission.max_leaf_chunk_leaves,
                self.palw_leaf_chunk_v3_admission_daa_score,
            )
        });

        ctx.mergeset_diff.add_transaction(&validated_coinbase, pov_daa_score).unwrap();
        ctx.multiset_hash.add_transaction(&validated_coinbase, pov_daa_score);
        let validated_coinbase_id = validated_coinbase.id();
        ctx.accepted_tx_ids.push(validated_coinbase_id);

        // kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): above the fence, build the
        // POST-ACCEPTANCE bond view the per-tx spend-skip is evaluated against = the selected-parent
        // bonds PLUS every bond freshly DECLARED by a StakeBond tx anywhere in this mergeset. Only
        // `Insert` mutations are applied (a fresh bond is always Pending/Active ⇒ non-releasable);
        // `Slash`/`Unbond` are deliberately omitted so an unaccepted or within-mergeset unbond can
        // never make a bond look releasable here (within-mergeset unbonds can't reach release — the
        // window is days). Including a bond declared by a StakeBond tx that turns out UTXO-invalid is
        // a harmless SAFE SUPERSET: its output-0 does not exist, so nothing can spend it. A
        // deterministic function of the shared (selected_parent_bond_view, mergeset) inputs, so it is
        // construction == validation. `None` (inert) below the fence ⇒ the legacy own-body gate is the
        // sole protection and acceptance is byte-identical to today.
        let bond_gate_view: Option<ActiveBondView> = self
            .dns_params
            .as_ref()
            // Active only at/above the mergeset fence AND at/above dns_activation (matching the legacy
            // gate's `dns_activation_daa_score` semantics). The dns_activation conjunct is defensive:
            // every sane config sets the mergeset fence ≥ dns_activation (and below dns_activation the
            // bond set is empty anyway), but pinning it makes the invariant explicit rather than implied.
            .filter(|p| {
                pov_daa_score >= p.bond_spend_gate_mergeset_activation_daa_score && pov_daa_score >= p.dns_activation_daa_score
            })
            .map(|_| {
                let mut view = selected_parent_bond_view.clone();
                let (min_bond, unbonding_floor) = self.dns_bond_floors();
                // The SAME raw block-tx set the acceptance loop below iterates (incl. each block's
                // coinbase, which is inert here: `bond_mutations_from_accepted_txs` only emits Inserts
                // for DNS-subnetwork StakeBond txs). This is a deliberately distinct, never-persisted
                // view used ONLY for the gate decision; the authoritative committed bond mutations are
                // derived from ACCEPTED txs (`dns_bond_mutations_from_acceptance`, processor.rs). The
                // two must stay superset-consistent (this raw view ⊇ the accepted-tx bond set).
                let mergeset_txs: Vec<Transaction> = once(ctx.selected_parent())
                    .chain(ctx.ghostdag_data.consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.deref()))
                    .flat_map(|b| (*self.block_transactions_store.get(b).unwrap()).clone())
                    .collect();
                let inserts: Vec<BondMutation> = bond_mutations_from_accepted_txs(&mergeset_txs, pov_daa_score, min_bond, unbonding_floor)
                    .into_iter()
                    .filter(|m| matches!(m, BondMutation::Insert(..)))
                    .collect();
                view.apply(&inserts);
                view
            });

        // kaspa-pq **ADR-0040 ECON-03 leg 5 — the provider-unbond authorization, as an acceptance-time
        // SKIP.**
        //
        // This replaces a gate in `verify_expected_utxo_state` that ran `palw_provider_unbond_
        // authorized` over the FULL MERGESET acceptance data and rejected the whole block on the first
        // unauthorized `0x37` (`RuleError::PalwProviderUnbondUnauthorized`, now removed). That was a
        // consensus denial of service: a miner does not choose the contents of the merge-blue blocks
        // it merges, so an attacker publishing one unauthorized request made every honest block that
        // merged it invalid. The resolution is the one the DNS bond spend-gate already took (see
        // `bond_gate_view` above): do not reject the block, simply do not apply the effect.
        //
        // WHY THIS COORDINATE, and not the mutation producer. Every registry derivation site reads
        // ACCEPTED transactions — `palw_provider_bond_mutations_from_acceptance` for the block being
        // validated, `palw_provider_bond_mutations_for_chain_block` for BOTH halves of a reorg, and
        // `stage_palw_provider_bond_mutations` for the persisted rows. Filtering acceptance therefore
        // filters all of them from one point, which is what keeps `ProviderBondView::apply`/`revert`
        // exact inverses and the registry single-valued. Filtering at the producer could not: the
        // revert half re-derives a chain block's mutations with no selected-parent view in hand, and
        // reconstructing one from the post-block view is not injective (an unbond stamp equal to this
        // block's DAA score is indistinguishable from one an equal-DAA ancestor wrote), so the two
        // halves could disagree and leave the view path-dependent.
        //
        // POINT OF VIEW — preserved exactly as the removed gate had it. `selected_parent_provider_
        // bond_view` is this block's SELECTED-PARENT registry, walked in lockstep by
        // `calculate_utxo_state_relatively` and never advanced by this block's own mutations. So a
        // bond created and unbonded inside one block cannot authorize itself (it does not resolve in
        // the selected-parent view ⇒ check (1) fails), and both callers of this function — block
        // validation and the virtual/template recompute (`calculate_virtual_state`) — read the
        // identical point of view, so construction == validation holds structurally.
        //
        // `None` (inert) unless the PALW fence is reached. The four ordinary shipped presets keep the
        // fence at `u64::MAX`; the explicit testnet-110/devnet-111 PALW presets activate it at DAA 0.
        // The per-block conjunct matches `palw_provider_bond_mutations_for_chain_block`'s fence
        // EXACTLY — the writer and this filter must share one gate, or a net with a finite non-zero
        // fence would write registry rows for blocks whose `0x37` transactions were never checked.
        let provider_unbond_filter = (self.palw_activation_daa_score != u64::MAX && pov_daa_score >= self.palw_activation_daa_score)
            .then_some(ProviderUnbondAuthFilter {
                provider_bond_view: selected_parent_provider_bond_view,
                da_state: &selected_parent_da_state,
                network_id: self.palw_network_id,
                daa_score: pov_daa_score,
            });

        // kaspa-pq **ADR-0040 ECON-03 leg 4 — the provider-bond SPEND gate's POST-ACCEPTANCE view.**
        //
        // Mirrors `bond_gate_view` above, for the provider-bond registry. Above the PALW fence, build
        // the view the per-tx spend-skip is evaluated against = the selected-parent provider bonds PLUS
        // every provider bond freshly DECLARED by a `0x30` ProviderBond tx anywhere in this mergeset.
        // Only `Insert` mutations are applied (a fresh bond is always Pending/Active ⇒ non-releasable —
        // release needs an Unbond stamp plus a whole clamped delay in EPOCHS, unreachable within one
        // mergeset); `Unbond`/`Slash` are deliberately omitted, so applying a raw-tx unbond can never
        // falsely make a bond look releasable here. Insert-only is a safe SUPERSET that locks MORE,
        // never less (same reasoning as the DNS `bond_gate_view` comment above). A deterministic
        // function of the shared (`selected_parent_provider_bond_view`, mergeset) inputs, so it is
        // construction == validation. `None` (inert) below the fence ⇒ acceptance is byte-identical.
        //
        // The fence conjunct is the SAME one leg-5's `provider_unbond_filter` (above) and the registry
        // writer `palw_provider_bond_mutations_for_chain_block` (processor.rs) use — the gate, the
        // authorizer, and the writer must share one gate, or a net with a finite non-zero fence would
        // lock outputs for blocks whose provider bonds were never written to the registry.
        let provider_bond_gate_view: Option<ProviderBondView> =
            (self.palw_activation_daa_score != u64::MAX && pov_daa_score >= self.palw_activation_daa_score).then(|| {
                let mut view = selected_parent_provider_bond_view.clone();
                let (min_bond, unbond_floor) = self.palw_provider_bond_floors();
                let mergeset_txs: Vec<Transaction> = once(ctx.selected_parent())
                    .chain(ctx.ghostdag_data.consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.deref()))
                    .flat_map(|b| (*self.block_transactions_store.get(b).unwrap()).clone())
                    .collect();
                let inserts: Vec<PalwProviderBondMutation> =
                    palw_provider_bond_mutations_from_accepted_txs(&mergeset_txs, pov_daa_score, min_bond, unbond_floor)
                        .into_iter()
                        .filter(|m| matches!(m, PalwProviderBondMutation::Insert(..)))
                        .collect();
                view.apply(&inserts);
                view
            });

        // K5 (ADR-0039 §11.3): derive the MERGING block's beacon state ONCE, for the ReplicaPalwHalted
        // reward gate below. Guards mirror `derive_palw_beacon_state_value` EXACTLY (inert fence +
        // genesis-SP) so `derive_palw_beacon_state_core`'s "missing fork-local accumulator" panic can
        // never fire on an edge block; `None` while inert ⇒ the reward classification is byte-identical.
        let pov_beacon = (self.palw_activation_daa_score != u64::MAX
            && pov_daa_score >= self.palw_activation_daa_score
            && ctx.selected_parent() != self.genesis.hash)
            .then(|| {
                self.derive_palw_beacon_state_core(
                    pov_daa_score,
                    ctx.selected_parent(),
                    ctx.selected_parent(),
                    selected_parent_bond_view,
                )
            })
            .flatten();

        // ADR-0040 §5.15.13 paid-work set.
        //
        // Resolved once per block from the selected parent's chain, never from this block's own
        // (not yet written) row, mirroring the DNS `already_rewarded` prefix set exactly. `paid` is
        // the cross-block half; the loop below adds the within-mergeset half, which is what makes the
        // rule total: without it, two algo-4 sources sharing a nullifier could be merged by the same
        // block and both be paid, since neither is in the other's chain prefix.
        let mut palw_paid_work = self.palw_paid_work_window(ctx.selected_parent(), pov_daa_score);

        for (i, (merged_block, txs)) in once((ctx.selected_parent(), selected_parent_transactions))
            .chain(
                ctx.ghostdag_data
                    .consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.deref())
                    .map(|b| (b, self.block_transactions_store.get(b).unwrap())),
            )
            .enumerate()
        {
            // Create a composed UTXO view from the selected parent UTXO view + the mergeset UTXO diff
            let composed_view = selected_parent_utxo_view.compose(&ctx.mergeset_diff);

            // The first block in the mergeset is always the selected parent
            let is_selected_parent = i == 0;

            // No need to fully validate selected parent transactions since selected parent txs were already validated
            // as part of selected parent UTXO state verification with the exact same UTXO context.
            let validation_flags = if is_selected_parent { TxValidationFlags::SkipScriptChecks } else { TxValidationFlags::Full };
            // kaspa-pq bond spend-gate (mergeset hardening): gate every accepted mergeset tx (incl.
            // the selected parent's body, also accepted here) against the post-acceptance view, so a
            // merge-blue spend of a non-releasable bond's output-0 is skipped. `None` (inert) below
            // the fence. The spend-skip is independent of `SkipScriptChecks`.
            let bond_filter = bond_gate_view.as_ref().map(|view| BondSpendFilter { bond_view: view, daa_score: pov_daa_score });
            // kaspa-pq ADR-0040 ECON-03 leg 4: gate every accepted mergeset tx (incl. the selected
            // parent's body, also accepted here) against the post-acceptance provider-bond view, so a
            // merge-blue spend of a non-releasable provider bond's output-0 is skipped. `None` (inert)
            // below the fence. Independent of `SkipScriptChecks`.
            let provider_bond_filter = provider_bond_gate_view.as_ref().map(|view| ProviderBondSpendFilter {
                provider_bond_view: view,
                da_state: &selected_parent_da_state,
                epoch_length_daa: self.palw_epoch_length_daa,
                daa_score: pov_daa_score,
            });
            let (mut validated_transactions, mut inner_multiset) = self.validate_transactions_with_muhash_in_parallel(
                &txs,
                &composed_view,
                pov_daa_score,
                validation_flags,
                bond_filter,
                provider_unbond_filter,
                provider_bond_filter,
            );

            if let Some(gate) = palw_da_acceptance_gate.as_mut() {
                let mut filtered = SmallVec::new();
                let mut filtered_multiset = MuHash::new();
                for (validated_tx, tx_idx) in validated_transactions {
                    if gate.allows(validated_tx.tx) {
                        filtered_multiset.combine(&MuHash::from_transaction(&validated_tx, pov_daa_score));
                        filtered.push((validated_tx, tx_idx));
                    }
                }
                validated_transactions = filtered;
                inner_multiset = filtered_multiset;
            }

            ctx.multiset_hash.combine(&inner_multiset);

            // kaspa-pq ADR-0018 §F bridge wiring: classify each accepted tx's fee. A tx creating
            // ≥1 EVM_DEPOSIT_LOCK output (recognised by the SAME `parse_evm_deposit_lock` the
            // claim path uses at processes/evm — so the lock-shape definition can never diverge)
            // is a bridge tx: its whole fee is finality-class, split at the validator-primary §F
            // finality ratios instead of the 90/10 normal ratios. DOUBLY gated at THIS
            // accumulation site — shared by coinbase construction and validation (both call
            // `calculate_utxo_state`), so c==v holds structurally:
            //   1. `finality_fee_activation_daa_score` — the §F wiring fence;
            //   2. `evm_activation_daa_score` — lock OUTPUTS are consensus-legal on every net
            //      (the output-class exemption is unconditional), but the BRIDGE only exists on
            //      an EVM-active net; without this gate a miner on an EVM-inert net (mainnet
            //      today) could self-include a never-claimable lock tx and reroute its fee
            //      75/25 to the §E pool. Both scores are consensus-fixed per net and
            //      `pov_daa_score` is path-identical, so the conjunction preserves c==v.
            // Below either fence `finality_fee` stays 0 ⇒ byte-identical splits.
            let finality_fee_active = pov_daa_score >= self.evm_activation_daa_score
                && self.dns_params.as_ref().is_some_and(|p| pov_daa_score >= p.finality_fee_activation_daa_score);
            let mut block_fee = 0u64;
            let mut finality_fee = 0u64;
            for (validated_tx, _) in validated_transactions.iter() {
                ctx.mergeset_diff.add_transaction(validated_tx, pov_daa_score).unwrap();
                ctx.accepted_tx_ids.push(validated_tx.id());
                block_fee += validated_tx.calculated_fee;
                if finality_fee_active
                    && validated_tx.tx.outputs.iter().any(|o| parse_evm_deposit_lock(&o.script_public_key).is_some())
                {
                    finality_fee += validated_tx.calculated_fee;
                }
            }

            ctx.mergeset_acceptance_data.push(MergesetBlockAcceptanceData {
                block_hash: merged_block,
                // For the selected parent, we prepend the coinbase tx
                accepted_transactions: is_selected_parent
                    .then_some(AcceptedTxEntry { transaction_id: validated_coinbase_id, index_within_block: 0 })
                    .into_iter()
                    .chain(
                        validated_transactions
                            .into_iter()
                            .map(|(tx, tx_idx)| AcceptedTxEntry { transaction_id: tx.id(), index_within_block: tx_idx }),
                    )
                    .collect(),
            });

            let coinbase_data = self.coinbase_manager.deserialize_coinbase_payload(&txs[0].payload).unwrap();
            // ADR-0039 §17.2 derivation seam (single source of truth for construction == validation):
            // classify the source block's work lane. `palw_work_reward_class` returns `HashMiner` for an
            // algo-3 hash-floor source (pre-PALW behavior, and the ONLY outcome while the lane is inert —
            // byte-identical) and `ReplicaPalw{provider scripts…}` for an algo-4 replica source. Placed
            // here so both the construction and validation callers of `expected_coinbase_transaction` see
            // the same class from one derivation.
            let work_reward_class = self.palw_work_reward_class(
                merged_block,
                pov_beacon.as_ref(),
                &mut palw_paid_work,
                selected_parent_provider_bond_view,
                &selected_parent_da_state,
                pov_daa_score,
            );
            // ADR-0040 §5.15.13 (G16): record what this block PAYS, at the single seam that decided it.
            // Recording here rather than at the coinbase builder is what makes construction ==
            // validation structural: both callers of `expected_coinbase_transaction` reach the reward
            // class through this one derivation, so the persisted row and the paid outputs cannot drift.
            if let WorkRewardClass::ReplicaPalw { batch_id, leaf_index, .. } = &work_reward_class {
                use crate::model::stores::palw::PalwStoreReader;
                let leaf = self.palw_store.leaf(*batch_id, *leaf_index).expect("classified ReplicaPalw ⇒ leaf present");
                ctx.palw_paid_work_ids.push(leaf.job_nullifier);
            }
            ctx.mergeset_rewards.insert(
                merged_block,
                BlockRewardData::new(
                    coinbase_data.subsidy,
                    block_fee,
                    finality_fee,
                    coinbase_data.miner_data.script_public_key,
                    work_reward_class,
                ),
            );
        }

        // kaspa-pq Phase 11 (ADR-0013 Addendum C / ADR-0016 §D.4): apply the
        // slashing side-effect over the fully-applied mergeset. Gated on
        // `dns_activation_daa_score` (= 0 on every current network), so it runs
        // from genesis everywhere (the 4-way reserve/victim split is the part
        // fenced behind `pos_v2_activation_daa_score` — see below).
        self.apply_slashing_side_effects(ctx, selected_parent_utxo_view, selected_parent_bond_view, pov_daa_score);
    }

    /// ADR-0039 §17.2: classify a mergeset source block's work lane for its coinbase reward. Called from
    /// [`Self::calculate_utxo_state`] — the SINGLE seam shared by coinbase construction and validation,
    /// so the class can never drift (c == v). An algo-3 hash-floor source is `HashMiner` (the single
    /// miner script is paid — the pre-PALW behavior); an algo-4 PALW replica source is `ReplicaPalw`,
    /// carrying the two provider reward scripts read from the leaf its ticket references
    /// (`palw_batch_id`/`palw_leaf_index`). The unique-blue vs red/duplicate payout decision (§17.4) is
    /// made downstream by `expected_coinbase_transaction`; this records only WHICH lane and the scripts.
    ///
    /// K5 (ADR-0039 §11.3): a `pov_beacon` (the merging block's derived beacon state) whose reconstructed
    /// mode is `Halted` for the SOURCE's minting epoch classifies it as `ReplicaPalwHalted` (paid
    /// nothing) — compute minted under an untrusted beacon earns no reward, mirroring the §17.4
    /// red/duplicate burn-by-don't-mint. A source minted Healthy (or in grace) and merged during Halted
    /// stays fully paid (the classification is keyed on the source's OWN epoch, via `halted_since`).
    ///
    /// The fast path returns `HashMiner` while PALW is gated (`palw_activation_daa_score == u64::MAX`),
    /// avoiding store reads on mainnet, testnet-10, simnet and devnet. The three PALW presets use an
    /// activation score of 0 and evaluate PALW reward classes.
    /// kaspa-pq **ADR-0040 §5.15.13 — gate G16 (P1-9-RELAND), the rule itself.**
    ///
    /// `paid_work` is the mutable paid-`job_nullifier` set for the block being built/validated: it
    /// enters seeded with the bounded selected-chain prefix ([`Self::palw_paid_work_window`]) and is
    /// extended, in canonical mergeset order, by each source this call decides to PAY. A source whose
    /// leaf's `job_nullifier` is already in it is classified
    /// [`WorkRewardClass::ReplicaPalwDuplicateWork`] and paid nothing.
    ///
    /// Identifier binding. `job_nullifier` is included in `leaf_hash`; the membership proof binds
    /// `leaf_hash` to `manifest.leaf_root`, and `leaf_root` is included in `content_id() == batch_id`.
    /// A stored leaf's `job_nullifier` is therefore immutable and batch-bound.
    ///
    /// Reward semantics. A duplicate does not invalidate the block or remove its lane weight and
    /// difficulty contribution; only the payout is withheld. The registry remains outside body
    /// validation because that coordinate cannot authorize a claim and rejection there could invalidate
    /// every block that references the batch.
    ///
    /// Claims are not authorized by an ML-DSA signature.
    /// First-in-canonical-order wins instead, which is well-defined because the order is
    /// consensus-fixed.
    ///
    /// Provider bonds are persisted at prefix 241 by `stage_palw_provider_bond_mutations`, and a
    /// `ProviderBondView` is composed here, so a claim
    /// could in principle be bound to a bond's owner key. The remaining two reasons still hold:
    /// `ReplicaExecutionReceiptV1::signature` is a wire field with no consensus decoder, and a leaf
    /// names bond OUTPOINTS rather than the signing identity that would have to authorise the claim.
    /// Adding claim authorisation is therefore now a possible slice rather than an impossible one — but
    /// it is not this one, and nothing in the tree does it.
    ///
    /// Scope: the bounded walk rejects duplicate claims
    /// while the batches involved are concurrently live. It does NOT close the same `job_nullifier`
    /// being re-registered into a fresh batch an arbitrary time later; see
    /// `PalwBatchAdmissionParams::max_batch_life_epochs` for why closing that needs either unbounded
    /// state or a leaf-format freshness binding, and hence a different slice.
    fn palw_work_reward_class(
        &self,
        merged_block: BlockHash,
        pov_beacon: Option<&kaspa_consensus_core::palw::PalwBeaconStateV1>,
        paid_work: &mut std::collections::HashSet<kaspa_hashes::Hash64>,
        provider_bond_view: &ProviderBondView,
        da_state: &PalwDaStateV1,
        pov_daa_score: u64,
    ) -> WorkRewardClass {
        use crate::model::stores::palw::PalwStoreReader;
        use kaspa_consensus_core::constants::PALW_HEADER_VERSION;
        use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA;
        if self.palw_activation_daa_score == u64::MAX {
            return WorkRewardClass::HashMiner; // inert fast path — no algo-4 source can exist on a live net
        }
        let header = self.headers_store.get_header(merged_block).unwrap();
        if header.version < PALW_HEADER_VERSION || header.pow_algo_id != POW_ALGO_ID_PALW_REPLICA {
            return WorkRewardClass::HashMiner;
        }
        // K5: an algo-4 source minted in a HALTED epoch (reconstructed from the merging block's beacon
        // state via the closed-form `halted_since` run arithmetic) is paid NOTHING — a trailing class with
        // no scripts, handled downstream like the §17.4 red/dup burn. Keyed on the SOURCE's own epoch, so
        // honest Healthy/grace work merged later stays paid. Early-return BEFORE the leaf read (no store
        // read, no scripts needed). Bounded residual (documented): a source from an OLDER halted run,
        // merged after a Healthy recovery within the merge-depth window, is outside this state's run and
        // gets paid — closed at activation by a per-epoch mode index along the selected chain.
        let source_epoch = header.daa_score / self.palw_epoch_length_daa.max(1);
        if pov_beacon
            .and_then(|s| s.halted_since(self.palw_beacon_grace_epochs))
            .is_some_and(|halted_since| source_epoch >= halted_since)
        {
            return WorkRewardClass::ReplicaPalwHalted { batch_id: header.palw_batch_id, leaf_index: header.palw_leaf_index };
        }
        // The leaf the algo-4 ticket references. Its presence was already proven by this block's own
        // body-stage clause check (`check_palw_ticket` → `resolve_palw_binding`), so a miss here is a
        // consensus-state invariant break (fail-closed), NOT a soft fallback to HashMiner — silently
        // downgrading would reroute the provider pair's 77% worker base to a single miner script.
        //
        // **ADR-0040 P1-2 — why re-reading the store here is now safe.**
        //
        // LEAF-01 was precisely that this read was of MUTABLE state: body validation proved a leaf's
        // presence, then reward time re-read whatever was at that key, so overwriting the leaf after
        // acceptance re-routed the 77 % worker base. The audit's remedy was "freeze the leaf hash and
        // reward scripts as an immutable snapshot at accepted-block time".
        //
        // P1-1 achieves that more cheaply than a snapshot: `DbPalwStore::insert_leaf` is now
        // content-addressed and WRITE-ONCE (identical content idempotent, different content refused), so
        // the bytes at `(batch_id, leaf_index)` cannot change after acceptance. A snapshot would copy
        // data that is already immutable. The invariant is enforced where the mutation would happen
        // rather than re-checked where it would be observed — the same "make the bad state
        // unrepresentable" move as `NoShowPenaltyDestination` having no `ToRequester` variant.
        //
        // The block is additionally bound to the leaf's CONTENT, not merely its key: clause 9's
        // eligibility draw hashes `leaf_hash`, so a different leaf at the same key would not satisfy the
        // draw this block already passed.
        let leaf = self.palw_store.leaf(header.palw_batch_id, header.palw_leaf_index).unwrap_or_else(|err| {
            panic!(
                "missing PALW leaf {}/{} for accepted algo-4 source {merged_block}: {err}",
                header.palw_batch_id, header.palw_leaf_index
            )
        });
        // kaspa-pq **ADR-0040 §16′** — resolve the replica premium `π` for THIS leaf.
        //
        // The premium is frozen at the leaf's COMMIT window, not at payout. That ordering is the whole
        // anti-grinding property: whoever later merges the algo-4 source cannot re-aim the split by
        // choosing when to merge it, and a producer cannot wait for a favourable π before revealing.
        //
        // v1 leaves have no `a_commit` field (ADR-0040 §4.2 adds it in LeafV2), so the commit window is
        // approximated by the leaf's own `registered_epoch` — which is deterministic, already committed,
        // and strictly in the leaf's past. It is an approximation ONLY in that registration and commit
        // may fall in different windows for a long-lived batch; LeafV2 makes it exact.
        // kaspa-pq **ADR-0040 ECON-03 (THE WIRE) — the collateral-resolution rule.**
        //
        // Before this existed, `leaf.provider_a_bond` / `provider_b_bond` were two outpoints that
        // consensus checked only for being different from each other; nothing required either to name
        // anything that existed. The 77 % `PALW_PROVIDER_BASE_BPS` worker base was therefore paid
        // against ZERO resolved collateral — the sentence ECON-03 was opened over. Here both outpoints
        // must resolve, at THIS block's point of view, to registry records that
        // `effective_provider_bond_status` calls `Active`. Anything else — unknown, sub-floor (so never
        // admitted by `palw_provider_bond_mutations_from_accepted_txs`), still `Pending`, already
        // `Unbonding`, or `Slashed` — yields `ReplicaPalwUnbackedCollateral` and pays NOTHING: no
        // provider outputs, no fee-worker output, no inclusion-pool add, zero validator pool, exactly
        // like the §17.4 red/duplicate burn-by-don't-mint.
        //
        // **Ordered BEFORE the G16 duplicate-work claim, deliberately.** If an unbacked leaf were
        // allowed to `paid_work.insert` its `job_nullifier` first, an attacker could poison a job id
        // with a zero-cost leaf naming two bonds that do not exist, and the genuinely bonded source of
        // the same job would then classify as a duplicate and be paid nothing. Resolution first means
        // an unbacked leaf consumes no claim.
        //
        // Order-independent: a pure function of `(leaf, provider_bond_view, pov_daa_score)`, all three
        // of which are fixed for the block before the mergeset loop starts. It mutates nothing, so the
        // classification of any one source does not depend on which sources were classified before it.
        //
        // Reward-only, like its two sibling zero-pay classes: the block stays valid and keeps its lane
        // weight. Making it a validity rule would let a third party brick an already-mined block by
        // timing an unbond, and would push a point-of-view read into body validation, which BIND-03
        // settled against.
        let (Some(bond_a), Some(bond_b)) = (
            provider_bond_view.active_provider_bond_at(&leaf.provider_a_bond, pov_daa_score),
            provider_bond_view.active_provider_bond_at(&leaf.provider_b_bond, pov_daa_score),
        ) else {
            return WorkRewardClass::ReplicaPalwUnbackedCollateral {
                batch_id: header.palw_batch_id,
                leaf_index: header.palw_leaf_index,
                provider_a_bond: leaf.provider_a_bond,
                provider_b_bond: leaf.provider_b_bond,
            };
        };
        // DA-01 reward gate: unresolved live obligations and any challenged/timed-out obligation
        // withhold the entire replica payout. This runs before the paid-work nullifier insertion, so
        // an unavailable leaf cannot poison a later honest claim for the same work.
        if !palw_da_reward_pair_allowed(da_state, &leaf.provider_a_bond, &leaf.provider_b_bond, pov_daa_score) {
            return WorkRewardClass::ReplicaPalwUnbackedCollateral {
                batch_id: header.palw_batch_id,
                leaf_index: header.palw_leaf_index,
                provider_a_bond: leaf.provider_a_bond,
                provider_b_bond: leaf.provider_b_bond,
            };
        }
        // kaspa-pq **ADR-0040 CRITICAL-1 — a leaf must prove it CONTROLS the bonds it names.**
        //
        // Resolving both outpoints to `Active` records (above) is NOT enough: `leaf.provider_a_bond` /
        // `provider_b_bond` are bare outpoints, so nothing above stops a leaf author from naming a
        // STRANGER's real, active bonds. Without this check that author would be paid the 77 % base
        // against collateral that is not theirs — and that they cannot be slashed for, since the slashable
        // party is the bond owner, not the leaf author.
        //
        // Option A (ADR-0040 CRITICAL-1): bind the payee to the owner. The leaf ALREADY commits
        // `provider_{a,b}_reward_script` (each constrained at admission to the exact 69-byte
        // `p2pkh_mldsa87` template — `palw_reward_script_is_coinbase_representable`), and the bond record
        // ALREADY commits `owner_public_key`. Requiring
        // `leaf.provider_a_reward_script == provider_bond_lock_spk(&bond_a.owner_public_key)` (and B)
        // makes payee ≡ bond owner ≡ slashable party the SAME identity: to be paid against bond X you must
        // pay X's owner, so naming a stranger's bond pays the stranger and steals nothing. It is proved
        // from fields both sides already commit — NO leaf field is added, so LEAF_LEN / LEAF_FNV / the
        // layout pin / LATEST_DB_VERSION do not move.
        //
        // Same coordinate, same order-independence proof as the resolution above: a pure function of
        // `(leaf, provider_bond_view, pov_daa_score)`, and `provider_bond_lock_spk` is pure. Reuses the
        // existing zero-pay class — a leaf that does not pay its bonds' owners is treated exactly like one
        // whose bonds do not resolve: `ReplicaPalwUnbackedCollateral`, paid NOTHING, block still valid.
        if leaf.provider_a_reward_script != provider_bond_lock_spk(&bond_a.owner_public_key)
            || leaf.provider_b_reward_script != provider_bond_lock_spk(&bond_b.owner_public_key)
        {
            return WorkRewardClass::ReplicaPalwUnbackedCollateral {
                batch_id: header.palw_batch_id,
                leaf_index: header.palw_leaf_index,
                provider_a_bond: leaf.provider_a_bond,
                provider_b_bond: leaf.provider_b_bond,
            };
        }
        // kaspa-pq ADR-0040 §5.15.13 (G16) — the duplicate-work decision, made AFTER the leaf read
        // (the nullifier lives in the leaf) and BEFORE the premium/scripts are assembled (a duplicate
        // is paid nothing, so neither is needed). `insert` returns false iff the nullifier was already
        // present, so the claim and the record are the same atomic step and cannot drift apart.
        if !paid_work.insert(leaf.job_nullifier) {
            return WorkRewardClass::ReplicaPalwDuplicateWork {
                batch_id: header.palw_batch_id,
                leaf_index: header.palw_leaf_index,
                job_nullifier: leaf.job_nullifier,
            };
        }
        let premium_pi_bps = self.palw_premium_at_window(leaf.registered_epoch);
        WorkRewardClass::ReplicaPalw {
            batch_id: header.palw_batch_id,
            leaf_index: header.palw_leaf_index,
            provider_a_script: leaf.provider_a_reward_script.clone(),
            provider_b_script: leaf.provider_b_reward_script.clone(),
            premium_pi_bps,
        }
    }

    /// kaspa-pq ADR-0040 §16′ — the replica premium in effect for a leaf committed in `epoch`.
    ///
    /// **Currently pinned at the neutral point.** The controller (`palw_premium`) is implemented and
    /// tested, but its per-window state is not yet persisted or driven from finalized cohort samples —
    /// that needs the DA/receipt accounting from P2-6/P2-7 to produce `PalwWindowSample`. Returning
    /// neutral here makes the split **byte-identical to the previous fixed 50/50**, so this slice is
    /// inert by construction until the sampler lands.
    ///
    /// # This is the ONE seam, and the governed range is enforced here
    ///
    /// π reaches the coinbase inside `WorkRewardClass::ReplicaPalw`, and construction and validation
    /// both read it from that carried value rather than looking it up again — which is what keeps the
    /// two sides from disagreeing. That property survives only while this stays the single place π is
    /// derived, so the range check belongs here too: clamping at the seam is symmetric by construction,
    /// whereas a check applied at either the construction or the validation call site would be a second
    /// derivation and could disagree with the other.
    ///
    /// The clamp is dead code today (the constant is the neutral point, and
    /// `premium_range_contains_the_neutral_point` pins that it is in range). It matters the moment the
    /// controller becomes live: an out-of-range π would otherwise reach `premium_split` and produce a
    /// split neither governance bound vetted — at π = 0, σ_A = 1 and the replica is paid nothing.
    ///
    /// Arithmetic safety is separate and already holds unconditionally: `denom = 10_000 + m·π ≥ 10_000`
    /// so there is no division by zero, and `base·10_000 < 2^128` for any `u64` base, so the `u128`
    /// intermediate cannot overflow.
    fn palw_premium_at_window(&self, _epoch: u64) -> u32 {
        use kaspa_consensus_core::palw_premium::{PALW_PREMIUM_BPS_ONE, PALW_PREMIUM_PI_MAX_BPS, PALW_PREMIUM_PI_MIN_BPS};
        PALW_PREMIUM_BPS_ONE.clamp(PALW_PREMIUM_PI_MIN_BPS, PALW_PREMIUM_PI_MAX_BPS)
    }

    /// kaspa-pq Phase 11 (ADR-0013 Addendum C / ADR-0016 §D.4): the atomic
    /// consensus side-effect of slashing. For each genuine equivocation evidence
    /// accepted in this block's mergeset whose bond still holds a locked output-0
    /// (resolved `Active`/`Unbonding` against the selected-parent bond view),
    /// remove that output-0 UTXO (`S` leaves the supply) and mint the reporter
    /// reward `R` at `(slashing_tx_id, 0)` — the slashing tx declares no outputs
    /// (isolation rule), so index 0 is always free. Net supply change is `R − S`;
    /// the remainder `S − R` is implicitly burned. Both add/remove are mirrored
    /// into `ctx.multiset_hash`, so the `utxo_commitment` reflects the side-effect.
    ///
    /// Resolution runs over the mergeset's *accepted* txs (the same set the
    /// acceptance data records) using the block's selected-parent bond view, so
    /// block validation and virtual recompute — which call
    /// [`Self::calculate_utxo_state`] with identical inputs — produce byte-for-
    /// byte identical side-effects, keeping construction == validation and the
    /// operation reorg-safe.
    ///
    /// Activation gating lives here; the resolved effects are applied by
    /// [`apply_slashing_effects_to_state`], whose per-effect `composed.get`
    /// lookup yields the exact stored UTXO entry (so its `block_daa_score`
    /// matches the multiset element being removed) and doubles as a release-race
    /// guard. Gated on `dns_activation_daa_score` (= 0 on every current
    /// network), so this runs from genesis today.
    fn apply_slashing_side_effects<V: UtxoView>(
        &self,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        selected_parent_bond_view: &ActiveBondView,
        pov_daa_score: u64,
    ) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return;
        };
        if pov_daa_score < dns_params.dns_activation_daa_score {
            return;
        }
        let accepted_txs = self.accepted_txs_from_acceptance_data(&ctx.mergeset_acceptance_data);
        // ADR-0018 "本格版" (PoS-v2) §slashing: the reserve + victim shares are fenced — `0` below
        // `pos_v2_activation_daa_score`, so `compute_slashing_distribution` degenerates to the pre-v2
        // 2-way (reporter + burn) on the devnet/simnet preset (fence = `u64::MAX`). On mainnet/testnet
        // (`PRODUCTION_DNS_PARAMS`, fence = 0) the full 4-way split runs from block 1.
        let (security_reserve_bps, victim_epoch_pool_bps) = if pov_daa_score >= dns_params.pos_v2_activation_daa_score {
            (dns_params.reward_params.security_reserve_bps, dns_params.reward_params.victim_epoch_pool_bps)
        } else {
            (0, 0)
        };
        let mut effects = resolve_slashing_side_effects(
            &accepted_txs,
            selected_parent_bond_view,
            pov_daa_score,
            dns_params.reward_params.slashing_reporter_reward_bps,
            security_reserve_bps,
            victim_epoch_pool_bps,
        );
        // ADR-0018 "本格版" (PoS-v2) victim compensation: for each slashed bond with a victim pool,
        // recompute the slashed validator's epoch's honest (non-slashed) included set from the
        // selected-parent window and build the victim outputs. Inert when fenced (pool = 0 ⇒ skip) —
        // i.e. on the devnet/simnet preset; on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0) the
        // victim pool is non-zero and these outputs are built from block 1. The recompute reads the same
        // selected-parent view in both the block-validation and virtual-recompute paths ⇒ construction == validation.
        for effect in effects.iter_mut() {
            if effect.victim_epoch_pool_sompi == 0 {
                continue;
            }
            let slashed_payload = selected_parent_bond_view.get(&effect.bond_outpoint).map(|b| b.owner_reward_spk_payload);
            effect.victim_outputs = self.slashed_epoch_victim_outputs(
                dns_params,
                ctx.selected_parent(),
                selected_parent_bond_view,
                effect.slashed_epoch,
                slashed_payload,
                effect.victim_epoch_pool_sompi,
            );
        }
        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): this block's security-reserve accrual = Σ of
        // the slashed bonds' reserve shares (0 when fenced). Feeds the reserve-balance recurrence
        // (`commit_utxo_state` persists `parent_balance + reserve_accrual − drip`). The reserve share
        // is NOT minted (it leaves the supply with the bond removal until it later drips back out).
        ctx.reserve_accrual = effects.iter().fold(0u64, |acc, e| acc.saturating_add(e.security_reserve_sompi));
        apply_slashing_effects_to_state(
            &effects,
            selected_parent_utxo_view,
            &mut ctx.mergeset_diff,
            &mut ctx.multiset_hash,
            pov_daa_score,
        );
    }

    /// kaspa-pq ADR-0022: the selected-parent window's per-block epoch contributions (oldest →
    /// newest by DAA), drawn from [`Self::selected_chain_overlay_window`] so a pruned-IBD node's
    /// below-pruning-point history is supplied by the imported pruning-point snapshot (the raw
    /// selected-chain walk cannot traverse below the pruning point). The single seam every coinbase
    /// reward recompute (victim compensation / reserve drip / deferred quality bonus) reads, so they
    /// match a from-genesis node's accumulator after a prune. Byte-equivalent to the former raw chain
    /// walk wherever the walk never reaches the pruning point — `selected_chain_overlay_window` skips
    /// empty-contribution blocks, which are tally-neutral in `recompute_epoch_tallies` — so this is a
    /// no-op on a non-pruned node (the only path on every net while the pruning point is genesis).
    fn selected_chain_epoch_contributions(
        &self,
        selected_parent: BlockHash,
        parent_daa: u64,
        walk_bound: u64,
    ) -> Vec<BlockEpochContribution> {
        let mut v: Vec<BlockEpochContribution> = self
            .selected_chain_overlay_window(selected_parent, parent_daa, walk_bound)
            .into_iter()
            .map(|c| BlockEpochContribution {
                block_daa_score: c.block_daa_score,
                rewarded_keys: c.rewarded_keys,
                quality_subpool: c.quality_subpool,
            })
            .collect();
        v.sort_by_key(|c| c.block_daa_score);
        v
    }

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2) §slashing — the victim-compensation outputs for one
    /// slashed bond: the `victim_pool` distributed stake-proportionally among the **honest**
    /// validators of the slashed validator's epoch. Recomputes `slashed_epoch`'s accumulator
    /// `included` set from the selected-parent window (the same bounded walk + pure
    /// `recompute_epoch_tallies` Phase 1/2 use), drops the slashed validator (matched by its
    /// `owner_reward_spk_payload`), and pays the rest via [`victim_compensation_outputs`]. Resolves
    /// bonds against `bond_view` (as-of the selected parent) so the block-validation and
    /// virtual-recompute paths build byte-identical outputs (construction == validation, reorg-safe
    /// — a finalized/buried epoch's blocks are immutable). Empty while the v2 fence is closed (no
    /// accumulator rows in the window) — i.e. on the devnet/simnet preset; on mainnet/testnet
    /// (`PRODUCTION_DNS_PARAMS`, fence = 0) it runs from block 1.
    fn slashed_epoch_victim_outputs(
        &self,
        dns_params: &DnsParams,
        selected_parent: BlockHash,
        bond_view: &ActiveBondView,
        slashed_epoch: u64,
        slashed_payload: Option<[u8; 64]>,
        victim_pool: u64,
    ) -> Vec<TransactionOutput> {
        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        let walk_bound = finalization_depth.saturating_add(epoch_len.saturating_mul(2));
        let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap();

        // Gather the selected-parent window's per-block contributions (sink-parent first, then
        // ancestors), stopping at the window edge or the v2 fence.
        let contributions = self.selected_chain_epoch_contributions(selected_parent, parent_daa, walk_bound);

        let bonds = bond_view.records();
        let tallies = recompute_epoch_tallies(parent_daa, epoch_len, finalization_depth, &contributions, &bonds);
        let included =
            tallies.into_iter().find(|(epoch, _)| *epoch == slashed_epoch).map(|(_, tally)| tally.included).unwrap_or_default();
        // Drop the slashed (equivocating) validator — it earns no victim compensation in its own
        // misbehavior epoch. Matched by the owner reward payload carried in the accumulator.
        let honest: Vec<_> =
            included.into_iter().filter(|(payload, _)| slashed_payload.is_none_or(|sp| payload.as_bytes() != sp)).collect();
        let total_honest_stake: u128 = honest.iter().map(|(_, stake)| *stake as u128).sum();
        victim_compensation_outputs(victim_pool, &honest, total_honest_stake)
    }

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4) §reserve drip — the security-reserve **drip**
    /// coinbase outputs THIS block emits for the epoch(s) it finalizes, plus the total dripped (for
    /// the reserve-balance recurrence). For each epoch the block finalizes (the same crossing as the
    /// quality bonus), drips `min(remaining_balance, reserve_drip_per_epoch_cap_sompi)` distributed
    /// stake-proportionally to that epoch's included validators (reusing the bonus distributor). The
    /// reserve decreases by exactly the **minted** amount (≤ budget), so it is value-conserving and
    /// the unspent tail rolls over. `parent_balance` is the selected parent's committed cumulative
    /// reserve balance (read by the caller from `reserve_balance_store`), so construction (template)
    /// and validation read the identical as-of-selected-parent balance ⇒ byte-identical, reorg-safe.
    /// Returns no outputs below the v2 fence / when the parent balance or the per-epoch cap is 0 /
    /// when no epoch crosses — so it is inert on the devnet/simnet preset (fence = `u64::MAX`); on
    /// mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0) the drip runs from block 1.
    pub(super) fn reserve_drip_outputs(
        &self,
        dns_params: &DnsParams,
        daa_score: u64,
        selected_parent: BlockHash,
        bond_view: &ActiveBondView,
        parent_balance: u64,
    ) -> (Vec<TransactionOutput>, u64) {
        let cap = dns_params.reward_params.reserve_drip_per_epoch_cap_sompi;
        if daa_score < dns_params.pos_v2_activation_daa_score || parent_balance == 0 || cap == 0 {
            return (Vec::new(), 0);
        }
        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap();
        let Some((e_min, e_max)) = epochs_finalized_at(parent_daa, daa_score, epoch_len, finalization_depth) else {
            return (Vec::new(), 0);
        };

        let walk_bound = finalization_depth.saturating_add(epoch_len.saturating_mul(2));
        let contributions = self.selected_chain_epoch_contributions(selected_parent, parent_daa, walk_bound);
        let bonds = bond_view.records();
        let tallies = recompute_epoch_tallies(daa_score, epoch_len, finalization_depth, &contributions, &bonds);

        let mut outputs = Vec::new();
        let mut remaining = parent_balance;
        let mut total_drip = 0u64;
        for (epoch, tally) in &tallies {
            if *epoch < e_min || *epoch > e_max || remaining == 0 {
                continue;
            }
            let budget = remaining.min(cap);
            if budget == 0 {
                continue;
            }
            // Stake-proportional distribution to the epoch's included validators (meets=true ⇒ pays).
            let drip = validator_quality_bonus_outputs(budget as u128, &tally.included, tally.expected_stake, true);
            let minted: u64 = drip.iter().fold(0u64, |acc, o| acc.saturating_add(o.value));
            outputs.extend(drip);
            remaining = remaining.saturating_sub(minted);
            total_drip = total_drip.saturating_add(minted);
        }
        (outputs, total_drip)
    }

    /// Verify that the current block fully respects its own UTXO view. We define a block as
    /// UTXO valid if all the following conditions hold:
    ///     1. The block header includes the expected `utxo_commitment`.
    ///     2. The block header includes the expected `accepted_id_merkle_root`.
    ///     3. The block header includes the expected `pruning_point`.
    ///     4. The block coinbase transaction rewards the mergeset blocks correctly.
    ///     5. All non-coinbase block transactions are valid against its own UTXO view.
    pub(super) fn verify_expected_utxo_state<V: UtxoView + Sync>(
        &self,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B): the bond set as-of this
        // block's selected parent. Consumed by the Model-B reward-eligibility
        // rule (PR-10.5′-b2b); the coinbase reward fan-out reader lands in b3.
        selected_parent_bond_view: &ActiveBondView,
        // Header-v4 binds the exact walked provider registry at this block's selected parent. This is
        // the same view used by acceptance-time collateral and DA gates, never the mutable tip store.
        selected_parent_provider_bond_view: &ProviderBondView,
        header: &Header,
    ) -> BlockProcessResult<()> {
        // Verify header UTXO commitment
        let expected_commitment = ctx.multiset_hash.finalize();
        if expected_commitment != header.utxo_commitment {
            return Err(BadUTXOCommitment(header.hash, header.utxo_commitment, expected_commitment));
        }
        trace!("correct commitment: {}, {}", header.hash, expected_commitment);

        // Verify header accepted_id_merkle_root
        let expected_accepted_id_merkle_root =
            self.calc_accepted_id_merkle_root(ctx.accepted_tx_ids.iter().copied(), ctx.selected_parent());

        if expected_accepted_id_merkle_root != header.accepted_id_merkle_root {
            return Err(BadAcceptedIDMerkleRoot(header.hash, header.accepted_id_merkle_root, expected_accepted_id_merkle_root));
        }

        // kaspa-pq ADR-0022: verify the DNS/PoS-v2 overlay-state commitment (as-of the
        // selected parent). The block-template builder committed the identical snapshot
        // (construction == validation). The legacy DNS snapshot has pruning-point import
        // support; PALW beacon state/accumulator transport remains an activation blocker
        // before Header-v3 can be used across a pruning boundary.
        // DNS-only pre-v3 networks and every PALW v3 network commit the overlay. Header-v3 must not
        // silently skip R_E merely because a malformed/custom Params set omitted `dns_params`.
        if self.dns_params.is_some() || header.version >= kaspa_consensus_core::constants::PALW_HEADER_VERSION {
            let snap = self.compute_overlay_snapshot(ctx.selected_parent(), selected_parent_bond_view);
            let expected_overlay = self.versioned_overlay_commitment_root(
                header.version,
                ctx.selected_parent(),
                &snap,
                selected_parent_provider_bond_view,
            );
            if expected_overlay != header.overlay_commitment_root {
                kaspa_core::warn!(
                    "[overlay-diag] block {} sp={} sp_daa={} bonds={} reserve={} window={} legacy_empty_root={} header_root={} computed_root={} window_detail={:?}",
                    header.hash,
                    ctx.selected_parent(),
                    self.headers_store.get_daa_score(ctx.selected_parent()).unwrap_or(u64::MAX),
                    snap.bonds.len(),
                    snap.reserve_balance,
                    snap.window.len(),
                    OverlaySnapshot::default().commitment_root(),
                    header.overlay_commitment_root,
                    expected_overlay,
                    snap.window
                        .iter()
                        .map(|c| (c.block_hash, c.block_daa_score, c.rewarded_keys.len(), c.quality_subpool))
                        .collect::<Vec<_>>()
                );
                return Err(BadOverlayCommitment(header.hash, header.overlay_commitment_root, expected_overlay));
            }
        }

        // kaspa-pq ADR-0039 C6 SLICE 2: authenticate this v3 block's retained beacon-seed field against
        // the consensus-derived R_E. A descendant reads THIS field (as its finality-buried anchor's
        // lagged R_E) for its clause-9 eligibility draw, so a miner-chosen seed must be caught here — the
        // same derivation `commit_palw_beacon_state` persists (construction == validation). Runs at the
        // VIRTUAL stage, where the selected parent's beacon state/accumulator are present (no body-stage
        // ordering hazard). Fail-closed. Pre-v3 headers carry zero and derive `None`; inert on every
        // shipped preset (`derive_palw_beacon_state_value` returns `None` while gated).
        if header.version >= kaspa_consensus_core::constants::PALW_HEADER_VERSION
            && let Some(derived) = self.derive_palw_beacon_state_value(header.hash, selected_parent_bond_view)
        {
            if header.palw_beacon_seed != derived.seed {
                return Err(BadPalwBeaconSeed(header.hash, header.palw_beacon_seed, derived.seed));
            }
            // K5 (ADR-0039 §11.3): an algo-4 chain block whose OWN derived beacon mode is Halted is
            // UTXO-invalid — the exact epoch-mode rule (vs. the body-stage clause-10 lagged
            // indicator). This suppresses chain candidacy (surfaces StatusDisqualifiedFromChain);
            // merged-blue teeth come from clause 10, so the two layer. The algo-3 hash lane is
            // untouched (pow_algo_id guard); DegradedGrace stays valid (only Halted rejects). c==v:
            // the template stamps `palw_beacon_seed` from the same `derive_palw_beacon_state_core`
            // and MUST suppress algo-4 candidates when Halted (`palw_template_lane_open`).
            if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA
                && derived.mode == kaspa_consensus_core::palw::PalwBeaconMode::Halted.to_u8()
            {
                return Err(PalwLaneHalted(header.hash, derived.epoch, derived.degraded_epochs));
            }
        }

        // ADR-MA §23.4: bind this v5 PALW-lane block's committed Compute Set references to the
        // revisions that GOVERN on its own fork, resolved from the selected parent's registry
        // view. Header-stage resolution (§13.2) already checked that the named records exist,
        // were effective at this DAA, are Active, and hold a nonzero share — but it reads the
        // fork-INDEPENDENT record stores, where a superseded revision, a losing branch's
        // revision, and an emergency-halted set are all indistinguishable from a governing one.
        // Only a fork-local view separates them, and the view exists at THIS coordinate.
        //
        // c==v: `palw_mint` stamps the header from `governing_references_at` on the same view, so
        // an honest template always satisfies this. Teeth are chain-candidacy suppression (the
        // `PalwLaneHalted` posture directly above): the block stays in the DAG, but it cannot
        // become a chain block, so it never durably contributes §14 credit.
        if self.palw_compute_registry_activation_daa_score != u64::MAX
            && header.daa_score >= self.palw_compute_registry_activation_daa_score
            && header.version >= kaspa_consensus_core::constants::PALW_COMPUTE_SET_HEADER_VERSION
            && header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA
        {
            let parent_view = self.palw_compute_registry_parent_view(ctx.selected_parent(), header.hash);
            if let Err(rejection) = parent_view.verify_header_references(
                &header.palw_compute_set_id,
                header.daa_score,
                header.palw_compute_policy_id,
                header.palw_allocation_plan_id,
            ) {
                return Err(PalwComputeSetGoverningMismatch(header.hash, rejection.to_string()));
            }
        }

        let txs = self.block_transactions_store.get(header.hash).unwrap();

        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): Model-B
        // reward-eligibility block-validity rule, run BEFORE the coinbase
        // check so the fan-out below can assume every included attestation is
        // eligible (its bond resolves to Active with a valid signature).
        // Inert below activation.
        self.check_attestation_reward_eligibility(&txs, selected_parent_bond_view, header.daa_score)?;

        // kaspa-pq optional DNS-finality hard inclusion: shipped presets keep this inert so missing
        // attestations degrade finality instead of invalidating PoW/GHOSTDAG blocks. Private
        // hard-inclusion forks still evaluate the deterministic selected-parent + accepted + body
        // view here.
        let candidate_accepted_txs = self.accepted_txs_from_acceptance_data(&ctx.mergeset_acceptance_data);
        self.check_mandatory_attestation_inclusion(
            &txs,
            &candidate_accepted_txs,
            selected_parent_bond_view,
            ctx.selected_parent(),
            header.daa_score,
        )?;

        // kaspa-pq Phase 10/11 (ADR-0009 §"SlashingEvidencePayload"): reject a
        // block whose slashing evidence is not genuine, so a forged evidence
        // can never mutate a bond to `Slashed`. Inert below activation.
        self.check_slashing_evidence_genuine(&txs, selected_parent_bond_view, header.daa_score)?;

        // kaspa-pq **ADR-0040 ECON-03 leg 5 — the AUTHORIZED EXIT.** Provider-unbond authorization is
        // NOT re-checked here. A block-level gate USED to live at this coordinate: it ran
        // `palw_provider_unbond_authorized` over `ctx.mergeset_acceptance_data` and rejected the whole
        // block (`RuleError::PalwProviderUnbondUnauthorized`) on the first unauthorized `0x37`. That is
        // a consensus denial of service — a miner does not choose the contents of the merge-blue blocks
        // it merges, so an attacker publishing one unauthorized request invalidated every honest block
        // that merged it. Authorization now lives one stage earlier, as the acceptance-time SKIP
        // `ProviderUnbondAuthFilter` in `calculate_utxo_state` (evaluated against the SAME
        // selected-parent view this gate used, so a bond created and unbonded within one block still
        // cannot authorize itself and construction == validation still holds): an unauthorized request
        // is never accepted, so it never enters `ctx.mergeset_acceptance_data`, never reaches the
        // registry writer, and mutates nothing — while the block stays valid. This mirrors the DNS
        // bond spend-gate's own mergeset-hardening resolution below.

        // kaspa-pq Phase 10/11 (ADR-0016 §D.2): the legacy bond-UTXO spend-gate. Rejects a block
        // whose OWN BODY spends a known non-releasable bond outpoint, against the selected-parent bond
        // view. Inert below `dns_activation_daa_score`.
        //
        // kaspa-pq bond spend-gate mergeset hardening: this own-body REJECT gate misses a spend that
        // rides in a MERGE-BLUE block of this chain block's mergeset (those txs are accepted by
        // `calculate_utxo_state`, never presented here). At/above the
        // `bond_spend_gate_mergeset_activation_daa_score` fence, protection moves to the acceptance-
        // time SKIP in `calculate_utxo_state` (which covers BOTH the mergeset and — when this block is
        // later merged — its own body), so this legacy gate is disabled to avoid an honest miner
        // self-rejecting an own-body bond-spend the skip would simply not accept. The fence is
        // `u64::MAX` on every current preset, so this gate runs unchanged (byte-identical) today.
        let mergeset_bond_gate_active =
            self.dns_params.as_ref().is_some_and(|p| header.daa_score >= p.bond_spend_gate_mergeset_activation_daa_score);
        if !mergeset_bond_gate_active {
            self.check_bond_spend_gate(&txs, selected_parent_bond_view, header.daa_score)?;
        }

        // kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): reject a block whose
        // StakeUnbondRequest is not owner-authorized (unknown/ineligible bond, or a
        // bad owner key / signature), so an attacker cannot force honest bonds into
        // Unbonding to grief them out of the active set. Genesis-active.
        self.check_unbond_request_authorized(&txs, selected_parent_bond_view, header.daa_score)?;

        // kaspa-pq Phase 10/11 + Phase 13 (ADR-0009 Addendum B §B.5 / ADR-0018
        // §F+§E): the validator reward outputs the coinbase must carry. The §F
        // carve (`carve`) splits each source block's reward Worker/Validator/
        // Service; the Validator total (`validator_pool`) funds the §E
        // participation distribution computed by `validator_reward_outputs_for_block`.
        // Both are gated on `dns_activation_daa_score` (= 0 on every current network)
        // with the §F carve in Stage 3 (`full_reward_split_daa_score` = 0), so the
        // overlay is active and the fan-out runs from genesis everywhere. The rewarded
        // `(bond, epoch)` keys are stashed for `commit_utxo_state` (§B.3(c)).
        let mergeset_non_daa = self.daa_excluded_store.get_mergeset_non_daa(header.hash).unwrap();
        // ADR-0018 §F staged rollout: None (Stage 1) / bootstrap (Stage 2) / full
        // (Stage 3) selected by DAA, identically to the construction path.
        let carve = self.dns_params.as_ref().and_then(|p| p.reward_fee_split(header.daa_score));
        let validator_pool = carve.map_or(0, |fs| {
            self.coinbase_manager.coinbase_validator_pool(&ctx.ghostdag_data, &ctx.mergeset_rewards, &mergeset_non_daa, fs)
        });
        let (validator_reward_outputs, rewarded_keys, newly_included_stake, expected_stake) = self.validator_reward_outputs_for_block(
            &txs,
            selected_parent_bond_view,
            header.daa_score,
            ctx.selected_parent(),
            validator_pool,
        );
        ctx.validator_rewarded_keys = rewarded_keys;

        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): stash this block's validator
        // quality sub-pool (the §E split's bonus share) for the per-epoch
        // accumulator. Gated by the v2 fence (`pos_v2_activation_daa_score`): on the
        // devnet/simnet preset (fence = `u64::MAX`) it stays 0 and `commit_utxo_state`
        // writes no row — inert there; on mainnet/testnet (`PRODUCTION_DNS_PARAMS`,
        // fence = 0) it is populated from block 1. (Below `dns_activation` the pool is
        // already 0, since `validator_pool` is.) Does NOT affect the coinbase, so
        // construction == validation is untouched.
        ctx.validator_quality_subpool =
            self.dns_params.as_ref().filter(|p| header.daa_score >= p.pos_v2_activation_daa_score).map_or(0, |p| {
                split_validator_pool(validator_pool as u128, p.reward_params.validator_participation_bps).1.min(u64::MAX as u128)
                    as u64
            });

        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): append the security-reserve **drip** outputs
        // (for the epoch[s] this block finalizes) after the participation + quality-bonus outputs, and
        // advance the per-block reserve-balance recurrence `balance_after = parent_balance +
        // reserve_accrual − drip`. The drip reads the selected parent's COMMITTED balance (so the
        // construction (template) and validation paths agree byte-for-byte). Inert below the v2 fence.
        let mut validator_reward_outputs = validator_reward_outputs;
        if let Some(dns_params) = self.dns_params.as_ref() {
            let parent_balance = self.reserve_balance_store.get(ctx.selected_parent()).unwrap_or(0);
            let (drip_outputs, drip_total) = self.reserve_drip_outputs(
                dns_params,
                header.daa_score,
                ctx.selected_parent(),
                selected_parent_bond_view,
                parent_balance,
            );
            validator_reward_outputs.extend(drip_outputs);
            ctx.reserve_balance_after = parent_balance.saturating_add(ctx.reserve_accrual).saturating_sub(drip_total);
        }

        // Verify coinbase transaction (incl. the §F carve + §E fan-out + §D bounty).
        self.verify_coinbase_transaction(
            &txs[0],
            header.daa_score,
            &ctx.ghostdag_data,
            &ctx.mergeset_rewards,
            &mergeset_non_daa,
            &validator_reward_outputs,
            carve,
            (newly_included_stake, expected_stake),
        )?;

        // Verify the header pruning point
        let reply = self.verify_header_pruning_point(header, ctx.ghostdag_data.to_compact())?;
        ctx.pruning_sample_from_pov = Some(reply.pruning_sample);

        // Verify all transactions are valid in context
        let current_utxo_view = selected_parent_utxo_view.compose(&ctx.mergeset_diff);
        let validated_transactions =
            self.validate_transactions_in_parallel(&txs, &current_utxo_view, header.daa_score, TxValidationFlags::Full);
        if validated_transactions.len() < txs.len() - 1 {
            // Some non-coinbase transactions are invalid
            return Err(InvalidTransactionsInUtxoContext(txs.len() - 1 - validated_transactions.len(), txs.len() - 1));
        }

        Ok(())
    }

    fn verify_header_pruning_point(
        &self,
        header: &Header,
        ghostdag_data: CompactGhostdagData,
    ) -> BlockProcessResult<PruningPointReply> {
        let reply = self.pruning_point_manager.expected_header_pruning_point(ghostdag_data);
        if reply.pruning_point != header.pruning_point {
            return Err(WrongHeaderPruningPoint(reply.pruning_point, header.pruning_point));
        }
        Ok(reply)
    }

    fn verify_coinbase_transaction(
        &self,
        coinbase: &Transaction,
        daa_score: u64,
        ghostdag_data: &GhostdagData,
        mergeset_rewards: &BlockHashMap<BlockRewardData>,
        mergeset_non_daa: &BlockHashSet,
        validator_reward_outputs: &[TransactionOutput],
        // kaspa-pq Phase 13 (ADR-0018 §F): the per-source-block reward carve,
        // threaded to `expected_coinbase_transaction`. `None` on every current
        // network (matches the construction path).
        carve: Option<&FeeSplitParams>,
        // kaspa-pq Phase 13 (ADR-0018 §D): `(newly_included_stake, expected_stake)`,
        // threaded to `expected_coinbase_transaction` for the inclusion bounty.
        inclusion: (u128, u128),
    ) -> BlockProcessResult<()> {
        // Extract only miner data from the provided coinbase
        let miner_data = self.coinbase_manager.deserialize_coinbase_payload(&coinbase.payload).unwrap().miner_data;
        let expected_coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                daa_score,
                miner_data,
                ghostdag_data,
                mergeset_rewards,
                mergeset_non_daa,
                validator_reward_outputs,
                carve,
                inclusion,
            )
            .unwrap()
            .tx;
        if hashing::tx::hash(coinbase) != hashing::tx::hash(&expected_coinbase) {
            // kaspa-pq diagnostic (coinbase mismatch): dump the mismatch SHAPE so the
            // cache-retarget class (equal output count, payload already retargeted, a
            // single SCRIPT-only diff at a miner-script output index — old miner's spk
            // left behind) is distinguishable in-field from a reward-generation skew
            // (differing output count, or an AMOUNT diff, or a validator/bond-owner
            // script diff). Emitted only on the already-failing branch → consensus-neutral.
            let (n_act, n_exp) = (coinbase.outputs.len(), expected_coinbase.outputs.len());
            let first_diff = (0..n_act.min(n_exp)).find(|&i| coinbase.outputs[i] != expected_coinbase.outputs[i]);
            match first_diff {
                Some(i) => {
                    let (a, e) = (&coinbase.outputs[i], &expected_coinbase.outputs[i]);
                    kaspa_core::warn!(
                        "[coinbase-mismatch] outputs act={n_act} exp={n_exp}; first diff @{i}: value_eq={} script_eq={} payload_eq={} (act_value={} exp_value={})",
                        a.value == e.value,
                        a.script_public_key == e.script_public_key,
                        coinbase.payload == expected_coinbase.payload,
                        a.value,
                        e.value,
                    );
                }
                None => {
                    kaspa_core::warn!(
                        "[coinbase-mismatch] outputs act={n_act} exp={n_exp}; no in-range output diff (count or payload mismatch) payload_eq={}",
                        coinbase.payload == expected_coinbase.payload,
                    );
                }
            }
            Err(BadCoinbaseTransaction)
        } else {
            Ok(())
        }
    }

    /// kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.5 / ADR-0013): the
    /// validator reward outputs a block's coinbase must carry. Derived
    /// deterministically from the block's included attestations
    /// (`attestations_from_accepted_txs`, canonical order) resolved against
    /// `bond_view` (the bond set as-of the block's selected parent) and the
    /// network `RewardParams`. ADR-0018 §E: each included validator earns a
    /// stake-proportional share of the §F validator pool's participation
    /// sub-pool against the epoch's expected (total active) stake, with
    /// within-block + cross-block `(bond, epoch)` dedup and a whole-output pool
    /// cap (see [`validator_participation_reward_outputs`]).
    ///
    /// Run identically by the coinbase **construction** (block-template) and
    /// **validation** paths, so they agree byte-for-byte. Returns no outputs
    /// unless the overlay is configured AND `daa_score` has reached
    /// `dns_activation_daa_score` (= 0 everywhere today) — so it is active
    /// from genesis on every current network. Callers run the §B.4
    /// eligibility rule first, so every attestation here resolves to an
    /// `Active` bond; the `if let Some` is a defensive skip.
    ///
    /// `selected_parent` is the block's selected parent — the chain tip the
    /// `(bond, epoch)` cross-block uniqueness walk starts from (§B.3(c)). The
    /// walk (this block + its selected-chain ancestors within
    /// `reward_uniqueness_window_blocks` DAA) reads the per-block
    /// `rewarded_epochs_store` to build the already-rewarded prefix set; the
    /// matching recency bound drops attestations whose target is older than the
    /// window, so the bounded walk is guaranteed to see any prior reward of the
    /// same pair. The overlay is genesis-active on every current network
    /// (`dns_activation_daa_score` = 0), so the walk reads the rows written as
    /// validators are rewarded.
    pub(super) fn validator_reward_outputs_for_block(
        &self,
        txs: &[Transaction],
        bond_view: &ActiveBondView,
        daa_score: u64,
        selected_parent: BlockHash,
        // kaspa-pq Phase 13 (ADR-0018 §F/§E): the validator-side coinbase pool
        // (`CoinbaseManager::coinbase_validator_pool`) this block's §E
        // participation rewards are distributed from. The caller computes it past
        // `dns_activation_daa_score` (= 0 on every current network), so it is
        // funded from genesis everywhere.
        validator_pool: u64,
        // kaspa-pq Phase 13 (ADR-0018 §D): also returns `(newly_included_stake,
        // expected_stake)` so the coinbase can pay the §D worker inclusion bounty.
    ) -> (Vec<TransactionOutput>, RewardedEpochKeys, u128, u128) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return (Vec::new(), Vec::new(), 0, 0);
        };
        if daa_score < dns_params.dns_activation_daa_score {
            return (Vec::new(), Vec::new(), 0, 0);
        }
        let window = dns_params.reward_uniqueness_window_blocks;

        // kaspa-pq DNS v3: canonical anchors as-of the selected parent (the deterministic
        // as-of-block view, like `bond_view` / the deferred-bonus path). Only an attestation
        // naming THIS chain's canonical lagged anchor for a READY, NON-DUPLICATE epoch earns a
        // reward — exactly the GoodAttestation v3 rule the PR4 StakeScore verifier applies, so
        // reward and StakeScore agree on what counts. A non-canonical or duplicate target earns
        // nothing (it is simply absent from `creditable`), and so never enters `rewarded_keys`
        // (hence never the §D bounty, §E pool, cross-block dedup, or the deferred bonus).
        let creditable = self.canonical_anchors_in_window(selected_parent, dns_params);

        // Resolve eligible, recent, CANONICAL attestations (canonical order). Recency
        // (§B.3(c)): an attestation whose target is older than the window earns
        // nothing — keeps the uniqueness walk below bounded.
        let mut attestations = Vec::new();
        for att in attestations_from_accepted_txs(txs) {
            if daa_score.saturating_sub(att.target_daa_score) > window {
                continue;
            }
            // v3 canonical gate: the epoch must be creditable (ready, non-duplicate) and the
            // attestation must name its canonical anchor exactly.
            let Some(anchor) = creditable.get(&att.epoch) else {
                continue;
            };
            if att.target_hash != anchor.anchor_hash || att.target_daa_score != anchor.anchor_daa_score {
                continue;
            }
            if let Some(bond) = bond_view.active_bond_at(&att.bond_outpoint, att.target_daa_score) {
                // ADR-0018 §E: carry the bond's stake — the proportional weight in the
                // participation distribution (against the expected-stake denominator).
                attestations.push((att.bond_outpoint, att.epoch, bond.owner_reward_spk_payload, bond.amount));
            }
        }

        // Build the already-rewarded prefix set: the selected parent and its
        // selected-chain ancestors within `window` DAA, unioning each block's
        // rewarded `(bond, epoch)` keys (§B.3(c)). ADR-0022: routed through
        // `selected_chain_overlay_window`, which merges the persisted below-pruning-
        // point window — so a pruned-IBD node dedups post-pruning blocks against the
        // pre-pruning rewards too (its walk cannot reach below the pruning point).
        // Inert merge on a from-genesis node.
        let mut already_rewarded = RewardedEpochSet::new();
        for c in self.selected_chain_overlay_window(selected_parent, daa_score, window) {
            for (bond_outpoint, epoch) in c.rewarded_keys.iter() {
                already_rewarded.insert(*bond_outpoint, *epoch);
            }
        }

        // ADR-0018 §E: distribute the participation sub-pool proportionally by stake against the
        // epoch's expected (total active) stake — the anti-capture denominator — with the same
        // within-block + cross-block (§B.3(c)) `(bond, epoch)` uniqueness and a whole-output pool
        // cap (Σ ≤ pool; the unspent remainder is not minted).
        //
        // M-04 (denominator definition): the per-block reward intentionally uses the INCLUSION-time
        // active set (`total_active_stake_at(daa_score)`, below) as the expected-stake denominator,
        // whereas the StakeScore security signal uses the epoch-ANCHOR-time set. Both are
        // per-block-deterministic (read from the same selected-parent bond view), so neither splits;
        // they differ only in reference point, which is correct — the reward pays inclusion in THIS
        // block against the stake live at THIS block, while StakeScore measures buried-epoch security.
        //
        // ADR-0018 "本格版" (PoS-v2): the participation/quality split is **fenced**. Below
        // `pos_v2_activation_daa_score` the FULL pool funds participation (effective bps = 10_000),
        // byte-identical to the pre-v2 behavior regardless of the configured `validator_participation_bps`
        // — so on the devnet/simnet preset (fence = `u64::MAX`) raising the quality share in the presets
        // stays inert. At/above the fence — i.e. on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0)
        // from block 1 — the configured split carves the quality-bonus sub-pool, which the per-epoch
        // accumulator accrues (Phase 1) and `deferred_quality_bonus_outputs` (below) pays at finalization.
        let expected_stake = bond_view.total_active_stake_at(daa_score) as u128;
        let participation_bps = if daa_score >= dns_params.pos_v2_activation_daa_score {
            dns_params.reward_params.validator_participation_bps
        } else {
            10_000
        };
        let (participation_pool, _quality_bonus_pool) = split_validator_pool(validator_pool as u128, participation_bps);

        // ADR-0018 §D base inclusion bounty: the stake this block NEWLY includes — the
        // same recency-filtered attestations under the same within-block + cross-block
        // (§B.3(c)) `(bond, epoch)` dedup as §E, but summed pre-pool-cap (the miner
        // included them regardless of what §E could pay). The coinbase pays the includer
        // a proportional share of the §D pool against `expected_stake`.
        let mut seen_in_block = RewardedEpochSet::new();
        let mut newly_included_stake: u128 = 0;
        for (bond_outpoint, epoch, _payload, stake) in &attestations {
            if already_rewarded.contains(bond_outpoint, *epoch) || !seen_in_block.insert(*bond_outpoint, *epoch) {
                continue;
            }
            newly_included_stake += *stake as u128;
        }

        let (mut outputs, rewarded_keys) =
            validator_participation_reward_outputs(participation_pool, expected_stake, &attestations, &already_rewarded);

        // ADR-0018 "本格版" (PoS-v2) §E deferred quality bonus: append the bonus outputs for any
        // epoch THIS block first buries beyond the finalization window (φS-gated), recomputed from
        // the selected-parent window. Inert below the v2 fence. The finalized epochs are old
        // (buried by `reward_window + max_reorg_horizon`) and disjoint from the participation
        // epochs (recent, within `reward_window`), so the two output sets never double-pay.
        outputs.extend(self.deferred_quality_bonus_outputs(dns_params, daa_score, selected_parent, bond_view));

        (outputs, rewarded_keys, newly_included_stake, expected_stake)
    }

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 2) §E — the **deferred quality-bonus** coinbase
    /// outputs THIS block emits for every epoch it newly finalizes. An epoch `E` is finalized-by-
    /// this-block iff its finalization threshold `(E+1)·L + finalization_depth` falls in
    /// `(selected_parent.daa_score, daa_score]` — a pure DAA crossing, so on any chain exactly one
    /// block pays `E` (the once-per-epoch guard; no extra store). For each crossed `E` the tally is
    /// recomputed from the selected-parent window via [`recompute_epoch_tallies`] (the same pure
    /// core the Phase-1 accumulator store uses); because `E` is buried beyond `reward_window +
    /// max_reorg_horizon` its contributing blocks are reorg-immutable, so the coinbase
    /// **construction** and **validation** paths — and any competing chain — build byte-identical
    /// outputs. Each crossed epoch that met φS pays its accrued quality pool to its included
    /// validators ([`validator_quality_bonus_outputs`]); one that missed φS pays nothing (rollover).
    ///
    /// Returns no outputs below the v2 fence (`pos_v2_activation_daa_score`), or when no epoch
    /// crosses this block — so it is inert on the devnet/simnet preset (fence = `u64::MAX`); on
    /// mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0) it pays from block 1, and is
    /// O(1) amortized (the deep window walk runs only on the ~1-in-`L` crossing blocks).
    fn deferred_quality_bonus_outputs(
        &self,
        dns_params: &DnsParams,
        daa_score: u64,
        selected_parent: BlockHash,
        // The bond set as-of the block's selected parent (the deterministic, as-of-block view the
        // participation path uses). Resolving the finalized epochs against THIS — not the live
        // `stake_bonds_store` (as-of the current sink) — is what keeps construction == validation
        // when a non-tip block is validated. Buried bonds are immutable, so this is also reorg-safe.
        bond_view: &ActiveBondView,
    ) -> Vec<TransactionOutput> {
        if daa_score < dns_params.pos_v2_activation_daa_score {
            return Vec::new();
        }
        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap();

        // The inclusive epoch range this block newly finalizes (thresholds crossed in
        // `(parent_daa, daa_score]`); `None` ⇒ this block finalizes nothing (the common case).
        let Some((e_min, e_max)) = epochs_finalized_at(parent_daa, daa_score, epoch_len, finalization_depth) else {
            return Vec::new();
        };

        // Recompute the finalized epochs' tallies from the selected-parent window (the same bounded
        // walk + pure core the Phase-1 accumulator uses), anchored at the selected parent so
        // construction and validation read the identical buried history.
        let walk_bound = finalization_depth.saturating_add(epoch_len.saturating_mul(2));
        let contributions = self.selected_chain_epoch_contributions(selected_parent, parent_daa, walk_bound);
        let bonds = bond_view.records();
        let tallies = recompute_epoch_tallies(daa_score, epoch_len, finalization_depth, &contributions, &bonds);

        // Pay each crossed epoch in `[e_min, e_max]` that met φS.
        let mut outputs = Vec::new();
        for (epoch, tally) in &tallies {
            if *epoch < e_min || *epoch > e_max {
                continue;
            }
            let included_sum: u128 = tally.included.iter().map(|(_, s)| *s as u128).sum();
            let meets = epoch_meets_quality_floor(included_sum, tally.expected_stake, dns_params.stake_event_quality_floor_bps);
            outputs.extend(validator_quality_bonus_outputs(tally.quality_pool_accrued, &tally.included, tally.expected_stake, meets));
        }
        outputs
    }

    /// kaspa-pq DNS-finality (E1/E3 §6.1/§6.2): classify ONE selected mempool tx for
    /// the block template. Folds the activation gate (`dns_params.is_some()` AND
    /// `daa_score >= dns_activation_daa_score`) then delegates to the pure
    /// [`classify_attestation_shard_for_template`]. Below the gate every tx is
    /// `KeepNonShard` — byte-identical to the pre-classifier behavior. Uses the SAME
    /// per-attestation eligibility checks ([`classify_one_attestation`]) as the §B.4
    /// block-validity rule, so a kept shard always passes validation and a dropped one
    /// would have self-disqualified the block; the template path additionally returns
    /// a drop REASON so the builder can refill + count.
    ///
    /// Recency is *not* filtered here: a stale-but-eligible shard is valid (§B.4
    /// ignores recency) and simply earns no reward, so it is `KeepEligible`.
    pub(super) fn classify_attestation_shard_for_template(
        &self,
        tx: &Transaction,
        bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> AttestationShardDecision {
        let activated = self.dns_params.as_ref().is_some_and(|p| daa_score >= p.dns_activation_daa_score);
        classify_attestation_shard_for_template(tx, bond_view, self.genesis.hash, activated)
    }

    /// kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): legacy block-template
    /// pre-filter, retained for the rare paths that still pass a pre-built `txs`
    /// vec into `build_block_template_from_virtual_state` WITHOUT going through the
    /// E3 selection-loop classification (which is the normal `build_block_template`
    /// path). Drops any `StakeAttestationShard` tx carrying an attestation that is
    /// not §B.4-eligible so a block mined from the template passes the eligibility
    /// rule rather than self-disqualifying. Idempotent after the E3 loop already
    /// classified (it finds nothing to drop). Non-shard txs are always retained.
    /// Inert below the activation gate. NOTE: callers that must keep a parallel
    /// `calculated_fees` vec 1:1 with `txs` must NOT use this (it removes from `txs`
    /// only); the E3 loop in `build_block_template` handles fee lockstep + refill.
    pub(super) fn retain_reward_eligible_attestation_shards(
        &self,
        txs: &mut Vec<Transaction>,
        bond_view: &ActiveBondView,
        daa_score: u64,
    ) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return;
        };
        if daa_score < dns_params.dns_activation_daa_score {
            return;
        }
        let net_id = self.genesis.hash;
        // A non-shard tx yields no attestations → `attestation_reward_eligibility`
        // returns Ok, so it is retained. A shard tx is retained iff *all* its
        // attestations are eligible.
        txs.retain(|tx| attestation_reward_eligibility(std::slice::from_ref(tx), bond_view, net_id, true).is_ok());
    }

    /// kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): the Model-B
    /// reward-eligibility **block-validity** rule. Rejects a block that
    /// includes a `StakeAttestationShard` whose attestation is not
    /// structurally reward-eligible against this block's own selected-parent
    /// bond view — its bond must resolve to `Active` (at the attestation's
    /// `target_daa_score`) **and** its ML-DSA-87 signature must verify. This
    /// makes "included ⇒ rewardable" a consensus invariant, so the coinbase
    /// reward fan-out (PR-10.5′-b3) needs no skip set. Reward **uniqueness**
    /// (Addendum B §B.3(c)) is a reward-emission concern, not a validity one
    /// (a duplicate `(bond, epoch)` is simply unrewarded), and is not checked
    /// here.
    ///
    /// Active when the overlay is configured **and** `daa_score` has reached
    /// `dns_activation_daa_score` (= 0 on every current network, so
    /// this runs from genesis today). The canonical
    /// digest + signature verification mirror the StakeScore aggregation pass
    /// (`processor.rs`) byte-for-byte and the validator-service signer.
    fn check_attestation_reward_eligibility(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        // Fold the gate: configured overlay AND past activation.
        let activated = self.dns_params.as_ref().is_some_and(|p| daa_score >= p.dns_activation_daa_score);
        // ADR-0009 Addendum A.3: the network_id discriminator is the genesis hash.
        attestation_reward_eligibility(txs, selected_parent_bond_view, self.genesis.hash, activated)
            .map_err(|(bond_tx, epoch)| IneligibleAttestationInBlock(bond_tx, epoch))
    }

    /// kaspa-pq DNS-finality optional hard inclusion rule.
    ///
    /// This is the consensus-level anti-censorship gate. It deliberately does NOT ask whether an
    /// attestation was visible in this node's mempool. Instead it uses only deterministic inputs:
    /// the selected-parent chain, the selected-parent active-bond view, the candidate acceptance
    /// data that this block deterministically commits, and this block's body.
    ///
    /// For the oldest ready, canonical, non-duplicate epoch whose selected-parent chain has not yet
    /// reached the configured stake quality floor, this block must either accept or include enough
    /// canonical, eligible attestations to bring the epoch to that floor. Counting candidate
    /// acceptance data is essential on a Kaspa-style DAG: a block's body is credited by its child,
    /// so the child must not demand the same signatures again. Once an epoch is certified, later
    /// blocks do not need to include it again. Shipped liveness-first presets leave this optional
    /// hard gate inert (`mandatory_attestation_inclusion_daa_score = u64::MAX`); private hard-gate
    /// deployments additionally require the conservative one-block single-shard capacity invariant,
    /// so a capacity-impossible validator set cannot halt the base ledger.
    pub(crate) fn check_mandatory_attestation_inclusion(
        &self,
        txs: &[Transaction],
        candidate_accepted_txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        selected_parent: BlockHash,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return Ok(());
        };
        if daa_score < dns_params.dns_activation_daa_score
            || daa_score < dns_params.mandatory_attestation_inclusion_daa_score
            || !dns_params.dns_v3_params_consistent()
        {
            return Ok(());
        }

        let anchors = self.canonical_anchors_in_window(selected_parent, dns_params);
        if anchors.is_empty() {
            return Ok(());
        }

        let bonds = selected_parent_bond_view.records();
        let (parent_contributions, _, _) =
            self.collect_stake_contributions_v2(selected_parent, None, &bonds, self.genesis.hash.as_byte_slice(), dns_params);
        let mut seen_parent: HashSet<(TransactionOutpoint, TransactionId, u64)> = HashSet::new();
        let mut parent_included_by_epoch: HashMap<u64, u64> = HashMap::new();
        for c in parent_contributions {
            if seen_parent.insert((c.bond_outpoint, c.validator_id, c.epoch)) {
                let entry = parent_included_by_epoch.entry(c.epoch).or_insert(0);
                *entry = entry.saturating_add(c.signed_stake_sompi);
            }
        }

        let bond_by_outpoint: HashMap<_, _> = bonds.iter().map(|b| (b.bond_outpoint, b)).collect();

        for (&epoch, anchor) in &anchors {
            let active_bonds: Vec<_> = bonds.iter().filter(|b| is_bond_active_at(b, anchor.anchor_daa_score)).collect();
            let expected_stake = active_bonds.iter().fold(0u64, |acc, b| acc.saturating_add(b.amount));
            if expected_stake == 0 || expected_stake < dns_params.min_active_stake_sompi {
                continue;
            }
            let active_validator_count = active_bonds.len() as u32;
            if active_validator_count < dns_params.min_active_validators {
                continue;
            }

            let full_floor_capacity = mandatory_attestation_mass_capacity(
                active_bonds.iter().map(|bond| bond.amount),
                expected_stake,
                0,
                dns_params.stake_event_quality_floor_bps,
                self.max_block_mass,
                dns_params.max_attestation_shard_mass,
            );
            if !full_floor_capacity.fits {
                // The rollout stage stays Bootstrap when the active set cannot satisfy the
                // conservative single-shard capacity invariant. Keep hard mandatory dormant too:
                // capacity must never become a consensus hard-stop for the base ledger, and
                // aggregate shard bodies remain valid if they independently satisfy the floor.
                return Ok(());
            }

            let parent_included = parent_included_by_epoch.get(&epoch).copied().unwrap_or(0);
            if epoch_meets_quality_floor(parent_included as u128, expected_stake as u128, dns_params.stake_event_quality_floor_bps) {
                continue;
            }

            let mut combined_included = parent_included;
            let mut seen_candidate: HashSet<(TransactionOutpoint, TransactionId, u64)> = HashSet::new();
            for att in attestations_from_accepted_txs(candidate_accepted_txs) {
                if att.epoch != epoch || att.target_hash != anchor.anchor_hash || att.target_daa_score != anchor.anchor_daa_score {
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
                if matches!(
                    verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
                    Ok(true)
                ) {
                    combined_included = combined_included.saturating_add(bond.amount);
                }
            }

            if epoch_meets_quality_floor(combined_included as u128, expected_stake as u128, dns_params.stake_event_quality_floor_bps) {
                continue;
            }

            for att in attestations_from_accepted_txs(txs) {
                if att.epoch != epoch || att.target_hash != anchor.anchor_hash || att.target_daa_score != anchor.anchor_daa_score {
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
                if matches!(
                    verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
                    Ok(true)
                ) {
                    combined_included = combined_included.saturating_add(bond.amount);
                }
            }

            if !epoch_meets_quality_floor(combined_included as u128, expected_stake as u128, dns_params.stake_event_quality_floor_bps)
            {
                return Err(MissingMandatoryAttestationInBlock(
                    epoch,
                    combined_included,
                    expected_stake,
                    dns_params.stake_event_quality_floor_bps,
                ));
            }

            // Only the oldest deficient epoch is mandatory for this block. If the block certifies
            // it, the next block can advance to any remaining backlog.
            return Ok(());
        }

        Ok(())
    }

    /// kaspa-pq Phase 10/11 (ADR-0009 §"SlashingEvidencePayload"): the stateful
    /// slashing-evidence genuineness rule. Rejects a block carrying a
    /// `SlashingEvidence` whose referenced bond is unknown in the block's
    /// selected-parent bond view, or one of whose two equivocating attestations
    /// does not ML-DSA-verify against that bond's `validator_pubkey` — so a
    /// forged-but-well-formed evidence (the §A.2 tx-level check is structural
    /// only) cannot mutate a bond to `Slashed`. Active when the overlay is
    /// configured **and** past `dns_activation_daa_score` (= 0
    /// everywhere today).
    fn check_slashing_evidence_genuine(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        let Some(params) = self.dns_params.as_ref() else { return Ok(()) };
        let activated = daa_score >= params.dns_activation_daa_score;
        slashing_evidence_genuine(
            txs,
            selected_parent_bond_view,
            self.genesis.hash,
            daa_score,
            params.evidence_window_blocks,
            activated,
        )
        .map_err(UnverifiableSlashingEvidenceInBlock)
    }

    /// kaspa-pq Phase 10/11 (ADR-0016 §D.2): the bond-UTXO spend-gate. Rejects a
    /// block that includes a transaction spending a **known** bond outpoint
    /// (present in the block's selected-parent bond view) whose bond is **not
    /// releasable** at the block's DAA score — releasable meaning the bond is
    /// `Unbonding` and `daa_score >= unbond_request_daa_score +
    /// unbonding_period_blocks`. A `Pending`/`Active` bond, an `Unbonding` bond
    /// before its release height, or a `Slashed` bond therefore cannot have its
    /// staked output-0 spent, which is what makes the declared `amount` real
    /// locked capital (D.1 pins `value == amount` to that output at acceptance).
    ///
    /// Like the sibling overlay checks this reads the same selected-parent
    /// [`ActiveBondView`], so it is per-block-deterministic and reorg-safe. Active
    /// when the overlay is configured **and** `daa_score` has reached
    /// `dns_activation_daa_score` (= 0 on every current network, so this
    /// runs from genesis today).
    fn check_bond_spend_gate(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        let activated = self.dns_params.as_ref().is_some_and(|p| daa_score >= p.dns_activation_daa_score);
        bond_spend_gate(txs, selected_parent_bond_view, daa_score, activated)
            .map_err(|(spending_tx, bond_outpoint)| NonReleasableBondSpendInBlock(spending_tx, bond_outpoint))
    }

    /// kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): the stake-unbond owner-
    /// authorization rule. Rejects a block carrying a `StakeUnbondRequest` that
    /// is not authorized by the bond owner (see [`unbond_request_authorized`]).
    /// Inert below activation.
    fn check_unbond_request_authorized(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        let activated = self.dns_params.as_ref().is_some_and(|p| daa_score >= p.dns_activation_daa_score);
        unbond_request_authorized(txs, selected_parent_bond_view, self.genesis.hash.as_byte_slice(), daa_score, activated)
            .map_err(|(tx_id, bond_outpoint)| UnauthorizedUnbondRequestInBlock(tx_id, bond_outpoint))
    }

    /// Validates transactions against the provided `utxo_view` and returns a vector with all transactions
    /// which passed the validation along with their original index within the containing block
    pub(crate) fn validate_transactions_in_parallel<'a, V: UtxoView + Sync>(
        &self,
        txs: &'a Vec<Transaction>,
        utxo_view: &V,
        pov_daa_score: u64,
        flags: TxValidationFlags,
    ) -> Vec<(ValidatedTransaction<'a>, u32)> {
        self.thread_pool.install(|| {
            txs
                .par_iter() // We can do this in parallel without complications since block body validation already ensured
                            // that all txs within each block are independent
                .enumerate()
                .skip(1) // Skip the coinbase tx.
                // `None`, `None`, `None`: the own-body / template path is not mergeset acceptance; the
                // legacy own-body bond gate (below the fence) covers spends, see
                // `verify_expected_utxo_state`, and provider-unbond authorization / provider-bond spend
                // gating are likewise acceptance-time concerns only.
                .filter_map(|(i, tx)| self.validate_transaction_in_utxo_context(tx, &utxo_view, pov_daa_score, flags, None, None, None).ok().map(|vtx| (vtx, i as u32)))
                .collect()
        })
    }

    /// Same as validate_transactions_in_parallel except during the iteration this will also
    /// calculate the muhash in parallel for valid transactions
    pub(crate) fn validate_transactions_with_muhash_in_parallel<'a, V: UtxoView + Sync>(
        &self,
        txs: &'a Vec<Transaction>,
        utxo_view: &V,
        pov_daa_score: u64,
        flags: TxValidationFlags,
        // kaspa-pq bond spend-gate (mergeset hardening): forwarded to the per-tx check so a mergeset
        // tx spending a non-releasable bond is SKIPPED (not accepted, not muhashed). `None` ⇒ inert.
        bond_filter: Option<BondSpendFilter>,
        // kaspa-pq ADR-0040 ECON-03 leg 5: forwarded to the per-tx check so an unauthorized `0x37`
        // provider-unbond tx is SKIPPED (not accepted, not muhashed, mutates no registry row). `None`
        // ⇒ inert.
        provider_unbond_filter: Option<ProviderUnbondAuthFilter>,
        // kaspa-pq ADR-0040 ECON-03 leg 4: forwarded to the per-tx check so a tx spending a
        // non-releasable provider bond's locked output-0 is SKIPPED (not accepted, not muhashed, the
        // output-0 stays in the set). `None` ⇒ inert.
        provider_bond_filter: Option<ProviderBondSpendFilter>,
    ) -> (SmallVec<[(ValidatedTransaction<'a>, u32); 2]>, MuHash) {
        self.thread_pool.install(|| {
            txs
                .par_iter() // We can do this in parallel without complications since block body validation already ensured
                            // that all txs within each block are independent
                .enumerate()
                .skip(1) // Skip the coinbase tx.
                .filter_map(|(i, tx)| self.validate_transaction_in_utxo_context(tx, &utxo_view, pov_daa_score, flags, bond_filter, provider_unbond_filter, provider_bond_filter).ok().map(|vtx| {
                    let mh = MuHash::from_transaction(&vtx, pov_daa_score);
                    (smallvec![(vtx, i as u32)], mh)
                }
                ))
                .reduce(
                    || (smallvec![], MuHash::new()),
                    |mut a, mut b| {
                        a.0.append(&mut b.0);
                        a.1.combine(&b.1);
                        a
                    },
                )
        })
    }

    /// Attempts to populate the transaction with UTXO entries and performs all utxo-related tx validations
    pub(super) fn validate_transaction_in_utxo_context<'a>(
        &self,
        transaction: &'a Transaction,
        utxo_view: &impl UtxoView,
        pov_daa_score: u64,
        flags: TxValidationFlags,
        // kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): when `Some`, a tx that spends
        // a known non-releasable bond's locked output-0 fails validation, so the caller's `filter_map`
        // SKIPS it (it is not accepted, its muhash/diff contribution is never produced, the bond stays
        // locked). `None` on every path that is not mergeset-acceptance, and on every net below the
        // fence ⇒ byte-identical to the legacy own-body-only gate.
        bond_filter: Option<BondSpendFilter>,
        // kaspa-pq (ADR-0040 ECON-03 leg 5): when `Some`, an unauthorized `0x37` provider-unbond tx
        // fails validation, so the caller's `filter_map` SKIPS it (not accepted, no registry mutation),
        // while the carrying/merging block stays valid. `None` on every path that is not
        // mergeset-acceptance, and on every net below the PALW fence ⇒ byte-identical to before.
        provider_unbond_filter: Option<ProviderUnbondAuthFilter>,
        // kaspa-pq (ADR-0040 ECON-03 leg 4): when `Some`, a tx that spends a known non-releasable
        // provider bond's locked output-0 fails validation, so the caller's `filter_map` SKIPS it (not
        // accepted, its muhash/diff contribution never produced, the output-0 stays locked). `None` on
        // every path that is not mergeset-acceptance, and on every net below the PALW fence ⇒
        // byte-identical to before.
        provider_bond_filter: Option<ProviderBondSpendFilter>,
    ) -> TxResult<ValidatedTransaction<'a>> {
        // kaspa-pq ADR-0040 ECON-03 leg 5: reject — so the caller SKIPS, NOT rejecting the whole block
        // — an unauthorized provider-unbond request, BEFORE input population so a `0x37` tx (which
        // carries no UTXO inputs) is judged on its authorization alone. Inert when the filter is `None`
        // (every path except fence-active mergeset acceptance); `None` for non-`0x37` traffic.
        if let Some(filter) = provider_unbond_filter
            && let Some(bond_outpoint) = filter.unauthorized(transaction)
        {
            return Err(TxRuleError::UnauthorizedProviderUnbond(bond_outpoint));
        }
        let mut entries = Vec::with_capacity(transaction.inputs.len());
        for input in transaction.inputs.iter() {
            if let Some(entry) = utxo_view.get(&input.previous_outpoint) {
                entries.push(entry);
            } else {
                // Missing at least one input. For perf considerations, we report once a single miss is detected and avoid collecting all possible misses.
                return Err(TxRuleError::MissingTxOutpoints);
            }
        }
        // kaspa-pq bond spend-gate (mergeset hardening): reject — so the caller skips, NOT rejecting
        // the whole block — any tx whose input spends a known non-releasable bond's locked output-0.
        // Inert when `bond_filter` is `None` (every path except fence-active mergeset acceptance).
        if let Some(filter) = bond_filter
            && let Some(input) = transaction.inputs.iter().find(|input| filter.locks(&input.previous_outpoint))
        {
            return Err(TxRuleError::SpendsNonReleasableBond(input.previous_outpoint));
        }
        // kaspa-pq ADR-0040 ECON-03 leg 4: reject — so the caller skips, NOT rejecting the whole block
        // — any tx whose input spends a known non-releasable PALW provider bond's locked output-0.
        // Inert when `provider_bond_filter` is `None` (every path except fence-active mergeset
        // acceptance). Rides the SAME per-tx acceptance walk as `bond_filter` above, so a merge-blue
        // spend is on this gated path, not only the chain block's own body.
        if let Some(filter) = provider_bond_filter
            && let Some(input) = transaction.inputs.iter().find(|input| filter.locks(&input.previous_outpoint))
        {
            return Err(TxRuleError::SpendsNonReleasableProviderBond(input.previous_outpoint));
        }
        let populated_tx = PopulatedTransaction::new(transaction, entries);
        let res = self.transaction_validator.validate_populated_transaction_and_get_fee(&populated_tx, pov_daa_score, flags, None);
        match res {
            Ok(calculated_fee) => Ok(ValidatedTransaction::new(populated_tx, calculated_fee)),
            Err(tx_rule_error) => {
                // TODO (relaxed): aggregate by error types and log through the monitor (in order to not flood the logs)
                info!("Rejecting transaction {} due to transaction rule error: {}", transaction.id(), tx_rule_error);
                Err(tx_rule_error)
            }
        }
    }

    /// Populates the mempool transaction with maximally found UTXO entry data
    pub(crate) fn populate_mempool_transaction_in_utxo_context(
        &self,
        mutable_tx: &mut MutableTransaction,
        utxo_view: &impl UtxoView,
    ) -> TxResult<()> {
        let mut has_missing_outpoints = false;
        for i in 0..mutable_tx.tx.inputs.len() {
            if mutable_tx.entries[i].is_some() {
                // We prefer a previously populated entry if such exists
                continue;
            }
            if let Some(entry) = utxo_view.get(&mutable_tx.tx.inputs[i].previous_outpoint) {
                mutable_tx.entries[i] = Some(entry);
            } else {
                // We attempt to fill as much as possible UTXO entries, hence we do not break in this case but rather continue looping
                has_missing_outpoints = true;
            }
        }
        if has_missing_outpoints {
            return Err(TxRuleError::MissingTxOutpoints);
        }
        Ok(())
    }

    /// Populates the mempool transaction with maximally found UTXO entry data and proceeds to validation if all found
    pub(super) fn validate_mempool_transaction_in_utxo_context(
        &self,
        mutable_tx: &mut MutableTransaction,
        utxo_view: &impl UtxoView,
        pov_daa_score: u64,
        args: &TransactionValidationArgs,
    ) -> TxResult<()> {
        self.populate_mempool_transaction_in_utxo_context(mutable_tx, utxo_view)?;

        // Calc the contextual storage mass
        let contextual_mass = self
            .transaction_validator
            .mass_calculator
            .calc_contextual_masses(&mutable_tx.as_verifiable())
            .ok_or(TxRuleError::MassIncomputable)?;

        // Set the inner mass field
        mutable_tx.tx.set_mass(contextual_mass.storage_mass);

        // At this point we know all UTXO entries are populated, so we can safely pass the tx as verifiable
        let mass_and_feerate_threshold = args
            .feerate_threshold
            .map(|threshold| (contextual_mass.max(mutable_tx.calculated_non_contextual_masses.unwrap()), threshold));
        let calculated_fee = self.transaction_validator.validate_populated_transaction_and_get_fee(
            &mutable_tx.as_verifiable(),
            pov_daa_score,
            TxValidationFlags::SkipMassCheck, // we can skip the mass check since we just set it
            mass_and_feerate_threshold,
        )?;
        mutable_tx.calculated_fee = Some(calculated_fee);
        Ok(())
    }

    /// Calculates the accepted_id_merkle_root based on the current DAA score and the accepted tx ids
    /// refer KIP-15 for more details
    ///
    /// PR-9.5c: `accepted_tx_ids` widened to `TransactionId`
    /// (= `Hash64`); return type widened to `AcceptedIdMerkleRoot`
    /// (= `Hash64`). The branch combination uses the keyed
    /// BLAKE2b-512 `AcceptedIdMerkleBranchHash64` hasher (same
    /// domain as `merkle::calc_accepted_id_merkle_root_pre_crescendo`)
    /// so the post-Crescendo path and the pre-Crescendo path
    /// produce values from the same hash family.
    pub(super) fn calc_accepted_id_merkle_root(
        &self,
        accepted_tx_ids: impl ExactSizeIterator<Item = kaspa_consensus_core::TransactionId>,
        selected_parent: kaspa_consensus_core::BlockHash,
    ) -> kaspa_consensus_core::AcceptedIdMerkleRoot {
        use kaspa_hashes::{AcceptedIdMerkleBranchHash64, HasherBase};
        let parent_root = self.headers_store.get_header(selected_parent).unwrap().accepted_id_merkle_root;
        let leaves_root = kaspa_consensus_core::merkle::calc_accepted_id_merkle_root_pre_crescendo(accepted_tx_ids.collect());
        let mut hasher = AcceptedIdMerkleBranchHash64::new();
        hasher.update(parent_root.as_byte_slice()).update(leaves_root.as_byte_slice());
        hasher.finalize()
    }
}

/// kaspa-pq DNS-finality (E1/§6.1): why a [`StakeAttestationShard`] tx was dropped
/// from a block TEMPLATE. These reasons exist ONLY on the template-construction path
/// (counters + refill diagnostics); block VALIDITY keeps mapping the same condition
/// to the single [`IneligibleAttestationInBlock`] error (the wire/consensus rule is
/// unchanged). Each variant mirrors exactly one branch of [`classify_one_attestation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttestationDropReason {
    /// The referenced bond does not resolve to `Active` at the attestation's
    /// `target_daa_score` in the as-of-selected-parent bond view (branch a).
    BondNotActiveAtTarget,
    /// The self-declared `validator_id` does not match the bond's
    /// `validator_pubkey_hash` (P-1A binding, branch a').
    ValidatorIdMismatch,
    /// The ML-DSA-87 signature does not verify over the canonical digest (branch b).
    BadSignature,
    /// The tx payload is not a decodable `StakeAttestationShardPayload` (a shard-
    /// subnetwork tx whose bytes do not borsh-decode). Never produced by
    /// [`attestations_from_accepted_txs`] (which silently skips undecodable
    /// payloads), but classified explicitly so a malformed shard is dropped with a
    /// reason rather than silently kept as a `KeepEligible{count:0}`.
    MalformedPayload,
}

impl AttestationDropReason {
    /// kaspa-pq DNS-finality (audit v24 H-5): map a drop reason to the mempool-hygiene
    /// kind the mining manager acts on.
    ///
    /// - `MalformedPayload` / `ValidatorIdMismatch` / `BadSignature` are intrinsic to the
    ///   shard itself — it can never become eligible as-is → `Terminal` (evict at once).
    /// - `BondNotActiveAtTarget` depends on the template's selected-parent bond VIEW, which a
    ///   reorg / a few more blocks can change → `Quarantine` (do not hard-evict; let TTL govern).
    pub(crate) fn template_drop_kind(self) -> kaspa_consensus_core::block::AttestationTemplateDropKind {
        use kaspa_consensus_core::block::AttestationTemplateDropKind;
        match self {
            AttestationDropReason::MalformedPayload
            | AttestationDropReason::ValidatorIdMismatch
            | AttestationDropReason::BadSignature => AttestationTemplateDropKind::Terminal,
            AttestationDropReason::BondNotActiveAtTarget => AttestationTemplateDropKind::Quarantine,
        }
    }
}

/// kaspa-pq DNS-finality (E1/§6.1): the per-tx decision the block-template builder
/// makes for one selected mempool tx. `KeepNonShard` and `KeepEligible` are added to
/// the template; `Drop` is rejected back to the selector (which refills from the next
/// candidate) and counted by reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttestationShardDecision {
    /// Not a `StakeAttestationShard` tx (subnetwork ≠ 0x11) — always kept.
    KeepNonShard,
    /// A shard tx all of whose attestations are §B.4-eligible — kept.
    KeepEligible { attestation_count: usize },
    /// A shard tx with ≥1 ineligible (or malformed) attestation — dropped + refilled.
    Drop { reason: AttestationDropReason, bond: TransactionOutpoint, epoch: u64 },
}

/// kaspa-pq DNS-finality (E1/§6.1): classify ONE attestation against the bond view
/// using the IDENTICAL checks the §B.4 block-validity rule and the StakeScore path
/// use. `Ok(())` ⇒ eligible; `Err(reason)` ⇒ the first failing branch. The block-
/// validity rule ([`attestation_reward_eligibility`]) and the template classifier
/// ([`classify_attestation_shard_for_template`]) both funnel through here so the
/// eligibility logic can never diverge between construction and validation.
fn classify_one_attestation(
    att: &StakeAttestation,
    bond_view: &ActiveBondView,
    net_id: BlockHash,
) -> Result<(), AttestationDropReason> {
    // (a) bond resolves to Active OR Dormant at the attestation's anchor. Dormancy
    // Fence (design v0.1 §4.5, PR-D4): a Dormant validator's attestation is ACCEPTED
    // as a block-validity matter so a valid block may carry it and trigger revival;
    // it earns no credit/reward (the credit path keeps using the Active-only
    // `active_bond_at`). Byte-identical when the fence is inert (no bond is Dormant).
    let Some(bond) = bond_view.active_or_dormant_bond_at(&att.bond_outpoint, att.target_daa_score) else {
        return Err(AttestationDropReason::BondNotActiveAtTarget);
    };
    // kaspa-pq DNS v2 (P-1A): the self-declared validator_id must match the bond's
    // validator_pubkey_hash (validator_id is not in the signed digest); reward eligibility
    // shares the StakeScore binding so no reward can be earned by a non-canonical validator_id.
    if att.validator_id != bond.validator_pubkey_hash {
        return Err(AttestationDropReason::ValidatorIdMismatch);
    }
    // (b) ML-DSA-87 signature verifies over the canonical digest.
    let digest = stake_attestation_message(
        net_id.as_byte_slice(),
        att.epoch,
        att.target_hash,
        att.target_daa_score,
        att.validator_set_commitment,
        att.bond_outpoint,
    )
    .as_bytes();
    if !matches!(verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT), Ok(true)) {
        return Err(AttestationDropReason::BadSignature);
    }
    Ok(())
}

/// kaspa-pq DNS-finality (E1/§6.1): classify ONE selected mempool tx for the block
/// template. A non-shard tx is `KeepNonShard`. A shard tx is decoded and each of its
/// attestations run through [`classify_one_attestation`]: the first failure ⇒
/// `Drop{reason,bond,epoch}`; all eligible ⇒ `KeepEligible{attestation_count}`. A
/// shard-subnetwork tx whose payload does not decode ⇒ `Drop{MalformedPayload}`
/// (carrying the tx id as a degenerate bond outpoint + epoch 0). When `activated`
/// is `false` (overlay dormant / below `dns_activation_daa_score`) EVERY tx is kept
/// (`KeepNonShard`) — byte-identical to the pre-classifier behavior.
pub(crate) fn classify_attestation_shard_for_template(
    tx: &Transaction,
    bond_view: &ActiveBondView,
    net_id: BlockHash,
    activated: bool,
) -> AttestationShardDecision {
    if !activated || tx.subnetwork_id != SUBNETWORK_ID_STAKE_ATTESTATION_SHARD {
        return AttestationShardDecision::KeepNonShard;
    }
    // A shard-subnetwork tx whose payload does not decode is malformed. (The §B.4
    // validity rule reaches this via `attestations_from_accepted_txs` skipping it ⇒
    // it would carry no attestations and be vacuously eligible; the template path
    // is stricter and drops it so a near-empty template is not served.)
    let Some(shard) = decode_attestation_shard(tx) else {
        return AttestationShardDecision::Drop {
            reason: AttestationDropReason::MalformedPayload,
            bond: TransactionOutpoint::new(tx.id(), 0),
            epoch: 0,
        };
    };
    for att in shard.attestations.iter() {
        if let Err(reason) = classify_one_attestation(att, bond_view, net_id) {
            return AttestationShardDecision::Drop { reason, bond: att.bond_outpoint, epoch: att.epoch };
        }
    }
    AttestationShardDecision::KeepEligible { attestation_count: shard.attestations.len() }
}

/// Pure core of the ADR-0009 Addendum B §B.4 reward-eligibility rule, split
/// out from [`VirtualStateProcessor::check_attestation_reward_eligibility`] so
/// it can be unit-tested without a full processor. `activated` folds the
/// `dns_params.is_some() && daa_score >= dns_activation_daa_score` gate; when
/// `false` the rule is a no-op. On every current network the gate is `true`
/// from genesis (`dns_activation_daa_score` = 0), so the rule is active. On the
/// first ineligible attestation returns `Err((bond tx id, epoch))`; the caller maps
/// it to [`IneligibleAttestationInBlock`]. An attestation is eligible iff its
/// bond resolves to `Active` in `bond_view` at the attestation's
/// `target_daa_score` **and** its ML-DSA-87 signature verifies over the
/// canonical [`stake_attestation_message`] digest (Addendum A.3 layout).
///
/// Shares its per-attestation checks with the template classifier via
/// [`classify_one_attestation`], so block validity and template adoption can
/// never disagree on what is eligible (only the template path keeps reasons).
fn attestation_reward_eligibility(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    net_id: BlockHash,
    activated: bool,
) -> Result<(), (TransactionId, u64)> {
    if !activated {
        return Ok(());
    }
    for att in attestations_from_accepted_txs(txs) {
        if classify_one_attestation(&att, bond_view, net_id).is_err() {
            return Err((att.bond_outpoint.transaction_id, att.epoch));
        }
    }
    Ok(())
}

/// Pure core of the ADR-0009 §"SlashingEvidencePayload" stateful genuineness
/// rule (testable without a processor). `activated` folds the
/// `dns_params.is_some() && daa_score >= dns_activation_daa_score` gate; when
/// `false` the rule is a no-op. For each `SlashingEvidence` among `txs` (the
/// structural triple + incompatibility are already enforced by the §A.2
/// stateless tx check), requires that the referenced bond resolves in
/// `bond_view`, still has slashable locked stake at the including DAA, and that **both**
/// equivocating attestations ML-DSA-verify
/// against that bond's `validator_pubkey` over their canonical
/// [`stake_attestation_message`] digests. On the first failure returns
/// `Err(bond_tx_id)`; the caller maps it to
/// [`UnverifiableSlashingEvidenceInBlock`].
fn slashing_evidence_genuine(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    net_id: BlockHash,
    including_daa: u64,
    evidence_window_blocks: u64,
    activated: bool,
) -> Result<(), TransactionId> {
    if !activated {
        return Ok(());
    }
    for ev in slashing_evidence_from_accepted_txs(txs) {
        // The bond must exist so we can verify against its validator key.
        let Some(bond) = bond_view.get(&ev.bond_outpoint) else {
            return Err(ev.bond_outpoint.transaction_id);
        };
        // A later replay against an already-slashed (or otherwise non-slashable) row must not emit a
        // second registry mutation. This current-coordinate guard mirrors the slashing side-effect
        // resolver; same-mergeset duplicates still share the pre-block view and are deterministically
        // first-only deduplicated by `bond_mutations_from_accepted_txs`.
        if !matches!(effective_bond_status(bond, including_daa), BondStatus::Active | BondStatus::Unbonding) {
            return Err(ev.bond_outpoint.transaction_id);
        }
        // audit #2: freshness — the evidence must be included within `evidence_window_blocks` of the
        // newer equivocating attestation's target. This bounds how far back slashing can reach and
        // keeps it inside the bond's still-locked window (the params invariant `unbonding_period >=
        // max_reorg_horizon + evidence_window` guarantees a fresh evidence still finds the staked
        // output-0 present for the slashing side-effect to remove). A stale equivocation dredged up
        // long after the fact to grief a since-honest validator is rejected.
        let newest_target = ev.attestation_a.target_daa_score.max(ev.attestation_b.target_daa_score);
        if including_daa.saturating_sub(newest_target) > evidence_window_blocks {
            return Err(ev.bond_outpoint.transaction_id);
        }
        for att in [&ev.attestation_a, &ev.attestation_b] {
            // kaspa-pq DNS v2 (P-1A): both equivocating attestations must be bound to the bond's
            // validator_pubkey_hash (validator_id is not in the signed digest), so slashing can't be
            // spoofed against a bond via a mismatched validator_id.
            if att.validator_id != bond.validator_pubkey_hash {
                return Err(ev.bond_outpoint.transaction_id);
            }
            // audit #2: the bond must have had slashable locked stake at the attestation's target —
            // Active or Unbonding, the same set `resolve_slashing_side_effects` can slash. An
            // equivocation by a bond that was still Pending (never activated) or already Slashed at
            // that target had no stake at risk, so it is not slashable.
            if !matches!(effective_bond_status(bond, att.target_daa_score), BondStatus::Active | BondStatus::Unbonding) {
                return Err(ev.bond_outpoint.transaction_id);
            }
            let digest = stake_attestation_message(
                net_id.as_byte_slice(),
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
                return Err(ev.bond_outpoint.transaction_id);
            }
        }
    }
    Ok(())
}

/// Pure core of the ADR-0016 §D.2 bond-UTXO spend-gate (testable without a
/// processor). `activated` folds the `dns_params.is_some() && daa_score >=
/// dns_activation_daa_score` gate; when `false` the rule is a no-op (every
/// current network). Scans every input of every transaction (the coinbase has
/// no inputs, so it contributes nothing); if an input's `previous_outpoint` is
/// a **known** bond outpoint in `bond_view` whose bond is **not releasable** at
/// `daa_score`, returns `Err((spending tx id, bond outpoint))` for the caller
/// to map to [`NonReleasableBondSpendInBlock`]. "Releasable" = the bond is
/// `Unbonding` (per [`effective_bond_status`]) **and** `daa_score >=
/// bond_release_daa_score` (`unbond_request_daa_score +
/// unbonding_period_blocks`). Non-bond outpoints are ignored, so ordinary
/// transactions are unaffected.
fn bond_spend_gate(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    daa_score: u64,
    activated: bool,
) -> Result<(), (TransactionId, TransactionOutpoint)> {
    if !activated {
        return Ok(());
    }
    for tx in txs {
        for input in tx.inputs.iter() {
            if let Some(bond) = bond_view.get(&input.previous_outpoint) {
                let releasable = effective_bond_status(bond, daa_score) == BondStatus::Unbonding
                    && bond_release_daa_score(bond).is_some_and(|release| daa_score >= release);
                if !releasable {
                    return Err((tx.id(), input.previous_outpoint));
                }
            }
        }
    }
    Ok(())
}

/// kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): pure core of the stake-unbond
/// owner-authorization block-validity rule (testable without a processor).
/// `activated` folds the `dns_params.is_some() && daa_score >=
/// dns_activation_daa_score` gate; when `false` the rule is a no-op. For each
/// `StakeUnbondRequest` among `txs`, requires: (a) the referenced bond resolves
/// in `bond_view` and is `Pending`/`Active` at `daa_score` (not already
/// `Unbonding`/`Slashed`, so at most one unbond mutation applies per bond per
/// chain — keeping `ActiveBondView` apply/revert clean); (b) the payload's
/// `owner_pubkey` hashes to the bond's `owner_pubkey_hash`; and (c) its
/// ML-DSA-87 signature verifies over the canonical [`unbond_request_message`]
/// digest under [`UNBOND_REQUEST_CONTEXT`]. On the first failure returns
/// `Err((unbond tx id, bond outpoint))`, mapped to
/// [`UnauthorizedUnbondRequestInBlock`]. This is what stops an attacker forcing
/// honest bonds into `Unbonding` to grief them out of the active set.
fn unbond_request_authorized(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    net_id: &[u8],
    daa_score: u64,
    activated: bool,
) -> Result<(), (TransactionId, TransactionOutpoint)> {
    if !activated {
        return Ok(());
    }
    for (tx_id, req) in unbond_requests_from_accepted_txs(txs) {
        // (a) the bond must exist and still be locked-but-not-yet-exiting.
        let Some(bond) = bond_view.get(&req.bond_outpoint) else {
            return Err((tx_id, req.bond_outpoint));
        };
        if !matches!(effective_bond_status(bond, daa_score), BondStatus::Pending | BondStatus::Active) {
            return Err((tx_id, req.bond_outpoint));
        }
        // (b) the signing key must be THIS bond's owner.
        if validator_id_from_pubkey(&req.owner_pubkey) != bond.owner_pubkey_hash {
            return Err((tx_id, req.bond_outpoint));
        }
        // (c) the owner's signature over the network- and bond-bound digest must verify (audit M-04).
        let digest = unbond_request_message(net_id, req.bond_outpoint).as_bytes();
        if !matches!(verify_mldsa87_with_context(&req.owner_pubkey, &digest, &req.signature, UNBOND_REQUEST_CONTEXT), Ok(true)) {
            return Err((tx_id, req.bond_outpoint));
        }
    }
    Ok(())
}

/// Pure core of the ADR-0013 Addendum C / ADR-0016 §D.4 slashing side-effect,
/// split out of [`VirtualStateProcessor::apply_slashing_side_effects`] so the
/// remove-stake + mint-reporter UTXO/multiset mutation can be unit-tested
/// without a full processor. The caller has already gated on activation and
/// resolved `effects` (canonical block order) against the selected-parent bond
/// view; this applies them.
///
/// For each effect the bond's locked output-0 is looked up in
/// `selected_parent_utxo_view` composed with the running `diff`. If present it
/// is removed — `S` leaves the supply — from both `diff` and `multiset`, and
/// then, when the reward is non-zero, the reporter UTXO is minted at
/// `(slashing_tx_id, 0)` into both (the slashing tx declares no outputs, so
/// index 0 is free). Net supply change is `R − S`; the remainder is implicitly
/// burned. The per-effect recompose lets a later effect observe an earlier
/// one's mutations, and the lookup doubles as a release-race guard: a bond
/// whose output-0 is already gone from the composed view is skipped rather than
/// double-removed. `mint_daa_score` (the block's DAA score) is stamped as the
/// minted entry's `block_daa_score`.
fn apply_slashing_effects_to_state<V: UtxoView>(
    effects: &[SlashingSideEffect],
    selected_parent_utxo_view: &V,
    diff: &mut UtxoDiff,
    multiset: &mut MuHash,
    mint_daa_score: u64,
) {
    for effect in effects {
        // The exact stored entry for the bond's locked output-0 (matches the
        // multiset element); `None` ⇒ already spent in this mergeset ⇒ skip.
        let Some(entry) = ({
            let composed = selected_parent_utxo_view.compose(&*diff);
            composed.get(&effect.bond_outpoint)
        }) else {
            continue;
        };
        // Remove S (the locked stake) from the diff and the multiset.
        diff.remove_utxo(&effect.bond_outpoint, &entry).expect("composed view reported the bond output-0 present");
        multiset.remove_utxo(&effect.bond_outpoint, &entry);

        // Mint the reporter reward R at (slashing_tx_id, 0), if non-zero.
        if let Some(out) = &effect.reporter_output {
            let mint_outpoint = TransactionOutpoint::new(effect.slashing_tx_id, 0);
            let mint_entry = UtxoEntry::new(out.value, out.script_public_key.clone(), mint_daa_score, false);
            diff.add_utxo(mint_outpoint, mint_entry.clone()).expect("slashing tx declares no outputs, so (slashing_tx_id, 0) is free");
            multiset.add_utxo(&mint_outpoint, &mint_entry);
        }

        // kaspa-pq ADR-0018 "本格版" (PoS-v2): mint the victim-compensation outputs at
        // `(slashing_tx_id, 2..)` (index 1 is reserved/kept free). Empty while the v2 fence is closed —
        // i.e. on the devnet/simnet preset (fence = `u64::MAX`) ⇒ no extra mints ⇒ byte-identical to the
        // pre-v2 2-way slashing; on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0) these mint from
        // block 1. The security-reserve share
        // (`effect.security_reserve_sompi`) is NOT minted here: it leaves the supply with the bond
        // removal (≡ burn) until Phase 4 accrues it to the reserve pool. Σ(reporter + victim) ≤ S, so
        // the slash stays value-conserving.
        for (i, out) in effect.victim_outputs.iter().enumerate() {
            let mint_outpoint = TransactionOutpoint::new(effect.slashing_tx_id, 2 + i as u32);
            let mint_entry = UtxoEntry::new(out.value, out.script_public_key.clone(), mint_daa_score, false);
            diff.add_utxo(mint_outpoint, mint_entry.clone())
                .expect("slashing tx declares no outputs, so (slashing_tx_id, 2+i) is free");
            multiset.add_utxo(&mint_outpoint, &mint_entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::*;

    mod palw_da_acceptance {
        use super::*;
        use crate::model::stores::palw::{DbPalwStore, PalwStore};
        use kaspa_consensus_core::{
            palw::{
                PALW_LEAF_CHUNK_VERSION_V3, PALW_MAX_LEAVES_PER_CHUNK, PalwBatchLifecycleV1, PalwBatchStatus, PalwLeafChunkV1,
                PalwProofType, PalwPublicLeafV1, palw_leaf_merkle_proof, palw_leaf_merkle_root,
            },
            subnets::SUBNETWORK_ID_PALW_LEAF_CHUNK,
            tx::{ScriptPublicKey, ScriptVec},
        };
        use kaspa_database::{
            create_temp_db,
            prelude::{CachePolicy, ConnBuilder},
        };
        use kaspa_hashes::Hash64;
        use std::sync::Arc;

        const DAA: u64 = 1_000;

        fn h(byte: u8) -> Hash64 {
            Hash64::from_bytes([byte; 64])
        }

        fn chunk_and_lifecycle(tag: u8) -> (PalwLeafChunkV1, PalwBatchLifecycleV1) {
            let batch_id = h(tag);
            let reward_script = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51]));
            let leaf = PalwPublicLeafV1 {
                version: 1,
                batch_id,
                leaf_index: 0,
                job_nullifier: h(tag.wrapping_add(1)),
                ticket_nullifier_commitment: h(tag.wrapping_add(2)),
                model_profile_id: h(tag.wrapping_add(3)),
                runtime_class_id: h(tag.wrapping_add(4)),
                shape_id: 1,
                quantum_count: 1,
                proof_type: PalwProofType::ReplicaExactV1.as_u8(),
                provider_a_bond: TransactionOutpoint::new(h(tag.wrapping_add(5)), 0),
                provider_b_bond: TransactionOutpoint::new(h(tag.wrapping_add(6)), 0),
                provider_a_reward_script: reward_script.clone(),
                provider_b_reward_script: reward_script,
                ticket_authority_pk_hash: h(tag.wrapping_add(7)),
                private_match_commitment: h(tag.wrapping_add(8)),
                receipt_da_object_version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
                receipt_da_root: h(tag.wrapping_add(9)),
                receipt_da_object_len: 1,
                receipt_da_chunk_count: 1,
                receipt_v3_compute_set_id: h(tag.wrapping_add(10)),
                receipt_v3_job_challenge: h(tag.wrapping_add(11)),
                receipt_v3_issued_epoch: 1,
                receipt_v3_expires_epoch: 2,
                registered_epoch: 1,
                activation_epoch: 2,
                expiry_epoch: 3,
                leaf_bond_sompi: 1,
                a_commit: Hash64::default(),
                a_commit_epoch: 0,
                provider_snapshot_root: Hash64::from_bytes([0x7c; 64]),
                assignment_proof_root: Hash64::from_bytes([0x7d; 64]),
                dispatch_kind: 0, // BeaconAssigned (external sentinels)
            };
            let mut projected = leaf.clone();
            projected.batch_id = Hash64::default();
            let hashes = [projected.leaf_hash()];
            let leaf_root = palw_leaf_merkle_root(&hashes);
            let proof = palw_leaf_merkle_proof(&hashes, 0).unwrap();
            let chunk = PalwLeafChunkV1 {
                version: PALW_LEAF_CHUNK_VERSION_V3,
                batch_id,
                chunk_index: 0,
                leaves: vec![leaf],
                proofs: vec![proof],
                witnesses: Vec::new(), // D3-b arity: filled per-test when the acceptance arm's clauses 11/12 are the subject
            };
            let lifecycle = PalwBatchLifecycleV1 {
                status: PalwBatchStatus::Registering,
                registration_epoch: 1,
                activation_not_before_epoch: 2,
                expiry_epoch: 3,
                leaf_count: 1,
                chunk_count: 1,
                chunks_present: [0; 4],
                leaf_root,
                cert_hash: None,
                cert_activation_epoch: 0,
                cert_expiry_epoch: 0,
                cert_approving_stake: 0,
                first_cert_daa: None,
                revoked_from_daa: None,
            };
            (chunk, lifecycle)
        }

        /// ADR-0045 D3-b — the two leaf-chunk gates must admit the SAME payload version.
        ///
        /// D3-b moved the chunk to v3 and made context-free validation reject anything else, but
        /// `palw_v4_leaf_chunk_matches_canonical_span` stayed pinned at v2. Mutually exclusive
        /// conditions on the same payload brick the lane in the quietest possible way: the carrier
        /// is refused before the PCPB arm runs, nothing logs, and the batch simply ages out of
        /// Registering with chunks 0/N. Pin the agreement rather than the constant, so a later
        /// version bump has to move both or fail here.
        #[test]
        fn leaf_chunk_span_predicate_admits_exactly_the_context_free_version() {
            let (mut chunk, lifecycle) = chunk_and_lifecycle(0x11);
            assert!(
                palw_v4_leaf_chunk_matches_canonical_span(&chunk, &lifecycle, PALW_MAX_LEAVES_PER_CHUNK as u16, true),
                "the span predicate must admit the version context-free validation requires"
            );
            // Bug report #6: below the v3 flag day NOTHING is admissible — the pre-fix binaries
            // skipped every chunk, and matching them below the fence is what re-unifies a
            // partitioned network instead of forking it at each node's upgrade moment.
            assert!(
                !palw_v4_leaf_chunk_matches_canonical_span(&chunk, &lifecycle, PALW_MAX_LEAVES_PER_CHUNK as u16, false),
                "below the flag day even the canonical v3 chunk must be skipped"
            );
            for stale in [kaspa_consensus_core::palw::PALW_LEAF_CHUNK_VERSION_V2, PALW_LEAF_CHUNK_VERSION_V3 + 1] {
                chunk.version = stale;
                assert!(
                    !palw_v4_leaf_chunk_matches_canonical_span(&chunk, &lifecycle, PALW_MAX_LEAVES_PER_CHUNK as u16, true),
                    "version {stale} is not the context-free version and must be refused"
                );
            }
        }

        fn gate<'a>(
            parent: &PalwDaStateV1,
            lifecycle: PalwBatchViewV1,
            store: &'a DbPalwStore,
            providers: &'a ProviderBondView,
        ) -> PalwDaAcceptanceGate<'a> {
            PalwDaAcceptanceGate::new(
                parent,
                lifecycle,
                store,
                Some(PalwBuriedBeaconV1 {
                    epoch: 1,
                    seed: h(0xe1),
                    anchor_hash: h(0xe2),
                    anchor_daa_score: DAA - PalwDaPolicyV1::STRICT_TESTNET.min_beacon_burial_daa,
                    observed_daa_score: DAA,
                }),
                providers,
                1,
                DAA,
                100,
                PALW_MAX_LEAVES_PER_CHUNK as u16,
                0, // v3 flag day passed: these tests exercise the gate's post-fence behavior
            )
        }

        fn two_chunk_fixture(tag: u8) -> ([PalwLeafChunkV1; 2], PalwBatchLifecycleV1) {
            let batch_id = h(tag);
            let reward_script = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51]));
            let leaves = (0..=PALW_MAX_LEAVES_PER_CHUNK as u32)
                .map(|leaf_index| PalwPublicLeafV1 {
                    version: 1,
                    batch_id,
                    leaf_index,
                    job_nullifier: h(tag.wrapping_add(leaf_index as u8).wrapping_add(1)),
                    ticket_nullifier_commitment: h(tag.wrapping_add(leaf_index as u8).wrapping_add(70)),
                    model_profile_id: h(tag.wrapping_add(3)),
                    runtime_class_id: h(tag.wrapping_add(4)),
                    shape_id: 1,
                    quantum_count: 1,
                    proof_type: PalwProofType::ReplicaExactV1.as_u8(),
                    provider_a_bond: TransactionOutpoint::new(h(tag.wrapping_add(5)), 0),
                    provider_b_bond: TransactionOutpoint::new(h(tag.wrapping_add(6)), 0),
                    provider_a_reward_script: reward_script.clone(),
                    provider_b_reward_script: reward_script.clone(),
                    ticket_authority_pk_hash: h(tag.wrapping_add(7)),
                    private_match_commitment: h(tag.wrapping_add(8)),
                    receipt_da_object_version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
                    receipt_da_root: h(tag.wrapping_add(leaf_index as u8).wrapping_add(140)),
                    receipt_da_object_len: 1,
                    receipt_da_chunk_count: 1,
                    receipt_v3_compute_set_id: h(tag.wrapping_add(10)),
                    receipt_v3_job_challenge: h(tag.wrapping_add(11)),
                    receipt_v3_issued_epoch: 1,
                    receipt_v3_expires_epoch: 2,
                    registered_epoch: 1,
                    activation_epoch: 2,
                    expiry_epoch: 3,
                    leaf_bond_sompi: 1,
                    a_commit: Hash64::default(),
                    a_commit_epoch: 0,
                    provider_snapshot_root: Hash64::from_bytes([0x7c; 64]),
                    assignment_proof_root: Hash64::from_bytes([0x7d; 64]),
                    dispatch_kind: 0, // BeaconAssigned (external sentinels)
                })
                .collect_vec();
            let projected_hashes = leaves
                .iter()
                .map(|leaf| {
                    let mut projected = leaf.clone();
                    projected.batch_id = Hash64::default();
                    projected.leaf_hash()
                })
                .collect_vec();
            let leaf_root = palw_leaf_merkle_root(&projected_hashes);
            let make_chunk = |chunk_index: u16, start: usize, end: usize| PalwLeafChunkV1 {
                version: PALW_LEAF_CHUNK_VERSION_V3,
                batch_id,
                chunk_index,
                leaves: leaves[start..end].to_vec(),
                proofs: (start..end)
                    .map(|index| palw_leaf_merkle_proof(&projected_hashes, index as u32).expect("fixture index is in range"))
                    .collect(),
                witnesses: Vec::new(), // D3-b arity: filled per-test when the acceptance arm's clauses 11/12 are the subject
            };
            let chunks =
                [make_chunk(0, 0, PALW_MAX_LEAVES_PER_CHUNK), make_chunk(1, PALW_MAX_LEAVES_PER_CHUNK, PALW_MAX_LEAVES_PER_CHUNK + 1)];
            let lifecycle = PalwBatchLifecycleV1 {
                status: PalwBatchStatus::Registering,
                registration_epoch: 1,
                activation_not_before_epoch: 2,
                expiry_epoch: 3,
                leaf_count: leaves.len() as u32,
                chunk_count: 2,
                chunks_present: [0; 4],
                leaf_root,
                cert_hash: None,
                cert_activation_epoch: 0,
                cert_expiry_epoch: 0,
                cert_approving_stake: 0,
                first_cert_daa: None,
                revoked_from_daa: None,
            };
            (chunks, lifecycle)
        }

        #[test]
        fn full_obligation_and_snapshot_byte_seams_skip_the_leaf_atomically() {
            let (chunk, lifecycle) = chunk_and_lifecycle(0x10);
            let mut view = PalwBatchViewV1::new();
            view.batches.insert(chunk.batch_id, lifecycle);
            let parent = PalwDaStateV1::default();
            let providers = ProviderBondView::new();
            let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let store = DbPalwStore::new(db, CachePolicy::Count(32));
            let leaf_tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_LEAF_CHUNK, 0, borsh::to_vec(&chunk).unwrap());

            let mut cardinality_full = gate(&parent, view.clone(), &store, &providers);
            cardinality_full.max_obligations = cardinality_full.state.obligations.len();
            let before = cardinality_full.state.clone();
            assert!(!cardinality_full.allows(&leaf_tx));
            assert_eq!(cardinality_full.state, before, "a full obligation set must skip without a partial reservation");

            let mut probe = gate(&parent, view.clone(), &store, &providers);
            assert!(probe.allows_leaf_chunk(&chunk));
            let exact_accepted_bytes = probe.state.canonical_snapshot_encoded_len().unwrap();

            let mut exact = gate(&parent, view.clone(), &store, &providers);
            exact.max_snapshot_bytes = exact_accepted_bytes;
            assert!(exact.allows_leaf_chunk(&chunk), "the exact canonical byte boundary is inclusive");

            let mut one_byte_short = gate(&parent, view, &store, &providers);
            one_byte_short.max_snapshot_bytes = exact_accepted_bytes - 1;
            let before = one_byte_short.state.clone();
            assert!(!one_byte_short.allows_leaf_chunk(&chunk));
            assert_eq!(one_byte_short.state, before, "snapshot-byte rejection must be atomic");
        }

        #[test]
        fn canonical_order_and_reorg_rebuild_choose_the_same_capacity_winner() {
            let (chunk_a, lifecycle_a) = chunk_and_lifecycle(0x20);
            let (chunk_b, lifecycle_b) = chunk_and_lifecycle(0x40);
            let mut view = PalwBatchViewV1::new();
            view.batches.insert(chunk_a.batch_id, lifecycle_a);
            view.batches.insert(chunk_b.batch_id, lifecycle_b);
            let parent = PalwDaStateV1::default();
            let providers = ProviderBondView::new();
            let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let store = DbPalwStore::new(db, CachePolicy::Count(32));

            let walk = |chunks: [&PalwLeafChunkV1; 2]| {
                let mut gate = gate(&parent, view.clone(), &store, &providers);
                // One leaf creates exactly two provider obligations under STRICT_TESTNET.
                gate.max_obligations = 2;
                let decisions = chunks.map(|chunk| gate.allows_leaf_chunk(chunk));
                (decisions, gate.state.state_root())
            };
            let first = walk([&chunk_a, &chunk_b]);
            let rebuilt_after_reorg = walk([&chunk_a, &chunk_b]);
            assert_eq!(first, rebuilt_after_reorg, "rebuilding from the same parent and canonical order must be path-independent");
            assert_eq!(first.0, [true, false]);

            let reverse = walk([&chunk_b, &chunk_a]);
            assert_eq!(reverse.0, [true, false]);
            assert_ne!(first.1, reverse.1, "capacity selection is order-sensitive, so only consensus order may drive the gate");
        }

        #[test]
        fn lifecycle_and_immutable_slot_rejections_consume_zero_da_capacity() {
            let (chunk, lifecycle) = chunk_and_lifecycle(0x60);
            let parent = PalwDaStateV1::default();
            let providers = ProviderBondView::new();
            let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let store = DbPalwStore::new(db, CachePolicy::Count(32));

            for status in [
                PalwBatchStatus::Missing,
                PalwBatchStatus::Committed,
                PalwBatchStatus::Auditing,
                PalwBatchStatus::Certified,
                PalwBatchStatus::Active,
                PalwBatchStatus::Slashed,
                PalwBatchStatus::Expired,
                PalwBatchStatus::Revoked,
            ] {
                let mut non_registering_view = PalwBatchViewV1::new();
                non_registering_view.batches.insert(chunk.batch_id, PalwBatchLifecycleV1 { status, ..lifecycle.clone() });
                let mut non_registering = gate(&parent, non_registering_view, &store, &providers);
                let before = non_registering.state.clone();
                assert!(!non_registering.allows_leaf_chunk(&chunk), "status {status:?} must reject a leaf chunk");
                assert_eq!(non_registering.state, before, "status {status:?} must reserve no obligations or snapshot bytes");
            }

            let mut conflict = chunk.leaves[0].clone();
            conflict.job_nullifier = h(0xf1);
            store.insert_leaf(chunk.batch_id, conflict.leaf_index, Arc::new(conflict)).unwrap();
            let mut registering_view = PalwBatchViewV1::new();
            registering_view.batches.insert(chunk.batch_id, lifecycle);
            let mut conflicting = gate(&parent, registering_view, &store, &providers);
            let before_conflict = conflicting.state.clone();
            assert!(!conflicting.allows_leaf_chunk(&chunk));
            assert_eq!(conflicting.state, before_conflict, "an immutable-slot conflict must reserve no DA capacity");

            let (valid_chunk, valid_lifecycle) = chunk_and_lifecycle(0x70);
            let mut valid_view = PalwBatchViewV1::new();
            valid_view.batches.insert(valid_chunk.batch_id, valid_lifecycle);
            let mut valid = gate(&parent, valid_view, &store, &providers);
            assert!(valid.allows_leaf_chunk(&valid_chunk), "a valid registering chunk with free immutable slots must still pass");
            assert!(!valid.state.obligations.is_empty());
        }

        #[test]
        fn canonical_leaf_span_is_required_before_a_chunk_can_set_its_lifecycle_bit() {
            let ([first, second], lifecycle) = two_chunk_fixture(0x90);
            let mut parent_view = PalwBatchViewV1::new();
            parent_view.batches.insert(first.batch_id, lifecycle.clone());
            let parent = PalwDaStateV1::default();
            let providers = ProviderBondView::new();
            let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let store = DbPalwStore::new(db, CachePolicy::Count(256));

            let mut partial = first.clone();
            partial.leaves.pop();
            partial.proofs.pop();
            let mut partial_gate = gate(&parent, parent_view.clone(), &store, &providers);
            let before_partial = partial_gate.state.clone();
            assert!(!partial_gate.allows_leaf_chunk(&partial), "a proper subset must not claim the canonical chunk bit");
            assert_eq!(partial_gate.state, before_partial, "a partial chunk must reserve no DA capacity");
            assert!(partial_gate.accepted_chunks.is_empty());

            let mut wrong_index = second.clone();
            wrong_index.chunk_index = 0;
            let mut wrong_index_gate = gate(&parent, parent_view.clone(), &store, &providers);
            let before_wrong_index = wrong_index_gate.state.clone();
            assert!(
                !wrong_index_gate.allows_leaf_chunk(&wrong_index),
                "the valid final leaf/proof cannot be replayed under chunk index zero"
            );
            assert_eq!(wrong_index_gate.state, before_wrong_index);
            assert!(wrong_index_gate.accepted_chunks.is_empty());

            // Model the exact same-set order used by the accepted-effect resolver: only a gate-approved
            // chunk is allowed to apply its bitmap transition. Relabelling the already-valid first span
            // as chunk 1 must not complete the batch; the real final span may do so afterwards.
            let mut same_set_gate = gate(&parent, parent_view, &store, &providers);
            let mut accepted_view = PalwBatchViewV1::new();
            accepted_view.batches.insert(first.batch_id, lifecycle);
            assert!(same_set_gate.allows_leaf_chunk(&first));
            assert!(accepted_view.apply_leaf_chunk(&first.batch_id, first.chunk_index));
            assert_eq!(accepted_view.entry(&first.batch_id).unwrap().status, PalwBatchStatus::Registering);

            let mut relabelled_first = first.clone();
            relabelled_first.chunk_index = 1;
            let after_first = same_set_gate.state.clone();
            assert!(
                !same_set_gate.allows_leaf_chunk(&relabelled_first),
                "the same valid leaves/proofs cannot claim a different chunk bit in one acceptance set"
            );
            assert_eq!(same_set_gate.state, after_first, "relabel rejection must preserve the first reservation exactly");
            assert_eq!(same_set_gate.accepted_chunks.len(), 1);
            assert_eq!(accepted_view.entry(&first.batch_id).unwrap().status, PalwBatchStatus::Registering);

            assert!(same_set_gate.allows_leaf_chunk(&second));
            assert!(accepted_view.apply_leaf_chunk(&second.batch_id, second.chunk_index));
            assert_eq!(accepted_view.entry(&first.batch_id).unwrap().status, PalwBatchStatus::Committed);
            assert_eq!(same_set_gate.accepted_chunks.len(), 2);
        }
    }

    #[test]
    fn da_reward_gate_withholds_payout_for_an_unresolved_provider_obligation() {
        use kaspa_consensus_core::palw::da::{PalwDaObligationStatusV1, PalwDaObligationV1};
        let h = |byte| kaspa_hashes::Hash64::from_bytes([byte; 64]);
        let provider_a = TransactionOutpoint::new(h(1), 0);
        let provider_b = TransactionOutpoint::new(h(2), 0);
        let obligation_id = h(3);
        let mut state = PalwDaStateV1::default();
        state.obligations.insert(
            obligation_id,
            PalwDaObligationV1 {
                version: 1,
                obligation_id,
                batch_id: h(4),
                leaf_index: 0,
                leaf_hash: h(5),
                object_root: h(6),
                object_len: 1,
                chunk_count: 1,
                chunk_index: 0,
                provider_bond: provider_a,
                beacon_epoch: 1,
                beacon_anchor: h(7),
                created_daa_score: 10,
                retention_until_daa_score: 100,
                status: PalwDaObligationStatusV1::Pending,
            },
        );
        assert!(!palw_da_reward_pair_allowed(&state, &provider_a, &provider_b, 50));
        assert!(palw_da_reward_pair_allowed(&state, &provider_a, &provider_b, 101));
        state.obligations.get_mut(&obligation_id).unwrap().status = PalwDaObligationStatusV1::TimedOut(h(8));
        assert!(!palw_da_reward_pair_allowed(&state, &provider_a, &provider_b, u64::MAX));
    }

    #[test]
    fn test_rayon_reduce_retains_order() {
        // this is an independent test to replicate the behavior of
        // validate_txs_in_parallel and validate_txs_with_muhash_in_parallel
        // and assert that the order of data is retained when doing par_iter
        let data: Vec<u16> = (1..=1000).collect();

        let collected: Vec<u16> = data
            .par_iter()
            .filter_map(|a| {
                let chance: f64 = rand::random();
                if chance < 0.05 {
                    return None;
                }
                Some(*a)
            })
            .collect();

        println!("collected len: {}", collected.len());

        collected.iter().tuple_windows().for_each(|(prev, curr)| {
            // Data was originally sorted, so we check if they remain sorted after filtering
            assert!(prev < curr, "expected {} < {} if original sort was preserved", prev, curr);
        });

        let reduced: SmallVec<[u16; 2]> = data
            .par_iter()
            .filter_map(|a: &u16| {
                let chance: f64 = rand::random();
                if chance < 0.05 {
                    return None;
                }
                Some(smallvec![*a])
            })
            .reduce(
                || smallvec![],
                |mut arr, mut curr_data| {
                    arr.append(&mut curr_data);
                    arr
                },
            );

        println!("reduced len: {}", reduced.len());

        reduced.iter().tuple_windows().for_each(|(prev, curr)| {
            // Data was originally sorted, so we check if they remain sorted after filtering
            assert!(prev < curr, "expected {} < {} if original sort was preserved", prev, curr);
        });
    }

    // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): the reward-eligibility
    // rule's pure core. Covers the gate + both reject branches (bond absent /
    // signature invalid). The accept-with-valid-signature path requires
    // ML-DSA-87 signing (libcrux) and is covered by the PR-10.5′-b3 end-to-end
    // integration test rather than here.
    mod attestation_reward_eligibility {
        use super::super::attestation_reward_eligibility as eligibility;
        use kaspa_consensus_core::{
            BlockHash,
            dns_finality::{
                ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN,
                StakeAttestation, StakeBondRecord, single_attestation_shard, stake_attestation_shard_tx,
            },
            tx::TransactionOutpoint,
        };
        use kaspa_hashes::Hash64;

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn attestation(bond_outpoint: TransactionOutpoint) -> StakeAttestation {
            StakeAttestation {
                version: DNS_PAYLOAD_VERSION_V1,
                validator_id: Hash64::from_bytes([0xa1; 64]),
                bond_outpoint,
                epoch: 1,
                target_hash: Hash64::from_bytes([0x55; 64]),
                target_daa_score: 10_000,
                validator_set_commitment: Hash64::from_bytes([0x66; 64]),
                // Garbage signature — never verifies. The accept path is tested
                // end-to-end in b3 (a real validator-signed attestation).
                signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
            }
        }

        fn active_bond(op: TransactionOutpoint) -> StakeBondRecord {
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0, // Active from genesis.
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
                last_attested_epoch: None,
                dormant_at_daa_score: None,
                dormant_at_epoch: None,
            }
        }

        const NET: fn() -> BlockHash = || Hash64::from_bytes([0x07; 64]);

        #[test]
        fn noop_when_not_activated() {
            // Even an attestation referencing an unknown bond passes when the
            // gate is closed (the `false` arg below). Current nets run with the
            // gate open (dns_activation = 0); this covers the pre-activation path.
            let tx = stake_attestation_shard_tx(&single_attestation_shard(attestation(outpoint(1))));
            assert_eq!(eligibility(&[tx], &ActiveBondView::new(), NET(), false), Ok(()));
        }

        #[test]
        fn rejects_attestation_with_unknown_bond() {
            // Activated + empty bond view ⇒ the bond does not resolve ⇒ reject.
            let tx = stake_attestation_shard_tx(&single_attestation_shard(attestation(outpoint(1))));
            assert_eq!(eligibility(&[tx], &ActiveBondView::new(), NET(), true), Err((Hash64::from_bytes([1; 64]), 1)));
        }

        #[test]
        fn rejects_attestation_with_invalid_signature() {
            // Activated + bond present & Active, but the (garbage) signature
            // fails verification ⇒ reject at branch (b).
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, active_bond(op))]);
            let tx = stake_attestation_shard_tx(&single_attestation_shard(attestation(op)));
            assert_eq!(eligibility(&[tx], &view, NET(), true), Err((Hash64::from_bytes([2; 64]), 1)));
        }

        #[test]
        fn ok_when_no_attestation_shards() {
            // Activated but no shard txs ⇒ nothing to check ⇒ Ok.
            assert_eq!(eligibility(&[], &ActiveBondView::new(), NET(), true), Ok(()));
        }
    }

    // kaspa-pq DNS-finality (E1/§6.1, tests T0–T2): the reason-returning TEMPLATE
    // classifier `classify_attestation_shard_for_template`. Shares its per-attestation
    // checks with the §B.4 validity rule (`classify_one_attestation`), so these also
    // pin that the two never diverge. The KeepEligible accept path uses a real
    // ML-DSA-87 signature (libcrux), so a kept shard provably also passes §B.4.
    mod classify_attestation_shard_for_template {
        use super::super::{AttestationDropReason, AttestationShardDecision, classify_attestation_shard_for_template as classify};
        use kaspa_consensus_core::{
            BlockHash,
            constants::TX_VERSION,
            dns_finality::{
                ATTESTATION_MLDSA87_CONTEXT, ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_ATTESTATION_SIG_LEN,
                STAKE_VALIDATOR_PUBKEY_LEN, StakeAttestation, StakeBondRecord, single_attestation_shard, stake_attestation_message,
                stake_attestation_shard_tx, validator_id_from_pubkey,
            },
            subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD},
            tx::{Transaction, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        const NET: fn() -> BlockHash = || Hash64::from_bytes([0x07; 64]);

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        // A bond whose validator key is `kp`, Active from genesis. `validator_pubkey_hash`
        // binds the pubkey so the P-1A id check passes for a matching `validator_id`.
        fn active_bond_with_key(op: TransactionOutpoint, kp: &mldsa::MLDSA87KeyPair) -> StakeBondRecord {
            let validator_pubkey = kp.verification_key.as_ref().to_vec();
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                validator_pubkey_hash: validator_id_from_pubkey(&validator_pubkey),
                validator_pubkey,
                amount: 1_000,
                activation_daa_score: 0, // Active from genesis.
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
                last_attested_epoch: None,
                dormant_at_daa_score: None,
                dormant_at_epoch: None,
            }
        }

        // Sign an attestation over the canonical digest with `kp`; `validator_id` is
        // supplied so the P-1A binding can be exercised independently of the signature.
        fn signed_attestation(
            kp: &mldsa::MLDSA87KeyPair,
            validator_id: Hash64,
            bond_outpoint: TransactionOutpoint,
            epoch: u64,
            target_daa_score: u64,
        ) -> StakeAttestation {
            let target_hash = Hash64::from_bytes([0x55; 64]);
            let vsc = Hash64::from_bytes([0x66; 64]);
            let digest = stake_attestation_message(NET().as_byte_slice(), epoch, target_hash, target_daa_score, vsc, bond_outpoint);
            let sig = mldsa::sign(&kp.signing_key, digest.as_bytes().as_slice(), ATTESTATION_MLDSA87_CONTEXT, [0x55u8; 32])
                .expect("ml-dsa-87 sign");
            StakeAttestation {
                version: DNS_PAYLOAD_VERSION_V1,
                validator_id,
                bond_outpoint,
                epoch,
                target_hash,
                target_daa_score,
                validator_set_commitment: vsc,
                signature: sig.as_ref().to_vec(),
            }
        }

        #[test]
        fn keep_non_shard_when_not_activated() {
            // Gate closed ⇒ even an ineligible shard is `KeepNonShard` (byte-identical
            // to the pre-classifier behavior, which kept everything).
            let kp = mldsa::generate_key_pair([1u8; 32]);
            let att = signed_attestation(&kp, validator_id_from_pubkey(kp.verification_key.as_ref()), outpoint(1), 1, 10_000);
            let tx = stake_attestation_shard_tx(&single_attestation_shard(att));
            assert_eq!(classify(&tx, &ActiveBondView::new(), NET(), false), AttestationShardDecision::KeepNonShard);
        }

        #[test]
        fn keep_non_shard_for_native_tx() {
            // A non-shard (native) tx is always `KeepNonShard`, gate open or not.
            let tx = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
            assert_eq!(classify(&tx, &ActiveBondView::new(), NET(), true), AttestationShardDecision::KeepNonShard);
        }

        // T0: active bond + valid sig + correct id ⇒ KeepEligible.
        #[test]
        fn t0_keep_eligible_for_active_bond_valid_sig_correct_id() {
            let kp = mldsa::generate_key_pair([2u8; 32]);
            let op = outpoint(2);
            let bond = active_bond_with_key(op, &kp);
            let view = ActiveBondView::from_records([(op, bond.clone())]);
            // Correct id (matches the bond's validator_pubkey_hash) + a real signature.
            let att = signed_attestation(&kp, bond.validator_pubkey_hash, op, 1, 10_000);
            let tx = stake_attestation_shard_tx(&single_attestation_shard(att));
            assert_eq!(classify(&tx, &view, NET(), true), AttestationShardDecision::KeepEligible { attestation_count: 1 });
        }

        // T1: inactive-bond-at-target ⇒ Drop(BondNotActiveAtTarget). The bond activates
        // AFTER the attestation's target, so `active_bond_at(target)` resolves to None.
        #[test]
        fn t1_drop_bond_not_active_at_target() {
            let kp = mldsa::generate_key_pair([3u8; 32]);
            let op = outpoint(3);
            let mut bond = active_bond_with_key(op, &kp);
            bond.activation_daa_score = 20_000; // not yet active at target 10_000
            bond.status = BondStatus::Pending;
            let view = ActiveBondView::from_records([(op, bond.clone())]);
            let att = signed_attestation(&kp, bond.validator_pubkey_hash, op, 1, 10_000);
            let tx = stake_attestation_shard_tx(&single_attestation_shard(att));
            assert_eq!(
                classify(&tx, &view, NET(), true),
                AttestationShardDecision::Drop { reason: AttestationDropReason::BondNotActiveAtTarget, bond: op, epoch: 1 }
            );
        }

        // T2: validator_id mismatch ⇒ Drop(ValidatorIdMismatch). The bond is Active and
        // the signature is valid, but the self-declared validator_id is wrong.
        #[test]
        fn t2_drop_validator_id_mismatch() {
            let kp = mldsa::generate_key_pair([4u8; 32]);
            let op = outpoint(4);
            let bond = active_bond_with_key(op, &kp);
            let view = ActiveBondView::from_records([(op, bond)]);
            let wrong_id = Hash64::from_bytes([0xff; 64]);
            let att = signed_attestation(&kp, wrong_id, op, 1, 10_000);
            let tx = stake_attestation_shard_tx(&single_attestation_shard(att));
            assert_eq!(
                classify(&tx, &view, NET(), true),
                AttestationShardDecision::Drop { reason: AttestationDropReason::ValidatorIdMismatch, bond: op, epoch: 1 }
            );
        }

        // Bad signature ⇒ Drop(BadSignature): active bond + correct id, garbage sig.
        #[test]
        fn drop_bad_signature() {
            let kp = mldsa::generate_key_pair([5u8; 32]);
            let op = outpoint(5);
            let bond = active_bond_with_key(op, &kp);
            let view = ActiveBondView::from_records([(op, bond.clone())]);
            let mut att = signed_attestation(&kp, bond.validator_pubkey_hash, op, 1, 10_000);
            att.signature = vec![0u8; STAKE_ATTESTATION_SIG_LEN]; // garbage — never verifies
            let tx = stake_attestation_shard_tx(&single_attestation_shard(att));
            assert_eq!(
                classify(&tx, &view, NET(), true),
                AttestationShardDecision::Drop { reason: AttestationDropReason::BadSignature, bond: op, epoch: 1 }
            );
        }

        // Malformed payload ⇒ Drop(MalformedPayload): a shard-subnetwork tx whose payload
        // is not a decodable StakeAttestationShardPayload.
        #[test]
        fn drop_malformed_payload() {
            let tx = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, 0, vec![0xde, 0xad]);
            match classify(&tx, &ActiveBondView::new(), NET(), true) {
                AttestationShardDecision::Drop { reason: AttestationDropReason::MalformedPayload, epoch, .. } => {
                    assert_eq!(epoch, 0);
                }
                other => panic!("expected Drop(MalformedPayload), got {other:?}"),
            }
        }

        // kaspa-pq audit v24 (H-5): the drop-reason → mempool-hygiene-kind mapping the mining
        // manager acts on. Intrinsic-to-the-shard reasons are TERMINAL (evict at once); the
        // view-dependent reason is QUARANTINE (do not hard-evict — a reorg may make it eligible).
        #[test]
        fn h5_drop_reason_template_kind_mapping() {
            use kaspa_consensus_core::block::AttestationTemplateDropKind;
            assert_eq!(AttestationDropReason::MalformedPayload.template_drop_kind(), AttestationTemplateDropKind::Terminal);
            assert_eq!(AttestationDropReason::ValidatorIdMismatch.template_drop_kind(), AttestationTemplateDropKind::Terminal);
            assert_eq!(AttestationDropReason::BadSignature.template_drop_kind(), AttestationTemplateDropKind::Terminal);
            assert_eq!(AttestationDropReason::BondNotActiveAtTarget.template_drop_kind(), AttestationTemplateDropKind::Quarantine);
        }

        // A garbage 64-byte pubkey in the bond cannot pass the id check anyway, so this
        // guards that an absurd key length does not panic (defensive).
        #[test]
        fn drop_when_bond_has_short_pubkey() {
            let kp = mldsa::generate_key_pair([6u8; 32]);
            let op = outpoint(6);
            let mut bond = active_bond_with_key(op, &kp);
            bond.validator_pubkey = vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN]; // not kp's key
            let view = ActiveBondView::from_records([(op, bond.clone())]);
            let att = signed_attestation(&kp, bond.validator_pubkey_hash, op, 1, 10_000);
            let tx = stake_attestation_shard_tx(&single_attestation_shard(att));
            // id matches (we used bond.validator_pubkey_hash), but the sig won't verify
            // against the mismatched bond key ⇒ BadSignature.
            assert_eq!(
                classify(&tx, &view, NET(), true),
                AttestationShardDecision::Drop { reason: AttestationDropReason::BadSignature, bond: op, epoch: 1 }
            );
        }
    }

    // kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): the stake-unbond owner-
    // authorization rule. Unlike the mods above, this exercises the full ACCEPT
    // path with a real ML-DSA-87 owner signature (libcrux) — owner-authorization
    // is THE security property (it blocks the active-set grief attack).
    mod unbond_request_authorized {
        use super::super::unbond_request_authorized as authz;
        use kaspa_consensus_core::{
            constants::TX_VERSION,
            dns_finality::{
                ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN,
                StakeBondRecord, StakeUnbondRequestPayload, UNBOND_REQUEST_CONTEXT, unbond_request_message, validator_id_from_pubkey,
            },
            subnets::SUBNETWORK_ID_STAKE_UNBOND,
            tx::{Transaction, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        // audit M-04: the signer and verifier must agree on the network id bound into the digest.
        const NET_ID: &[u8] = b"audit-m04-unbond-test-net";

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn owner_kp(seed: u8) -> mldsa::MLDSA87KeyPair {
            mldsa::generate_key_pair([seed; 32])
        }

        // A bond whose `owner_pubkey_hash` binds to `kp`, Active from genesis.
        fn bond_owned_by(op: TransactionOutpoint, kp: &mldsa::MLDSA87KeyPair) -> StakeBondRecord {
            let owner_pubkey = kp.verification_key.as_ref().to_vec();
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: validator_id_from_pubkey(&owner_pubkey),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
                last_attested_epoch: None,
                dormant_at_daa_score: None,
                dormant_at_epoch: None,
            }
        }

        fn unbond_tx(op: TransactionOutpoint, owner_pubkey: Vec<u8>, signature: Vec<u8>) -> Transaction {
            let payload = borsh::to_vec(&StakeUnbondRequestPayload {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey,
                signature,
            })
            .unwrap();
            Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_UNBOND, 0, payload)
        }

        fn signed_unbond_tx(op: TransactionOutpoint, kp: &mldsa::MLDSA87KeyPair) -> Transaction {
            let digest = unbond_request_message(NET_ID, op).as_bytes();
            let sig = mldsa::sign(&kp.signing_key, &digest, UNBOND_REQUEST_CONTEXT, [0x99u8; 32]).expect("sign");
            unbond_tx(op, kp.verification_key.as_ref().to_vec(), sig.as_ref().to_vec())
        }

        #[test]
        fn noop_when_not_activated() {
            let op = outpoint(1);
            let tx = unbond_tx(op, vec![0u8; STAKE_VALIDATOR_PUBKEY_LEN], vec![0u8; STAKE_ATTESTATION_SIG_LEN]);
            assert_eq!(authz(&[tx], &ActiveBondView::new(), NET_ID, 10_000, false), Ok(()));
        }

        #[test]
        fn accepts_owner_authorized_request() {
            let op = outpoint(2);
            let kp = owner_kp(2);
            let view = ActiveBondView::from_records([(op, bond_owned_by(op, &kp))]);
            assert_eq!(authz(&[signed_unbond_tx(op, &kp)], &view, NET_ID, 10_000, true), Ok(()));
        }

        #[test]
        fn rejects_unknown_bond() {
            let op = outpoint(3);
            let kp = owner_kp(3);
            assert!(authz(&[signed_unbond_tx(op, &kp)], &ActiveBondView::new(), NET_ID, 10_000, true).is_err());
        }

        #[test]
        fn rejects_request_signed_by_non_owner() {
            // The grief attack: a bond owned by `owner`, request signed by `attacker` → blocked.
            let op = outpoint(4);
            let owner = owner_kp(4);
            let attacker = owner_kp(40);
            let view = ActiveBondView::from_records([(op, bond_owned_by(op, &owner))]);
            assert!(authz(&[signed_unbond_tx(op, &attacker)], &view, NET_ID, 10_000, true).is_err());
        }

        #[test]
        fn rejects_bad_signature() {
            let op = outpoint(5);
            let kp = owner_kp(5);
            let view = ActiveBondView::from_records([(op, bond_owned_by(op, &kp))]);
            let tx = unbond_tx(op, kp.verification_key.as_ref().to_vec(), vec![0u8; STAKE_ATTESTATION_SIG_LEN]);
            assert!(authz(&[tx], &view, NET_ID, 10_000, true).is_err());
        }

        #[test]
        fn rejects_already_unbonding_bond() {
            // at-most-once: a bond already Unbonding cannot be unbonded again (clean revert).
            let op = outpoint(6);
            let kp = owner_kp(6);
            let mut rec = bond_owned_by(op, &kp);
            rec.unbond_request_daa_score = Some(1);
            let view = ActiveBondView::from_records([(op, rec)]);
            assert!(authz(&[signed_unbond_tx(op, &kp)], &view, NET_ID, 10_000, true).is_err());
        }
    }

    // kaspa-pq Phase 10/11 (ADR-0009 §"SlashingEvidencePayload" / item 2): the
    // stateful slashing-evidence genuineness rule's pure core. Covers the gate +
    // both reject branches (bond absent / signature invalid). The
    // accept-with-valid-signatures path needs ML-DSA-87 signing (libcrux) and is
    // covered by the dedicated reward-bearing e2e rather than here.
    mod slashing_evidence_genuine {
        use super::super::slashing_evidence_genuine as genuine;
        use kaspa_consensus_core::{
            BlockHash,
            constants::TX_VERSION,
            dns_finality::{
                ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN,
                SlashingEvidencePayload, StakeAttestation, StakeBondRecord,
            },
            subnets::SUBNETWORK_ID_SLASHING_EVIDENCE,
            tx::{Transaction, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn attestation(bond_outpoint: TransactionOutpoint, target: u8) -> StakeAttestation {
            StakeAttestation {
                version: DNS_PAYLOAD_VERSION_V1,
                validator_id: Hash64::from_bytes([0xa1; 64]),
                bond_outpoint,
                epoch: 1,
                target_hash: Hash64::from_bytes([target; 64]),
                target_daa_score: 10_000,
                validator_set_commitment: Hash64::from_bytes([0x66; 64]),
                signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN], // garbage — never verifies
            }
        }

        fn active_bond(op: TransactionOutpoint) -> StakeBondRecord {
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
                last_attested_epoch: None,
                dormant_at_daa_score: None,
                dormant_at_epoch: None,
            }
        }

        // Two incompatible attestations for the same bond (equivocation).
        fn evidence_tx(op: TransactionOutpoint) -> Transaction {
            let ev = SlashingEvidencePayload {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                attestation_a: attestation(op, 0x55),
                attestation_b: attestation(op, 0x99),
                reporter_reward_spk_payload: [0xee; 64],
            };
            Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_SLASHING_EVIDENCE, 0, borsh::to_vec(&ev).unwrap())
        }

        const NET: fn() -> BlockHash = || Hash64::from_bytes([0x07; 64]);

        // The fixture attestations target DAA 10_000; a block at FRESH_DAA with a WINDOW-block
        // evidence window is well within the freshness bound.
        const FRESH_DAA: u64 = 10_000;
        const WINDOW: u64 = 200_000;

        #[test]
        fn noop_when_not_activated() {
            // Forged evidence passes when the gate is closed (every current net).
            assert_eq!(genuine(&[evidence_tx(outpoint(1))], &ActiveBondView::new(), NET(), FRESH_DAA, WINDOW, false), Ok(()));
        }

        #[test]
        fn rejects_evidence_with_unknown_bond() {
            // Activated + empty bond view ⇒ bond unknown ⇒ reject.
            assert_eq!(
                genuine(&[evidence_tx(outpoint(1))], &ActiveBondView::new(), NET(), FRESH_DAA, WINDOW, true),
                Err(Hash64::from_bytes([1; 64]))
            );
        }

        #[test]
        fn rejects_evidence_with_invalid_signatures() {
            // Activated + bond present + fresh, but the (garbage) attestation signatures fail
            // verification ⇒ a forged evidence cannot slash the bond.
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, active_bond(op))]);
            assert_eq!(genuine(&[evidence_tx(op)], &view, NET(), FRESH_DAA, WINDOW, true), Err(Hash64::from_bytes([2; 64])));
        }

        #[test]
        fn rejects_replayed_evidence_for_an_already_slashed_bond() {
            let op = outpoint(3);
            let mut bond = active_bond(op);
            bond.slashed_at_daa_score = Some(FRESH_DAA - 1);
            bond.status = BondStatus::Slashed;
            let view = ActiveBondView::from_records([(op, bond)]);
            assert_eq!(genuine(&[evidence_tx(op)], &view, NET(), FRESH_DAA, WINDOW, true), Err(Hash64::from_bytes([3; 64])));
        }

        #[test]
        fn rejects_stale_evidence_outside_the_window() {
            // audit #2: bond present, but the including block is more than `evidence_window_blocks`
            // past the equivocating attestations' target ⇒ stale ⇒ reject.
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, active_bond(op))]);
            let stale_daa = 10_000 + WINDOW + 1; // target=10_000; diff = WINDOW+1 > WINDOW.
            assert_eq!(genuine(&[evidence_tx(op)], &view, NET(), stale_daa, WINDOW, true), Err(Hash64::from_bytes([2; 64])));
        }

        #[test]
        fn rejects_evidence_when_bond_not_slashable_at_target() {
            // audit #2: bond present + fresh + validator_id matches, but the bond was still Pending
            // (activation after the target) ⇒ no stake at risk at that target ⇒ not slashable.
            let op = outpoint(2);
            let mut bond = active_bond(op);
            bond.validator_pubkey_hash = Hash64::from_bytes([0xa1; 64]); // match the attestation's validator_id
            bond.activation_daa_score = 50_000; // Pending at target 10_000
            let view = ActiveBondView::from_records([(op, bond)]);
            assert_eq!(genuine(&[evidence_tx(op)], &view, NET(), FRESH_DAA, WINDOW, true), Err(Hash64::from_bytes([2; 64])));
        }

        #[test]
        fn ok_when_no_slashing_evidence() {
            assert_eq!(genuine(&[], &ActiveBondView::new(), NET(), FRESH_DAA, WINDOW, true), Ok(()));
        }
    }

    // kaspa-pq Phase 10/11 (ADR-0016 §D.2): the bond-UTXO spend-gate's pure
    // core. Covers the gate plus each releasability branch: Active/Pending/
    // mid-unbonding/Slashed bonds are locked (reject), a released bond and a
    // non-bond input are spendable (accept).
    mod bond_spend_gate {
        use super::super::bond_spend_gate as gate;
        use kaspa_consensus_core::{
            constants::TX_VERSION,
            dns_finality::{ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_VALIDATOR_PUBKEY_LEN, StakeBondRecord},
            subnets::SUBNETWORK_ID_NATIVE,
            tx::{Transaction, TransactionInput, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        // A normal (non-overlay) tx with a single input spending `op`.
        fn spending_tx(op: TransactionOutpoint) -> Transaction {
            let input = TransactionInput::new(op, vec![], 0, 0);
            Transaction::new(TX_VERSION, vec![input], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![])
        }

        // A bond record with all DAA-stamped fields cleared (so its effective
        // status is derived purely from `activation_daa_score`). The caller
        // tweaks the fields to select Pending/Active/Unbonding/Slashed.
        fn bond(op: TransactionOutpoint) -> StakeBondRecord {
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbonding_period_blocks: 5_000,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
                last_attested_epoch: None,
                dormant_at_daa_score: None,
                dormant_at_epoch: None,
            }
        }

        const DAA: u64 = 10_000;

        #[test]
        fn noop_when_not_activated() {
            // Spending an Active bond is fine while the gate is closed (the
            // gate-closed path; current nets run with it open, dns_activation = 0).
            let op = outpoint(1);
            let view = ActiveBondView::from_records([(op, bond(op))]);
            assert_eq!(gate(&[spending_tx(op)], &view, DAA, false), Ok(()));
        }

        #[test]
        fn rejects_spend_of_active_bond() {
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, bond(op))]); // activation 0 ⇒ Active at DAA.
            let tx = spending_tx(op);
            assert_eq!(gate(std::slice::from_ref(&tx), &view, DAA, true), Err((tx.id(), op)));
        }

        #[test]
        fn rejects_spend_of_pending_bond() {
            let op = outpoint(3);
            let mut b = bond(op);
            b.activation_daa_score = DAA + 1; // not yet active ⇒ Pending.
            let view = ActiveBondView::from_records([(op, b)]);
            let tx = spending_tx(op);
            assert_eq!(gate(std::slice::from_ref(&tx), &view, DAA, true), Err((tx.id(), op)));
        }

        #[test]
        fn rejects_spend_of_unbonding_before_release() {
            let op = outpoint(4);
            let mut b = bond(op);
            b.unbond_request_daa_score = Some(DAA - 1); // Unbonding, but release = DAA-1+5000 > DAA.
            let view = ActiveBondView::from_records([(op, b)]);
            let tx = spending_tx(op);
            assert_eq!(gate(std::slice::from_ref(&tx), &view, DAA, true), Err((tx.id(), op)));
        }

        #[test]
        fn allows_spend_of_releasable_bond() {
            let op = outpoint(5);
            let mut b = bond(op);
            b.unbond_request_daa_score = Some(1_000); // release = 1_000 + 5_000 = 6_000 ≤ DAA.
            let view = ActiveBondView::from_records([(op, b)]);
            assert_eq!(gate(&[spending_tx(op)], &view, DAA, true), Ok(()));
        }

        #[test]
        fn rejects_spend_of_slashed_bond() {
            let op = outpoint(6);
            let mut b = bond(op);
            b.slashed_at_daa_score = Some(5_000); // Slashed ⇒ terminal, never releasable.
            let view = ActiveBondView::from_records([(op, b)]);
            let tx = spending_tx(op);
            assert_eq!(gate(std::slice::from_ref(&tx), &view, DAA, true), Err((tx.id(), op)));
        }

        #[test]
        fn ignores_non_bond_inputs() {
            // An input that is not a known bond outpoint is unaffected, even
            // when the gate is active.
            assert_eq!(gate(&[spending_tx(outpoint(7))], &ActiveBondView::new(), DAA, true), Ok(()));
        }

        #[test]
        fn ok_when_no_inputs() {
            let tx = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
            assert_eq!(gate(&[tx], &ActiveBondView::new(), DAA, true), Ok(()));
        }
    }

    // kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): the `BondSpendFilter::locks`
    // predicate that drives the acceptance-time SKIP (the merge-blue-aware complement to the legacy
    // own-body `bond_spend_gate`). Same releasability semantics, exercised per-outpoint.
    mod bond_spend_filter {
        use super::super::BondSpendFilter;
        use kaspa_consensus_core::{
            dns_finality::{ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_VALIDATOR_PUBKEY_LEN, StakeBondRecord},
            tx::TransactionOutpoint,
        };
        use kaspa_hashes::Hash64;

        const DAA: u64 = 10_000;

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn bond(op: TransactionOutpoint) -> StakeBondRecord {
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbonding_period_blocks: 5_000,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
                last_attested_epoch: None,
                dormant_at_daa_score: None,
                dormant_at_epoch: None,
            }
        }

        #[test]
        fn locks_active_and_pending_unlocks_releasable_and_non_bond() {
            let active = outpoint(1);
            let pending_op = outpoint(2);
            let mut pending = bond(pending_op);
            pending.activation_daa_score = DAA + 1; // Pending (not yet active).
            let releasable_op = outpoint(3);
            let mut releasable = bond(releasable_op);
            releasable.unbond_request_daa_score = Some(1_000); // release = 1_000 + 5_000 = 6_000 ≤ DAA.

            let view = ActiveBondView::from_records([(active, bond(active)), (pending_op, pending), (releasable_op, releasable)]);
            let filter = BondSpendFilter { bond_view: &view, daa_score: DAA };

            assert!(filter.locks(&active), "Active bond's output-0 must be locked (skip the spend)");
            assert!(filter.locks(&pending_op), "Pending bond's output-0 must be locked");
            assert!(!filter.locks(&releasable_op), "a releasable (Unbonding past release) bond is spendable");
            assert!(!filter.locks(&outpoint(9)), "a non-bond outpoint is never locked");
        }
    }

    // kaspa-pq ADR-0040 ECON-03 leg 4: the provider-bond SPEND gate (`ProviderBondSpendFilter`) — the
    // acceptance-time SKIP that makes a provider bond ACTUALLY collateral. A bonded output-0 may leave
    // the UTXO set ONLY when `is_provider_bond_releasable_at` (Unbonding AND past its clamped release
    // DAA) AND the selected-parent DA state permits exit. Three axes are exercised:
    //   1. `locks` per-outpoint over each releasability branch (direct, like `bond_spend_filter`);
    //   2. the unbond/new-obligation race remains locked after the registry delay;
    //   3. the MERGE-BLUE coverage the design calls non-negotiable — the post-acceptance view is built
    //      the production way (`palw_provider_bond_mutations_from_accepted_txs`, Insert-only), and the
    //      per-tx acceptance skip is modelled by `accept`, exactly as `validate_transaction_in_utxo_
    //      context` returning `Err` makes the mergeset `filter_map` drop a tx with NO block-level
    //      error. Because the SAME filter is passed to every merged block (selected parent AND every
    //      merge-blue block; utxo_validation.rs loop), a spend riding in a merge-blue block is on the
    //      gated path — the exact full-mergeset walk that already carries `BondSpendFilter`, so the
    //      historical own-body-only defect (memory: bond-spend-gate-mergeset-bypass) is avoided by
    //      construction.
    mod provider_bond_spend_filter {
        use super::super::ProviderBondSpendFilter;
        use kaspa_consensus_core::{
            constants::TX_VERSION,
            palw::{
                PALW_PAYLOAD_VERSION_V1, PalwProviderBondMutation, PalwProviderBondPayloadV1, PalwProviderBondRecord,
                ProviderBondView,
                da::{PalwDaObligationStatusV1, PalwDaObligationV1, PalwDaStateV1},
                palw_provider_bond_mutations_from_accepted_txs,
            },
            subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_PROVIDER_BOND},
            tx::{Transaction, TransactionInput, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;

        const EPOCH_LEN: u64 = 100;
        const DELAY_EPOCHS: u64 = 5; // release = unbond_request + 5*100 = +500 DAA.
        const DAA: u64 = 10_000; // the point of view for every assertion below.
        const MIN_BOND: u64 = 1_000;
        const UNBOND_FLOOR: u64 = 0; // no clamp in these fixtures (the clamp is proven in leg-5 tests).

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        // An Active provider-bond record (no unbond, no slash) at `op`, worth MIN_BOND.
        fn bond(op: TransactionOutpoint) -> PalwProviderBondRecord {
            PalwProviderBondRecord {
                version: PALW_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                owner_public_key: vec![],
                operator_group_id: Hash64::from_bytes([0x01; 64]),
                runtime_classes: vec![],
                capacity_by_shape: vec![],
                reward_key_root: Hash64::from_bytes([0x04; 64]),
                amount_sompi: MIN_BOND,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbond_delay_epochs: DELAY_EPOCHS,
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
            }
        }

        fn filter<'a>(view: &'a ProviderBondView, da_state: &'a PalwDaStateV1) -> ProviderBondSpendFilter<'a> {
            ProviderBondSpendFilter { provider_bond_view: view, da_state, epoch_length_daa: EPOCH_LEN, daa_score: DAA }
        }

        // A native tx with a single input spending `op` (models an owner reclaiming their bonded coins).
        fn spending_tx(op: TransactionOutpoint) -> Transaction {
            Transaction::new(TX_VERSION, vec![TransactionInput::new(op, vec![], 0, 0)], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![])
        }

        // A `0x30` ProviderBond tx declaring a bond worth `amount`; returns (bond outpoint, tx).
        fn bond_tx(seed: u8, amount: u64) -> (TransactionOutpoint, Transaction) {
            let payload = PalwProviderBondPayloadV1 {
                version: PALW_PAYLOAD_VERSION_V1,
                owner_public_key: vec![seed; 64],
                operator_group_id: Hash64::from_bytes([seed; 64]),
                runtime_classes: vec![],
                capacity_by_shape: vec![],
                reward_key_root: Hash64::from_bytes([0x04; 64]),
                amount_sompi: amount,
                unbond_delay_epochs: DELAY_EPOCHS,
            };
            let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_PROVIDER_BOND, 0, borsh::to_vec(&payload).unwrap());
            (TransactionOutpoint::new(tx.id(), 0), tx)
        }

        // The POST-ACCEPTANCE view built EXACTLY as `calculate_utxo_state` builds it: the
        // selected-parent records PLUS every provider-bond Insert declared anywhere in the mergeset,
        // Insert-only. If production's construction changes, this build diverges and the merge-blue
        // test notices.
        fn post_acceptance_view(
            seed: impl IntoIterator<Item = (TransactionOutpoint, PalwProviderBondRecord)>,
            mergeset_txs: &[Transaction],
        ) -> ProviderBondView {
            let mut view = ProviderBondView::from_records(seed);
            let inserts: Vec<PalwProviderBondMutation> =
                palw_provider_bond_mutations_from_accepted_txs(mergeset_txs, DAA, MIN_BOND, UNBOND_FLOOR)
                    .into_iter()
                    .filter(|m| matches!(m, PalwProviderBondMutation::Insert(..)))
                    .collect();
            view.apply(&inserts);
            view
        }

        // The acceptance-time skip modelled exactly: keep the txs no input of which is locked — as
        // `validate_transaction_in_utxo_context` returning `Err(SpendsNonReleasableProviderBond)` makes
        // the mergeset `filter_map` drop the tx, with NO block-level error.
        fn accept(txs: &[Transaction], f: ProviderBondSpendFilter) -> Vec<Transaction> {
            txs.iter().filter(|tx| !tx.inputs.iter().any(|i| f.locks(&i.previous_outpoint))).cloned().collect()
        }

        /// TEST 1 — `locks` over every releasability branch, per the required table: a bond with no
        /// unbond request (Active) is LOCKED, a bond mid-delay (Unbonding, release not yet elapsed) is
        /// LOCKED, a Pending bond is LOCKED, a Slashed bond is LOCKED, a released bond (Unbonding past
        /// its clamped release) is SPENDABLE, and a non-bond outpoint is never locked.
        #[test]
        fn locks_every_non_releasable_branch_and_unlocks_the_released() {
            // Active — before any unbond request.
            let active = outpoint(1);

            // Pending — activation in the future.
            let pending_op = outpoint(2);
            let mut pending = bond(pending_op);
            pending.activation_daa_score = DAA + 1;

            // Unbonding but mid-delay: request at 9_800 ⇒ release = 9_800 + 500 = 10_300 > DAA.
            let mid_delay_op = outpoint(3);
            let mut mid_delay = bond(mid_delay_op);
            mid_delay.unbond_request_daa_score = Some(9_800);

            // Slashed — terminal, never releasable.
            let slashed_op = outpoint(4);
            let mut slashed = bond(slashed_op);
            slashed.unbond_request_daa_score = Some(1_000); // even with an elapsed request...
            slashed.slashed_at_daa_score = Some(5_000); // ...slash dominates ⇒ not Unbonding ⇒ locked.

            // Released — request at 1_000 ⇒ release = 1_500 ≤ DAA.
            let released_op = outpoint(5);
            let mut released = bond(released_op);
            released.unbond_request_daa_score = Some(1_000);

            let view = ProviderBondView::from_records([
                (active, bond(active)),
                (pending_op, pending),
                (mid_delay_op, mid_delay),
                (slashed_op, slashed),
                (released_op, released),
            ]);
            let da_state = PalwDaStateV1::default();
            let f = filter(&view, &da_state);

            assert!(f.locks(&active), "Active bond's output-0 must be locked — no exit requested");
            assert!(f.locks(&pending_op), "Pending bond's output-0 must be locked");
            assert!(f.locks(&mid_delay_op), "Unbonding-but-mid-delay bond's output-0 must be locked");
            assert!(f.locks(&slashed_op), "Slashed bond's output-0 must be locked");
            assert!(!f.locks(&released_op), "a released (Unbonding past clamped release) bond is spendable");
            assert!(!f.locks(&outpoint(9)), "a non-bond outpoint is never locked");
        }

        /// TEST 2 — spending a bonded output-0 BEFORE any unbond request is REJECTED (skipped), while a
        /// spend of an unrelated non-bond outpoint in the SAME block is accepted. The gate touches only
        /// the locked collateral.
        #[test]
        fn spend_before_unbond_is_skipped_non_bond_untouched() {
            let (bond_op, decl) = bond_tx(0x11, MIN_BOND);
            let view = post_acceptance_view([], &[decl]); // bond declared in the mergeset ⇒ Active, non-releasable.
            let da_state = PalwDaStateV1::default();
            let f = filter(&view, &da_state);

            let spend_bond = spending_tx(bond_op);
            let spend_other = spending_tx(outpoint(0x99));
            let accepted = accept(&[spend_bond, spend_other.clone()], f);
            assert_eq!(accepted, vec![spend_other], "the bond spend is skipped; the non-bond spend survives");
        }

        /// TEST 3 — spending DURING the delay is REJECTED, and spending AFTER the clamped release is
        /// ACCEPTED. Same bond, two points of view expressed as two unbond-request stamps.
        #[test]
        fn spend_during_delay_rejected_after_release_accepted() {
            let bond_op = outpoint(0x21);

            // Mid-delay: request at 9_800 ⇒ release 10_300 > DAA ⇒ locked ⇒ spend skipped.
            let mut mid = bond(bond_op);
            mid.unbond_request_daa_score = Some(9_800);
            let mid_view = ProviderBondView::from_records([(bond_op, mid)]);
            let da_state = PalwDaStateV1::default();
            assert!(accept(&[spending_tx(bond_op)], filter(&mid_view, &da_state)).is_empty(), "a spend mid-delay is skipped");

            // Released: request at 1_000 ⇒ release 1_500 ≤ DAA ⇒ spendable ⇒ spend accepted.
            let mut done = bond(bond_op);
            done.unbond_request_daa_score = Some(1_000);
            let done_view = ProviderBondView::from_records([(bond_op, done)]);
            assert_eq!(
                accept(&[spending_tx(bond_op)], filter(&done_view, &da_state)).len(),
                1,
                "a spend past the clamped release is accepted"
            );
        }

        /// Regression for an unbond/new-leaf race: unbond authorization is selected-parent-relative,
        /// so an Active provider can request unbond in the same mergeset in which a new leaf creates
        /// its DA obligation. Once the registry delay elapses, the spend gate must still consult the
        /// inherited DA state; registry releasability alone would let the collateral leave while the
        /// newer retention obligation remains live.
        #[test]
        fn released_registry_bond_stays_locked_while_a_da_obligation_is_live() {
            let bond_op = outpoint(0x22);
            let mut released = bond(bond_op);
            released.unbond_request_daa_score = Some(1_000);
            let view = ProviderBondView::from_records([(bond_op, released)]);

            let obligation_id = Hash64::from_bytes([0x91; 64]);
            let mut da_state = PalwDaStateV1::default();
            da_state.obligations.insert(
                obligation_id,
                PalwDaObligationV1 {
                    version: 1,
                    obligation_id,
                    batch_id: Hash64::from_bytes([0x92; 64]),
                    leaf_index: 0,
                    leaf_hash: Hash64::from_bytes([0x93; 64]),
                    object_root: Hash64::from_bytes([0x94; 64]),
                    object_len: 1,
                    chunk_count: 1,
                    chunk_index: 0,
                    provider_bond: bond_op,
                    beacon_epoch: 1,
                    beacon_anchor: Hash64::from_bytes([0x95; 64]),
                    created_daa_score: DAA - 1,
                    retention_until_daa_score: DAA + 100,
                    status: PalwDaObligationStatusV1::Pending,
                },
            );

            assert!(filter(&view, &da_state).locks(&bond_op), "live DA retention must override an elapsed registry release");
            assert!(
                accept(&[spending_tx(bond_op)], filter(&view, &da_state)).is_empty(),
                "the collateral spend is skipped while its same-mergeset obligation is live"
            );

            da_state.obligations.get_mut(&obligation_id).unwrap().retention_until_daa_score = DAA - 1;
            assert!(!filter(&view, &da_state).locks(&bond_op), "the spend opens only after both registry and DA exit gates pass");
        }

        /// TEST 4 — THE COVERAGE REQUIREMENT: a spend riding in a MERGE-BLUE transaction is REJECTED,
        /// not only the chain block's own body. The bond is declared in the selected parent (seed), and
        /// the spend rides in a NON-selected-parent (merge-blue) block. Because `calculate_utxo_state`
        /// passes the SAME `ProviderBondSpendFilter` to every merged block on the acceptance walk, the
        /// merge-blue block's spend is gated — modelled here by running `accept` (the per-tx skip) over
        /// the merge-blue block's own tx list with that one filter. This is the exact full-mergeset walk
        /// `BondSpendFilter` rides, so the historical own-body-only bypass cannot recur.
        #[test]
        fn a_merge_blue_spend_is_rejected() {
            let (bond_op, decl) = bond_tx(0x31, MIN_BOND);
            // The bond lives in the selected-parent registry (an ancestor accepted `decl`); the current
            // block's mergeset declares no new bonds.
            let view = post_acceptance_view(
                palw_provider_bond_mutations_from_accepted_txs(&[decl], DAA, MIN_BOND, UNBOND_FLOOR).into_iter().filter_map(
                    |m| match m {
                        PalwProviderBondMutation::Insert(op, rec) => Some((op, rec)),
                        _ => None,
                    },
                ),
                &[],
            );
            let da_state = PalwDaStateV1::default();
            let f = filter(&view, &da_state);

            // The merge-blue block's body (NOT the selected parent's) carries the spend.
            let merge_blue_body = [spending_tx(bond_op)];
            assert!(f.locks(&bond_op), "the still-Active bond is locked at this point of view");
            assert!(accept(&merge_blue_body, f).is_empty(), "a merge-blue tx spending the non-releasable bond is skipped");
        }
    }

    // kaspa-pq ADR-0040 ECON-03 leg 5: the provider-unbond AUTHORIZATION skip (`ProviderUnbondAuthFilter`)
    // — the merge-blue-DoS-safe replacement for the removed block-level `PalwProviderUnbondUnauthorized`
    // gate. These pin the CONSEQUENCE of a verdict at the acceptance coordinate the pipeline actually
    // uses. `accept` keeps only the txs the filter does not reject — exactly as
    // `validate_transaction_in_utxo_context` returning `Err` makes the mergeset `filter_map` drop a tx,
    // with NO block-level error to raise (that is the whole point of moving the check here) — and only
    // those SURVIVORS drive `palw_provider_bond_mutations_from_accepted_txs`, the same registry producer
    // `calculate_utxo_state_relatively` applies to the provider-bond view. So a skipped `0x37` mutates
    // nothing while its carrying/merging block stays valid.
    mod provider_unbond_auth_filter {
        use super::super::ProviderUnbondAuthFilter;
        use kaspa_consensus_core::{
            palw::{
                PALW_PAYLOAD_VERSION_V1, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT, PalwProviderBondPayloadV1, PalwProviderBondStatus,
                PalwProviderUnbondRequestV1, ProviderBondView,
                da::{PalwDaObligationStatusV1, PalwDaObligationV1, PalwDaStateV1},
                effective_provider_bond_status, palw_provider_bond_mutations_from_accepted_txs, provider_bond_release_daa_score,
            },
            subnets::{SUBNETWORK_ID_PALW_PROVIDER_BOND, SUBNETWORK_ID_PALW_PROVIDER_UNBOND},
            tx::{Transaction, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        const NET: u32 = 7;
        const BOND_DAA: u64 = 500; // bonds accepted here ⇒ Active from 500 onward.
        const POV: u64 = 600; // the unbond point of view; every bond is Active here.
        const MIN_BOND: u64 = 1_000;
        const UNBOND_FLOOR: u64 = 20; // the network floor. Declared delay (10) is BELOW it, so the
        const DECLARED_DELAY: u64 = 10; // release stamp must be CLAMPED up to the floor, not the declared value.
        const EPOCH_LEN: u64 = 100;

        fn h(b: u8) -> Hash64 {
            Hash64::from_bytes([b; 64])
        }

        // A distinct bonded provider: its owner keypair, its bond outpoint, and the `0x30` bond tx that
        // registered it (so callers can compose a multi-bond selected-parent view).
        fn make_bond(seed: u8, group: u8) -> (mldsa::MLDSA87KeyPair, TransactionOutpoint, Transaction) {
            let owner = mldsa::generate_key_pair([seed; 32]);
            let payload = PalwProviderBondPayloadV1 {
                version: PALW_PAYLOAD_VERSION_V1,
                owner_public_key: owner.verification_key.as_ref().to_vec(),
                operator_group_id: h(group),
                runtime_classes: vec![h(2)],
                capacity_by_shape: vec![(1, 10)],
                reward_key_root: h(4),
                amount_sompi: MIN_BOND,
                unbond_delay_epochs: DECLARED_DELAY,
            };
            let bond_tx =
                Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_PROVIDER_BOND, 0, borsh::to_vec(&payload).unwrap());
            let outpoint = TransactionOutpoint::new(bond_tx.id(), 0);
            (owner, outpoint, bond_tx)
        }

        // The selected-parent provider-bond registry seeded from a set of accepted bond txs.
        fn view_of(bond_txs: &[Transaction]) -> ProviderBondView {
            let mut view = ProviderBondView::new();
            view.apply(&palw_provider_bond_mutations_from_accepted_txs(bond_txs, BOND_DAA, MIN_BOND, UNBOND_FLOOR));
            view
        }

        // A `0x37` provider-unbond tx for `outpoint`, signed by `signer` while CLAIMING `claimed_pk` as
        // the owner key (so a test can vary the signer and the claim independently).
        fn unbond_tx(signer: &mldsa::MLDSA87KeyPair, claimed_pk: Vec<u8>, outpoint: TransactionOutpoint) -> Transaction {
            let mut req = PalwProviderUnbondRequestV1 {
                version: PALW_PAYLOAD_VERSION_V1,
                bond_outpoint: outpoint,
                owner_public_key: claimed_pk,
                signature: vec![],
            };
            let d = req.signing_hash(NET);
            req.signature =
                mldsa::sign(&signer.signing_key, d.as_bytes().as_slice(), PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT, [0x5a; 32])
                    .expect("sign")
                    .as_ref()
                    .to_vec();
            Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_PROVIDER_UNBOND, 0, borsh::to_vec(&req).unwrap())
        }

        // The acceptance-time skip modelled exactly: keep the txs the filter does NOT reject.
        fn accept(txs: &[Transaction], filter: ProviderUnbondAuthFilter) -> Vec<Transaction> {
            txs.iter().filter(|tx| filter.unauthorized(tx).is_none()).cloned().collect()
        }

        fn filter_for<'a>(view: &'a ProviderBondView, da_state: &'a PalwDaStateV1) -> ProviderUnbondAuthFilter<'a> {
            ProviderUnbondAuthFilter { provider_bond_view: view, da_state, network_id: NET, daa_score: POV }
        }

        /// TEST 1 — an unauthorized `0x37` riding in a merge-blue transaction does NOT invalidate the
        /// merging block, and the named bond's record is UNCHANGED (still Active, no unbond stamp). The
        /// forgery is the strongest one: the attacker signs while claiming the OWNER's key, so only the
        /// signature is wrong. Under the removed gate this same tx made `verify_expected_utxo_state`
        /// return `PalwProviderUnbondUnauthorized`, bricking every honest block that merged it.
        #[test]
        fn an_unauthorized_request_is_skipped_and_the_block_stays_valid() {
            let (owner, op, bond_tx) = make_bond(0x11, 1);
            let view = view_of(&[bond_tx]);
            let attacker = mldsa::generate_key_pair([0x22; 32]);
            let bad = unbond_tx(&attacker, owner.verification_key.as_ref().to_vec(), op);

            let da_state = PalwDaStateV1::default();
            let filter = filter_for(&view, &da_state);
            // The filter refuses the tx ⇒ acceptance SKIPS it. There is NO block-level error path left.
            assert_eq!(filter.unauthorized(&bad), Some(op));

            // Only accepted txs reach the registry producer; the bad tx is not among them.
            let accepted = accept(&[bad], filter);
            assert!(accepted.is_empty(), "an unauthorized 0x37 is not accepted");

            // Applying the surviving mutations leaves the registry byte-identical to before the block.
            let mut after = view.clone();
            after.apply(&palw_provider_bond_mutations_from_accepted_txs(&accepted, POV, MIN_BOND, UNBOND_FLOOR));
            assert_eq!(after, view, "the merging block mutates no registry row");
            let rec = after.get(&op).unwrap();
            assert_eq!(rec.unbond_request_daa_score, None, "no unbond stamp");
            assert_eq!(effective_provider_bond_status(rec, POV), PalwProviderBondStatus::Active, "the bond is still Active");
        }

        /// TEST 2 — an authorized `0x37` still moves the record to Unbonding with the CLAMPED release
        /// stamp. The owner's own signature authorizes the exit; the exit clock starts at the acceptance
        /// DAA, and the release height uses the network floor (20), not the smaller declared delay (10).
        #[test]
        fn an_authorized_request_unbonds_with_the_clamped_release_stamp() {
            let (owner, op, bond_tx) = make_bond(0x11, 1);
            let view = view_of(&[bond_tx]);
            let good = unbond_tx(&owner, owner.verification_key.as_ref().to_vec(), op);

            let da_state = PalwDaStateV1::default();
            let filter = filter_for(&view, &da_state);
            assert_eq!(filter.unauthorized(&good), None, "the owner's own signature authorizes the exit");

            let accepted = accept(&[good], filter);
            assert_eq!(accepted.len(), 1, "an authorized 0x37 is accepted");

            let mut after = view.clone();
            after.apply(&palw_provider_bond_mutations_from_accepted_txs(&accepted, POV, MIN_BOND, UNBOND_FLOOR));
            let rec = after.get(&op).unwrap();
            assert_eq!(rec.unbond_request_daa_score, Some(POV), "the exit clock starts at the acceptance DAA");
            assert_eq!(effective_provider_bond_status(rec, POV), PalwProviderBondStatus::Unbonding);
            // Declared 10 was clamped UP to the floor 20, so release = stamp + 20*epoch, not 10*epoch.
            assert_eq!(provider_bond_release_daa_score(rec, EPOCH_LEN), Some(POV + UNBOND_FLOOR * EPOCH_LEN));
        }

        /// DA-01 named integration: owner authorization alone cannot exit while a selected-parent DA
        /// retention obligation is live. The transaction is skipped at the same acceptance seam as a
        /// bad signature, so it never reaches the provider registry writer.
        #[test]
        fn da_exit_gate_skips_an_owner_authorized_unbond_with_a_live_obligation() {
            let (owner, op, bond_tx) = make_bond(0x11, 1);
            let view = view_of(&[bond_tx]);
            let good = unbond_tx(&owner, owner.verification_key.as_ref().to_vec(), op);
            let mut da_state = PalwDaStateV1::default();
            let obligation_id = h(0x91);
            da_state.obligations.insert(
                obligation_id,
                PalwDaObligationV1 {
                    version: 1,
                    obligation_id,
                    batch_id: h(0x92),
                    leaf_index: 0,
                    leaf_hash: h(0x93),
                    object_root: h(0x94),
                    object_len: 1,
                    chunk_count: 1,
                    chunk_index: 0,
                    provider_bond: op,
                    beacon_epoch: 1,
                    beacon_anchor: h(0x95),
                    created_daa_score: POV - 1,
                    retention_until_daa_score: POV + 100,
                    status: PalwDaObligationStatusV1::Pending,
                },
            );

            let filter = filter_for(&view, &da_state);
            assert_eq!(filter.unauthorized(&good), Some(op));
            assert!(accept(&[good], filter).is_empty(), "live DA retention must keep the exit out of acceptance data");
        }

        /// TEST 3 — a block carrying BOTH an authorized and an unauthorized request applies exactly the
        /// authorized one. Two distinct bonds: A's owner authorizes A's exit; an attacker forges B's.
        /// Only A moves to Unbonding; B is untouched, and the block is valid either way.
        #[test]
        fn a_block_with_one_authorized_and_one_unauthorized_applies_only_the_authorized() {
            let (owner_a, op_a, bond_a) = make_bond(0x11, 1);
            let (owner_b, op_b, bond_b) = make_bond(0x33, 2);
            let view = view_of(&[bond_a, bond_b]);
            let attacker = mldsa::generate_key_pair([0x44; 32]);

            let good_a = unbond_tx(&owner_a, owner_a.verification_key.as_ref().to_vec(), op_a);
            let bad_b = unbond_tx(&attacker, owner_b.verification_key.as_ref().to_vec(), op_b);

            let da_state = PalwDaStateV1::default();
            let filter = filter_for(&view, &da_state);
            assert_eq!(filter.unauthorized(&good_a), None, "A's owner authorizes A's exit");
            assert_eq!(filter.unauthorized(&bad_b), Some(op_b), "the forged exit for B is refused");

            let accepted = accept(&[good_a.clone(), bad_b], filter);
            assert_eq!(accepted, vec![good_a], "only the authorized request is accepted");

            let mut after = view.clone();
            after.apply(&palw_provider_bond_mutations_from_accepted_txs(&accepted, POV, MIN_BOND, UNBOND_FLOOR));
            // A moved to Unbonding...
            let a = after.get(&op_a).unwrap();
            assert_eq!(a.unbond_request_daa_score, Some(POV));
            assert_eq!(effective_provider_bond_status(a, POV), PalwProviderBondStatus::Unbonding);
            // ...B is exactly as it was.
            let b = after.get(&op_b).unwrap();
            assert_eq!(b.unbond_request_daa_score, None);
            assert_eq!(effective_provider_bond_status(b, POV), PalwProviderBondStatus::Active);
        }
    }

    // kaspa-pq Phase 11 (ADR-0013 Addendum C / ADR-0016 §D.4): the slashing
    // side-effect *application* core. Given already-resolved effects, asserts
    // the remove-stake + mint-reporter mutation of the UTXO diff and the
    // multiset (and so the utxo_commitment): the stake leaves the supply, the
    // reporter UTXO is minted at (slashing_tx_id, 0), a zero reward mints
    // nothing (whole stake burns), and a missing output-0 is skipped whole
    // (release-race guard) so a reporter is never minted without the matching
    // stake removal. The expected commitment is rebuilt independently from the
    // final UTXO set, proving the add/remove history nets to the right state.
    mod slashing_side_effect_application {
        use super::super::apply_slashing_effects_to_state as apply;
        use kaspa_consensus_core::{
            dns_finality::SlashingSideEffect,
            muhash::MuHashExtensions,
            tx::{ScriptPublicKey, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry},
            utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff},
        };
        use kaspa_hashes::Hash64;
        use kaspa_muhash::MuHash;
        use std::collections::HashMap;

        const BOND_DAA: u64 = 1_000; // DAA at which the bond's output-0 was created.
        const MINT_DAA: u64 = 2_000; // DAA of the slashing block (stamped on the mint).

        fn spk(b: u8) -> ScriptPublicKey {
            ScriptPublicKey::from_vec(0, vec![b; 32])
        }

        fn bond_outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn slashing_tx_id(b: u8) -> TransactionId {
            Hash64::from_bytes([b; 64])
        }

        // The locked output-0 UTXO of a bond worth `amount`, as it sits in the
        // selected-parent UTXO set (the base view + the seeded multiset).
        fn bond_entry(amount: u64) -> UtxoEntry {
            UtxoEntry::new(amount, spk(0xb0), BOND_DAA, false)
        }

        // An effect slashing `amount`, paying a reporter `reward` (≤ amount) to
        // spk(0xee) minted at (tx, 0); `reward == 0` ⇒ no reporter output.
        fn effect(bond: TransactionOutpoint, amount: u64, reward: u64, tx: TransactionId) -> SlashingSideEffect {
            SlashingSideEffect {
                bond_outpoint: bond,
                slashed_amount_sompi: amount,
                reporter_output: (reward > 0).then(|| TransactionOutput::new(reward, spk(0xee))),
                burned_sompi: amount - reward,
                slashing_tx_id: tx,
                // PoS-v2 4-way fields: inert (2-way) in these apply-path tests.
                security_reserve_sompi: 0,
                victim_epoch_pool_sompi: 0,
                slashed_epoch: 0,
                victim_outputs: vec![],
            }
        }

        // Independent reconstruction of a multiset over an explicit UTXO set —
        // the apply path must reach the same commitment regardless of the
        // add/remove history that produced it.
        fn multiset_of(utxos: &[(TransactionOutpoint, UtxoEntry)]) -> MuHash {
            let mut mh = MuHash::new();
            for (op, e) in utxos {
                mh.add_utxo(op, e);
            }
            mh
        }

        #[test]
        fn removes_stake_and_mints_reporter() {
            let bond_op = bond_outpoint(0x01);
            let tx = slashing_tx_id(0x0a);
            let (amount, reward) = (1_000u64, 250u64);
            let entry = bond_entry(amount);

            // Base view holds the bond's locked output-0; empty diff; multiset
            // already contains the bond UTXO (it is in the committed set).
            let base: UtxoCollection = HashMap::from([(bond_op, entry.clone())]);
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = multiset_of(&[(bond_op, entry.clone())]);

            apply(&[effect(bond_op, amount, reward, tx)], &base, &mut diff, &mut multiset, MINT_DAA);

            let mint_op = TransactionOutpoint::new(tx, 0);
            let mint_entry = UtxoEntry::new(reward, spk(0xee), MINT_DAA, false);

            // Diff: stake removed, reporter minted, nothing else touched.
            assert_eq!(diff.remove.get(&bond_op), Some(&entry));
            assert_eq!(diff.remove.len(), 1);
            assert_eq!(diff.add.get(&mint_op), Some(&mint_entry));
            assert_eq!(diff.add.len(), 1);

            // Commitment now equals a set that only ever held the reporter mint:
            // the removal cancelled the bond and the net set is exactly R.
            assert_eq!(multiset.finalize(), multiset_of(&[(mint_op, mint_entry)]).finalize());
        }

        #[test]
        fn mints_victim_outputs_at_index_two_onward() {
            // PoS-v2 4-way: reporter minted at (tx,0), victim compensations at (tx,2),(tx,3) — index
            // 1 stays free — and the security-reserve share is NOT minted (it burns until Phase 4).
            let bond_op = bond_outpoint(0x06);
            let tx = slashing_tx_id(0x0f);
            let amount = 1_000u64;
            let entry = bond_entry(amount);

            let base: UtxoCollection = HashMap::from([(bond_op, entry.clone())]);
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = multiset_of(&[(bond_op, entry.clone())]);

            // reporter 100, reserve 200 (unminted), victim pool 700 → two victim outputs 300 + 400.
            let mut eff = effect(bond_op, amount, 100, tx);
            eff.security_reserve_sompi = 200;
            eff.victim_epoch_pool_sompi = 700;
            eff.victim_outputs = vec![TransactionOutput::new(300, spk(0xc1)), TransactionOutput::new(400, spk(0xc2))];

            apply(&[eff], &base, &mut diff, &mut multiset, MINT_DAA);

            let r = (TransactionOutpoint::new(tx, 0), UtxoEntry::new(100, spk(0xee), MINT_DAA, false));
            let v1 = (TransactionOutpoint::new(tx, 2), UtxoEntry::new(300, spk(0xc1), MINT_DAA, false));
            let v2 = (TransactionOutpoint::new(tx, 3), UtxoEntry::new(400, spk(0xc2), MINT_DAA, false));

            // Bond removed; reporter + two victims minted; index 1 unused; reserve (200) NOT minted.
            assert_eq!(diff.remove.get(&bond_op), Some(&entry));
            assert_eq!(diff.add.len(), 3);
            assert_eq!(diff.add.get(&r.0), Some(&r.1));
            assert_eq!(diff.add.get(&v1.0), Some(&v1.1));
            assert_eq!(diff.add.get(&v2.0), Some(&v2.1));
            assert!(!diff.add.contains_key(&TransactionOutpoint::new(tx, 1)));
            // Commitment equals a set holding only reporter + victim mints (bond cancelled, reserve burned).
            assert_eq!(multiset.finalize(), multiset_of(&[r, v1, v2]).finalize());
        }

        #[test]
        fn zero_reward_burns_whole_stake() {
            let bond_op = bond_outpoint(0x02);
            let tx = slashing_tx_id(0x0b);
            let entry = bond_entry(1_000);

            let base: UtxoCollection = HashMap::from([(bond_op, entry.clone())]);
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = multiset_of(&[(bond_op, entry.clone())]);

            apply(&[effect(bond_op, 1_000, 0, tx)], &base, &mut diff, &mut multiset, MINT_DAA);

            // Stake removed, nothing minted; commitment back to the empty set.
            assert_eq!(diff.remove.get(&bond_op), Some(&entry));
            assert!(diff.add.is_empty());
            assert_eq!(multiset.finalize(), MuHash::new().finalize());
        }

        #[test]
        fn skips_effect_when_output0_already_absent() {
            // Release-race guard: the bond's output-0 is not in the composed
            // view (already spent in this mergeset). The whole effect — removal
            // AND reporter mint — is skipped, so a reporter is never minted
            // without the matching stake removal.
            let bond_op = bond_outpoint(0x03);
            let tx = slashing_tx_id(0x0c);
            let base: UtxoCollection = HashMap::new(); // output-0 already gone.
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = MuHash::new();

            apply(&[effect(bond_op, 1_000, 250, tx)], &base, &mut diff, &mut multiset, MINT_DAA);

            assert!(diff.add.is_empty());
            assert!(diff.remove.is_empty());
            assert_eq!(multiset.finalize(), MuHash::new().finalize());
        }

        #[test]
        fn applies_each_of_several_distinct_bonds() {
            let (op_a, op_b) = (bond_outpoint(0x04), bond_outpoint(0x05));
            let (tx_a, tx_b) = (slashing_tx_id(0x0d), slashing_tx_id(0x0e));
            // Distinct amounts ⇒ distinct multiset elements; bond b's reward is 0
            // (burns entirely), bond a's reward is non-zero (mints a reporter).
            let (amt_a, amt_b, rew_a) = (1_000u64, 4_000u64, 100u64);
            let (e_a, e_b) = (bond_entry(amt_a), bond_entry(amt_b));

            let base: UtxoCollection = HashMap::from([(op_a, e_a.clone()), (op_b, e_b.clone())]);
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = multiset_of(&[(op_a, e_a.clone()), (op_b, e_b.clone())]);

            apply(&[effect(op_a, amt_a, rew_a, tx_a), effect(op_b, amt_b, 0, tx_b)], &base, &mut diff, &mut multiset, MINT_DAA);

            let mint_a = TransactionOutpoint::new(tx_a, 0);
            let mint_a_entry = UtxoEntry::new(rew_a, spk(0xee), MINT_DAA, false);

            // Both stakes removed; only a's reporter minted (b's reward is 0).
            assert_eq!(diff.remove.len(), 2);
            assert!(diff.remove.contains_key(&op_a) && diff.remove.contains_key(&op_b));
            assert_eq!(diff.add.len(), 1);
            assert_eq!(diff.add.get(&mint_a), Some(&mint_a_entry));

            // Net committed set = a's reporter mint only.
            assert_eq!(multiset.finalize(), multiset_of(&[(mint_a, mint_a_entry)]).finalize());
        }
    }
}
