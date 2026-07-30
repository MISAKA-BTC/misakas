//! ADR-0040 — the REAL algo-4 mint path: consensus derives, the miner draws, consensus builds.
//!
//! This is the seeding-free replacement for `palw_demo.rs`. The difference is not cosmetic. The demo
//! mint WRITES a fabricated leaf, an empty-vote certificate and an `Active` lifecycle view into the
//! real stores and then mints against its own forgery; nothing here writes anything. The leaf comes
//! from `palw_store` and the batch's eligibility comes from the overlay view at the sink — if either is
//! absent the mint fails, which is the entire point: a miner on a shared network cannot manufacture
//! provenance, so neither may the code path it uses.
//!
//! See [`kaspa_consensus_core::palw_mint`] for why this is a two-call interface rather than one.

use kaspa_consensus_core::{
    api::ConsensusApi,
    block::{TemplateBuildMode, TemplateTransactionSelector},
    coinbase::MinerData,
    header::PalwHeaderFields,
    palw::{BeaconDnsAnchor, chain_commit, dns_finality_certificate_hash_v1, palw_seed_carry_run},
    palw_mint::{PalwAlgo4MintFacts, PalwAlgo4Stamp, PalwAlgo4Template, PalwMintError},
    pow_layer0::POW_ALGO_ID_PALW_REPLICA,
};
use kaspa_hashes::Hash64;

use super::Consensus;
use crate::{
    model::stores::{headers::HeaderStoreReader, palw::PalwStoreReader},
    processes::palw::{resolve_palw_buried_epoch_seeds, resolve_palw_lagged_anchor},
};

type MintResult<T> = Result<T, PalwMintError>;

impl Consensus {
    /// Derive the frozen, consensus-owned inputs for one algo-4 mint attempt off the current sink.
    ///
    /// Read-only. Fails NotReady when the chain simply is not ready (no finality-buried anchor yet, the
    /// lane is halted, the batch is not block-eligible in this history) and Fault when the requested
    /// provenance does not exist on chain — a miner asking to mint a leaf nobody registered.
    pub(crate) fn palw_algo4_mint_facts_impl(
        &self,
        batch_id: Hash64,
        leaf_index: u32,
        miner_data: MinerData,
    ) -> MintResult<PalwAlgo4MintFacts> {
        // The GHOSTDAG-fixed interval. Built as a throwaway template because `daa_score` is exactly what
        // the template assigns; clause 5 then pins `palw_target_daa_interval == daa_score`, so reading it
        // any other way would be a second derivation of a consensus value.
        let sink = self.get_sink();
        let target_daa_interval = self
            .build_block_template(miner_data, Box::new(EmptySelector::default()), TemplateBuildMode::Infallible)
            .map_err(|e| PalwMintError::not_ready(format!("target-interval template: {e:?}")))?
            .block
            .header
            .daa_score;
        self.palw_algo4_facts_at(sink, target_daa_interval, batch_id, leaf_index)
    }

