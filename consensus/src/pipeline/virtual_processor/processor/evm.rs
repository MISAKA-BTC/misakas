//! **`VirtualStateProcessor`: evm methods** — moved out of `processor.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl VirtualStateProcessor {
    /// kaspa-pq EVM Lane v0.4 (§2.3): the lazy chain-context EVM step for one
    /// selected-chain candidate. Gated on `evm_activation_daa_score` (a single
    /// u64 compare on every current network); no-replay and the commitment
    /// check live in `processes::evm::evm_validate`. `Err` = the block is
    /// disqualified from the chain (commitment fault), mirroring a UTXO fault.
    #[cfg(feature = "evm")]
    pub(super) fn evm_chain_context_step<V: UtxoView>(
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
                Some(std::sync::Arc::new(self.palw_evm_view_with_market_gate_v1(state, params, selected_parent, header.daa_score)))
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
                // Lane accept-order: past its fence, parents-first at this block's DAA score.
                let sorted_mergeset: Vec<BlockHash> = self.acceptance_ordered_mergeset(&ctx.ghostdag_data, header.daa_score);
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
                            self.evm_bridge_ledger_activation_daa_score,
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
                            self.evm_bridge_ledger_activation_daa_score,
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

    /// Non-`evm` builds cannot validate the lane. On every default network the
    /// lane is `u64::MAX`-inert so this is unreachable; on an evm-ACTIVE net a
    /// non-evm binary must refuse to follow a chain it cannot validate rather
    /// than silently fork.
    #[cfg(not(feature = "evm"))]
    pub(super) fn evm_chain_context_step<V: UtxoView>(
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

    /// kaspa-pq EVM Lane v0.4 (§15): producer-side EVM fields for a template
    /// built from the current virtual state. Runs the SAME acceptance-execution
    /// core the verifier uses, so a block mined from this template reproduces
    /// `evm_commitment_root` byte-for-byte. The own payload is empty until the
    /// EVM mempool lands (§16). NOTE: the commitment derives from the header's
    /// timestamp — a miner must not mutate the template timestamp (refreshing
    /// the template re-derives the commitment).
    #[cfg(feature = "evm")]
    pub(super) fn evm_template_fields(
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
            (Some(state), Some(params)) => Some(std::sync::Arc::new(self.palw_evm_view_with_market_gate_v1(
                state,
                params,
                template_selected_parent,
                header.daa_score,
            ))),
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
        // Lane accept-order: the order validation will apply at this template's DAA score.
        let sorted_mergeset: Vec<BlockHash> = self.acceptance_ordered_mergeset(&virtual_state.ghostdag_data, virtual_state.daa_score);
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
                    self.evm_bridge_ledger_activation_daa_score,
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
                    self.evm_bridge_ledger_activation_daa_score,
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
    pub(super) fn evm_template_fields(
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

    /// **RFC-0008 v2: re-shape a standard template into an `EXEC_SLICE` block.** The lane block of [`Self::round_adapt_block_template`]
    /// with no round (a slice holds no permit): the same parents (the lane tips on the sink's chain, and the anchor), the same
    /// recomputed GHOSTDAG / DAA / bits / median time / pruning point, the same algo 10 with an empty `palw_commitment` for the caller to
    /// sign after solving — and a body of **its coinbase alone** (a slice has no user transaction batch). Stamped at the template's own
    /// time, or one past the median if that is later.
    pub fn exec_v2_slice_adapt_block_template(
        &self,
        template: BlockTemplate,
        payout: kaspa_consensus_core::tx::ScriptPublicKey,
    ) -> Result<BlockTemplate, RuleError> {
        if !self.palw_exec_v2_active_at(self.virtual_stores.read().state.get().unwrap().daa_score) {
            return Err(RuleError::UnknownPowAlgoId(kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1));
        }
        self.exec_lane_adapt_block_template(template, None, payout)
    }

    /// The one builder of a lane block's template. `round` is `Some` for a permit holder (a v1 round block, a v2 `EXEC_TX`) and `None`
    /// for a v2 `EXEC_SLICE`.
    pub(super) fn exec_lane_adapt_block_template(
        &self,
        mut template: BlockTemplate,
        round: Option<u64>,
        payout: kaspa_consensus_core::tx::ScriptPublicKey,
    ) -> Result<BlockTemplate, RuleError> {
        use kaspa_consensus_core::palw_exec_v2::{PalwExecLaneCoordsV1, palw_exec_lane_coords_v1};
        use kaspa_consensus_core::palw_execution_lane_v1::{PALW_EXEC_ROUND_MS, palw_execution_mergeset_rule_v1};
        use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1;
        let virtual_state = self.virtual_stores.read().state.get().unwrap();
        let Some(lane) = self.palw_execution_lane.filter(|lane| lane.activation.is_active(virtual_state.daa_score)) else {
            return Err(RuleError::UnknownPowAlgoId(POW_ALGO_ID_PALW_ROUND_V1));
        };
        let sink = virtual_state.ghostdag_data.selected_parent;
        if let (Some(round), Some(params)) = (round, self.palw_state_params_v2.as_ref())
            && params.bond_budget_active_at(virtual_state.daa_score)
        {
            let (_, state) = self
                .palw_v2_state_at(sink)
                .ok_or_else(|| RuleError::RoundFeeBudgetRefused("the recipient state is unavailable".into()))?;
            let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                block: Default::default(),
                daa_score: virtual_state.daa_score,
                blue_score: virtual_state.ghostdag_data.blue_score,
                subsidy: self.coinbase_manager.calc_block_subsidy(virtual_state.daa_score),
            };
            let mut budget = state
                .round_fee_admission_v1(params, &point, self.palw_escrow_carve_at(point.daa_score, point.daa_score))
                .ok_or_else(|| RuleError::RoundFeeBudgetRefused("the recipient budget is unavailable".into()))?;
            let view = self
                .palw_round_view_v1(round)
                .ok_or_else(|| RuleError::RoundFeeBudgetRefused("the execution schedule is unavailable".into()))?;
            let recipients: std::collections::BTreeSet<_> = view
                .permits
                .iter()
                .filter(|permit| !view.used.contains(&permit.index))
                .filter(|permit| {
                    state.bond(&permit.bond).is_some_and(|bond| {
                        kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&bond.payout_payload.as_bytes()) == payout
                    })
                })
                .map(|permit| permit.bond)
                .collect();
            if recipients.len() != 1 || template.calculated_fees.len() + 1 != template.block.transactions.len() {
                return Err(RuleError::RoundFeeBudgetRefused(
                    "an unambiguous bonded permit and validated transaction fees are required".into(),
                ));
            }
            let burned = self.palw_v2_bond_burn_obligations(&state, point.daa_score);
            let maximum_sompi = template
                .block
                .transactions
                .iter()
                .skip(1)
                .zip(&template.calculated_fees)
                .try_fold(0u64, |sum, (tx, fee)| {
                    let bond_burned = tx
                        .inputs
                        .iter()
                        .map(|input| burned.get(&input.previous_outpoint).copied().unwrap_or(0))
                        .fold(0u64, u64::saturating_add);
                    sum.checked_add(self.palw_collectible_transaction_fee_v1(tx, *fee, bond_burned, point.daa_score))
                })
                .ok_or_else(|| RuleError::RoundFeeBudgetRefused("the collectible fee overflows".into()))?;
            budget
                .admit(kaspa_consensus_core::palw_bond_budget_v1::PalwRoundFeeIncomeV1 {
                    block: Default::default(),
                    bond: *recipients.first().expect("one permit recipient"),
                    maximum_sompi,
                    actual_sompi: maximum_sompi,
                })
                .map_err(|why| RuleError::RoundFeeBudgetRefused(why.to_string()))?;
        }
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
        // Either envelope: a permit holder's coordinates, or a slice (no round).
        let coords_of = |block: BlockHash| {
            self.headers_store.get_header(block).ok().and_then(|h| palw_exec_lane_coords_v1(&h.palw_commitment).ok())
        };
        let mut round_tips: Vec<(u64, BlockHash)> = self.body_tips_store
                .read()
                .get()
                .unwrap()
                .read()
                .iter()
                .copied()
                .filter(|tip| self.ghostdag_manager.is_round_block(*tip))
                .filter_map(|tip| {
                    coords_of(tip).map(|coords| match coords {
                        PalwExecLaneCoordsV1::Permit { round, .. } => (round, tip),
                        PalwExecLaneCoordsV1::Slice { .. } => (0, tip),
                    })
                })
                // A permit holder merges only rounds older than its own; a slice holds no round and merges any tip.
                .filter(|(tip_round, _)| round.is_none_or(|round| *tip_round < round))
                .filter(|(_, tip)| {
                    self.ghostdag_store.get_selected_parent(*tip).ok().is_some_and(|tip_anchor| {
                        self.reachability_service.try_is_chain_ancestor_of(tip_anchor, anchor).unwrap_or(false)
                    })
                })
                .collect();
        round_tips.sort_by_key(|(tip_round, tip)| std::cmp::Reverse((*tip_round, *tip)));
        let max_block_parents = self.max_block_parents.get(self.headers_store.get_daa_score(anchor).unwrap()) as usize;
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
            let lane_members: Option<Vec<_>> = ghostdag
                .unordered_mergeset_without_selected_parent()
                .filter(|member| self.ghostdag_manager.is_round_block(*member))
                .map(&coords_of)
                .collect();
            let Some(lane_members) = lane_members else { continue };
            let slices = lane_members.iter().filter(|coords| matches!(coords, PalwExecLaneCoordsV1::Slice { .. })).count();
            let members: Vec<_> = lane_members
                .iter()
                .filter_map(|coords| match coords {
                    PalwExecLaneCoordsV1::Permit { round, permit_index, bond } => Some((*round, *permit_index, *bond)),
                    PalwExecLaneCoordsV1::Slice { .. } => None,
                })
                .collect();
            // **A tip the validator would refuse to merge is not a parent** (the 2026-10-03 execution-lane wedge): a round
            // tip older than the merge-depth window — one no chain block ever merged — made EVERY round template
            // parent on it, and the header stage refused every block (`ViolatingBoundedMergeDepth`, 10,106 times on b0).
            // The same predicate the validator runs (`BlockDepthManager::merge_breaking_reds`), over the same data.
            if !self
                .depth_manager
                .merge_breaking_reds(&ghostdag, self.depth_manager.calc_merge_depth_root(&ghostdag, pruning_point))
                .is_empty()
            {
                continue;
            }
            if ghostdag.mergeset_size() as u64 - (members.len() + slices) as u64
                > self.mergeset_size_limit.get(self.headers_store.get_daa_score(anchor).unwrap())
                || (members.len() + slices) as u64 > lane.max_per_mergeset
                || palw_execution_mergeset_rule_v1(round, &members, round_width, lane.max_per_mergeset).is_err()
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
        // A permit holder is stamped inside its round; a slice holds no round, so it is stamped at the template's own time.
        let (round_start, timestamp) = match round {
            Some(round) => {
                let round_start = self.genesis.timestamp.saturating_add(round.saturating_mul(PALW_EXEC_ROUND_MS));
                let timestamp = round_start.max(past_median_time + 1);
                if timestamp >= round_start.saturating_add(PALW_EXEC_ROUND_MS) {
                    return Err(RuleError::TimeTooOld(round_start, past_median_time));
                }
                (round_start, timestamp)
            }
            None => {
                let timestamp = template.block.header.timestamp.max(past_median_time + 1);
                (timestamp, timestamp)
            }
        };
        let _ = round_start;
        // **A round the clock floor will refuse is not built** (the 2026-10-03 back-signing waste): the header stage's H5
        // (`step_stamp_admits`) refuses a block stamped before the slot its window's clock cursor opens, and a ticket signed
        // late for an old round is stamped at that round's start — 986 such round blocks were refused on b0. Asked here with the
        // validator's own predicate, over the clock this very template's window computed, so the producer neither solves
        // nor signs a round the chain cannot take.
        if let Err(early) = daa_window.clock.step_stamp_admits(timestamp) {
            return Err(RuleError::ClockStepBeforeItsSlot(Default::default(), timestamp, early.next_slot_ms));
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
        if round.is_none() {
            // An `EXEC_SLICE` carries no user transaction batch: the coinbase alone.
            transactions.truncate(1);
        }
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
}
