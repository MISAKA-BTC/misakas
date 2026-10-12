//! **`VirtualStateProcessor`: template methods** — moved out of `processor.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl VirtualStateProcessor {
    pub(super) fn validate_block_template_transactions_in_parallel<V: UtxoView + Sync>(
        &self,
        txs: &[Transaction],
        virtual_state: &VirtualState,
        utxo_view: &V,
    ) -> Vec<TxResult<u64>> {
        self.thread_pool
            .install(|| txs.par_iter().map(|tx| self.validate_block_template_transaction(tx, virtual_state, &utxo_view)).collect())
    }

    pub(super) fn validate_block_template_transaction(
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
        // **The 2026-09-23 Position route matrix, P-B3, at the template** (the review's MEDIUM
        // point): the mempool asked the lifecycle gate when the carrier entered, and the class can
        // regress between that and this template (Held tracks load and panel drawability). Asked
        // again here, a refused carrier is `InvalidInBlockTemplate` — the mining manager evicts it —
        // rather than mined into a fold that drops it. Node-local like the mempool check: a block
        // another node mines is judged by the fold alone. `None` below the audit fence (testnet-11).
        if let Some(refusal) = self.palw_mempool_market_refusal(tx, virtual_state.daa_score) {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwModelMarketNotEligible(refusal));
        }
        // **MSK-26A: never lead a template with a DNS slashing/precommit evidence that is not genuine
        // at the tip** — the mining manager evicts it (P-B3 pattern). Node policy, not a block rule.
        if let Some(refusal) = self.palw_slashing_evidence_mempool_refusal(tx, virtual_state.daa_score) {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwSlashingEvidenceNotGenuine(refusal));
        }
        let ValidatedTransaction { calculated_fee, .. } =
            // `None`: mempool/template single-tx context, not mergeset acceptance (bond spend-gate inert here).
            self.validate_transaction_in_utxo_context(tx, utxo_view, virtual_state.daa_score, TxValidationFlags::Full, None)?;
        // **ADR-0152 H-1 at the template** (P2-9 review, finding 5): the tip moves after admission —
        // the first accusation of a claim opens its session and every other one becomes a drop — so
        // the gate is asked again here, and a refused carrier is `InvalidInBlockTemplate`: the mining
        // manager evicts it rather than leading a template with it. `None` below R-core+.
        if let Some(refusal) = self.palw_mempool_h1_carrier_refusal(tx, virtual_state.daa_score) {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwH1CarrierRefused(refusal));
        }
        if let Some(refusal) = self.palw_mempool_kernel_route_refusal(tx, virtual_state.daa_score) {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwKernelRouteRefused(refusal));
        }
        Ok(calculated_fee)
    }

    pub(super) fn latest_ready_epoch_for_template_snapshot(&self, virtual_state: &VirtualState) -> Option<u64> {
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
        // **ADR-0142: past `palw_clock_cursor` the slot RULE retires — and the slot does not.**
        //
        // The selected-parent rule is gone, but a beat still earns the chain a DAA only at or past
        // the cursor, and this used to answer `earliest = now` regardless. The miner took that at
        // its word and ground a beat the moment one slot was taken: measured on the live and
        // private t12, 74% of blocks were heartbeats and 11% of beats got a tick, each of the other
        // 89% adding a blue score and a relay for nothing. So `earliest` is the cursor's next
        // slot, read from the SAME decision the DAA score is computed from (`DaaWindow::clock`,
        // i.e. `palw_clock_step_v1`) for THIS template's own parents — not the virtual's, which may
        // have moved since the template was built.
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
            // Past the cursor: the template's own timestamp, or the next slot if that is later — the
            // shape the slot rule's answer always had (`Ok` → the template's time, `Err` → the
            // boundary), so a caller that waits for `earliest` waits for exactly the slot.
            let slot = self.palw_clock_step_for_parents(template.block.header.direct_parents())?.next_slot_ms();
            template.block.header.timestamp.max(slot.unwrap_or(0))
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
        let built_at = template.block.header.timestamp;
        template.block.header.timestamp = built_at.max(earliest);
        // **A stamp the builder did not execute against is a commitment for another block.** On an
        // EVM-active network the builder ran the lane against ITS timestamp; stamped later, the
        // beat carried the old `evm_commitment_root` and every node disqualified it from the chain
        // — the node's own beat, on testnet-12, whose lane is active from DAA 0 in every default
        // `kaspad` build. Found while diagnosing the 2026-09-24 merge battery: H1 began stamping
        // beats for the cursor's slot, and before it this path never moved a testnet-12 stamp.
        if template.block.header.timestamp != built_at {
            self.heartbeat_recommit_evm_for_stamp(&mut template, built_at)?;
        }
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
        // **RFC-0008 v2: a heartbeat covers the lane the template's state already accepted.** The anchor trailer rides the template's
        // coinbase (the virtual accepted the covered EXEC blocks, so the block built on it must say which heads it covers); the
        // re-serialisation below keeps it, or the beat would commit a UTXO state for blocks it does not name. Read only past the fence:
        // below it (and where it is not armed) the miner data is exactly the template's, a trailer in a miner's tag being its bytes.
        let exec_v2 = self.palw_exec_v2_active_at(template.block.header.daa_score);
        let coinbase = &mut template.block.transactions[0];
        let miner_data = match kaspa_consensus_core::palw_exec_v2_anchor::palw_exec_v2_anchor_split(&coinbase.payload)
            .ok()
            .filter(|_| exec_v2)
            .and_then(|(_, anchor)| anchor)
            .and_then(|anchor| {
                kaspa_consensus_core::palw_exec_v2_anchor::palw_exec_v2_anchor_append(&template.miner_data.extra_data, &anchor).ok()
            }) {
            Some(extra) => MinerData::new(template.miner_data.script_public_key.clone(), extra),
            None => template.miner_data.clone(),
        };
        let zeroed = kaspa_consensus_core::coinbase::CoinbaseData { blue_score, subsidy: 0, miner_data };
        coinbase.payload =
            self.coinbase_manager.serialize_coinbase_payload(&zeroed).expect("a payload the manager just built reserializes");
        coinbase.finalize();
        template.block.header.hash_merkle_root = calc_hash_merkle_root(template.block.transactions.iter());
        template.block.header.finalize();
        Ok((template, earliest))
    }

    /// The wall clock the template builder reads. Production: `unix_now()`, always. A test may substitute a simulated clock
    /// (`template_clock`); there is no way to do so in a non-test build.
    pub(super) fn template_now(&self) -> u64 {
        #[cfg(test)]
        {
            let simulated = self.template_clock.load(std::sync::atomic::Ordering::Relaxed);
            if simulated != 0 {
                return simulated;
            }
        }
        unix_now()
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
        template: BlockTemplate,
        round: u64,
        payout: kaspa_consensus_core::tx::ScriptPublicKey,
    ) -> Result<BlockTemplate, RuleError> {
        self.exec_lane_adapt_block_template(template, Some(round), payout)
    }

    pub(super) fn build_block_template_with_selector_provider<F>(
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
                        AttestationDropReason::DnsRetired | AttestationDropReason::MalformedPayload => *dropped_malformed += 1,
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
        // **P-B1: one payout-queue budget across the template's market carriers.** A carrier that
        // does not fit alone was refused by `validate_block_template_transaction` (and is evicted);
        // one that fits alone but not after the carriers already kept is only deferred: its slot is
        // freed for the refill and it stays in the pool for a later template. `None` below the fence.
        let mut market_budget = self.palw_template_market_budget(virtual_state.daa_score);
        let mut market_deferred = 0usize;
        let mut market_deferred_ids: std::collections::HashSet<kaspa_consensus_core::tx::TransactionId> =
            std::collections::HashSet::new();
        let mut market_room_for = |tx: &Transaction| -> bool {
            match market_budget.as_mut() {
                Some(budget) => budget.admit(tx).is_ok(),
                None => true,
            }
        };
        let results = self.validate_block_template_transactions_in_parallel(&txs, &virtual_state, &virtual_utxo_view);
        for (tx, res) in txs.iter().zip(results) {
            match res {
                Err(e) => {
                    invalid_transactions.insert(tx.id(), e);
                    tx_selector.reject_selection(tx.id());
                }
                Ok(fee) => {
                    if !market_room_for(tx) {
                        market_deferred += 1;
                        market_deferred_ids.insert(tx.id());
                        tx_selector.reject_selection_for_refill(tx.id());
                    } else if classify_keep(
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

        let mut has_rejections = !invalid_transactions.is_empty() || !dropped_shard_ids.is_empty() || !market_deferred_ids.is_empty();
        if has_rejections {
            txs.retain(|tx| {
                !invalid_transactions.contains_key(&tx.id())
                    && !dropped_shard_ids.contains(&tx.id())
                    && !market_deferred_ids.contains(&tx.id())
            });
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
                        if !market_room_for(&tx) {
                            // P-B1: deferred to a later template, not refused (see above).
                            market_deferred += 1;
                            tx_selector.reject_selection_for_refill(tx.id());
                            has_rejections = true;
                        } else if classify_keep(
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

        if market_deferred > 0 {
            debug!(
                "[palw-market] {market_deferred} market carrier(s) deferred from this template: the payout queue has no room left \
                 for their moves or refunds after the ones it already carries (P-B1)"
            );
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
        // **The template's stamp, decided before anything is built on it** (`palw_template_stamp_v1`:
        // the median, H5's floor, and the lead cap's construction half). Past median time is the
        // exclusive lower bound for valid block time. **H5 (the clock floor): a template that steps the
        // clock on a beat's grant is stamped at or past the slot it consumes** — the header stage
        // refuses it otherwise (`ClockStepBeforeItsSlot`). Raised here rather than left to the miner
        // because every lane's template can be the step: on testnet-12 the first block of any lane
        // built after a granted beat carries the tick. Decided on the virtual's own clock decision —
        // the one this block's DAA score was computed from. **The lead cap**: such a step stamped past
        // this node's clock plus 132 s is refused by every header stage, so it is refused here.
        let lead_capped = self.palw_clock_lead_cap.is_some_and(|fence| fence.is_active(virtual_state.daa_score));
        let clock = if self.palw_clock_floor.is_some() || lead_capped {
            Some(self.window_manager.block_daa_window(&virtual_state.ghostdag_data)?.clock)
        } else {
            None
        };
        let timestamp = self.palw_template_stamp_v1(clock, lead_capped, virtual_state.past_median_time, self.template_now())?;
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
        let (newly_included_stake, expected_stake) = if self.dns_retired_at(virtual_state.daa_score) {
            self.native_inclusion_ratio(
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_non_daa,
                &self.palw_v2_template_point(&virtual_state),
            )
        } else {
            (newly_included_stake, expected_stake)
        };
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
        let palw_escrow_withheld = palw_at_selected_parent
            .as_ref()
            .map(|(block, state)| self.palw_v2_escrow_withheld_at(state, *block, virtual_state.daa_score))
            .unwrap_or(0);
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
        // RFC-0009, construction side: how each merged V4 receipt block is split — same state, same virtual mergeset, same entitlement.
        let palw_receipt_v4_payouts = palw_at_selected_parent
            .as_ref()
            .map(|(_, state)| {
                self.palw_v2_receipt_v4_payouts(
                    state,
                    &virtual_state.ghostdag_data,
                    &virtual_state.mergeset_non_daa,
                    &palw_unentitled_blues,
                )
            })
            .unwrap_or_default();
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
        // **RFC-0008 v2: the anchor trailer the covered EXEC blocks need.** Virtual accepted them (they are in its reds), so the block
        // built on it must say which heads it covers or its own validation would recompute a different UTXO state. Carried by the
        // miner data's extra bytes, after the miner's tag.
        let coinbase_miner_data = match self
            .palw_exec_v2_virtual_anchor
            .lock()
            .clone()
            .filter(|(selected_parent, _)| *selected_parent == virtual_state.ghostdag_data.selected_parent)
            .filter(|_| self.palw_exec_v2_active_at(virtual_state.daa_score))
        {
            Some((_, anchor)) => {
                match kaspa_consensus_core::palw_exec_v2_anchor::palw_exec_v2_anchor_append(&miner_data.extra_data, &anchor) {
                    Ok(extra) => MinerData::new(miner_data.script_public_key.clone(), extra),
                    Err(_) => miner_data.clone(),
                }
            }
            None => miner_data.clone(),
        };
        let coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                virtual_state.daa_score,
                // The template path always builds the bonded lanes' coinbase; a heartbeat
                // template is derived from it by `heartbeat_adapt_block_template`, which
                // re-declares the subsidy as zero (ADR-0060 Decision 1.4).
                self.coinbase_manager.calc_block_subsidy(virtual_state.daa_score),
                coinbase_miner_data,
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
                // RFC-0009: the merged V4 receipt blocks' split payouts.
                &palw_receipt_v4_payouts,
            )
            .unwrap();
        // **The coinbase is fixed HERE, before `evm_template_fields` commits the lane** (phase2-plan
        // §5.4, F6): the lane never reads the block's own coinbase, so the vesting mints and seat
        // pay it renders need no re-commit — and a later edit of these outputs must re-derive the
        // lane (`palw_v2_payout_outputs`' rules). T50 (`p2_evm_twin`) holds build == validate with
        // the lane as shipped over mint-carrying attempts and a restamped heartbeat.
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
        let header = Header::new_finalized(
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            timestamp,
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
                header.with_palw_state_root(self.palw_committed_root_v1(parent_state))
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
}