    /// The whole derivation, given a sink and the interval its template fixes. Shared by the facts call
    /// (which learns the interval from a throwaway template) and the template call (which learns it from
    /// the real one) so both sides see one derivation, not two.
    fn palw_algo4_facts_at(
        &self,
        sink: kaspa_consensus_core::BlockHash,
        target_daa_interval: u64,
        batch_id: Hash64,
        leaf_index: u32,
    ) -> MintResult<PalwAlgo4MintFacts> {
        let params = &self.config.params;
        let network_id = params.net.suffix().unwrap_or(0);
        let dns_params = params.dns_params.clone().ok_or_else(|| PalwMintError::fault("PALW preset without dns_params"))?;

        // Clause 6: the finality-buried DNS anchor, resolved by the same burial-only walk the body
        // check runs (headers + reachability, no virtual state).
        let anchor = resolve_palw_lagged_anchor(&self.storage.headers_store, &self.services.reachability_service, &dns_params, sink)
            .ok_or_else(|| PalwMintError::not_ready("no finality-buried DNS anchor off the sink yet"))?;
        let anchor_header = self
            .storage
            .headers_store
            .get_header(anchor.anchor_hash)
            .map_err(|e| PalwMintError::fault(format!("anchor header read failed: {e:?}")))?;
        let anchor_facts = BeaconDnsAnchor {
            hash: anchor.anchor_hash,
            blue_score: anchor.anchor_blue_score,
            daa_score: anchor.anchor_daa_score,
            overlay_root: anchor_header.overlay_commitment_root,
        };
        // Clause 9's lagged R_E, retained on the buried anchor's own header.
        let beacon_seed = anchor_header.palw_beacon_seed;

        // Clause 10, verbatim: the lane is closed iff the buried seed-carry run exceeds grace. Mining
        // while closed builds blocks this node's own body check rejects — a node bricking its own lane.
        // Matching the ENFORCED predicate rather than a stricter proxy also means we do not sit out
        // intervals that would in fact have been accepted.
        let samples = resolve_palw_buried_epoch_seeds(
            &self.storage.headers_store,
            &self.services.reachability_service,
            anchor.anchor_hash,
            params.palw_activation_daa_score,
            params.palw_epoch_length_daa,
            params.palw_beacon_grace_epochs.saturating_add(2),
        );
        let lane_open = palw_seed_carry_run(&samples) <= params.palw_beacon_grace_epochs;

        let chain_commit_expected =
            chain_commit(&anchor_facts.hash, &dns_finality_certificate_hash_v1(&anchor_facts), target_daa_interval, network_id);

        // THE PROVENANCE GATE. The leaf is READ, never fabricated.
        let leaf = self
            .storage
            .palw_store
            .leaf(batch_id, leaf_index)
            .map_err(|_| PalwMintError::fault(format!("leaf ({batch_id:?}, {leaf_index}) is not on chain — nothing to mint")))?;

        // ADR-MA §13.1/§12 — on a registry-active net the template must commit the leaf's set and
        // its governing (policy, plan) at THIS interval, and run per-set difficulty for that
        // sublane. The set is the LEAF's committed identity — a miner does not choose it; below
        // the fence everything here is zero/None (byte-identical inert path).
        let registry_active = params.palw_compute_registry_activation_daa_score != u64::MAX
            && target_daa_interval >= params.palw_compute_registry_activation_daa_score;
        let (compute_set_id, compute_policy_id, allocation_plan_id, per_set) = if registry_active {
            use crate::model::stores::palw_compute_registry::PalwComputeRegistryStoreReader;
            use kaspa_consensus_core::palw_compute_set::ComputeSetState;
            let set_id = leaf.receipt_v3_compute_set_id;
            if set_id == Hash64::default() {
                return Err(PalwMintError::fault("registry-active mint requires a leaf committed to a compute set"));
            }
            let registry_view = self
                .storage
                .palw_compute_registry_store
                .view(sink)
                .map_err(|e| PalwMintError::fault(format!("compute registry view read failed: {e:?}")))?
                .ok_or_else(|| PalwMintError::not_ready("no compute registry view at the sink"))?;
            let (policy_id, state, plan_id) = registry_view
                .governing_references_at(&set_id, target_daa_interval)
                .ok_or_else(|| PalwMintError::not_ready(format!("set {set_id} has no governing policy/plan at this interval")))?;
            // §13.2: only a known ACTIVE set mints (Shadow/Deprecated/Halted/Retired all refuse) —
            // the same predicate the header stage enforces, checked here so we never build a
            // self-rejecting block.
            if state != ComputeSetState::Active {
                return Err(PalwMintError::not_ready(format!("set {set_id} is {state:?}, not Active, at this interval")));
            }
            let plan = self
                .storage
                .palw_compute_registry_store
                .plan(plan_id)
                .map_err(|e| PalwMintError::fault(format!("compute registry plan read failed: {e:?}")))?
                .ok_or_else(|| PalwMintError::fault("governing plan record is missing from the content store"))?;
            let share = plan
                .entries
                .iter()
                .find(|entry| entry.compute_set_id == set_id)
                .map(|entry| entry.target_share_bps)
                .filter(|share| *share > 0)
                .ok_or_else(|| PalwMintError::not_ready(format!("set {set_id} holds no share under the governing plan (§12.2)")))?;
            (
                set_id,
                policy_id,
                plan_id,
                Some(crate::processes::difficulty::PalwSetSublane { compute_set_id: set_id, target_share_bps: share }),
            )
        } else {
            (Hash64::default(), Hash64::default(), Hash64::default(), None)
        };

        // §16.3 lane bits, through the same helper `pre_pow_validation` runs (§12 per-set on a
        // registry-active net).
        let replica_bits = self
            .virtual_processor
            .palw_lane_bits_for_template(POW_ALGO_ID_PALW_REPLICA, per_set)
            .map_err(|e| PalwMintError::not_ready(format!("lane bits: {e:?}")))?;

        // The batch must be block-eligible at this epoch in the sink's past, judged from the SAME view
        // the body check reads. The view is written in the sink's own body-commit batch.
        let epoch = target_daa_interval / params.palw_epoch_length_daa.max(1);
        let view = self
            .storage
            .palw_overlay_view_store
            .view(sink)
            .map_err(|e| PalwMintError::fault(format!("overlay view read failed: {e:?}")))?
            .ok_or_else(|| PalwMintError::not_ready("no PALW overlay view at the sink"))?;
        let lifecycle = view
            .resolvable_batch(&batch_id, epoch, target_daa_interval)
            .ok_or_else(|| PalwMintError::not_ready(format!("batch {batch_id:?} is not block-eligible at epoch {epoch}")))?;
        let epoch_certificate_hash =
            lifecycle.cert_hash.ok_or_else(|| PalwMintError::not_ready("batch is eligible but carries no certificate hash"))?;

        Ok(PalwAlgo4MintFacts {
            network_id,
            sink,
            beacon_seed,
            chain_commit: chain_commit_expected,
            target_daa_interval,
            replica_bits,
            compute_set_id,
            compute_policy_id,
            allocation_plan_id,
            epoch,
            epoch_certificate_hash,
            leaf: (*leaf).clone(),
            lane_open,
        })
    }

    /// Steps 2–5 of the ADR-0040 construction order, atomically: parents, coinbase, the transaction set
    /// and its order, timestamp/DAA, and every PALW header field EXCEPT the two the authorization
    /// commitment substitutes out. Returns an UNSIGNED [`MutableBlock`]; the caller signs it, appends the
    /// authorization transaction, recomputes `hash_merkle_root`, stamps `palw_authorization_hash`, and
    /// finalizes — and must change nothing else. Returns the exact v4 stamp target alongside the
    /// unsigned block so the node can grind only after the final authorization/root are installed.
    ///
    /// Every producer-supplied value in `stamp` is re-derived here and compared. The one exception is
    /// `ticket_nullifier`, which is the miner's secret and cannot be re-derived — so it is not trusted
    /// either: `nonce` is computed FROM it here (I-3) rather than accepted, and clause 1 decides at body
    /// validation whether it actually opens the leaf's commitment.
    pub(crate) fn palw_build_algo4_template_impl(
        &self,
        miner_data: MinerData,
        selector: Box<dyn TemplateTransactionSelector>,
        stamp: PalwAlgo4Stamp,
    ) -> MintResult<PalwAlgo4Template> {
        // Staleness: a moved sink means a different selected parent, hence a different anchor, view and
        // interval. Fail rather than build against facts that no longer hold.
        let sink_now = self.get_sink();
        if sink_now != stamp.sink {
            return Err(PalwMintError::not_ready("sink advanced between the facts and the template"));
        }

        let mut mb = self
            .build_block_template(miner_data, selector, TemplateBuildMode::Infallible)
            .map_err(|e| PalwMintError::not_ready(format!("algo-4 template: {e:?}")))?
            .block;

        // Clause 5 pins the target interval to this block's own DAA score. If GHOSTDAG moved between the
        // facts call and here, the producer's draw was against a different interval and is void.
        if mb.header.daa_score != stamp.target_daa_interval {
            return Err(PalwMintError::not_ready(format!(
                "target interval moved: drew against {} but the template is at {}",
                stamp.target_daa_interval, mb.header.daa_score
            )));
        }

        // Re-derive the rest of the producer's claims rather than believing them — off THIS template's
        // interval, with no second throwaway build.
        let facts = self.palw_algo4_facts_at(sink_now, mb.header.daa_score, stamp.batch_id, stamp.leaf_index)?;
        if facts.chain_commit != stamp.chain_commit {
            return Err(PalwMintError::fault("chain_commit does not survive re-derivation"));
        }
        if facts.replica_bits != stamp.replica_bits {
            return Err(PalwMintError::fault("replica_bits does not survive re-derivation"));
        }
        if facts.epoch_certificate_hash != stamp.epoch_certificate_hash {
            return Err(PalwMintError::fault("epoch_certificate_hash does not survive re-derivation"));
        }
        if !facts.lane_open {
            return Err(PalwMintError::not_ready("lane closed (clause 10) — refusing to build a self-rejecting block"));
        }

        // Keep the GHOSTDAG-derived component work and the template-stamped beacon seed; the virtual
        // stage re-derives and authenticates both.
        let keep_hash_work = mb.header.blue_hash_work;
        let keep_compute_work = mb.header.blue_compute_work;
        let keep_beacon_seed = mb.header.palw_beacon_seed;
        let (palw_spam_accumulator_commitment, spam_target_bits) =
            if mb.header.version >= kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION {
                self.virtual_processor
                    .palw_spam_replica_fields_for_template(&mb.header)
                    .map_err(|e| PalwMintError::not_ready(format!("anti-spam candidate state: {e}")))?
            } else {
                (Hash64::default(), 0)
            };

        mb.header.pow_algo_id = POW_ALGO_ID_PALW_REPLICA;
        mb.header.bits = facts.replica_bits;
        // I-3: the nonce is DERIVED from the nullifier, never accepted from the producer. Accepting one
        // would restore exactly the grinding freedom the pinning exists to remove.
        mb.header.nonce = palw_pinned_nonce(&stamp.ticket_nullifier);
        mb.header = mb.header.with_palw_fields(PalwHeaderFields {
            blue_hash_work: keep_hash_work,
            blue_compute_work: keep_compute_work,
            palw_beacon_seed: keep_beacon_seed,
            palw_batch_id: stamp.batch_id,
            palw_leaf_index: stamp.leaf_index,
            palw_ticket_nullifier: stamp.ticket_nullifier,
            palw_epoch_certificate_hash: facts.epoch_certificate_hash,
            palw_chain_commit: facts.chain_commit,
            palw_target_daa_interval: facts.target_daa_interval,
            // Left DEFAULT: the caller stamps it after signing. The AUTH-02 commitment zeroes this field,
            // so signing against the default is what the verifier recomputes.
            palw_authorization_hash: Hash64::default(),
            palw_proof_type: stamp.proof_type,
            palw_spam_accumulator_commitment,
            palw_spam_nonce: 0,
            // ADR-MA §13.1 — the re-derived Header-v5 Compute Set references (all-zero below the
            // registry fence, where `facts` resolves them to defaults). Facts-derived, never
            // producer-supplied: the set is the LEAF's committed identity.
            palw_compute_set_id: facts.compute_set_id,
            palw_compute_policy_id: facts.compute_policy_id,
            palw_allocation_plan_id: facts.allocation_plan_id,
        });
        Ok(PalwAlgo4Template { block: mb, spam_target_bits })
    }
}

/// `low64(nullifier)` — the consensus-side twin of `misaka_palw_miner::mining::pinned_nonce` (I-3).
fn palw_pinned_nonce(nullifier: &Hash64) -> u64 {
    let b = nullifier.as_byte_slice();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// Yields nothing, then drains — a coinbase-only template whose re-selection loop terminates.
#[derive(Default)]
struct EmptySelector(bool);
impl TemplateTransactionSelector for EmptySelector {
    fn select_transactions(&mut self) -> Vec<kaspa_consensus_core::tx::Transaction> {
        self.0 = true;
        vec![]
    }
    fn reject_selection(&mut self, _tx_id: kaspa_consensus_core::tx::TransactionId) {}
    fn is_successful(&self) -> bool {
        true
    }
}
