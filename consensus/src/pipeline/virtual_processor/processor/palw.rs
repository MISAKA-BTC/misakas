//! **`VirtualStateProcessor`: palw methods** — moved out of `processor.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl VirtualStateProcessor {
    /// **The 2026-09-23 Position route matrix, P-B3, at the mempool and at the template: the fold's
    /// market gate on a carrier before it relays and before it is mined.**
    /// `palw_model_market_carrier_refusal_v1` against the registry tip under the extras the virtual's
    /// next block folds with — built only for a seed or a buy. Asked by
    /// `validate_mempool_transaction_impl` when a carrier enters the pool AND by
    /// `validate_block_template_transaction` every time a template is built, so a carrier admitted
    /// while its class served and whose class has since regressed (Probation → Held, say) is
    /// evicted as `InvalidInBlockTemplate` instead of mined into a refusal. Node-local: no block rule
    /// reads it. `None` below `palw_audit_2026_09_23` (testnet-11), where the fold refuses nothing
    /// on this ground, and off ConsensusV2.
    ///
    /// It reads the TIP's lifecycle, and the fold of the block the carrier lands in steps the
    /// registry before it applies objects (fold step 1d), so the answer can be one lifecycle step
    /// behind the fold's, in either direction. A carrier that slips through that step is refused by
    /// the fold like any other; past the audit fence that refusal is refunded (P-B1).
    pub(super) fn palw_mempool_market_refusal(&self, tx: &Transaction, virtual_daa_score: u64) -> Option<String> {
        if !self.palw_market_gated_at(tx, virtual_daa_score) {
            return None;
        }
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        self.palw_mempool_market_refusal_with(tx, virtual_daa_score, chain_point, &state)
    }

    /// Whether [`Self::palw_mempool_market_refusal`] asks anything of `tx` at `virtual_daa_score`: a
    /// lifecycle carrier, at or past `palw_audit_2026_09_23` (the gate's fence) — asked before any tip
    /// is loaded, by the gate and by the sweep alike.
    pub(super) fn palw_market_gated_at(&self, tx: &Transaction, virtual_daa_score: u64) -> bool {
        tx.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
            && self.palw_audit_2026_09_23_at(virtual_daa_score)
    }

    /// [`Self::palw_mempool_market_refusal`] on a given tip (`chain_point`, `state`): the class's
    /// lifecycle (P-B3), then the payout queue's room for this one carrier (P-B1) — the sweep asks it
    /// on the one snapshot it checked against the virtual (the V01 review's LOW 3).
    pub(super) fn palw_mempool_market_refusal_with(
        &self,
        tx: &Transaction,
        virtual_daa_score: u64,
        chain_point: kaspa_consensus_core::BlockHash,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
    ) -> Option<String> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        if let Some(refusal) =
            kaspa_consensus_core::palw_state_v2::palw_model_market_carrier_refusal_v1(state, state_params, tx, || {
                self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                    block: chain_point,
                    daa_score: virtual_daa_score,
                    blue_score: 0,
                    subsidy: 0,
                }).into_legacy_extras()
            })
        {
            return Some(refusal.to_string());
        }
        // **P-B1 (refunds count against the cap): a carrier whose move or refund the payout queue
        // could not take is refused too**, so it is neither relayed nor mined while the queue is
        // full — the fold would refuse it and have no row to pay it back from, and its MSK would stay
        // in the sink. A fresh budget: this one carrier against the room the next block leaves. The
        // template carries one budget across its carriers as well (`palw_template_market_budget`).
        kaspa_consensus_core::palw_state_v2::PalwModelCarrierBudgetV1::at_tip(state).admit(tx).err().map(|full| {
            format!("{full}: its move or its refund would overflow the payout queue (P-B1: refunds count against the cap)")
        })
    }

    /// **MSK-26A (2026-09 pre-freeze security review): this fleet never relays or mines a DNS slashing
    /// / precommit evidence that is not genuine at its tip.**
    ///
    /// A NODE policy, not a consensus rule (the fold alone judges a block another node mined). The
    /// own-body block-validity rule (`check_slashing_evidence_genuine`) already rejects a block that
    /// carries forged evidence in its OWN body, but evidence riding in a MERGE-blue block still
    /// reaches the UTXO side-effect and — until `palw_slashing_evidence_utxo_genuine` is armed —
    /// burns an honest validator's staked output-0 on unchecked signatures. So this fleet refuses to
    /// admit or template such a transaction at all: `proved_slash_targets` over the single tx, at the
    /// tip's bond view, is exactly the registry path's genuineness (both signatures verify against
    /// the accused bond's registered validator key, fresh, and the bond held slashable stake). Empty
    /// means the evidence proves no slash, so this node neither relays nor mines it. `None` when the
    /// DNS overlay is off or the tx is not slashing/precommit evidence, so every other path is
    /// unchanged. Read at the tip, so a genuine evidence about a bond this node has not yet synced is
    /// declined here and admitted once the bond is in view — a conservative node policy, never a fork.
    pub(in crate::pipeline::virtual_processor) fn palw_slashing_evidence_mempool_refusal(
        &self,
        tx: &Transaction,
        virtual_daa_score: u64,
    ) -> Option<String> {
        use kaspa_consensus_core::dns_finality::{DnsTxKind, dns_tx_kind};
        let dns_params = self.dns_params.as_ref()?;
        if virtual_daa_score < dns_params.dns_activation_daa_score {
            return None;
        }
        if !matches!(dns_tx_kind(&tx.subnetwork_id), Some(DnsTxKind::SlashingEvidence) | Some(DnsTxKind::PrecommitEvidence)) {
            return None;
        }
        // The tip bond view (the stake-bond store snapshot = state at the sink): the same view the
        // block-validity genuineness rule and the template's reward fan-out read.
        let bond_view = self.initial_active_bond_view();
        let proved = super::super::utxo_validation::proved_slash_targets(
            std::slice::from_ref(tx),
            &bond_view,
            self.genesis.hash,
            virtual_daa_score,
            dns_params.evidence_window_blocks,
        );
        proved.is_empty().then(|| {
            "its attestation signatures do not verify against the accused bond's registered validator key at this tip (or the \
             named bond is unknown, stale or not slashable): a forged slash this fleet will neither relay nor mine"
                .to_string()
        })
    }

    /// **ADR-0152 v3.1 H-1 (P2-9 review, finding 5): the fold's own answer on an H-1 carrier, before
    /// this node admits, relays, spares or mines it** — the P-B3 pattern for the objects H-1 obliges a
    /// heartbeat to carry (`palw_heartbeat_carriers_v1`).
    ///
    /// Those objects buy three privileges on testnet-12: the first half of every template (the
    /// carrier lane), a reserve in a full mempool, and a heartbeat's pass through the relay's H2
    /// limits. Sold by object KIND, all three went to anything that decodes — five 8-byte-signature
    /// accusations out-ranked one honest ML-DSA-87 filing at the same feerate. So the carrier is put to
    /// the fold before it gets any of them: the acceptance layer (`palw_v2_validate_objects` — the
    /// bond it names exists and signed it, the evidence adjudicates) and then the object's own arm
    /// (`palw_v2_apply_one_object_v1` — the claim exists and is in its window, no session is open, the
    /// offence is not already convicted, the reveal has a pending reward), on the tip, at the
    /// virtual's DAA. Asked by `validate_mempool_transaction_impl` when the carrier enters and by
    /// `validate_block_template_transaction` every time a template is built, so one that the tip
    /// stops taking (the second accusation of a claim, once the first has opened its session) is
    /// evicted as `InvalidInBlockTemplate` instead of mined into a drop its filer pays for. "Node
    /// policy never builds what the fold refuses."
    ///
    /// Node-local, never a block rule: the fold alone judges a block another node mined. `None` for
    /// every transaction that carries no H-1 object and below `Params::palw_rcore_plus`, so every
    /// network but testnet-12 admits and templates exactly as before. It reads the TIP and skips the
    /// next block's pre-object sweeps, so it can be one fold step behind in either direction — what
    /// slips through is the fold's, as it always was.
    ///
    /// **Every possession proof too** (the 2026-09-25 model-registry review, M1): past R-core+ a
    /// proof whose row nears staleness takes a reserved place and the head of the template lane
    /// (`palw_readiness_escalation_v1`), and whether it does moves with the DAA after it entered —
    /// so every proof is put to the fold here, at admission and at every template, and a junk proof
    /// naming any bond's absent row is never kept, let alone hurried. A proof is judged at its own
    /// span's first DAA when that is at most one DAA past the virtual's
    /// (`palw_readiness_gate_daa_v1`): a seat names its own virtual's span, and a node the newest
    /// block has not reached yet must not turn the honest proof away where no gate asked before.
    pub(in crate::pipeline::virtual_processor) fn palw_mempool_h1_carrier_refusal(
        &self,
        tx: &Transaction,
        virtual_daa_score: u64,
    ) -> Option<String> {
        let object = self.palw_h1_gated_object_at(tx, virtual_daa_score)?;
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        self.palw_mempool_h1_carrier_refusal_with(&object, virtual_daa_score, chain_point, &state)
    }

    /// The object [`Self::palw_mempool_h1_carrier_refusal`] puts to the fold: `None` below
    /// `palw_rcore_plus` (the gate's fence) and for a transaction that carries no gated object —
    /// asked before any tip is loaded, by the gate and by the sweep alike.
    pub(super) fn palw_h1_gated_object_at(
        &self,
        tx: &Transaction,
        virtual_daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        if !self.palw_rcore_plus_at(virtual_daa_score) {
            return None;
        }
        kaspa_consensus_core::palw_readiness_escalation_v1::palw_gated_carrier_object_of_tx_v1(tx)
    }

    /// [`Self::palw_mempool_h1_carrier_refusal`] on a given tip (`chain_point`, `state`) for the
    /// object the transaction carries — the DAA the gate judges a possession proof at, the point, both
    /// layers — so a test can put a proof through it on a state whose class root it can open (the M1
    /// review, LOW 1: the gate's clock was reachable only through the tip).
    pub(in crate::pipeline::virtual_processor) fn palw_mempool_h1_carrier_refusal_with(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        virtual_daa_score: u64,
        chain_point: kaspa_consensus_core::BlockHash,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
    ) -> Option<String> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        // M1: a proof names its seat's virtual span — judged at that span's first DAA when the seat
        // is at most one block ahead of this node, so a peer the newest block has not reached yet
        // does not turn an honest proof away (`palw_readiness_gate_daa_v1`).
        let daa_score = match object {
            kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::SeatReadinessProved { span, .. }
            | kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::SeatReadinessProvedV2 { span, .. } => {
                match self.palw_model_registry_fold_at(virtual_daa_score) {
                    Some(fold) => kaspa_consensus_core::palw_readiness_escalation_v1::palw_readiness_gate_daa_v1(
                        virtual_daa_score,
                        *span,
                        fold.span_daa,
                    ),
                    None => virtual_daa_score,
                }
            }
            _ => virtual_daa_score,
        };
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score,
            // The next block's: past the tip's, which is all the fold asks of it for these objects.
            blue_score: state.last_point().map_or(0, |last| last.blue_score.saturating_add(1)),
            subsidy: 0,
        };
        self.palw_h1_carrier_refusal_on(state, state_params, &point, object)
    }

    /// The single-Origin writer starts with a fresh work budget, so this one proof cannot exhaust it.
    /// Ask the core's read-only verdict once, retaining the no-V2-bundle gate and both refusal variants.
    pub(super) fn palw_exec_origin_rehearsal_v1_on(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1> {
        if !matches!(object, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ExecOriginRecordedV1 { .. }) {
            return None;
        }
        if self.palw_panel_params_v2.is_none() {
            return Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::NotAccepted(
                "PALW objects on a network with no V2 bundle".into(),
            ));
        }
        Some(kaspa_consensus_core::palw_state_v2::palw_v2_origin_rehearsal_v1(state, state_params, point, object, || {
            self.palw_transition_extras_for_objects(state, point, std::slice::from_ref(object)).into_legacy_extras()
        }))
    }

    /// [`Self::palw_mempool_h1_carrier_refusal`]'s question on a given state and point: the
    /// acceptance layer first (cheap refusals — an unknown bond, a signature that does not verify —
    /// before any state is cloned), then the fold's own conditions. Origin asks the read-only
    /// verdict once; all other objects use the single-object writer. `None` when both take it.
    /// A dormant public-filer exception relays an authenticated binding which the objective court
    /// convicts, while the DA fold still refuses it as an answer. No unauthenticated binding gets
    /// this exception.
    pub(in crate::pipeline::virtual_processor) fn palw_h1_carrier_refusal_on(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<String> {
        if let Some(verdict) = self.palw_exec_origin_rehearsal_v1_on(state, state_params, point, object) {
            use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1 as Verdict;
            return match verdict {
                Verdict::Accepted => None,
                Verdict::NotAccepted(why) => Some(why),
                Verdict::Refused(why) => Some(why.to_string()),
            };
        }
        if let Err(why) = self.palw_v2_validate_objects(state, state_params, point, std::slice::from_ref(object)) {
            return Some(why);
        }
        let refusal = kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_retained_v1(
            state,
            state_params,
            point,
            object,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for_objects(state, point, std::slice::from_ref(object)),
        )
        .err()?;
        if matches!(refusal, kaspa_consensus_core::palw_state_v2::PalwStateV2Error::DaOpeningRefused { .. })
            && self.palw_legacy_public_filer_at(point.daa_score)
            && let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::MaterialDisclosedV2 {
                claim,
                unit: kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::Event { row: 0, tile: 0 },
                answer: kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Event(event),
                ..
            } = object
            && let Some(target) = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(state, claim)
            && let Ok(proof) = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_v1(
                *claim,
                target.executor_bond,
                kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::IdentityMismatch { binding: event.binding().clone() },
                None,
            )
            && matches!(
                self.palw_object_rehearsal_v1_on(state, state_params, point, &proof.object),
                kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted
            )
        {
            // The DA arm already checked the live demand and discloser's liability; the
            // acceptance layer checked its signature/size. The second rehearsal proves the
            // claim's authenticated identity fault. Carriage changes no DA answered bit and
            // applies no conviction: the outside verifier still files its own proof.
            return None;
        }
        Some(refusal.to_string())
    }

    /// **The 2026-09-25 sweep, V01: [`Self::palw_mempool_h1_carrier_refusal`] on each of `txs`, at
    /// the virtual's DAA** — the gate the mempool asks at admission and the template asks at every
    /// build, asked now by every node's mempool at each new block over the carriers it holds. The
    /// template's re-ask was the only way out for a carrier the tip stopped taking, so a node that
    /// never builds a template (a seat-only node, a relay) kept it — and a seat's one carrier slot
    /// with it — until the Low-priority expiry, 720 DAA later. Node-local; all `None` with no
    /// virtual state, and below `palw_rcore_plus` (the gate's own fence).
    ///
    /// **In the template's order: the UTXO context before the gate** (the V01 review, MEDIUM). The
    /// mempool hears of a block after the virtual may already have folded it — `on_new_block` walks
    /// an ancestor batch one block at a time while the virtual resolves the batch at once, and an
    /// IBD block the same — so the tip can already hold a carrier the pool still holds, and the fold
    /// refuses a duplicate: the honest carrier would be evicted as "refused", its redeemers with
    /// it. The template never asks such a carrier (its inputs are gone, `MissingTxOutpoints` first),
    /// and neither does the sweep: a transaction with an input the virtual UTXO set does not hold is
    /// not judged (`None`) — mined, the block handler removes it; double-spent, the double-spend
    /// sweep does; chained on a pooled parent, it is judged at the block after the parent's.
    ///
    /// **One tip, the virtual's own.** The tip row is written before the virtual commits (the sink
    /// search writes it at every candidate), so the gate's usual read can stand a block ahead of the
    /// UTXO set just read. The sweep loads the tip once, under the same virtual read as the UTXO
    /// facts, and judges nothing unless it stands at the virtual's sink; every transaction is then
    /// put to the gate's own layers (`palw_mempool_h1_carrier_refusal_with`) on that one snapshot.
    pub fn palw_h1_carrier_refusals_v1_impl(&self, txs: &[Arc<Transaction>]) -> Vec<Option<String>> {
        let Some((virtual_daa, chain_point, state, spendable)) = self.palw_sweep_snapshot_v1(txs) else {
            return vec![None; txs.len()];
        };
        txs.iter()
            .zip(spendable)
            .map(|(tx, spendable)| {
                if !spendable {
                    return None;
                }
                let object = self.palw_h1_gated_object_at(tx, virtual_daa)?;
                self.palw_mempool_h1_carrier_refusal_with(&object, virtual_daa, chain_point, &state)
            })
            .collect()
    }

    /// **The mempool sweep's market half** (the V01 review's LOW 3; the user's request of
    /// 2026-09-25) — `ConsensusApi::palw_market_carrier_refusals_v1`. The P-B3 / P-B1 gate
    /// (`palw_mempool_market_refusal`) was asked when a seed, a buy, a carrier sell or a top-up
    /// entered the pool and at every template, so a node that builds no template kept one its class's
    /// lifecycle or the payout queue stopped taking — and the user's inputs with it — until the
    /// Low-priority expiry. Asked here in the H-1 sweep's order and on its snapshot
    /// ([`Self::palw_sweep_snapshot_v1`]): what the virtual can still spend, on the virtual's own tip,
    /// through the gate's own layers (`palw_mempool_market_refusal_with`). Node-local; all `None` with
    /// no virtual state and below `palw_audit_2026_09_23` (the gate's own fence).
    pub fn palw_market_carrier_refusals_v1_impl(&self, txs: &[Arc<Transaction>]) -> Vec<Option<String>> {
        let Some((virtual_daa, chain_point, state, spendable)) = self.palw_sweep_snapshot_v1(txs) else {
            return vec![None; txs.len()];
        };
        txs.iter()
            .zip(spendable)
            .map(|(tx, spendable)| {
                if !spendable || !self.palw_market_gated_at(tx, virtual_daa) {
                    return None;
                }
                self.palw_mempool_market_refusal_with(tx, virtual_daa, chain_point, &state)
            })
            .collect()
    }

    /// **The one snapshot a mempool sweep judges on** (the V01 review, MEDIUM): under one virtual read,
    /// the virtual's DAA, the PALW tip — only when it stands at the virtual's sink (the tip row is
    /// written ahead of the virtual commit; mid-resolve the next block's sweep asks again) — and, per
    /// transaction, whether the virtual UTXO set holds every input it spends (the template's order:
    /// the UTXO context before the gate). `None` with no virtual state, off `ConsensusV2` or with no tip.
    #[allow(clippy::type_complexity)]
    pub(super) fn palw_sweep_snapshot_v1(
        &self,
        txs: &[Arc<Transaction>],
    ) -> Option<(
        u64,
        kaspa_consensus_core::BlockHash,
        Arc<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>,
        Vec<bool>,
    )> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().ok()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        if chain_point != virtual_state.ghostdag_data.selected_parent {
            return None;
        }
        let spendable: Vec<bool> = txs
            .iter()
            .map(|tx| tx.inputs.iter().all(|input| UtxoView::get(&virtual_read.utxo_set, &input.previous_outpoint).is_some()))
            .collect();
        Some((virtual_state.daa_score, chain_point, state, spendable))
    }

    /// **P-B1 at the template: one payout-queue budget across every market carrier the template
    /// selects**, so two carriers that each fit the room alone but not together are not both mined —
    /// the second stays in the pool for a later template. `None` below `palw_audit_2026_09_23`
    /// (testnet-11) and off ConsensusV2, where no budget is kept. Node-local.
    pub(super) fn palw_template_market_budget(
        &self,
        virtual_daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwModelCarrierBudgetV1> {
        if !self.palw_audit_2026_09_23_at(virtual_daa_score) {
            return None;
        }
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        Some(kaspa_consensus_core::palw_state_v2::PalwModelCarrierBudgetV1::at_tip(&state))
    }

    /// ADR-0109 Decision 3: the PALW bond gate at the virtual tip — the bonds the registry holds
    /// locked (the set the acceptance path skips spends of) and the burn each released, slashed bond's
    /// spend owes (audit C-08 part three) — memoised per (registry tip, DAA). The same two functions
    /// the chain walk and the template read (`palw_v2_locked_bond_outpoints`,
    /// `palw_v2_bond_burn_obligations`), so the mempool refuses exactly the spends every block skips.
    /// `None` when the network has no V2 registry or the registry has no tip yet.
    pub(super) fn palw_mempool_bond_gate(&self, now_daa: u64) -> Option<Arc<PalwMempoolBondGate>> {
        let params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_v2_reader_state(params).ok().flatten()?;
        let mut cache = self.palw_mempool_locked_cache.lock();
        if let Some((cached_tip, cached_daa, gate)) = cache.as_ref()
            && *cached_tip == tip
            && *cached_daa == now_daa
        {
            return Some(gate.clone());
        }
        let gate = Arc::new(PalwMempoolBondGate {
            locked: self.palw_v2_locked_bond_outpoints(&state, now_daa),
            burns: self.palw_v2_bond_burn_obligations(&state, now_daa),
        });
        *cache = Some((tip, now_daa, gate.clone()));
        Some(gate)
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
        self.palw_candidate_order_v2_checked(candidate).ok().flatten()
    }

    /// A local capacity refusal aborts the selection attempt; it cannot rank a
    /// candidate below another branch or be counted as a consensus refusal.
    pub(crate) fn palw_candidate_order_v2_checked(
        &self,
        candidate: BlockHash,
    ) -> Result<
        Option<kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1>,
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
    > {
        let Some(state) = self.palw_candidate_state_for_selection_v2(candidate)? else { return Ok(None) };
        let (frontier_blue_score, _) = state.safe_frontier();
        Ok(Some(kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1::new(
            frontier_blue_score,
            state.safe_weight(),
            state.bounded_immature(),
            candidate,
        )))
    }

    pub(crate) fn palw_candidate_state_for_selection_v2(
        &self,
        candidate: BlockHash,
    ) -> Result<
        Option<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>,
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
    > {
        match self.palw_candidate_state_v2_checked(candidate) {
            Ok(state) => Ok(Some(state)),
            Err(PalwWeighFaultV2::NativeCapacityUnavailable) => Err(Self::palw_selection_capacity_unavailable_v1()),
            Err(PalwWeighFaultV2::NoOpinion | PalwWeighFaultV2::StoreUnreadable) => Ok(None),
        }
    }

    fn palw_selection_capacity_unavailable_v1() -> kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1 {
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
            kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1,
        >(
            "candidate selection native state",
            "local typed native state or delta capacity unavailable; retry without changing fork choice",
        )
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
    #[cfg(test)]
    pub(crate) fn palw_frontier_provenance_outcome(&self, candidate: BlockHash, prev_sink: BlockHash) -> DnsReorgOutcome {
        self.palw_frontier_provenance_outcome_checked(candidate, prev_sink).expect("fixture has native selection capacity")
    }

    pub(crate) fn palw_frontier_provenance_outcome_checked(
        &self,
        candidate: BlockHash,
        prev_sink: BlockHash,
    ) -> Result<DnsReorgOutcome, kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;

        // Read at the INCUMBENT's DAA: a candidate's own score is attacker-chosen, and this is the
        // same one-sided clock the confirmed-anchor TTL uses a few hundred lines below.
        let incumbent_daa = self.headers_store.get_daa_score(prev_sink).unwrap_or(0);
        if !self.palw_frontier_provenance.is_some_and(|fence| fence.is_active(incumbent_daa)) {
            return Ok(DnsReorgOutcome::GateInactive);
        }
        let Some(panel_params) = self.palw_panel_params_v2.as_ref() else { return Ok(DnsReorgOutcome::GateInactive) };

        // **A veto that cannot name a fork point abstains.** The horizon is `finality_depth`, which
        // is exactly the deepest reorg the sink search will ever offer, and it is canonical-side —
        // it bounds how far THIS node would rewind, not how long the challenger's branch is, which
        // is the right metric for an attack whose private branch may be thousands of blocks while
        // the canonical-side fork depth is one. Refusing on a missing ancestor would be the
        // permanent-partition shape this file already learned to escape.
        let horizon = self.finality_depth;
        let Some(ancestor) = self.chain_common_ancestor_within(candidate, prev_sink, horizon) else {
            warn!("PALW frontier provenance abstains: no common ancestor for {candidate} and {prev_sink} within {horizon}");
            return Ok(DnsReorgOutcome::GateInactive);
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
                (Err(PalwWeighFaultV2::NativeCapacityUnavailable), _) | (_, Err(PalwWeighFaultV2::NativeCapacityUnavailable)) => {
                    return Err(Self::palw_selection_capacity_unavailable_v1());
                }
                (Err(PalwWeighFaultV2::StoreUnreadable), _) | (_, Err(PalwWeighFaultV2::StoreUnreadable)) => {
                    warn!(
                        "deep reorg refused: this node's own PALW snapshot is unavailable, so ADR-0065 D2 cannot clear \
                         candidate {candidate} — a veto that cannot read its evidence does not wave the reorg through"
                    );
                    return Ok(DnsReorgOutcome::FrontierProvenanceViolation);
                }
                _ => {
                    warn!("PALW frontier provenance abstains: this node cannot materialize both sides of the fork at {ancestor}");
                    return Ok(DnsReorgOutcome::GateInactive);
                }
            };

        let minted: std::collections::BTreeSet<_> =
            challenger_state.bonds_iter().map(|(k, _)| *k).filter(|k| ancestor_state.bond(k).is_none()).collect();
        if minted.is_empty() {
            return Ok(DnsReorgOutcome::GateInactive);
        }

        // **The fast path is exact, not an approximation.** `PalwPanelParamsV2::new` enforces
        // `2*quorum > seat_count`, so a panel cannot reach quorum out of newly-minted seats unless
        // more than `seat_count - quorum` of them exist. Below that threshold no scan can find a
        // violation, so not scanning is a proof rather than a shortcut. On the shipped (5, 3) that
        // is 2, so a static registry never pays for the walk.
        if !kaspa_consensus_core::palw_panel_v2::palw_minted_seats_can_reach_quorum_v1(minted.len(), panel_params) {
            return Ok(DnsReorgOutcome::GateInactive);
        }

        // Past it, the panels themselves. Read off the DELTAS rather than the challenger's tip
        // state: `retire_claim` deletes a claim and its panel while `safe_frontier` never retreats,
        // so the frontier can stand on a panel the tip no longer holds. The deltas record every
        // panel this branch ever bound, verbatim.
        let path = self.dag_traversal_manager.calculate_chain_path(ancestor, candidate, None);
        let store = self.palw_state_v2_store.read();
        for block in path.added.iter() {
            let (_, delta) = match store.delta_of_retained_v1(*block) {
                Ok(row) => row,
                Err(kaspa_database::prelude::StoreError::LocalResourceUnavailable(_)) => {
                    return Err(Self::palw_selection_capacity_unavailable_v1());
                }
                Err(_) => {
                    drop(store);
                    warn!("PALW frontier provenance abstains: delta for {block} is unavailable");
                    return Ok(DnsReorgOutcome::GateInactive);
                }
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
                    return Ok(DnsReorgOutcome::FrontierProvenanceViolation);
                }
            }
        }
        Ok(DnsReorgOutcome::GateInactive)
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
    ) -> Option<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1> {
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
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1, PalwWeighFaultV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Err(PalwWeighFaultV2::NoOpinion) };
        let store = self.palw_state_v2_store.read();
        let (tip_block, tip_state) = match store.load_tip_retained_v1(state_params) {
            Ok(Some(loaded)) => loaded,
            Ok(None) => return Err(PalwWeighFaultV2::NoOpinion),
            Err(kaspa_database::prelude::StoreError::LocalResourceUnavailable(reason)) => {
                warn!("PALW candidate weighing awaits local native capacity: {reason}");
                return Err(PalwWeighFaultV2::NativeCapacityUnavailable);
            }
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
        crate::processes::palw_state_walk::walk_chain_path_retained_v1(&store, state_params, tip_state, &removed, &added).map_err(
            |e| {
                use crate::processes::palw_state_walk::PalwStateWalkError as W;
                if e.native_capacity_v1().is_some() {
                    warn!("PALW candidate walk awaits local native capacity: {e}");
                    return PalwWeighFaultV2::NativeCapacityUnavailable;
                }
                match e {
                    W::MissingDelta(_) | W::NoAnchor => PalwWeighFaultV2::NoOpinion,
                    W::Store(_) | W::State(_) | W::Resource(_) => {
                        warn!("PALW state fault: this node's own V2 delta rows would not walk ({e}); it can weigh no candidate");
                        PalwWeighFaultV2::StoreUnreadable
                    }
                }
            },
        )
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
        self.palw_weighing_point_v2_checked(sink).ok().flatten()
    }

    pub(crate) fn palw_weighing_point_v2_checked(
        &self,
        sink: BlockHash,
    ) -> Result<Option<BlockHash>, kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        if self.reachability_service.has_reachability_data(sink) {
            return Ok(Some(sink));
        }
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Ok(None) };
        // **`load_tip`, not `tip_record`** — fail CLOSED on a snapshot that cannot be read, and
        // the raw row cannot tell you that. `set_tip_batch` derives the root from the state it is
        // handed, so a tampered carriage is detected only by rebuilding it and demanding the
        // recorded root, which is what `load_tip` does. Reading just the row would hand a chain
        // whose state this node cannot reproduce back as its own standing — a state fault
        // promoted to the only opinion this consensus has.
        let tip = match self.palw_state_v2_store.read().load_tip_retained_v1(state_params) {
            Ok(Some((block, _))) => block,
            Ok(None) => return Ok(None),
            Err(kaspa_database::prelude::StoreError::LocalResourceUnavailable(_)) => {
                return Err(Self::palw_selection_capacity_unavailable_v1());
            }
            Err(e) => {
                warn!("PALW state fault: the V2 tip snapshot is unreadable ({e}); this consensus offers no weighing point");
                return Ok(None);
            }
        };
        Ok(self.reachability_service.has_reachability_data(tip).then_some(tip))
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
        let state = match store.load_tip_retained_v1(state_params) {
            Ok(Some((_, state))) => state,
            Ok(None) => return true,
            Err(_) => return false,
        };
        let Ok(header) = self.headers_store.get_header(candidate_point) else { return false };
        // G14 lane closure G-K4 (dormant: rows exist only past the kernel route's fence): a segmented claim's served parts live only
        // in block bodies, so pruning never passes the first block that served one while that claim is still convictable.
        if state.kernel_route().and_then(|route| route.served_material_pruning_ceiling_v1()).is_some_and(|c| header.blue_score > c) {
            return false;
        }
        let (frontier, _) = state.safe_frontier();
        // **RFC-0018 §7.7 (Gate C)**: past the fence the frontier moves only on SecurityCertified claims, so pruning must not wait for
        // it — the ceiling is the pruning-progress rule, independent of security credit: below the oldest unresolved claim and the
        // oldest Final claim whose certification is still undecided (history an open claim, court or certification still reads),
        // never above. Below the fence (every shipped network) the frontier rule, unchanged.
        let gate_c =
            state_params.rfc0018_v1().is_some_and(|mirror| mirror.gate_c_at(state.last_point().map_or(0, |point| point.daa_score)));
        if gate_c {
            // Two bounds, both independent of whether anything certifies: the progress ceiling (open claims, courts, undecided
            // certifications) and the ROLLBACK bound — the lowest frontier a still-open conviction window could move the frontier
            // back to (§7.5a/§7.7). Pruning never passes a point that may become the frontier again.
            let progress = kaspa_consensus_core::palw_security_certification_v1::palw_pruning_progress_ceiling_v1(&state);
            let rollback = kaspa_consensus_core::palw_security_certification_v1::palw_frontier_rollback_bound_v1(&state, state_params);
            return match (progress, rollback) {
                (None, None) => true,
                (a, b) => header.blue_score <= a.into_iter().chain(b).min().unwrap_or(u64::MAX),
            };
        }
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
    ///
    /// **Past `Params::palw_rcore_plus` (ADR-0152 V-2, testnet-12) `Final` names a vesting ROW, not
    /// a payout**: the same producer, seat and reserve amounts, with the payees fixed at `Final`
    /// (I-5), held until V-4 matures the row; the payout is named at maturity, when step 3d moves the
    /// row's legs into the queue `palw_v2_payout_outputs` renders — so "which claims became
    /// spendable" is then "which rows matured", and the reporter reward (ADR-0152 §3.6 R), the other
    /// non-producer payee this paragraph anticipated, reaches the queue through the same step.
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
    /// **RFC-0007 §I.6: the equivocations this node has seen and the chain has not yet convicted** — the evidence a seat's node
    /// carries (any funded node may). Entries the tip already convicts, or whose round is no longer provable, are forgotten here.
    pub fn palw_v2_pending_vertex_equivocations_impl(&self) -> Vec<kaspa_consensus_core::palw_vertex_v1::PalwVertexEquivocationV1> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
            return Vec::new();
        };
        let tip_daa = self.lkg_virtual_state.load().daa_score;
        let mut watch = self.palw_vertex_watch.lock().unwrap();
        for evidence in watch.pending() {
            let convicted = state.vertex_round_row_v1(evidence.a.round, &evidence.a.seat_bond).is_some_and(|row| row.convicted);
            let stale = tip_daa.saturating_sub(evidence.a.signed_daa.max(evidence.b.signed_daa))
                > kaspa_consensus_core::palw_vertex_v1::PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1;
            if convicted || stale {
                watch.forget(evidence.a.round, &evidence.a.seat_bond);
            }
        }
        watch.pending()
    }

    pub fn palw_disputable_claims_v2_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
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
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        // The tip's DAA: this is the verdict the close WOULD get if it rode the next block.
        let daa_score = self.virtual_stores.read().state.get().ok()?.daa_score;
        let step_ladder = self.palw_court_step_ladder_at(daa_score, court);
        let form = self.palw_prompt_ids_form_at(daa_score);
        kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v5(
            &state,
            session_id,
            proof,
            court,
            step_ladder,
            form,
            self.palw_held_context_at(daa_score),
            // ADR-0152 v3.1 addendum §4-bis.9: the court door.
            self.palw_offence_attribution_at(daa_score),
            // RFC-0001 §A: a decode-token close does not try an FP Job V4 claim.
            self.palw_fp_decode_rules
                .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never())
                .map(|f| f.daa_score()),
            // RFC-0006 R6-CELL round 4: the coverage closure's mirror (None on every shipped network).
            state_params.cell_coverage_closure_from_daa(),
            // RFC-0006 consumer v2: `palw_memory_routing_court_v2`'s mirror (None on every shipped network).
            state_params.memory_routing_court_from_daa(),
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
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_producer_v2::palw_court_duties_v2(&state, mine)
    }

    /// **ADR-0152 §4-ter.3 step 6: the held forfeits and open DA sessions `mine` holds**, at the tip.
    pub fn palw_held_pursuit_seeds_v1_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> kaspa_consensus_core::palw_producer_v2::PalwHeldPursuitSeedsV1 {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Default::default() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
            return Default::default();
        };
        kaspa_consensus_core::palw_producer_v2::palw_held_pursuit_seeds_v1(&state, mine)
    }

    /// **The data-availability duties this node holds** (ADR-0062 D3): every claim under an open
    /// accusation whose producing bond is in `mine`, with the event it must open.
    pub fn palw_da_duties_v2_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwDaDutyV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_producer_v2::palw_da_duties_v2(&state, state_params, mine)
    }

    /// **ADR-0152 X7 / DA-4 (Phase 2, P2-7): the R-core court's duties this node holds** — at the
    /// tip, for the DAA the virtual's next block folds at, under the extras that block folds with
    /// (the in-run rows' fence, the second clock), so an answer the responder builds is one the fold
    /// takes from that discloser. Empty below `palw_rcore_plus`.
    pub fn palw_disclosure_duties_v1_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> kaspa_consensus_core::palw_producer_v2::PalwDisclosureDutiesV1 {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Default::default() };
        let Some((chain_point, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
            return Default::default();
        };
        let Some(candidate_daa) = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score) else {
            return Default::default();
        };
        self.palw_disclosure_duties_v1_at(&state, chain_point, candidate_daa, mine)
    }

    /// [`Self::palw_disclosure_duties_v1_impl`] on a given state, for the block at `now_daa` on
    /// `chain_point` — the one place the fold's extras are resolved for the duty read (the real-claim
    /// tests call it on the state they folded).
    pub(crate) fn palw_disclosure_duties_v1_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        chain_point: kaspa_consensus_core::BlockHash,
        now_daa: u64,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> kaspa_consensus_core::palw_producer_v2::PalwDisclosureDutiesV1 {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Default::default() };
        let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score: now_daa,
            blue_score: 0,
            subsidy: 0,
        });
        kaspa_consensus_core::palw_producer_v2::palw_disclosure_duties_v1(state, state_params, &extras, mine, now_daa)
    }

    /// **ADR-0152 §3.8 (Phase 2, P2-6): what an automatic accusation of `claim` by `accuser` comes
    /// to** — at the tip, for the DAA the virtual's next block folds at, under the extras that block
    /// folds with (the second clock A-6's room reads, the in-run rows' fence), so an accusation the
    /// seat files is one the fold opens a session for. `None` with no tip state.
    /// **Lane LG14-A (RPC 204): one claim's dispute view at the tip.**
    pub fn palw_legacy_dispute_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwLegacyDisputeViewV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        state.palw_legacy_dispute_view_v1(state_params, &claim)
    }

    /// G14 ③ (lane gdn-filer): the tip's DAA for a node-side read — the virtual's, as `palw_court_close_verdict_v2_impl` reads it.
    fn palw_gdn_step_tip_daa_v1(&self) -> Option<u64> {
        self.virtual_stores.read().state.get().ok().map(|s| s.daa_score)
    }

    pub fn palw_gdn_step_candidates_v1_impl(
        &self,
        me: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
        limit: usize,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwGdnStepCandidateV1> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some(daa) = self.palw_gdn_step_tip_daa_v1() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else { return Vec::new() };
        kaspa_consensus_core::palw_state_v2::palw_gdn_step_candidates_v1(
            &state,
            state_params,
            &me,
            daa,
            self.palw_offence_attribution_at(daa),
            after,
            limit,
        )
    }

    pub fn palw_gdn_step_candidate_v1_impl(
        &self,
        me: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        claim: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwGdnStepCandidateV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let daa = self.palw_gdn_step_tip_daa_v1()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        kaspa_consensus_core::palw_state_v2::palw_gdn_step_candidate_v1(
            &state,
            state_params,
            &me,
            &claim,
            daa,
            self.palw_offence_attribution_at(daa),
        )
    }

    /// The fold's own `GdnStepAccusedV2` limits (`palw_accept.rs`'s `memory_routing_court` extras + `objects.rs`'s
    /// `class_step_ladder_v1`), resolved at the tip's DAA.
    pub fn palw_gdn_court_limits_v2_impl(
        &self,
        class_id: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_gdn_court_v2::PalwGdnCourtLimitsV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let court = self.palw_court_params_v2.as_ref()?;
        let daa = self.palw_gdn_step_tip_daa_v1()?;
        if !self.palw_memory_routing_court_v2_at(daa) {
            return None;
        }
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        Some(kaspa_consensus_core::palw_gdn_court_v2::PalwGdnCourtLimitsV2 {
            ladder: state.class_step_ladder_v1(&class_id, self.palw_court_step_ladder_at(daa, court)),
            max_close_bytes: court.max_close_bytes(),
        })
    }

    pub fn palw_staged_paid_authority_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedPaidAuthorityV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("staged snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        Ok(kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_paid_authority_v1(&state, key)?
            .filter(|p| now < p.terms.material_until_daa))
    }

    pub fn palw_staged_owned_context_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<
        Option<(
            kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
            kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        )>,
        String,
    > {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("staged snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_owned_context_v1(&state, key)
    }

    pub fn palw_tokenizer_artifact_inventory_v1_impl(
        &self,
        class: kaspa_consensus_core::Hash64,
        opener: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwTokenizerArtifactInventoryProgressV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("inventory snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_tokenizer_artifact_inventory_v1(&state, class, opener)
    }

    pub fn palw_staged_preclaim_quote_v1_impl(
        &self,
        original: kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        header: kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    ) -> Result<
        Option<(kaspa_consensus_core::BlockHash, kaspa_consensus_core::palw_staged_preclaim_v1::PalwStagedPreclaimQuoteV1)>,
        String,
    > {
        header.check_wire_v1()?;
        if borsh::object_length(&original).map_err(|e| e.to_string())?
            > kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES
        {
            return Err("staged preclaim quote exceeds the original bounded FP descriptor".into());
        }
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) = self
            .palw_v2_reader_state(params)
            .map_err(|e| e.to_string())?
            .ok_or("PALW V2 authenticated preclaim snapshot unavailable")?;
        let point = state.last_point().ok_or("preclaim snapshot has no accepted clock")?;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(point.daa_score)) {
            return Ok(None);
        }
        if !state.staged_required_families_v1(&original) {
            return Ok(None);
        }
        let extras = self.palw_transition_extras_for(point);
        let config = extras.staged_public_court.ok_or("preclaim quote has no native prospective config")?;
        let maximum_custodians =
            state.staged_maximum_custodians_v1(original.job.class_id, extras.point().daa_score, params, &extras).map_err(|e| e.to_string())?;
        let base_ladder =
            extras.held_context_ladder.or(extras.shard_court_ladder).ok_or("preclaim quote has no native held/shard leaf ladder")?;
        let maximum_leaves = state.class_step_ladder_v1(&original.job.class_id, base_ladder);
        let quote = state.staged_preclaim_quote_v1(
            &original,
            &header,
            point.daa_score,
            maximum_custodians,
            maximum_leaves,
            config,
            kaspa_consensus_core::palw_staged_preclaim_v1::PalwStagedRouteGatesV1::of_extras_v1(&extras),
        )?;
        Ok(quote.map(|quote| (point.block, quote)))
    }

    pub fn palw_staged_flat_source_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedFlatProgressV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("Flat snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_flat_source_v1(&state, key)
    }

    /// The greedy Float32 source, read only where its own fence is in force at the accepted snapshot.
    pub fn palw_staged_float_source_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedFloatProgressV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("Float32 snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now))
            || !self.palw_staged_float_constraint_v1.is_some_and(|f| f.is_active(now))
        {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_float_source_v1(&state, key)
    }

    pub fn palw_staged_float_pin_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        source: kaspa_consensus_core::Hash64,
        answerer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatConstraintPinV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("Float32 pin snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now))
            || !self.palw_staged_float_constraint_v1.is_some_and(|f| f.is_active(now))
        {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_float_pin_v1(&state, key, source, answerer)
    }

    /// A TIR claim's native decode view, read only where the TIR route is in force at the snapshot.
    #[allow(clippy::type_complexity)]
    pub fn palw_staged_tir_view_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
        maximum_leaves: u64,
    ) -> Result<
        Option<(
            kaspa_consensus_core::palw_decode_view_v1::PalwDecodeViewV1,
            Box<kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>,
        )>,
        String,
    > {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("TIR view snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now))
            || !self.palw_staged_tir_route_v1.is_some_and(|f| f.is_active(now))
        {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_view_v1(&state, claim, maximum_leaves)
    }

    pub fn palw_staged_owned_native_seed_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedNativeSeedContextV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("native seed snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_tir_route_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_owned_native_seed_v1(&state, key)
    }

    pub fn palw_staged_tir_prefix_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedTirPrefixProgressV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("TIR prefix snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_tir_route_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_prefix_v1(&state, key)
    }

    /// G14 R4: the accepted ORIGINAL TIR frame a public verifier files against (`None` below `palw_tir_public_conviction_v1`).
    pub fn palw_tir_public_frame_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_tir_public_claim_v1::PalwTirPublicFrameViewV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("TIR public frame snapshot has no accepted clock")?.daa_score;
        if !self.palw_tir_public_conviction_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        Ok(kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_frame_view_v1(&state, claim))
    }

    /// G14 R4: the filer scheduler's preflight of a public conviction (a discarded dry run at the next DAA).
    pub fn palw_tir_public_conviction_preflight_v1_impl(
        &self,
        filer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        claim: kaspa_consensus_core::Hash64,
        evidence: kaspa_consensus_core::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
    ) -> Result<Option<Result<(), String>>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let point = state.last_point().ok_or("TIR conviction preflight snapshot has no accepted clock")?;
        let next = point.daa_score.saturating_add(1);
        if !self.palw_tir_public_conviction_v1.is_some_and(|f| f.is_active(next)) {
            return Ok(None);
        }
        let extras = self.palw_transition_extras_for(point);
        Ok(Some(
            kaspa_consensus_core::palw_state_v2::palw_tir_public_conviction_preflight_v1(
                &state, params, &extras, next, filer, claim, &evidence,
            )
            .map_err(|e| e.to_string()),
        ))
    }

    /// G14 R3/R4: run a node worker's read against the accepted native snapshot (`Ok(false)`, closure not run, below
    /// `palw_tir_public_conviction_v1`). The state is the reader snapshot's `Arc`; nothing is cloned or written.
    pub fn palw_tir_public_read_v1_impl(
        &self,
        read: &mut dyn FnMut(&kaspa_consensus_core::palw_state_v2::PalwChainStateV2) -> Result<(), String>,
    ) -> Result<bool, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("TIR public reader snapshot has no accepted clock")?.daa_score;
        if !self.palw_tir_public_conviction_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(false);
        }
        let accepted: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2 = &state;
        read(accepted)?;
        Ok(true)
    }

    /// G14 R1/R2: the producer's pre-signing quote of an ORIGINAL TIR frame (`None` below `palw_tir_public_conviction_v1`).
    pub fn palw_tir_public_original_quote_v1_impl(
        &self,
        original: kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        header: kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    ) -> Result<Option<(u64, kaspa_consensus_core::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1)>, String> {
        kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(header.clone()).check_wire_v1()?;
        if borsh::object_length(&original).map_err(|e| e.to_string())?
            > kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES
        {
            return Err("TIR original quote exceeds the original bounded FP descriptor".into());
        }
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let point = state.last_point().ok_or("TIR original quote snapshot has no accepted clock")?;
        if !self.palw_tir_public_conviction_v1.is_some_and(|f| f.is_active(point.daa_score)) {
            return Ok(None);
        }
        let extras = self.palw_transition_extras_for(point);
        let plan = kaspa_consensus_core::palw_state_v2::palw_tir_public_original_quote_v1(
            &state,
            params,
            &extras,
            point.daa_score,
            &original,
            &header,
        )
        .map_err(|e| e.to_string())?;
        Ok(plan.map(|plan| (point.daa_score, plan)))
    }

    pub fn palw_staged_tir_prefix_leaf_index_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        ordinal: u64,
        maximum_leaves: u64,
    ) -> Result<Option<u64>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("TIR prefix snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_tir_route_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_prefix_leaf_index_v1(&state, key, ordinal, maximum_leaves)
    }

    pub fn palw_staged_tir_layout_v1_impl(
        &self,
        class_id: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1> {
        let params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(params).ok().flatten()?;
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&state, class_id).cloned()
    }

    pub fn palw_staged_flat_pin_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        source: kaspa_consensus_core::Hash64,
        answerer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("Flat pin snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_flat_pin_v1(&state, key, source, answerer)
    }

    pub fn palw_staged_resolved_public_context_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Result<
        Option<(
            kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
            kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        )>,
        String,
    > {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("public context snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_resolved_public_context_v1(&state, claim)
    }

    pub fn palw_staged_original_commitment_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("original commitment snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_original_commitment_v1(&state, claim)
    }

    pub fn palw_v2_accepted_daa_v1_impl(&self) -> Option<u64> {
        let params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(params).ok()??;
        state.last_point().map(|point| point.daa_score)
    }

    pub fn palw_staged_custody_delegations_to_v1_impl(
        &self,
        custodian: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<kaspa_consensus_core::Hash64>,
    ) -> Result<
        (
            Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
            Option<kaspa_consensus_core::Hash64>,
        ),
        String,
    > {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("custody delegation snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok((Vec::new(), None));
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_custody_delegations_to_v1(&state, custodian, after, 256)
    }

    pub fn palw_staged_history_grant_duty_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
        seat: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        receipt: kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwHistoryGrantDutyV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("history grant snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_history_grant_duty_v1(&state, claim, seat, &receipt)
    }

    pub fn palw_staged_history_recovery_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        seed: kaspa_consensus_core::Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedHistoryRecoveryProgressV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("history recovery snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_history_recovery_v1(&state, key, seed)
    }

    pub fn palw_history_response_duties_v1_impl(
        &self,
        signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(
            u64,
            kaspa_consensus_core::Hash64,
            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
            kaspa_consensus_core::Hash64,
        )>,
    ) -> Result<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedHistoryDutyPageV1, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (snapshot_block, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now_daa = state.last_point().ok_or("history duty snapshot has no accepted clock")?.daa_score;
        let (keys, next) = if self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now_daa)) {
            state.staged_history_duties_after_v1(signer, now_daa, after, 64)?
        } else {
            (Vec::new(), None)
        };
        Ok(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedHistoryDutyPageV1 { snapshot_block, now_daa, keys, next })
    }

    pub fn palw_staged_public_context_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedPublicContextV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("public context snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_public_context_v1(&state, claim)
    }

    pub fn palw_staged_flat_duties_v1_impl(
        &self,
        signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
    ) -> Result<
        (
            Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
            Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
        ),
        String,
    > {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("Flat duty snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok((Vec::new(), None));
        }
        state.staged_flat_duties_after_v1(signer, now, after, 64)
    }

    pub fn palw_staged_selected_duties_v1_impl(
        &self,
        signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
    ) -> Result<
        (
            Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
            Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
        ),
        String,
    > {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("selected duty snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok((Vec::new(), None));
        }
        state.staged_selected_duties_after_v1(signer, now, after, 64)
    }

    pub fn palw_staged_selected_events_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedSelectedProgressV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("selected snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_selected_events_v1(&state, key)
    }

    pub fn palw_staged_bundle_plan_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
        actual_raw_cap: Option<u64>,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedBundlePlanV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("bundle quote has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        state.staged_bundle_plan_v1(claim, actual_raw_cap)
    }

    pub fn palw_staged_owned_seed_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("staged snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        let authority = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_paid_authority_v1(&state, key)?;
        if !authority.is_some_and(|authority| now < authority.terms.material_until_daa) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_owned_seed_v1(&state, key)
    }

    pub fn palw_staged_owned_progress_v1_impl(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) =
            self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 accepted snapshot unavailable")?;
        // All clocks and cursors come from this SAME rooted reader snapshot. Do not reacquire
        // a virtual/coinbase lock for a newer clock that could straddle a concurrent reorg.
        let now = state.last_point().ok_or("staged snapshot has no accepted clock")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Ok(None);
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_owned_progress_v1(&state, key)
    }

    pub fn palw_prefix_response_duties_v1_impl(
        &self,
        signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
    ) -> Result<kaspa_consensus_core::palw_staged_court_state_v1::PalwPrefixResponsePageV1, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) = self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("accepted snapshot unavailable")?;
        let now = state.last_point().ok_or("accepted clock unavailable")?.daa_score;
        if !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(now)) {
            return Err("public prefix response route not active".into());
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_prefix_response_duties_v1(&state, signer, after)
    }

    pub fn palw_prefix_page_write_v1_impl(
        &self,
        snapshot: kaspa_consensus_core::BlockHash,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        seed_id: kaspa_consensus_core::Hash64,
        opening: kaspa_consensus_core::palw_prefix_gdn_paged_v1::PalwGdnOwnedPageOpeningV1,
    ) -> Result<Vec<i32>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) = self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("accepted snapshot unavailable")?;
        let point = state.last_point().ok_or("accepted clock unavailable")?;
        if point.block != snapshot || !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(point.daa_score)) {
            return Err("prefix page preview snapshot changed branch or clock".into());
        }
        let progress = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_owned_progress_v1(&state, key)?
            .ok_or("prefix page preview lacks reservation")?;
        if progress.seed_id != Some(seed_id) || progress.now_daa >= progress.terms.stream_until_daa {
            return Err("prefix page preview seed or streaming clock changed".into());
        }
        let Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedCursorV1::PublicPrefix {
            cursor: kaspa_consensus_core::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1::HeadPage { write: true, .. },
            pending: None,
            completed: None,
            ..
        }) = progress.cursor
        else {
            return Err("prefix page preview is not accepted owned write cursor".into());
        };
        let Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::PublicPrefix(row)) =
            state.staged_owned_row_v1(key.0, key.1)
        else {
            return Err("public prefix owned row absent".into());
        };
        row.preview_page_write_v1(&opening)
    }

    pub fn palw_prefix_history_record_v1_impl(
        &self,
        snapshot: kaspa_consensus_core::BlockHash,
        request: kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRequestV1,
    ) -> Result<Option<kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRecordV1>, String> {
        self.palw_prefix_history_record_at_v1(Some(snapshot), request)
    }
    pub fn palw_prefix_history_record_current_v1_impl(
        &self,
        request: kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRequestV1,
    ) -> Result<Option<kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRecordV1>, String> {
        self.palw_prefix_history_record_at_v1(None, request)
    }
    pub fn palw_staged_cache_gc_page_v1_impl(
        &self,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
        limit: usize,
    ) -> Result<kaspa_consensus_core::palw_prefix_history_v1::PalwStagedCacheGcPageV1, String> {
        let pruning_block = self.pruning_point_store.read().pruning_point().map_err(|e| e.to_string())?;
        let irreversible_daa = self.headers_store.get_daa_score(pruning_block).map_err(|e| e.to_string())?;
        let (leases, next) = self
            .palw_state_v2_store
            .read()
            .staged_cache_gc_candidates_v1(irreversible_daa, after, limit)
            .map_err(|e| e.to_string())?;
        Ok(kaspa_consensus_core::palw_prefix_history_v1::PalwStagedCacheGcPageV1 { pruning_block, irreversible_daa, leases, next })
    }
    pub fn palw_staged_cache_gc_ack_v1_impl(
        &self,
        lease: kaspa_consensus_core::palw_prefix_history_v1::PalwStagedCacheLeaseV1,
    ) -> Result<bool, String> {
        let pruning_block = self.pruning_point_store.read().pruning_point().map_err(|e| e.to_string())?;
        let irreversible_daa = self.headers_store.get_daa_score(pruning_block).map_err(|e| e.to_string())?;
        self.palw_state_v2_store.write().acknowledge_staged_cache_gc_v1(lease, irreversible_daa).map_err(|e| e.to_string())
    }
    pub fn palw_shared_claim_cache_gc_page_v1_impl(
        &self,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
        limit: usize,
    ) -> Result<kaspa_consensus_core::palw_prefix_history_v1::PalwSharedClaimCacheGcPageV1, String> {
        let pruning_block = self.pruning_point_store.read().pruning_point().map_err(|e| e.to_string())?;
        let irreversible_daa = self.headers_store.get_daa_score(pruning_block).map_err(|e| e.to_string())?;
        let (leases, next) = self
            .palw_state_v2_store
            .read()
            .shared_claim_cache_gc_candidates_v1(irreversible_daa, after, limit)
            .map_err(|e| e.to_string())?;
        Ok(kaspa_consensus_core::palw_prefix_history_v1::PalwSharedClaimCacheGcPageV1 {
            pruning_block,
            irreversible_daa,
            leases,
            next,
        })
    }
    pub fn palw_shared_claim_cache_gc_ack_v1_impl(
        &self,
        lease: kaspa_consensus_core::palw_prefix_history_v1::PalwSharedClaimCacheLeaseV1,
    ) -> Result<bool, String> {
        let pruning_block = self.pruning_point_store.read().pruning_point().map_err(|e| e.to_string())?;
        let irreversible_daa = self.headers_store.get_daa_score(pruning_block).map_err(|e| e.to_string())?;
        self.palw_state_v2_store.write().acknowledge_shared_claim_cache_gc_v1(lease, irreversible_daa).map_err(|e| e.to_string())
    }
    fn palw_prefix_history_record_at_v1(
        &self,
        snapshot: Option<kaspa_consensus_core::BlockHash>,
        request: kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRequestV1,
    ) -> Result<Option<kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRecordV1>, String> {
        request.check_v1()?;
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) = self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("accepted snapshot unavailable")?;
        let point = state.last_point().ok_or("accepted clock unavailable")?;
        if snapshot.is_some_and(|expected| expected != point.block)
            || !self.palw_staged_public_court_v1.is_some_and(|f| f.is_active(point.daa_score))
        {
            return Err("public history export snapshot changed branch or activation".into());
        }
        kaspa_consensus_core::palw_staged_court_state_v1::palw_prefix_history_request_check_v1(&state, snapshot, &request)?;
        self.palw_state_v2_store.read().prefix_history_record_v1(&request).map_err(|e| e.to_string())
    }

    /// **RFC-0016 op 255**: the per-model report (and the quote for `class`) at the virtual tip, rendered as JSON.
    /// **RFC-0006 consumer v2 (op 245): a class's router binding row at the virtual tip** (read-only; aux 139 of the reader
    /// state). `armed` is `palw_memory_routing_court_v2`'s mirror (false on every shipped preset).
    #[allow(clippy::type_complexity)]
    pub fn palw_router_binding_v2_impl(
        &self,
        class: kaspa_consensus_core::Hash64,
    ) -> Result<Option<(u64, bool, Option<(u64, Vec<u8>)>)>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let Some((_, state)) = self.palw_v2_reader_state(params).map_err(|e| e.to_string())? else { return Ok(None) };
        let now = self.virtual_stores.read().state.get().map_err(|e| e.to_string())?.daa_score;
        let armed = params.memory_routing_court_from_daa().is_some();
        let row = match kaspa_consensus_core::palw_state_v2::palw_router_binding_row_of_v2(&state, &class) {
            Some(row) => Some((row.bound_from_daa, borsh::to_vec(&row).map_err(|e| e.to_string())?)),
            None => None,
        };
        Ok(Some((now, armed, row)))
    }

    pub fn palw_rfc0016_report_v1_impl(
        &self,
        class: Option<kaspa_consensus_core::Hash64>,
    ) -> Result<Option<(u64, bool, String)>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let Some((chain_point, state)) = self.palw_v2_reader_state(params).map_err(|e| e.to_string())? else { return Ok(None) };
        let now = self.virtual_stores.read().state.get().map_err(|e| e.to_string())?.daa_score;
        let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score: now,
            blue_score: 0,
            subsidy: 0,
        });
        let (rows, miners) = kaspa_consensus_core::palw_state_v2::palw_rfc0016_model_report_v1(&state, params, &extras, now);
        let quote = class.and_then(|c| kaspa_consensus_core::palw_state_v2::palw_rfc0016_quote_v1(&state, params, &extras, &c, now));
        let armed = params.rfc0016_v1().is_some();
        // RFC-0016 V5 (user decision 2026-10-15): the epoch is a DAA count; the sink's own timestamp lets a reader measure the
        // DAA-versus-wall-clock drift (never assumed 24 h).
        let clock = kaspa_consensus_core::palw_rfc0016_v1::report::PalwRfc0016EpochClockV1::at(
            params.epoch_length(),
            now,
            self.headers_store.get_header(chain_point).ok().map(|header| header.timestamp),
        );
        let json = kaspa_consensus_core::palw_rfc0016_v1::report::palw_rfc0016_report_json_with_clock_v1(
            now,
            armed,
            &rows,
            &miners,
            quote.as_ref(),
            &clock,
        );
        Ok(Some((now, armed, json)))
    }

    pub fn palw_terminal_progress_v1_impl(
        &self,
        key: &kaspa_consensus_core::palw_terminal_carriage_v1::PalwTerminalKeyV1,
    ) -> Result<Option<kaspa_consensus_core::palw_terminal_carriage_v1::PalwTerminalProgressV1>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) = self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 tip unavailable")?;
        let now = state.last_point().ok_or("terminal progress snapshot has no accepted clock")?.daa_score;
        if !self.palw_legacy_public_filer_at(now) {
            return Ok(None);
        }
        let court = self.palw_court_params_v2.as_ref().ok_or("terminal evidence needs V2 court parameters")?;
        kaspa_consensus_core::palw_terminal_carriage_v1::palw_terminal_progress_v1(&state, key, court.max_close_bytes())
    }

    pub fn palw_legacy_answer_progress_v2_impl(
        &self,
        key: &kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwLegacyAnswerKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwLegacyAnswerProgressV2>, String> {
        let params = self.palw_state_params_v2.as_ref().ok_or("PALW V2 state parameters unavailable")?;
        let (_, state) = self.palw_v2_reader_state(params).map_err(|e| e.to_string())?.ok_or("PALW V2 tip unavailable")?;
        Ok(state.legacy_answer_pending_v2(key).map(|row| row.progress()))
    }

    pub fn palw_legacy_answer_history_v2_impl(
        &self,
        block: kaspa_consensus_core::BlockHash,
        chunk: &kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwLegacyAnswerChunkV2,
        maximum: u64,
    ) -> Result<Option<kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldAnswerCarriageV2>, String> {
        use kaspa_consensus_core::palw_legacy_answer_chunks_v2::*;
        if chunk.index >= PALW_LEGACY_ANSWER_MAX_CHUNKS_V2 || chunk.bytes.len() > PALW_LEGACY_ANSWER_CHUNK_BYTES_V2 as usize {
            return Ok(None);
        }
        let row = self.palw_state_v2_store.read().legacy_answer_history_row_v2(block, chunk, maximum).map_err(|e| e.to_string())?;
        let Some(row) = row else { return Ok(None) };
        // Bad carrier bytes are not a broken reader. Independently authenticated material may
        // still be useful even when the court refused a carrier after its deadline.
        Ok(row.preview(chunk).ok().flatten())
    }

    /// **Lane LG14-A (RPC 205): the claims a live reservation holds at the tip.**
    pub fn palw_legacy_disputes_v1_impl(
        &self,
        reserver: Option<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
        limit: usize,
    ) -> Vec<kaspa_consensus_core::Hash64> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else { return Vec::new() };
        state.palw_legacy_disputes_live_v1(reserver.as_ref(), limit)
    }

    /// **Lane LG14-A: would the next block take this reservation?** The fold's tag-154 arm on the tip, at the virtual's next DAA, under
    /// the extras that block folds with (the fence below it refuses, as the fold does).
    pub fn palw_legacy_dispute_reservation_check_v1_impl(
        &self,
        reservation: &kaspa_consensus_core::palw_legacy_public_filer_v1::PalwDisputeReservationV1,
    ) -> Option<Result<u128, String>> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let now_daa = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score)?;
        let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score: now_daa,
            blue_score: 0,
            subsidy: 0,
        });
        Some(
            kaspa_consensus_core::palw_state_v2::palw_legacy_dispute_reservation_check_v1(
                &state,
                state_params,
                &extras,
                reservation,
                now_daa,
            )
            .map_err(|e| e.to_string()),
        )
    }

    /// **Lane LG14-A: the claims `me`'s fraud filer may pursue at the tip** (at the virtual's DAA; empty below the fence).
    pub fn palw_fraud_filer_candidates_v1_impl(
        &self,
        me: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwFraudFilerCandidateV1> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else { return Vec::new() };
        let Some(now_daa) = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score) else {
            return Vec::new();
        };
        state.palw_fraud_filer_candidates_v1(state_params, me, now_daa)
    }

    pub fn palw_fraud_filer_candidates_for_claims_v1_impl(
        &self,
        me: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        claims: &[Hash64],
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwFraudFilerCandidateV1> {
        let Some(params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(params).ok().flatten() else { return Vec::new() };
        let Some(now_daa) = self.virtual_stores.read().state.get().ok().map(|state| state.daa_score) else { return Vec::new() };
        state.palw_fraud_filer_candidates_for_claims_v1(params, me, now_daa, claims)
    }

    /// **Lane LG14-A (RPC 206): one bond's standing as a public filer at the tip.**
    pub fn palw_fraud_filer_status_v1_impl(
        &self,
        bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwFraudFilerStatusV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let now_daa = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score)?;
        state.palw_fraud_filer_status_v1(state_params, bond, now_daa)
    }

    pub fn palw_da_accusation_check_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
        accuser: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let candidate_daa = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score)?;
        Some(self.palw_da_accusation_check_v1_at(&state, chain_point, candidate_daa, &claim, &accuser))
    }

    /// [`Self::palw_da_accusation_check_v1_impl`] on a given state, for the block at `now_daa` on
    /// `chain_point` — the one place the fold's extras are resolved for the accusation read (the
    /// real-claim tests call it on the state they folded). No state params: the court is dormant.
    pub(crate) fn palw_da_accusation_check_v1_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        chain_point: kaspa_consensus_core::BlockHash,
        now_daa: u64,
        claim: &kaspa_consensus_core::Hash64,
        accuser: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1 {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1::Refused(
                kaspa_consensus_core::palw_state_v2::PalwStateV2Error::DaCourtDormant,
            );
        };
        let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score: now_daa,
            blue_score: 0,
            subsidy: 0,
        });
        kaspa_consensus_core::palw_producer_v2::palw_da_accusation_check_v1(state, state_params, &extras, claim, accuser, now_daa)
    }

    /// **ADR-0152 R-3/R-4 (Phase 2, P2-8): the reporter filer's read of one filing** — at the tip,
    /// for the DAA the virtual's next block folds at: the commitment's row, the conviction and the
    /// reward under `offence_key`, and (when `gated` is given — the evidence, the signed commitment,
    /// or the court accusation the filer falls back to) this processor's object gate on it at that
    /// DAA, so an object the node sends is one the gate admits. `None` with no tip state.
    pub fn palw_reporter_filing_read_v1_impl(
        &self,
        offence_key: kaspa_consensus_core::Hash64,
        commitment: kaspa_consensus_core::Hash64,
        reporter: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        gated: Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwReporterFilingReadV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let candidate_daa = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score)?;
        self.palw_reporter_filing_read_v1_at(&state, chain_point, candidate_daa, &offence_key, &commitment, &reporter, gated.as_ref())
    }

    /// [`Self::palw_reporter_filing_read_v1_impl`] on a given state, for the block at `now_daa` on
    /// `chain_point` — the real-claim tests call it on the state they folded. The gate is
    /// [`Self::palw_v2_validate_objects`] itself: the adjudicator the fold runs, with the signature
    /// half, never a restatement of it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn palw_reporter_filing_read_v1_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        chain_point: kaspa_consensus_core::BlockHash,
        now_daa: u64,
        offence_key: &kaspa_consensus_core::Hash64,
        commitment: &kaspa_consensus_core::Hash64,
        reporter: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        gated: Option<&kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwReporterFilingReadV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let mut read = kaspa_consensus_core::palw_state_v2::palw_reporter_filing_read_v1(
            state,
            state_params,
            now_daa,
            offence_key,
            commitment,
            reporter,
        );
        if let Some(object) = gated {
            let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                block: chain_point,
                daa_score: now_daa,
                blue_score: 0,
                subsidy: 0,
            };
            // This is a local filing preflight, not a direct-carrier admission exemption.
            // The actual declaration and every carrier still pass the ordinary signed object gate.
            read.object_gate = Some(match object {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectiveOffence {
                    kind,
                    accused,
                    evidence_id,
                    evidence,
                } if evidence.len() as u64 > kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES => {
                    self.palw_terminal_filing_preflight_v1(
                        state,
                        state_params,
                        &point,
                        reporter,
                        *kind,
                        *accused,
                        *evidence_id,
                        evidence,
                    )
                }
                _ => self.palw_v2_validate_objects(state, state_params, &point, std::slice::from_ref(object)),
            });
        }
        Some(read)
    }

    pub(super) fn palw_terminal_filing_preflight_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        reporter: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        kind: kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1,
        accused: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        evidence_id: kaspa_consensus_core::Hash64,
        evidence: &[u8],
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_terminal_carriage_v1 as t;
        if !self.palw_legacy_public_filer_at(point.daa_score)
            || !self.palw_objective_offence_at(point.daa_score)
            || !self.palw_offence_attribution_at(point.daa_score)
        {
            return Err("terminal public-filer court is dormant".into());
        }
        let court = self.palw_court_params_v2.as_ref().ok_or("terminal evidence needs V2 court parameters")?;
        if evidence.len() as u64 > court.max_close_bytes().min(t::PALW_TERMINAL_MAX_BYTES_V1) {
            return Err("terminal proof exceeds the court envelope".into());
        }
        let claim = t::palw_terminal_claim_v1(kind, evidence)?;
        let reservation = state.legacy_dispute_reservation_v1(&claim, reporter).ok_or("terminal filer has no live reservation")?;
        let record = state.legacy_dispute_v1(&claim).ok_or("terminal reservation lacks its record")?;
        let owner = state.bond(reporter).ok_or("terminal filer has no registered bond")?;
        if !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err("terminal filer is not active".into());
        }
        let placeholder = t::PalwTerminalDeclarationV1 {
            version: 1,
            claim,
            reserver: *reporter,
            reserved_daa: reservation.reserved_daa,
            kind,
            accused,
            evidence_id,
            heavy_prompt_ids: 0,
            total_bytes: 0,
            part_bytes: t::PALW_TERMINAL_PART_BYTES_V1,
            part_digests: Vec::new(),
            signature: Vec::new(),
        };
        let d = t::palw_terminal_declaration_v1(placeholder, evidence, court.max_close_bytes(), |_| {
            Ok(vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN])
        })?;
        d.check_window(point.daa_score, record.hard_deadline_daa)?;
        if matches!(kind, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::TirIdentityMismatch)
            && !self.palw_tir_at(point.daa_score)
        {
            return Err("terminal identity court is dormant".into());
        }
        if t::palw_terminal_is_constraint_v1(kind, evidence) && !params.fp_decode_constraint_active_at(point.daa_score) {
            return Err("terminal constraint court is dormant".into());
        }
        if !params.capacity_batch_active_at(point.daa_score)
            && kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_evidence_is_windowed_v1(evidence)
        {
            return Err("terminal Windowed court is dormant".into());
        }
        let bond = state.bond(&accused).ok_or("terminal accused has no registered bond")?;
        // RFC06-4: `_v2` with the block's two closure mirrors (both `None` on every shipped network: the V1 call byte for byte).
        t::palw_terminal_adjudicate_v2(
            state,
            &d,
            evidence,
            court.max_close_bytes(),
            params.fp_decode_rules_at(point.daa_score),
            self.palw_identity_rules_v1(point.daa_score),
            self.palw_session_rule_at(point.daa_score),
            Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidSigCheckV1 {
                chain_domain: self.palw_network_domain_v2(),
                seat_pubkey: &bond.pubkey,
                seat_active: matches!(
                    bond.status,
                    kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active
                        | kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { .. }
                ),
                verify: &Self::verify_mldsa87_with_context_bool,
            }),
            kaspa_consensus_core::palw_cell_coverage_closure_v1::PalwClosureMirrorsV1::of(params),
        )?;
        Ok(())
    }

    /// **ADR-0152 DA-3 / J-6 (Phase 2, P2-8d): what a `StepLeaf` demand of `claim` by `accuser`
    /// comes to** — at the tip, for the DAA the virtual's next block folds at, under that block's
    /// extras, exactly as [`Self::palw_da_accusation_check_v1_impl`] reads P2-6's accusation. `None`
    /// with no tip state.
    pub fn palw_da_step_leaf_demand_check_v1_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
        accuser: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        leaf: u64,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwDaStepLeafDemandCheckV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let candidate_daa = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score)?;
        Some(self.palw_da_step_leaf_demand_check_v1_at(&state, chain_point, candidate_daa, &claim, &accuser, leaf))
    }

    /// [`Self::palw_da_step_leaf_demand_check_v1_impl`] on a given state, for the block at `now_daa`
    /// on `chain_point` (the real-claim tests call it on the state they folded). No state params: the
    /// court is dormant.
    pub(crate) fn palw_da_step_leaf_demand_check_v1_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        chain_point: kaspa_consensus_core::BlockHash,
        now_daa: u64,
        claim: &kaspa_consensus_core::Hash64,
        accuser: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        leaf: u64,
    ) -> kaspa_consensus_core::palw_producer_v2::PalwDaStepLeafDemandCheckV1 {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return kaspa_consensus_core::palw_producer_v2::PalwDaStepLeafDemandCheckV1::Refused(
                kaspa_consensus_core::palw_state_v2::PalwStateV2Error::DaCourtDormant,
            );
        };
        let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score: now_daa,
            blue_score: 0,
            subsidy: 0,
        });
        kaspa_consensus_core::palw_producer_v2::palw_da_step_leaf_demand_check_v1(
            state,
            state_params,
            &extras,
            claim,
            accuser,
            leaf,
            now_daa,
        )
    }

    /// **ADR-0152 Phase 2, P2-8e: what the fold makes of `object` in the virtual's next block** — at
    /// the tip, for the virtual's DAA, the acceptance layer and then the object's own arm
    /// ([`Self::palw_object_rehearsal_v1_on`]). `None` with no tip state. A read: node policy asks it
    /// before it queues an object no H-1 gate rehearses (the held dissection's opening).
    pub fn palw_object_rehearsal_v1_impl(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let daa_score = self.virtual_stores.read().state.get().ok()?.daa_score;
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score,
            // The next block's: past the tip's, as the H-1 gate reads it.
            blue_score: state.last_point().map_or(0, |last| last.blue_score.saturating_add(1)),
            subsidy: 0,
        };
        Some(self.palw_object_rehearsal_v1_on(&state, state_params, &point, object))
    }

    /// [`Self::palw_object_rehearsal_v1_impl`] on a given state and point — the H-1 gate's two
    /// layers ([`Self::palw_h1_carrier_refusal_on`]) with the fold's reason kept typed. The acceptance
    /// layer first (cheap refusals before any state is cloned), then `palw_v2_apply_one_object_v1`
    /// under the point's own flags and extras. Origin uses the equivalent read-only verdict above.
    pub(crate) fn palw_object_rehearsal_v1_on(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1 {
        use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1;
        if let Some(verdict) = self.palw_exec_origin_rehearsal_v1_on(state, state_params, point, object) {
            return verdict;
        }
        if let Err(why) = self.palw_v2_validate_objects(state, state_params, point, std::slice::from_ref(object)) {
            return PalwObjectRehearsalV1::NotAccepted(why);
        }
        match kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_retained_v1(
            state,
            state_params,
            point,
            object,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for_objects(state, point, std::slice::from_ref(object)),
        ) {
            Ok(_) => PalwObjectRehearsalV1::Accepted,
            Err(why) => PalwObjectRehearsalV1::Refused(why),
        }
    }

    /// The pre-optimization acceptance + writer oracle, with this node's private flags/extras.
    /// No admission bypass: used only by equivalence tests, never compiled into a production node.
    #[cfg(test)]
    pub(crate) fn palw_object_rehearsal_with_writer_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1 {
        use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1;
        if let Err(why) = self.palw_v2_validate_objects(state, state_params, point, std::slice::from_ref(object)) {
            return PalwObjectRehearsalV1::NotAccepted(why);
        }
        match kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_retained_v1(
            state,
            state_params,
            point,
            object,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for_objects(state, point, std::slice::from_ref(object)),
        ) {
            Ok(_) => PalwObjectRehearsalV1::Accepted,
            Err(why) => PalwObjectRehearsalV1::Refused(why),
        }
    }

    /// **Lane B of the panel-seed stopgap (2026-09-26): the claims an operator's non-seat filer may
    /// accuse**, at the tip, for the DAA the virtual's next block folds at
    /// (`palw_operator_da_v1::palw_operator_da_candidates_v1`). Empty with no tip state. A read: node
    /// policy, never a block rule.
    pub fn palw_operator_da_candidates_v1_impl(
        &self,
        operators: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Vec<kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaCandidateV1> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Ok(Some((_, state))) = self.palw_v2_reader_state(state_params) else { return Vec::new() };
        let Some(candidate_daa) = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score) else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_operator_da_v1::palw_operator_da_candidates_v1(&state, state_params, operators, candidate_daa)
    }

    /// **ADR-0160 F-Q (stage 2): the credited claims `me`'s audit duty may receipt**, at the tip
    /// (`palw_audit_door_v1::palw_capacity_audit_candidates_v1`). Empty with no tip state. A read: node
    /// policy, never a block rule.
    pub fn palw_capacity_audit_candidates_v1_impl(
        &self,
        me: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Vec<kaspa_consensus_core::palw_audit_door_v1::PalwAuditCandidateV1> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Ok(Some((_, state))) = self.palw_v2_reader_state(state_params) else { return Vec::new() };
        kaspa_consensus_core::palw_audit_door_v1::palw_capacity_audit_candidates_v1(&state, state_params, me)
    }

    /// **Lane B: one operator bond's standing**, at the tip, for the DAA the virtual's next block folds
    /// at, over the second clock's raw depth there (`palw_operator_da_v1::palw_operator_da_standing_v1`).
    /// `None` with no tip state. A read: node policy, never a block rule.
    pub fn palw_operator_da_standing_v1_impl(
        &self,
        bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaStandingV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let candidate_daa = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score)?;
        let raw_depth = self.palw_second_clock_depth_at(&state, candidate_daa);
        kaspa_consensus_core::palw_operator_da_v1::palw_operator_da_standing_v1(&state, state_params, bond, candidate_daa, raw_depth)
    }

    /// **ADR-0152 Phase 2, P2-8e review (MED): each asked claim's one deadline in the sweep queue**,
    /// at the tip ([`Self::palw_claim_deadlines_v1_on`]). Empty with no tip state. A read: node policy
    /// dates a seat's court filing by it.
    pub fn palw_claim_deadlines_v1_impl(
        &self,
        claims: &[kaspa_consensus_core::Hash64],
    ) -> Vec<(kaspa_consensus_core::Hash64, Option<u64>)> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Ok(Some((_, state))) = self.palw_v2_reader_state(state_params) else { return Vec::new() };
        Self::palw_claim_deadlines_v1_on(&state, claims)
    }

    /// [`Self::palw_claim_deadlines_v1_impl`] on a given state: the index the sweep reads
    /// (`PalwChainStateV2::deadline_of`), one scan a claim — asked only for the few claims a seat has
    /// a court filing queued on.
    pub(crate) fn palw_claim_deadlines_v1_on(
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        claims: &[kaspa_consensus_core::Hash64],
    ) -> Vec<(kaspa_consensus_core::Hash64, Option<u64>)> {
        claims.iter().map(|claim| (*claim, state.deadline_of(claim))).collect()
    }

    /// **The 2026-09-25 model-registry review, M1: how urgently do `carriers`' rows need them at the
    /// tip?** — for each, the row the tip holds for its `(bond, class)`, judged at the virtual's DAA by
    /// the one predicate (`PalwReadinessCarrierV1::urgency`) under the registry fold's own span clock,
    /// globals and readiness rule; one tip load and one fold for the whole batch. The pool's carrier
    /// index asks it when a proof enters and, for every proof it holds, at every new block; the proof
    /// itself was put to the fold by the carrier gate ([`Self::palw_mempool_h1_carrier_refusal`]) when
    /// it entered. Node-local; all `None` below `palw_rcore_plus`, so every network but testnet-12
    /// keeps a proof an ordinary transaction.
    pub fn palw_readiness_urgency_v1_impl(
        &self,
        carriers: &[kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1],
    ) -> Vec<Option<kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1>> {
        let Some(virtual_daa) = self.virtual_stores.read().state.get().ok().map(|virtual_state| virtual_state.daa_score) else {
            return vec![None; carriers.len()];
        };
        self.palw_readiness_urgency_at(carriers, virtual_daa)
    }

    /// [`Self::palw_readiness_urgency_v1_impl`] at a given DAA.
    pub(in crate::pipeline::virtual_processor) fn palw_readiness_urgency_at(
        &self,
        carriers: &[kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1],
        virtual_daa: u64,
    ) -> Vec<Option<kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1>> {
        let none = || vec![None; carriers.len()];
        if carriers.is_empty() || !self.palw_rcore_plus_at(virtual_daa) {
            return none();
        }
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return none() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else { return none() };
        self.palw_readiness_urgency_on(&state, carriers, virtual_daa)
    }

    /// [`Self::palw_readiness_urgency_at`] on a given state (a test's rows).
    pub(in crate::pipeline::virtual_processor) fn palw_readiness_urgency_on(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        carriers: &[kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1],
        virtual_daa: u64,
    ) -> Vec<Option<kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1>> {
        let none = || vec![None; carriers.len()];
        if !self.palw_rcore_plus_at(virtual_daa) {
            return none();
        }
        let Some(fold) = self.palw_model_registry_fold_at(virtual_daa) else { return none() };
        carriers
            .iter()
            .map(|carrier| {
                carrier.urgency(
                    state.seat_readiness(&carrier.bond, &carrier.class_id),
                    virtual_daa,
                    fold.span_daa,
                    &fold.globals,
                    fold.readiness_v2_active,
                )
            })
            .collect()
    }

    /// **Who may be served a claim's private material**, at the tip (ADR-0077 Decision 16's
    /// transport half) — see `PalwChainStateV2::claim_readers_v2`.
    pub fn palw_claim_readers_v2_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
            return Vec::new();
        };
        state.claim_readers_v2(&claim)
    }

    /// **The tip a P2-8c filer asks from** — the tip block's state at the point the licence assembler
    /// asks from (the tip block, the virtual's DAA and blue score), so what the filer is told is what
    /// the next block folds. `None` with no tip state.
    pub(super) fn palw_false_valid_tip_v1(
        &self,
    ) -> Option<(
        Arc<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>,
        kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    )> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        Some((state, point))
    }

    /// `palw_network_domain_v2_for(network id, genesis)` — the domain the gate verifies every
    /// receipt under.
    pub(super) fn palw_false_valid_chain_domain_v1(&self) -> kaspa_consensus_core::Hash64 {
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(self.network_id_bytes.as_slice(), Some(self.genesis.hash))
    }

    /// **ADR-0152 v3.1 N10 (Phase 2, P2-8c): what filing this `PanelFalseValidV2` object comes to**
    /// at the tip ([`Self::palw_false_valid_filing_check_v1_at`]). `None` with no tip state.
    pub fn palw_false_valid_filing_check_v1_impl(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidFilingCheckV1> {
        let (state, point) = self.palw_false_valid_tip_v1()?;
        Some(self.palw_false_valid_filing_check_v1_at(&state, &point, object))
    }

    /// **ADR-0152 v3.1 N10 (P2-8c): which of `receipts` the chain relied on**, at the tip — one answer
    /// a receipt, in order ([`Self::palw_false_valid_receipt_relied_v1_at`]). `None` with no tip state
    /// or below `Params::palw_rcore_plus`, where the filer does not run.
    pub fn palw_false_valid_relied_receipts_v1_impl(
        &self,
        receipts: &[kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidReceiptV1],
    ) -> Option<Vec<bool>> {
        let (state, point) = self.palw_false_valid_tip_v1()?;
        if !self.palw_rcore_plus_at(point.daa_score) {
            return None;
        }
        Some(receipts.iter().map(|receipt| self.palw_false_valid_receipt_relied_v1_at(&state, receipt).is_ok()).collect())
    }

    /// [`Self::palw_false_valid_relied_receipts_v1_impl`] for one receipt on a given state: the core
    /// admission (`palw_false_valid_receipt_relied_v1`) under this chain's domain and the gate's own
    /// ML-DSA-87 verifier — the real-claim tests call it on the state they folded.
    pub(crate) fn palw_false_valid_receipt_relied_v1_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        receipt: &kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidReceiptV1,
    ) -> Result<(), &'static str> {
        kaspa_consensus_core::palw_false_valid_filing_v1::palw_false_valid_receipt_relied_v1(
            state,
            receipt,
            self.palw_false_valid_chain_domain_v1(),
            &Self::verify_mldsa87_with_context_bool,
        )
    }

    /// [`Self::palw_false_valid_filing_check_v1_impl`] on a given state at `point` — the one place the
    /// answers are asked in order (the real-claim tests call it on the state they folded):
    ///
    /// 1. **the ledger key, the receipt's admission and the adjudicator, classified**
    ///    (`palw_false_valid_filing_check_v1`): a (seat, claim) already convicted; a receipt the
    ///    chain never relied on (`Unrelied`: its signature under this chain's domain and the seat's
    ///    registered key with the gate's verifier, its lock or row) — before the liability rule can
    ///    call junk `NotLiable`; then `palw_check_panel_false_valid_v2` without the signature, under
    ///    the rules and decode switch the gate and the fold read at `point`, so a liability refusal
    ///    (`NotLiable`) and an open court (`Wait`) are told apart from every other refusal;
    /// 2. **the gate** (`palw_v2_validate_objects`): the adjudicator again with the signature, the
    ///    seat `Active` or `Retiring`;
    /// 3. **the fold** (`palw_v2_apply_one_object_v1` on the tip): the lock the seat must still hold
    ///    or the liability row that lists it, the heavy-prompt budget, and S-4's funnel — so the
    ///    answer is the fold's own and cannot drift from it.
    ///
    /// `Dormant` below `Params::palw_rcore_plus` (P2-8c, like everything Phase 2 adds, lives past
    /// it; the fence requires `palw_offence_attribution`, so kind 3 exists wherever this answers).
    pub(crate) fn palw_false_valid_filing_check_v1_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidFilingCheckV1 {
        use kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidFilingCheckV1 as Check;
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Check::Dormant };
        if !self.palw_rcore_plus_at(point.daa_score) {
            return Check::Dormant;
        }
        let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectiveOffence {
            kind: kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2,
            accused,
            evidence,
            ..
        } = object
        else {
            return Check::Refused("not a PanelFalseValidV2 filing".into());
        };
        let check = kaspa_consensus_core::palw_false_valid_filing_v1::palw_false_valid_filing_check_at_v2(
            state,
            accused,
            evidence,
            state_params.fp_decode_rules_at(point.daa_score),
            self.palw_identity_rules_v1(point.daa_score),
            self.palw_false_valid_chain_domain_v1(),
            &Self::verify_mldsa87_with_context_bool,
            self.palw_session_rule_at(point.daa_score),
        );
        if !matches!(check, Check::File { .. }) {
            return check;
        }
        if let Err(why) = self.palw_v2_validate_objects(state, state_params, point, std::slice::from_ref(object)) {
            return Check::Refused(why);
        }
        match kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_retained_v1(
            state,
            state_params,
            point,
            object,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for_objects(state, point, std::slice::from_ref(object)),
        ) {
            Ok(_) => check,
            Err(why) => Check::Refused(why.to_string()),
        }
    }

    /// What a node's receipt pool reads off the tip — see the trait doc. Node policy: a read.
    pub fn palw_receipt_pool_facts_v1_impl(
        &self,
        claims: &[kaspa_consensus_core::Hash64],
        bonds: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwReceiptPoolFactsV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        Some(kaspa_consensus_core::palw_panel_v2::palw_receipt_pool_facts_v1(&state, claims, bonds))
    }

    /// A claim's committed roots and price at the tip (ADR-0111 Decision 2) — see the trait doc.
    pub fn palw_claim_roots_v2_impl(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Option<(kaspa_consensus_core::Hash64, kaspa_consensus_core::Hash64, u64)> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        state.claim(&claim).map(|c| (c.execution_root, c.trace_root, c.work_leaves))
    }

    /// **RFC-0006: the layer-shard plans at the tip** (`palw_tir_shard_plans_v1`).
    pub fn palw_tir_shard_plans_v1_impl(
        &self,
    ) -> Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_tir_shard_v1::PalwTirShardPlanV1)> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Ok(Some((_, state))) = self.palw_v2_reader_state(state_params) else { return Vec::new() };
        state.tir_shard_plans_iter().map(|(class, plan)| (*class, plan.clone())).collect()
    }

    /// The payout payload the chain has registered for `bond`, if it is registered at all.
    pub fn palw_bond_payout_payload_v2_impl(
        &self,
        bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::Hash64> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        state.bond(bond).map(|b| b.payout_payload)
    }

    /// The bond this key already registered, if any. See the trait doc: this is what keeps a
    /// left-in `--palw-register-bond` from locking collateral again on every restart.
    /// The class table as an operator needs it. See the trait doc: share and budget are read
    /// together because "budget 0" alone cannot say whether the class was never granted share.
    pub fn palw_v2_class_table_impl(&self) -> Vec<kaspa_consensus_core::palw_state_v2::PalwClassRowV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Ok(Some((_, state))) = self.palw_v2_reader_state(state_params) else { return Vec::new() };
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
        let (tip, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        // ADR-0132: the tip's `bits` prices the network draw every class win faces at this height.
        let network_bits = self.headers_store.get_header(tip).map(|h| h.bits).unwrap_or(0);
        Some(kaspa_consensus_core::palw_economic_compute_v1::palw_class_census_v1(&state, state_params, network_bits))
    }

    /// ADR-0135: the model registry as the tip state holds it — every class's lifecycle row, the
    /// seats ready for it now, the claims in flight, and every seat's last possession proof.
    pub fn palw_model_registry_v1_impl(&self) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryReadV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let tip_daa = self.headers_store.get_header(tip).map(|h| h.daa_score).unwrap_or(0);
        let fold = self.palw_model_registry_fold_at(tip_daa);
        let tip_point =
            kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 { block: tip, daa_score: tip_daa, blue_score: 0, subsidy: 0 };
        let work = self.palw_work_target_fold_for(&tip_point);
        // 2026-09-24 DoS audit review of #11: the read asks the class gate's questions — free-prompt
        // claims counted past the audit fence, and room (not the cap) where the fold judges by room —
        // so a producer's pre-check (`registry_holds_class`) refuses what the fold will refuse.
        let panel_room_enforced = self.palw_work_target_at(tip_daa) && fold.as_ref().is_some_and(|f| f.governs_at(tip_daa));
        Some(kaspa_consensus_core::palw_model_registry_v1::palw_model_registry_read_v2(
            &state,
            state_params,
            tip_daa,
            self.palw_model_registry.map(|f| f.daa_score()),
            fold.as_ref(),
            work.as_ref(),
            self.palw_audit_2026_09_23_at(tip_daa),
            panel_room_enforced,
        ))
    }

    /// **ADR-0152-adjacent (op 200): one class's Activation Pool at the tip** — the row, the terms
    /// in force at the tip's DAA, what (a) would pay now and the span its next audit falls at (R2's
    /// stagger over the registry fold's period). `None` off ConsensusV2 or before the first state.
    pub fn palw_activation_pool_v1_impl(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_activation_pool_v1::PalwActivationPoolReadV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let tip_daa = self.headers_store.get_header(tip).map(|h| h.daa_score).unwrap_or(0);
        let fold = self.palw_model_registry_fold_at(tip_daa);
        let schedule = fold.as_ref().map(|fold| {
            (
                fold.span_daa,
                kaspa_consensus_core::palw_model_registry_v1::palw_admission_audit_period_spans_v2(
                    state_params.epoch_length(),
                    fold.span_daa,
                    fold.admission_audit_period_daa,
                ),
            )
        });
        Some(kaspa_consensus_core::palw_activation_pool_v1::palw_activation_pool_read_v1(
            &state,
            &class_id,
            &state_params.base_class_id(),
            self.palw_activation_pool_at(tip_daa),
            tip_daa,
            schedule,
        ))
    }

    /// Class panel status, bonded seats, and per-claim assignments at the tip.
    pub fn palw_panel_network_view_v1_impl(&self) -> Option<kaspa_consensus_core::palw_panel_view_v1::PalwPanelNetworkViewV1> {
        let registry = self.palw_model_registry_v1_impl()?;
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let tip_daa = self.headers_store.get_header(tip).map(|h| h.daa_score).unwrap_or(0);
        let schedule = kaspa_consensus_core::palw_panel_view_v1::PalwVerificationScheduleV1 {
            v2: self.palw_verification_v2,
            s3: self.palw_verification_s3,
            s2: self.palw_verification_s2,
        };
        Some(kaspa_consensus_core::palw_panel_view_v1::palw_panel_network_view_v1(&state, state_params, tip_daa, &registry, schedule))
    }

    /// ADR-0132: the attempt-lane claims of the tip state as the end-to-end ledger observes them.
    pub fn palw_claim_ledger_observations_v1_impl(
        &self,
    ) -> Option<Vec<kaspa_consensus_core::palw_economics_ledger_v1::PalwClaimLedgerObservationV1>> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        Some(kaspa_consensus_core::palw_economics_ledger_v1::palw_claim_ledger_observations_v1(&state))
    }

    pub fn palw_bond_of_pubkey_v2_impl(
        &self,
        pubkey: &[u8],
    ) -> Option<(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2)> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
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
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
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
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
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
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
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
        if record.artifact_root != registered_root || record.registered_daa != class.registered_daa {
            return None;
        }
        let carriage = kaspa_consensus_core::palw_class_sync_v1::validate_class_declaration_v1(
            class_id,
            class,
            state.class_semantics_v1(&class_id),
            &record.carriage,
        )
        .ok()?;
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
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
            return Vec::new();
        };
        let store = self.palw_class_carriage_store.read();
        state
            .class_ids()
            .into_iter()
            .filter_map(|class_id| store.get(class_id).map(|record| (class_id, record.carriage.clone())))
            .collect()
    }

    pub fn palw_native_snapshot_inventory_v1_impl(
        &self,
        point: BlockHash,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeSnapshotInventoryV1,
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
    > {
        use kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1;
        self.palw_state_v2_store
            .read()
            .native_snapshot_inventory_v1(point)
            .map_err(|why| NativeCoverageV1::unsupported::<Self>("accepted-point inventory store", &why.to_string()))?
            .ok_or_else(|| {
                NativeCoverageV1::unsupported::<Self>(
                    "accepted-point inventory",
                    "requested point has no atomically persisted typed inventory; pre-upgrade snapshots require trusted recapture",
                )
            })
    }

    pub fn pruning_point_palw_state_shared_v1_impl(
        &self,
        point: BlockHash,
    ) -> Option<std::sync::Arc<kaspa_consensus_core::palw_native_resource_v1::NativeStateCarriageV1>> {
        let params = self.palw_state_params_v2.as_ref()?;
        self.palw_state_v2_store.read().load_pruning_snapshot_carriage_cached(params, point).ok().flatten()
    }

    /// Declarations bound to the requested snapshot, never to this node's tip.
    /// Strict coverage is negotiated by protocol 107; old peers retain their
    /// historical manual-material fallback. Frozen graphs are retained as sync
    /// material, while the execution getter still refuses to serve them.
    pub fn palw_class_carriages_for_snapshot_v1_impl(
        &self,
        snapshot: &kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
        complete: bool,
    ) -> Result<Vec<(kaspa_hashes::Hash64, Vec<u8>)>, String> {
        let store = self.palw_class_carriage_store.read();
        let mut declarations = Vec::new();
        for (id, class) in &snapshot.classes {
            if snapshot.tir_classes.contains_key(id) || snapshot.gen_classes.contains_key(id) {
                continue; // These carry their IR/program records in the state itself.
            }
            if self.palw_snapshot_builtin_class_v1(*id, class, snapshot.class_semantics.get(id))? {
                continue;
            }
            let Some(record) = store.get(*id) else {
                if complete {
                    return Err(format!("snapshot class {id} has no declaration index row"));
                }
                continue;
            };
            if record.artifact_root != class.artifact_root || record.registered_daa != class.registered_daa {
                if complete {
                    return Err(format!("snapshot class {id} declaration index belongs to another registration"));
                }
                continue;
            }
            match kaspa_consensus_core::palw_class_sync_v1::validate_class_declaration_v1(
                *id,
                class,
                snapshot.class_semantics.get(id),
                &record.carriage,
            ) {
                Ok(_) => {
                    if complete {
                        self.palw_snapshot_declaration_identity_v1(
                            *id,
                            class,
                            snapshot.class_semantics.get(id),
                            snapshot.kernel_route.as_ref(),
                            &record.carriage,
                        )?;
                    }
                    declarations.push((*id, record.carriage.clone()));
                }
                Err(error) if complete => return Err(error),
                Err(_) => {}
            }
        }
        Ok(declarations)
    }

    /// Validate the offered set before a durable pruning-state installation.
    pub fn palw_validate_snapshot_declarations_v1_impl(
        &self,
        snapshot: &kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
        declarations: &[(kaspa_hashes::Hash64, Vec<u8>)],
        complete: bool,
    ) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for (id, bytes) in declarations {
            if !seen.insert(*id) {
                return Err(format!("duplicate snapshot declaration {id}"));
            }
            let class = snapshot.classes.get(id).ok_or_else(|| format!("unknown snapshot declaration {id}"))?;
            if snapshot.tir_classes.contains_key(id) || snapshot.gen_classes.contains_key(id) {
                return Err(format!("class {id} already carries its program in the snapshot"));
            }
            kaspa_consensus_core::palw_class_sync_v1::validate_class_declaration_v1(
                *id,
                class,
                snapshot.class_semantics.get(id),
                bytes,
            )?;
            if complete {
                self.palw_snapshot_declaration_identity_v1(
                    *id,
                    class,
                    snapshot.class_semantics.get(id),
                    snapshot.kernel_route.as_ref(),
                    bytes,
                )?;
            }
        }
        if complete {
            for (id, class) in &snapshot.classes {
                if seen.contains(id) || snapshot.tir_classes.contains_key(id) || snapshot.gen_classes.contains_key(id) {
                    continue;
                }
                if !self.palw_snapshot_builtin_class_v1(*id, class, snapshot.class_semantics.get(id))? {
                    return Err(format!("snapshot class {id} is missing its required declaration"));
                }
            }
        }
        Ok(())
    }

    fn palw_snapshot_declaration_identity_v1(
        &self,
        id: kaspa_hashes::Hash64,
        class: &kaspa_consensus_core::palw_state_v2::PalwClassStateV2,
        semantics: Option<&kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1>,
        route: Option<&kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1>,
        bytes: &[u8],
    ) -> Result<(), String> {
        // Already checked against the declaration by the shared helper.
        if semantics.is_some() {
            return Ok(());
        }
        // RFC06-8: a class registered past `palw_class_registration_origin_v1` carries its registering block and admission digest
        // in the snapshot's own state (kernel route aux 141); bytes that hash to that digest ARE the original declaration. A class
        // registered before the fence has no row and falls through to the genesis rule below, unchanged.
        if kaspa_consensus_core::palw_class_registration_origin_v1::palw_class_declaration_origin_authenticated_v1(
            route, &id, class, bytes,
        ) {
            return Ok(());
        }
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
        for object in &self.palw_genesis_objects_v2 {
            if let PalwConsensusObjectV2::ClassRegistered {
                class_id,
                artifact_root,
                pwu_rule,
                slash_value_per_pwu,
                admission: Some(authentic),
                ..
            } = object
            {
                if *class_id == id
                    && *artifact_root == class.artifact_root
                    && *pwu_rule == class.pwu_rule
                    && *slash_value_per_pwu == class.slash_value_per_pwu
                    && class.registered_daa == 0
                    && class.registrant_bond.is_none()
                    && borsh::to_vec(authentic.as_ref()).map_err(|e| e.to_string())? == bytes
                {
                    return Ok(());
                }
            }
        }
        Err(format!("class {id}: missing authenticated legacy canonical identity for complete snapshot capability"))
    }

    fn palw_snapshot_builtin_class_v1(
        &self,
        id: kaspa_hashes::Hash64,
        class: &kaspa_consensus_core::palw_state_v2::PalwClassStateV2,
        semantics: Option<&kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1>,
    ) -> Result<bool, String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
        let Some((root, pwu, slash)) = self.palw_genesis_objects_v2.iter().find_map(|object| match object {
            PalwConsensusObjectV2::ClassRegistered {
                class_id, artifact_root, pwu_rule, slash_value_per_pwu, admission: None, ..
            } if *class_id == id => Some((*artifact_root, *pwu_rule, *slash_value_per_pwu)),
            _ => None,
        }) else {
            return Ok(false);
        };
        if class.artifact_root != root
            || class.pwu_rule != pwu
            || class.slash_value_per_pwu != slash
            || class.registered_daa != 0
            || class.registrant_bond.is_some()
        {
            return Ok(false);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(court) = self.palw_court_params_v2.as_ref() {
            if let Some(entry) =
                misaka_palw_base0::classes::canonical_classes_v1(court).into_iter().find(|entry| entry.class_id() == id)
            {
                let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(
                    &entry.profile,
                    entry.canonical_job.0,
                    entry.canonical_job.1,
                );
                kaspa_consensus_core::palw_class_sync_v1::validate_class_context_v1(id, class, semantics, &entry.profile, &canonical)?;
                return Ok(true);
            }
        }
        // A matching catalog label alone is not a compiled execution declaration.
        Ok(false)
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
            .palw_v2_reader_state(state_params)
            .ok()
            .flatten()
            .ok_or("this node holds no V2 state to check a declaration against")?;
        let class = state.class(&class_id).ok_or_else(|| format!("this chain does not hold class {class_id}"))?;
        // Retain frozen class material for future snapshots without enabling
        // execution; the execution getter remains status gated.
        kaspa_consensus_core::palw_class_sync_v1::validate_class_declaration_v1(
            class_id,
            class,
            state.class_semantics_v1(&class_id),
            carriage_bytes,
        )?;
        let record = crate::model::stores::palw_class_carriage::PalwClassCarriageRecord {
            registered_daa: class.registered_daa,
            artifact_root: class.artifact_root,
            carriage: carriage_bytes.to_vec(),
        };
        self.palw_class_carriage_store.write().insert(class_id, record).map_err(|e| format!("cannot store the declaration: {e}"))
    }

    /// A bond's claims at the tip (ADR-0122 §6.5). See the trait docs. `with_vesting` adds claim
    /// row v3's vesting half (ADR-0152, phase2-plan §1.6) — the RPC's read only; node policy reads
    /// the rows alone (review of P2-10, finding 4) — read at the NEXT block: the virtual's DAA and
    /// the raw second-clock depth there, the two facts the next fold's step 3d reads, as
    /// `palw_vesting_v1_impl` reads it.
    pub fn palw_claim_rows_v1_impl(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        role: kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1,
        include_terminal: bool,
        limit: usize,
        with_vesting: bool,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwBondClaimsV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let vesting_at = with_vesting.then(|| {
            let next_daa = self.palw_next_block_daa_for_reads(&state);
            (next_daa, self.palw_settled_anchor_depth_at(next_daa))
        });
        // T12-058 (read side): whether the next block's fold refuses a retirement while a lock is
        // live — the fence `BondRetireRequested`'s refusal reads (`objective_offence_at`).
        let objective_offence = vesting_at.is_some_and(|(next_daa, _)| self.palw_objective_offence_at(next_daa));
        let mut claims = kaspa_consensus_core::palw_producer_v2::palw_bond_claims_with_locks_v1(
            &state,
            state_params,
            &bond,
            role,
            include_terminal,
            limit,
            vesting_at,
            objective_offence,
        );
        // V08 (the pre-t12 sweep): past R-core+ a `Provisional` row's date is its anchor slot, where
        // SW-8 binds or voids it — not the bind window's backstop. Read-side only.
        if let Some(panel) = self.palw_panel_params_v2.as_ref() {
            kaspa_consensus_core::palw_producer_v2::palw_claim_rows_bind_by_anchor_slot_v1(
                &mut claims.rows,
                state_params,
                panel.anchor_delay(),
            );
        }
        Some(claims)
    }

    /// **The DAA the next block folds at, for a read of the committed tip** (ADR-0152 P2-10): the
    /// virtual's, never below `tip + 1`. The virtual can trail the PALW tip for a moment (the tip
    /// store and the virtual state are written apart), and a read must not answer for a block at or
    /// below the one its state already folded; a virtual state that cannot be read is the same
    /// floor, not a missing answer — the registration terms' read clamps the same way (review of
    /// P2-10, finding 4).
    pub(super) fn palw_next_block_daa_for_reads(&self, state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2) -> u64 {
        let tip_daa = state.last_point().map(|point| point.daa_score).unwrap_or(0);
        let floor = tip_daa.saturating_add(1);
        self.virtual_stores.read().state.get().map(|virtual_state| virtual_state.daa_score).unwrap_or(floor).max(floor)
    }

    /// **ADR-0152 V-1…V-8, read side: `getPalwVesting` (op 199)** — the vesting table as the next
    /// block's step 3d will find it (phase2-plan §1.6, P2-10). The committed tip through
    /// `load_tip_cached` (the read-side rule above), the virtual's DAA as the next block's, and the
    /// RAW second-clock depth at that DAA (I-8: the pure functions compute the escape themselves).
    /// Everything else is `palw_vesting_read_v1`, which answers "what moves next" only through
    /// `palw_vesting_next_block_plan_v1` — never the planner on a committed state.
    pub fn palw_vesting_v1_impl(
        &self,
        query: kaspa_consensus_core::palw_vesting_read_v1::PalwVestingQueryV1,
        limit: usize,
        after: Option<(u64, kaspa_hashes::Hash64)>,
    ) -> Option<kaspa_consensus_core::palw_vesting_read_v1::PalwVestingReadV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let next_daa = self.palw_next_block_daa_for_reads(&state);
        Some(kaspa_consensus_core::palw_vesting_read_v1::palw_vesting_read_v1(
            &state,
            state_params,
            next_daa,
            self.palw_settled_anchor_depth_at(next_daa),
            &query,
            limit,
            after,
        ))
    }

    pub fn palw_reward_vesting_v2_impl(
        &self,
        query: kaspa_consensus_core::palw_vesting_read_v1::PalwVestingQueryV1,
        limit: usize,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
        exec_after: Option<kaspa_consensus_core::palw_reward_vesting_read_v2::PalwExecRewardCursorV2>,
    ) -> Option<kaspa_consensus_core::palw_reward_vesting_read_v2::PalwRewardVestingReadV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let next_daa = self.palw_next_block_daa_for_reads(&state);
        Some(kaspa_consensus_core::palw_reward_vesting_read_v2::palw_reward_vesting_read_v2(
            &state,
            state_params,
            next_daa,
            self.palw_settled_anchor_depth_at(next_daa),
            &query,
            limit,
            after,
            exec_after,
        ))
    }

    /// **RFC-0008 v2: `getPalwExecV2Status` (op 240)** — the EXEC v2 lane's observation over the committed tip (`load_tip_cached`):
    /// roots, slices and the state of each verification claim, `block`'s refusal record (its delta's `ExecV2Verdict` notes), the lane's
    /// health. A read: no rule calls it. `None` where the payload is not armed.
    pub fn palw_exec_v2_observation_v1_impl(
        &self,
        roots: Vec<kaspa_hashes::Hash64>,
        block: Option<BlockHash>,
    ) -> Option<kaspa_consensus_core::palw_exec_v2_verify::PalwExecV2ObservationV1> {
        let fence = self.palw_exec_v2?;
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        let tip_daa = self.headers_store.get_daa_score(tip).unwrap_or_default();
        let record = block.map(|hash| {
            let notes = self
                .palw_state_v2_store
                .read()
                .delta_of_retained_v1(hash)
                .ok()
                .map(|(_, delta)| {
                    delta
                        .entries
                        .iter()
                        .filter_map(|entry| match entry {
                            kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::ExecV2Verdict { carrier, code } => {
                                Some((*carrier, *code))
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            (hash, notes)
        });
        let lane = self.palw_exec_v2_health(&state, tip, tip_daa);
        Some(kaspa_consensus_core::palw_exec_v2_verify::palw_exec_v2_observation_with_retention_v2(
            &state,
            Some(fence.daa_score()),
            tip,
            tip_daa,
            &roots,
            record,
            lane,
            Some(state_params.window_court()),
        ))
    }

    /// **RFC-0008 v2 amendment 1: the slice statement `bond` signs for `(root, index)`, backed by the kernel claim `claim`**, over the
    /// committed tip (the EXEC_SLICE producer's input). A read.
    pub fn palw_exec_v2_slice_statement_v1_impl(
        &self,
        root: kaspa_hashes::Hash64,
        index: u32,
        claim: kaspa_hashes::Hash64,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<kaspa_consensus_core::palw_exec_v2::PalwWorkSliceV1, String> {
        let state_params = self.palw_state_params_v2.as_ref().ok_or("this network has no PALW state")?;
        let (tip, state) =
            self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten().ok_or("no PALW tip on this node yet")?;
        let tip_daa = self.headers_store.get_daa_score(tip).map_err(|error| error.to_string())?;
        kaspa_consensus_core::palw_exec_v2_verify::palw_exec_v2_slice_statement_at_depth_v2(
            &state,
            state_params,
            root,
            index,
            claim,
            bond,
            self.palw_settled_anchor_depth_at(tip_daa),
        )
    }

    /// **RFC-0008 v2 amendment 1, §10.3: the relay prefilter of an `EXEC_SLICE` block** against the committed tip: the root is open, the
    /// executor authorised, the index inside `[next, next + pending depth)`, the verification claim held by the kernel route (where the
    /// route exists — the fold asks the binding only there). Node-local: validity never reads it.
    pub fn palw_exec_v2_slice_relayable_v1_impl(&self, slice: &kaspa_consensus_core::palw_exec_v2::PalwWorkSliceV1) -> bool {
        use kaspa_consensus_core::palw_work_slice_v2::PALW_EXEC_V2_MAX_PENDING_DEPTH;
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return true };
        let Ok(Some((_, state))) = self.palw_state_v2_store.read().load_tip_cached(state_params) else { return true };
        let Some(root) = state.exec_v2_root_v1(&slice.root_claim_id) else { return false };
        root.is_open()
            && root.authorises(&slice.executor_bond)
            && slice.slice_index >= root.next_index
            && slice.slice_index < root.next_index.saturating_add(PALW_EXEC_V2_MAX_PENDING_DEPTH)
            && state.kernel_route().is_none_or(|route| {
                route.rows.contains_key(&(
                    misaka_palw_kernel::rows::TABLE_CLAIMS_V1,
                    kaspa_consensus_core::palw_exec_v2_verify::palw_exec_v2_kernel_key_v1(&slice.evidence_root),
                ))
            })
    }

    /// **RFC-0008 v2: has an anchor on the committed tip's chain covered `block`?** (The covered set within the anchoring window.)
    pub fn palw_exec_v2_anchored_v1_impl(&self, block: BlockHash) -> bool {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return false };
        let Ok(Some((_, state))) = self.palw_state_v2_store.read().load_tip_cached(state_params) else { return false };
        state.exec_v2_anchored_v1(&block)
    }

    /// **RFC-0010: `getPalwPanelV3Status` (op 220)** — the permissionless Panel's observation over the committed tip
    /// (`load_tip_cached`, the read-side rule above). A read of the state as it stands: no clock, no draw, no rule calls it.
    pub fn palw_panel_v3_observation_v1_impl(
        &self,
        ids: Vec<kaspa_hashes::Hash64>,
        limit: usize,
    ) -> Option<kaspa_consensus_core::palw_permissionless_panel_v1::PanelV3ObservationV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        Some(kaspa_consensus_core::palw_permissionless_panel_v1::panel_v3_observation_v1(&state, state_params, &ids, limit))
    }

    /// **ADR-0160 §7.5: the capacity shadow** — `palw_capacity_shadow_with_v1` over the committed
    /// tip (`load_tip_cached`), at the next block's DAA with the RAW second-clock depth there, as
    /// the vesting read asks. A read for `getPalwCapacityShadow` and kaspad's interval log; no rule
    /// calls it (`palw_capacity_shadow_is_node_only` pins that this is the one consensus-crate call).
    pub fn palw_capacity_shadow_v1_impl(
        &self,
        mut options: kaspa_consensus_core::palw_capacity_shadow_v1::PalwCapacityShadowOptionsV1,
    ) -> Option<kaspa_consensus_core::palw_capacity_shadow_v1::PalwCapacityShadowV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let next_daa = self.palw_next_block_daa_for_reads(&state);
        options.raw_depth = self.palw_settled_anchor_depth_at(next_daa);
        // The default display (review of lane shadow, finding 1): the schedule F-L arms once the
        // params carry one — `rcore/cap-int` passes `palw_capacity_aggregate_liability`'s steps here
        // in place of `None` — else the uncredited ramp (q = 0, m_c = E). ADR-0160 v1's reference
        // ramp (q = 143‰) is priced only when a caller names it, and prices as the uncredited one
        // (below q_seat nothing is credited; review of lane shadow, round 2, finding 2).
        if options.steps.is_empty() {
            // rcore/cap-s1: the schedule F-L arms, once the params carry one (the shadow's own step type
            // is the formulas module's; mapped field for field).
            let armed: Option<Vec<kaspa_consensus_core::palw_capacity_formulas_v1::PalwCapacityStepV1>> =
                state_params.capacity_liability().map(|value| {
                    value
                        .steps
                        .iter()
                        .map(|step| kaspa_consensus_core::palw_capacity_formulas_v1::PalwCapacityStepV1 {
                            from_daa: step.from_daa,
                            rho: step.rho,
                            q_credit_permille: step.q_credit_permille,
                        })
                        .collect()
                });
            options.steps = kaspa_consensus_core::palw_capacity_shadow_v1::palw_capacity_display_steps_v1(armed.as_deref());
        }
        // `E` of a claim the next block would accept: the tip's subsidy under the carve the fold
        // resolves at that DAA (ADR-0126's overlay on testnet-12), for a state with no claim to read.
        let subsidy = state.last_point().map(|point| point.subsidy).unwrap_or(0);
        options.reference_escrow_sompi = Some(kaspa_consensus_core::palw_state_v2::palw_claim_escrow_v1(
            state_params,
            subsidy,
            self.palw_escrow_carve_at(next_daa, next_daa),
        ));
        Some(kaspa_consensus_core::palw_capacity_shadow_v1::palw_capacity_shadow_with_v1(&state, state_params, next_daa, &options))
    }

    pub fn palw_seat_duties_v2_impl(
        &self,
        mine: &[kaspa_consensus_core::palw_state_v2::PalwBondKeyV2],
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_v2_reader_state(state_params).ok().flatten() else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&state, state_params, mine)
    }

    /// **RFC-0006 × G14: the shards `watcher` may watch at the tip** (agent SHARD) — the same store-tip read as the seat duties.
    pub fn palw_tir_shard_watch_duties_v1_impl(
        &self,
        watcher: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Some((_, state)) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten() else {
            return Vec::new();
        };
        kaspa_consensus_core::palw_tir_shard_watch_v1::palw_tir_shard_watch_duties_v1(&state, state_params, watcher)
    }

    /// **ADR-0148: the free-prompt lane's price for one job, at the virtual** — the fold's own
    /// function over the tip state, at the DAA a commitment sent now would be accepted at, with the
    /// bond's room by the fold's two terms. A gateway reads this AFTER its job ran and BEFORE the
    /// commitment is written, so what it checks and what the ledger reserves are one expression.
    /// Past `palw_rcore_plus` the price is the fold's `ProducerBelowFloor` for a bond short of the
    /// producer floor, then its `BondClassShareExceeded` for a bond already at its ADR-0152 T-2(a)
    /// share of the class — refusals the commitment arm reaches, in that order, before it prices.
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
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
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
            // RFC-0002 Phase H: the IR class lane opens with the TIR public frame (the fold's own reading of the same fence).
            tir_free_prompt_active: self.palw_tir_public_conviction_v1.is_some_and(|fence| fence.is_active(daa_score)),
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
        // **ADR-0152 v3.1 T-2(a) (S-6): the executor's share, which the fold's commitment arm asks
        // before it prices** — so past `palw_rcore_plus` a bond at its share of the class is
        // answered with the fold's own refusal by name, and neither a gateway nor this node's
        // canonical rail writes a commitment the acceptance rehearsal would drop. The share is per
        // bond: without one there is nothing to ask.
        let share_refusal = bond.filter(|_| state_params.rcore_plus_active_at(daa_score)).and_then(|outpoint| {
            let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                block: chain_point,
                daa_score,
                blue_score: 0,
                subsidy: 0,
            });
            kaspa_consensus_core::palw_state_v2::palw_bond_class_share_admits_v1(
                &state,
                state_params,
                &extras,
                &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint),
                &class_id,
                daa_score,
            )
            .err()
        });
        // **ADR-0152 U2 (P6): the producer floor, which the commitment arm asks before the class
        // gate and the share** — so a bond short of it is answered with the fold's own
        // `ProducerBelowFloor` by name, first, and neither a gateway nor the canonical rail runs a
        // job into a commitment the acceptance rehearsal drops. `None` below the fence.
        let floor_refusal = bond.and_then(|outpoint| {
            let key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint);
            kaspa_consensus_core::palw_state_v2::palw_bond_producer_floor_shortfall_v1(&state, state_params, &key, daa_score).map(
                |shortfall| {
                    let floor = state_params.min_collateral_sompi();
                    kaspa_consensus_core::palw_state_v2::PalwStateV2Error::ProducerBelowFloor {
                        bond: key,
                        collateral: floor.saturating_sub(shortfall),
                        floor,
                    }
                },
            )
        });
        let price = match (floor_refusal, share_refusal) {
            (Some(refusal), _) | (None, Some(refusal)) => Err(refusal),
            (None, None) => price,
        };
        let bond_room = bond.and_then(|outpoint| {
            // ADR-0152 SR-7: the one committed ledger past `palw_rcore_plus`, as the FP ceiling reads it.
            kaspa_consensus_core::palw_state_v2::palw_fp_bond_room_v2(
                &state,
                state_params,
                &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint),
                self.palw_capability_bound_at(daa_score),
                daa_score,
                self.palw_settled_anchor_depth_at(daa_score),
            )
        });
        Some(kaspa_consensus_core::palw_state_v2::PalwFpPriceAnswerV1 { daa_score, price, bond_room })
    }

    /// **ADR-0089 Decision 2's window, with the fold's market gate in it** (the 2026-09-23 Position
    /// route matrix, P-B3). `evm_view_v1` of `state` — the EVM block's selected parent — and, past
    /// `palw_audit_2026_09_23` at `daa_score` (the EVM block's own), the classes whose seed and buy
    /// `palw_model_market_admits_v1` refuses under the extras that block folds with, so the writer
    /// reverts those moves before it takes an escrow the fold would refund. The ONE builder the
    /// block validator, the template and the RPC's simulator all call, with the selected parent as
    /// the extras' point on both consensus sides (the gate reads only DAA-resolved fences and the
    /// state's lifecycles, so the two sides fill the same set). Below the fence the set is empty
    /// and the window is exactly `evm_view_v1`'s: testnet-11's EVM execution does not move.
    pub fn palw_evm_view_with_market_gate_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        selected_parent: BlockHash,
        daa_score: u64,
    ) -> kaspa_consensus_core::evm::model_market::PalwEvmViewV1 {
        let mut view = state.evm_view_v1(kaspa_consensus_core::evm::EVM_CHAIN_ID, params.base_class_id());
        // ADR-0162: past the virtual fence the set is the classes whose market does not trade, and it
        // is filled there too (the writer reverts a buy on one at the call).
        if self.palw_audit_2026_09_23_at(daa_score) || self.palw_model_virtual_v1_from_at(daa_score).is_some() {
            let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                block: selected_parent,
                daa_score,
                blue_score: 0,
                subsidy: 0,
            });
            view.market_refused_classes =
                kaspa_consensus_core::palw_state_v2::palw_evm_market_refused_classes_v1(state, params, &extras);
        }
        view
    }

    /// **The 2026-09-23 Position route matrix, P-B3: the fold's market gate, asked before anyone
    /// pays.** The tip's state under the extras the virtual's next block folds with — resolved as the
    /// producer's class gate above resolves them — through `palw_model_market_admits_v1`, the
    /// function `model_seed_v1` and `model_buy_v1` themselves call, so the preview and the fold ask
    /// one predicate. Its input is the tip's lifecycle, and the fold of the next block steps the
    /// registry before it applies objects, so the answer can be one lifecycle step stale in either
    /// direction. On the carrier lane a refused seed or buy still lands its carrier (refunded past
    /// the audit fence, P-B1): a payer who asks here first is the one who does not wait for that.
    pub fn palw_model_market_gate_v1_impl(
        &self,
        line_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::api::PalwModelMarketGateReadV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let class_id = state.model_line_or_founding(&line_id)?.class_id;
        let virtual_read = self.virtual_stores.read();
        let candidate_daa = virtual_read.state.get().ok()?.daa_score;
        drop(virtual_read);
        let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score: candidate_daa,
            blue_score: 0,
            subsidy: 0,
        });
        Some(kaspa_consensus_core::api::PalwModelMarketGateReadV1 {
            lifecycle: state.model_lifecycle(&class_id).map(|row| format!("{:?}", row.state)).unwrap_or_default(),
            refusal: kaspa_consensus_core::palw_state_v2::palw_model_market_admits_v1(&state, state_params, &extras, &class_id)
                .err()
                .map(|refusal| refusal.to_string()),
        })
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
        let Ok(Some((chain_point, state))) = self.palw_v2_reader_state(state_params) else {
            return Vec::new();
        };
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(bond);
        let pricing = self.palw_fp_pricing();

        // 2026-09-24 DoS audit #5: a convicted execution's quanta are refused at admission, so
        // listing them would send a producer to build a block the network disqualifies. The set is
        // built ONCE per call (the audit review): asking `palw_execution_root_is_forfeited_v1` per
        // claim scanned every consumed offence for every Final free-prompt claim, O(claims ×
        // offences) on the producer's path to being paid.
        let forfeited = state
            .last_point()
            .is_some_and(|point| self.palw_audit_2026_09_23_at(point.daa_score))
            .then(|| state.palw_forfeited_execution_roots_v1());
        let mut out = Vec::new();
        for (claim_id, claim) in state.claims_iter() {
            if claim.bond != key {
                continue;
            }
            let PalwClaimPhaseV2::Final { final_daa } = claim.phase else { continue };
            let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source else { continue };
            if forfeited.as_ref().is_some_and(|roots| roots.contains(&claim.execution_root)) {
                continue;
            }
            // RFC-0018 18-A (Gate C): an uncertified governed claim is not spendable (the admission refuses it), and a certified one's
            // draw slot counts from its certification — the same predicate the admission and the fold read.
            let Ok(final_daa) =
                kaspa_consensus_core::palw_rfc0018_v1::palw_fp_receipt_clock_v1(state_params, &state, claim_id, claim, final_daa)
            else {
                continue;
            };
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
    pub(in crate::pipeline::virtual_processor) fn palw_v2_bonds_declared_in_mergeset(
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
        self.palw_locked_bond_outpoints_v2_at(self.lkg_virtual_state.load().daa_score)
    }

    /// [`Self::palw_locked_bond_outpoints_v2_impl`] at an explicit DAA: the wallet's answer is the
    /// tip's `palw_v2_locked_bond_outpoints` at the node's virtual DAA, and nothing else — split out
    /// so ADR-0152 T23 can hold the wallet's set, the block path's and the burn obligations to
    /// `palw_bond_collateral_is_locked_v6` at the DAAs B-3 turns on (`F + 2,999`, `F + 3,000`, a
    /// licence halt, a carried row) without a chain that long.
    pub(in crate::pipeline::virtual_processor) fn palw_locked_bond_outpoints_v2_at(&self, now_daa: u64) -> Vec<TransactionOutpoint> {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return Vec::new();
        };
        let Ok(Some((_, state))) = self.palw_v2_reader_state(state_params) else {
            return Vec::new();
        };
        let mut out: Vec<TransactionOutpoint> = self.palw_v2_locked_bond_outpoints(&state, now_daa).into_iter().collect();
        // Deterministic order, so two nodes answering the same question give the same answer and a
        // paging caller cannot be handed a shuffled set.
        out.sort_by(|a, b| (a.transaction_id, a.index).cmp(&(b.transaction_id, b.index)));
        out
    }

    pub(in crate::pipeline::virtual_processor) fn palw_v2_locked_bond_outpoints(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        now_daa: u64,
    ) -> std::collections::HashSet<TransactionOutpoint> {
        let Some(_bond_params) = self.palw_bond_params_v2.as_ref() else {
            return Default::default();
        };
        // Both clocks, after the liveness escape; past the fence the gate also reads what the bond
        // still stands behind — its reservations and its live slashable locks (2026-09-24 DoS
        // audit, fix #1). One predicate for the lock and the burn beside it.
        // RFC-0015 §8.3.3 / §13.2 (2026-10-15): on a network with an OPV policy a bond's live duties (its OPV reservation to the
        // liability horizon, its budget window to `d + W` and liability hold) hold its collateral even below the audit fence.
        let duty_gate = self.palw_audit_2026_09_23_at(now_daa) || self.palw_kernel_opv_fence().is_some();
        state
            .bonds_iter()
            .filter(|(key, record)| self.palw_v2_bond_is_locked(state, key, record, now_daa, duty_gate))
            .map(|(key, _)| key.0)
            .collect()
    }

    /// **The one withdrawal gate both UTXO sites read** (ADR-0152 B-3, S-SPEC §3.10, §9 P3):
    /// `palw_bond_collateral_is_locked_v6` with the RAW second-clock depth (`palw_settled_anchor_depth_at`)
    /// — v6 computes the liveness escape itself from the state's ring and the params' court window,
    /// which is exactly what `palw_second_clock_depth_at` computed here before, and adds the accuser
    /// and vesting-payee clauses past `palw_rcore_plus`. Actual forced EXEC vesting also holds its
    /// payees below that fence; without EXEC receipts the old v5 rule remains byte-identical. A
    /// node with no state params keeps v5 with the unbounded second-clock hold (fail closed), as before.
    pub(super) fn palw_v2_bond_is_locked(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        key: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        record: &kaspa_consensus_core::palw_state_v2::PalwBondStateV2,
        now_daa: u64,
        duty_gate: bool,
    ) -> bool {
        let delay = self.palw_bond_withdrawal_delay_at(now_daa);
        match self.palw_state_params_v2.as_ref() {
            Some(params) => kaspa_consensus_core::palw_state_v2::palw_bond_collateral_is_locked_v6(
                state,
                params,
                key,
                record,
                now_daa,
                delay,
                self.palw_settled_anchor_depth_at(now_daa),
                duty_gate,
            ),
            None => kaspa_consensus_core::palw_state_v2::palw_bond_collateral_is_locked_v5(
                state,
                key,
                record,
                now_daa,
                delay,
                self.palw_second_clock_depth_at(state, now_daa),
                duty_gate,
                u64::MAX,
            ),
        }
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
    ///
    /// **Past `Params::palw_rcore_plus` (ADR-0152 V-2) nothing here moves: withholding stays at
    /// acceptance, and only the RELEASE is deferred.** The release path is `Final → vesting row →
    /// step 3d at maturity → pending_payouts → the next block's coinbase`
    /// ([`Self::palw_v2_payout_outputs`], unchanged): `Final` names the escrow in a row instead of
    /// the queue, a conviction inside the conviction window burns the row (V-5) — withheld here and
    /// never minted, exactly as a void's forfeit — and only a matured row's legs reach a coinbase.
    /// So every sompi withheld here stands, at any committed state, in exactly one of: the escrow
    /// of a claim not yet resolved (`Provisional` through `ReceiptLicensed`, still recorded on the
    /// claim), minted from a row, moved and not yet minted (a queue row the next coinbase renders),
    /// a live row, a row burned (or a void's forfeit, or a skipped attempt's carve), the work-price
    /// remainder named nowhere, the ADR-0091 buyback slice, or `panel_reserve_sompi` — V-3's
    /// coinbase identity (T03, which attributes the minted term by coinbase position against the
    /// parent queue's keys, since one payout script receives round fees, vesting mints and seat pay
    /// alike). Reporter rewards and market rows are minted from the same queue and are NOT in that
    /// identity: they are funded by slashes and by sinks, never by this carve.
    pub(in crate::pipeline::virtual_processor) fn palw_v2_escrow_withheld_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
        paying_daa: u64,
    ) -> u64 {
        let recorded = state
            .claims_iter()
            .filter(|(_, claim)| claim.accepted_block == block)
            .fold(0u64, |acc, (_, claim)| acc.saturating_add(claim.escrowed_reward));
        let skipped = self.palw_v2_skipped_own_attempt_carve(state, block);
        // One carve, withheld once: from the claim the fold admitted, or as the carve of the attempt
        // it skipped — never both. Only an attempt escrows (a free-prompt claim records 0), and an
        // attempt's claim is recorded against its CARRYING block, so a block whose own attempt was
        // skipped has no escrowing claim accepted in it. T03's identity counts each carve once.
        debug_assert!(
            recorded == 0 || skipped == 0,
            "block {block}: an attempt's carve is withheld from its claim or as skipped, not both"
        );
        self.native_base_withheld_at(block, paying_daa, recorded.saturating_add(skipped))
    }

    /// The carve of `block`'s own attempt where the fold skipped it — see
    /// [`Self::palw_v2_escrow_withheld_at`]. Zero below `palw_audit_2026_09_23` (at the block's own
    /// DAA, the height its fold resolved the fence at), for a block that carries no attempt, and for
    /// one whose attempt the state holds as a claim accepted in it.
    pub(super) fn palw_v2_skipped_own_attempt_carve(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
    ) -> u64 {
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
        let Ok(envelope) = kaspa_consensus_core::palw_attempt_material_v3::native_attempt_base_v3(&header.palw_commitment) else {
            return 0;
        };
        let own = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&envelope.attempt);
        if state.claim(&own).is_some_and(|claim| claim.accepted_block == block) {
            return 0;
        }
        state_params.worker_carve_at(
            self.coinbase_manager.calc_block_subsidy(header.daa_score),
            self.palw_escrow_for_carrier_at(block, header.daa_score, header.daa_score),
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
    pub(in crate::pipeline::virtual_processor) fn palw_v2_merged_escrow_withheld(
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
        let (works, skips) = self.palw_v2_merged_works(ghostdag_data, state, state_params, mergeset_non_daa, point);
        for work in &works {
            if let PalwMergedOwnedWorkV1::Attempt(blue, _, subsidy, carve, _, _) = work {
                withheld.insert(
                    *blue,
                    self.native_base_withheld_at(*blue, point.daa_score, state_params.worker_carve_at(*subsidy, *carve)),
                );
            }
        }
        // ADR-0176 C1 (user decision 2026-10-15): a merged attempt-lane block whose attempt the state refused (a skip) is paid no
        // subsidy past a V3 budget — its whole worker base is withheld and never minted (0 elsewhere, so the map is unchanged there).
        for (blue, _) in &skips {
            let base = self.palw_r_cap_base_withheld_at(*blue, point.daa_score, 0);
            if base > 0 {
                withheld.entry(*blue).or_insert(base);
            }
        }
        withheld
    }

    /// **ADR-0176 C1 (user decision 2026-10-15): the attempt-lane worker base under the per-bond `R` cap.** Past a V3 bond budget
    /// ([`kaspa_consensus_core::palw_bond_budget_v1::palw_r_cap_covers_worker_base_v1`]) an attempt-algo carrier's WHOLE
    /// subsidy-derived worker base (`split_block_subsidy(its subsidy, the paying block's split).worker_base_sompi`, exactly the term
    /// the coinbase pays it) is withheld: `max(recorded, base)`. The claim's escrow — its `R` reservation — is the only subsidy that
    /// carrier is ever paid (at `Final`); the uncarved remainder, and the whole base of a budget-refused attempt, are never minted.
    /// The fee-derived worker shares are user-paid and stay outside `R` (separate accounting): the withheld amount never exceeds the
    /// subsidy-derived share. `recorded` on every network without such a budget — every shipped preset.
    pub(in crate::pipeline::virtual_processor) fn palw_r_cap_base_withheld_at(
        &self,
        carrier: BlockHash,
        paying_daa: u64,
        recorded: u64,
    ) -> u64 {
        let Some(state_params) = self.palw_state_params_v2.as_ref() else {
            return recorded;
        };
        let covers = kaspa_consensus_core::palw_bond_budget_v1::palw_r_cap_covers_worker_base_v1(state_params.bond_budget(), paying_daa);
        if !covers {
            return recorded;
        }
        let Ok(header) = self.headers_store.get_header(carrier) else {
            return recorded;
        };
        if !kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id) {
            return recorded;
        }
        let subsidy = self.coinbase_manager.calc_block_subsidy(header.daa_score);
        let base = match self.fee_split_at(paying_daa) {
            Some(split) => kaspa_consensus_core::dns_finality::split_block_subsidy(subsidy, &split).worker_base_sompi,
            None => subsidy,
        };
        kaspa_consensus_core::palw_bond_budget_v1::palw_r_cap_withheld_base_v1(true, recorded, base)
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
    pub(super) fn palw_v2_template_point(
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

    /// **LIVE-R1 N3: the PALW state every `palw_*` reader answers from — the state AT the committed
    /// virtual sink**, with the same signature as the store's `load_tip_cached` it replaces.
    ///
    /// The readers used to read the tip ROW, and the tip row is not the sink: `calculate_utxo_state_relatively`
    /// writes it wherever each walk of the sink search ends, so while a search is running it stands on
    /// whatever candidate was walked last. On a node that refuses a heavier branch on every resolve (devnet
    /// r1's B) a search is running most of the time and the row stands ON THE REFUSED BRANCH — B's model
    /// registry row for the class it had folded at DAA 75 read empty while B's sink still carried it, and
    /// its templates logged the tip 1–100 blocks into the other branch. Answering at the sink is the
    /// invariant every reader already assumed (each pairs the state with the virtual's DAA).
    ///
    /// Lock-free on the virtual: the sink comes from `lkg_virtual_state`, never `virtual_stores`, because
    /// template building and the mempool call these readers under a held virtual read guard, and a second
    /// read of that parking_lot lock behind the commit's queued upgrade is a deadlock (D6's shape). The state
    /// at a sink is a pure function of the chain, so the derived snapshot is cached by sink alone: one walk
    /// per sink change, whatever the tip row does in between. A failed walk is
    /// unavailable: another branch's tip cannot supply the sink's state.
    pub(crate) fn palw_v2_reader_state(
        &self,
        params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
    ) -> kaspa_database::prelude::StoreResult<
        Option<(BlockHash, Arc<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>)>,
    > {
        let sink = self.lkg_virtual_state.load().ghostdag_data.selected_parent;
        if let Some((at, state)) = self.palw_reader_snapshot.lock().as_ref()
            && *at == sink
        {
            return Ok(Some((sink, Arc::clone(state))));
        }
        let store = self.palw_state_v2_store.read();
        let Some((tip_block, tip_state)) = store.load_tip_cached(params)? else { return Ok(None) };
        if tip_block == sink {
            // The common case: the store's own cache holds it; keep no second copy.
            *self.palw_reader_snapshot.lock() = None;
            return Ok(Some((tip_block, tip_state)));
        }
        if !self.reachability_service.has_reachability_data(sink) || !self.reachability_service.has_reachability_data(tip_block) {
            return Err(kaspa_database::prelude::StoreError::DataInconsistency(format!(
                "native reader cannot authenticate sink {sink} from tip {tip_block}: reachability unavailable"
            )));
        }
        let path = self.dag_traversal_manager.calculate_chain_path(tip_block, sink, None);
        let (removed, added): (Vec<BlockHash>, Vec<BlockHash>) = (path.removed.to_vec(), path.added.to_vec());
        match crate::processes::palw_state_walk::walk_chain_path_retained_v1(
            &store,
            params,
            tip_state.try_clone_v1().map_err(|e| kaspa_database::prelude::StoreError::LocalResourceUnavailable(e.to_string()))?,
            &removed,
            &added,
        ) {
            Ok(state) => {
                let state = Arc::new(state);
                *self.palw_reader_snapshot.lock() = Some((sink, Arc::clone(&state)));
                Ok(Some((sink, state)))
            }
            Err(error) => {
                if let Some(unavailable) = error.native_capacity_v1() {
                    Err(kaspa_database::prelude::StoreError::LocalResourceUnavailable(unavailable.to_string()))
                } else {
                    Err(kaspa_database::prelude::StoreError::DataInconsistency(format!(
                        "native reader cannot reconstruct accepted sink {sink} from tip {tip_block}: {error}"
                    )))
                }
            }
        }
    }

    /// **LIVE-R1 N2, the consensus half**: whether this node's last resolves settled after refusing a
    /// heavier candidate on the PALW deep-reorg rule, the candidate having been UTXO-validated and weighed
    /// (both sides' `PalwCandidateOrderV1` read) — never a header-only or unweighable chain. `None` once a
    /// resolve settles without such a refusal. The flows combine it with which chain the node's outbound,
    /// long-lived peers are on (`PartitionWatch`); on its own it is NOT a reason to stop, because an
    /// attacker's released heavy junk branch produces exactly this record on every honest node.
    pub fn palw_partition_refusal_v1(&self) -> Option<kaspa_consensus_core::api::PalwPartitionRefusalV1> {
        *self.palw_refusal_streak.lock()
    }

    /// PALW V2 state as-of `at` — the same reconstruction `calculate_utxo_state_relatively` uses,
    /// so a template cannot commit a root the virtual walk will then refuse.
    ///
    /// The stored tip and virtual's selected parent agree between rounds. They disagree after an
    /// unclean shutdown (tip written, virtual not) and whenever this template's selected parent is
    /// not the store tip. Walking is a no-op when they agree. testnet-11 2026-09-21 `df80394b`
    /// committed the store tip `a9a629…` while validators compared the selected-parent root
    /// `7d36f3…` and disqualified a UTXO-valid chain block on that mismatch alone.
    pub(super) fn palw_v2_state_at(
        &self,
        at: BlockHash,
    ) -> Option<(BlockHash, Arc<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>)> {
        let params = self.palw_state_params_v2.as_ref()?;
        let store = self.palw_state_v2_store.read();
        let (tip_block, tip_state) = store.load_tip_cached(params).ok().flatten()?;
        if tip_block == at {
            return Some((tip_block, tip_state));
        }
        let path = self.dag_traversal_manager.calculate_chain_path(tip_block, at, None);
        let removed: Vec<BlockHash> = path.removed.to_vec();
        let added: Vec<BlockHash> = path.added.to_vec();
        match crate::processes::palw_state_walk::walk_chain_path_retained_v1(
            &store,
            params,
            tip_state.try_clone_v1().ok()?,
            &removed,
            &added,
        ) {
            Ok(state) => {
                warn!(
                    "PALW V2 state tip stood at {tip_block} while the template selected parent is {at}; re-derived the parent state over {} reverted and {} applied deltas so the committed root matches the validating walk",
                    removed.len(),
                    added.len()
                );
                Some((at, Arc::new(state)))
            }
            Err(e) => match store.load_pruning_snapshot_retained_v1(params) {
                Ok(Some((snap_block, snap_state))) => {
                    let snap_path = self.dag_traversal_manager.calculate_chain_path(snap_block, at, None);
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

    pub(in crate::pipeline::virtual_processor) fn palw_v2_unentitled_blues(
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
        let classified = ghostdag_data.classify_palw_mergeset_v1(|member| self.ghostdag_manager.is_round_block(member));
        let mergeset_daa = classified.blues().iter().chain(classified.genuine_reds()).filter(|h| !mergeset_non_daa.contains(h));
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

    /// **RFC-0009: how each merged V4 receipt block is paid** — `{ block → (the executor bond's payout script, the authorization's fee
    /// bps) }` for every entitled in-DAA-window blue or red (the selected parent included) whose header carries a valid `PFS4` spend.
    ///
    /// Read from headers and the SAME selected-parent `state` the entitlement and the escrow read, so the template and every validating
    /// node compute one map. The miner leg goes to `bond.payout_payload` of the CLAIM's executor bond (the admission proved the spend names
    /// it), derived to a script exactly as every PALW payout is. A block whose claim or bond the state no longer holds gets no entry and is
    /// paid in full to its own script as before — deterministic on both paths, never an extra mint (the split only moves value between two
    /// payees of one block). Empty (and nothing read) while `palw_receipt_spend_v4` is dormant, which is every shipped preset.
    pub(in crate::pipeline::virtual_processor) fn palw_v2_receipt_v4_payouts(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        ghostdag_data: &GhostdagData,
        mergeset_non_daa: &BlockHashSet,
        unentitled: &BlockHashSet,
    ) -> BlockHashMap<kaspa_consensus_core::palw_receipt_v4::PalwReceiptV4Payout> {
        use kaspa_consensus_core::palw_receipt_v4::{
            PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS, PalwReceiptSpendEnvelopeV4, PalwReceiptV4Payout, palw_receipt_v4_carriage_is_v4,
            palw_receipt_v4_miner_script,
        };
        let mut payouts = BlockHashMap::default();
        let Some(fence) = self.palw_receipt_spend_v4 else {
            return payouts;
        };
        let classified = ghostdag_data.classify_palw_mergeset_v1(|member| self.ghostdag_manager.is_round_block(member));
        for block in classified.blues().iter().chain(classified.genuine_reds()) {
            if mergeset_non_daa.contains(block) || unentitled.contains(block) {
                continue;
            }
            let Ok(header) = self.headers_store.get_header(*block) else { continue };
            if header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3
                || !fence.is_active(header.daa_score)
                || !palw_receipt_v4_carriage_is_v4(&header.palw_commitment)
            {
                continue;
            }
            let Ok(envelope) = PalwReceiptSpendEnvelopeV4::decode(&header.palw_commitment) else { continue };
            let Some(claim) = state.claim(&envelope.spend.claim_id) else { continue };
            let Some(bond) = state.bond(&claim.bond) else { continue };
            payouts.insert(
                *block,
                PalwReceiptV4Payout {
                    miner_script: palw_receipt_v4_miner_script(&bond.payout_payload),
                    fee_bps: envelope.spend.authorization.builder_fee_bps.min(PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS),
                },
            );
        }
        payouts
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
    pub(in crate::pipeline::virtual_processor) fn palw_v2_bond_burn_obligations(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        now_daa: u64,
    ) -> std::collections::HashMap<TransactionOutpoint, u64> {
        let Some(_bond_params) = self.palw_bond_params_v2.as_ref() else {
            return Default::default();
        };
        // The same predicate as the lock beside it — see `palw_v2_locked_bond_outpoints`.
        // RFC-0015 §8.3.3 / §13.2 (2026-10-15): on a network with an OPV policy a bond's live duties (its OPV reservation to the
        // liability horizon, its budget window to `d + W` and liability hold) hold its collateral even below the audit fence.
        let duty_gate = self.palw_audit_2026_09_23_at(now_daa) || self.palw_kernel_opv_fence().is_some();
        state
            .bonds_iter()
            .filter(|(key, record)| !self.palw_v2_bond_is_locked(state, key, record, now_daa, duty_gate))
            .map(|(key, record)| (key.0, kaspa_consensus_core::palw_state_v2::palw_bond_burn_obligation_v2(record)))
            .filter(|(_, owed)| *owed > 0)
            .collect()
    }

    /// **The PALW payouts a block's coinbase mints: the first [`PALW_V2_MAX_PAYOUTS_PER_BLOCK`] rows
    /// of the SELECTED PARENT's `pending_payouts`, in key order — and nothing else, on every network**
    /// (ADR-0042 Decision 10; ADR-0152 V-4 "on moving a row", V-7; phase2-plan F1, §2.7).
    ///
    /// **Where the rows come from.** Below `Params::palw_rcore_plus` a claim's `Final` writes them:
    /// the producer's share under the raw `claim_id`, each credited seat's under its payee key
    /// (`0xFE`, accumulated), beside the market's fee legs and the carrier refunds (`0xFF`). **Past it
    /// (testnet-12) `Final` writes no row at all** — it names a vesting row (V-2), and step 3d of a
    /// LATER block's fold moves the row's legs into this queue once V-4 matures it: the producer's leg
    /// under its A-KEY key (`palw_vesting_payout_key_v1`, `0x00`), each seat's under the same `0xFE`
    /// payee key, the reserve into `panel_reserve_sompi` (never a row); a reporter reward the same way
    /// under `palw_reporter_payout_key_v1`. So the release path is `Final → row → 3d → queue → this`,
    /// and this function did not change for it: a moved leg is minted byte-identically on the build
    /// and the validate path because both render the one committed queue (T58's build == validate).
    ///
    /// **The queue lemma** (phase2-plan F2, ADR-0152 V-7, T58): past `palw_rcore_plus` step 3d is the
    /// ONLY writer of a key below the market's `0xFF` (the market and the refunds key under it; a
    /// Final writes a row); it creates at most [`palw_vesting_v1::PALW_V2_VESTING_LEGS_PER_BLOCK`]
    /// (= this width) new keys per block (the planner's one exception, a row wider than that, needs a
    /// panel of more than seven seats); and step 1b of the next block drains this very prefix. So the
    /// non-market part of every committed queue is at most this width, every non-market row is in the
    /// prefix rendered here, and **every leg is minted exactly one block after its move**.
    /// Carry-over lives in the vesting table, never in the queue; the `0x00` A-KEY prefix is what
    /// keeps a producer leg whose raw `claim_id` begins `0xFF` out of the market's tail (T47). Debug
    /// builds assert the lemma here, at the one site whose correctness rests on it.
    ///
    /// **Rules for any later change** (phase2-plan §2.7, F6): never render anything from `vesting`
    /// directly — a row is not money until 3d has moved it into this queue (V-6); and never mutate a
    /// template's coinbase outputs after it is built, because the EVM lane's commitment
    /// (`evm_template_fields`) is derived after the coinbase and never re-reads it — a future path
    /// that must do so re-derives through `evm_template_fields`, as `heartbeat_recommit_evm_for_stamp`
    /// does.
    ///
    /// [`PALW_V2_MAX_PAYOUTS_PER_BLOCK`]: kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK
    /// [`palw_vesting_v1::PALW_V2_VESTING_LEGS_PER_BLOCK`]: kaspa_consensus_core::palw_vesting_v1::PALW_V2_VESTING_LEGS_PER_BLOCK
    pub(super) fn palw_v2_payout_outputs(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
    ) -> Vec<TransactionOutput> {
        use kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK;
        // The queue lemma, where it holds: past `palw_rcore_plus` at the block that committed
        // `state`. Every testnet-12 panel has five seats, so a row costs at most six keys and the
        // planner's full-width head belt (a row wider than the budget) never fires; a preset that
        // armed R-core+ with a panel of more than seven seats would trip this, and should — its rows
        // would then wait in the queue, which is M-10's delay, not a mint of the wrong amount.
        debug_assert!(
            !state.last_point().is_some_and(|point| self.palw_rcore_plus_at(point.daa_score))
                || kaspa_consensus_core::palw_vesting_v1::palw_vesting_non_market_rows_waiting_v1(state)
                    <= PALW_V2_MAX_PAYOUTS_PER_BLOCK,
            "the queue lemma (ADR-0152 V-7, T58): past palw_rcore_plus the non-market part of a committed queue fits one drain"
        );
        // The SAME prefix the transition drains — see `PALW_V2_MAX_PAYOUTS_PER_BLOCK`. Both sides
        // read the selected parent's queue in `BTreeMap` key order, so "the first N" names one set
        // on every node. Paying more than the transition clears would pay a claim twice; clearing
        // more than the coinbase pays would destroy the reward the escrow exists to deliver. The
        // `take` IS the width bound: `PALW_V2_COINBASE_EXTRA_OUTPUTS` is defined from the same
        // constant, so the coinbase's output budget cannot drift from what is rendered here.
        state
            .pending_payouts_iter()
            .take(PALW_V2_MAX_PAYOUTS_PER_BLOCK)
            .map(|(_, payout)| {
                TransactionOutput::new(
                    payout.amount,
                    kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payout.payload.as_bytes()),
                )
            })
            .collect()
    }

    /// The fold's two answers on a licence object that the V1 and coverage assemblers read
    /// (ADR-0152 SR-6, Phase 2 P2-5) — its backed subset (`palw_v2_licence_backed_seats_v1`) and
    /// whether it licenses at all ([`Self::palw_v2_offered_licence_licenses_v1`]) — at this
    /// processor's fences and extras, reachable from the sibling test module.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_licence_fold_answers_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> (Option<Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>>, bool) {
        let backed = kaspa_consensus_core::palw_state_v2::palw_v2_licence_backed_seats_v1(
            state,
            state_params,
            point,
            object,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for(point),
        );
        (backed, self.palw_v2_offered_licence_licenses_v1(state, state_params, point, object))
    }

    /// [`Self::palw_v2_derived_panel_bindings`] — the chain's own derivation of the bindings `block`
    /// owes on the parent `state` — reachable from the sibling test module, so ADR-0152 T89 can hold
    /// build (this), accept (`palw_v2_accepted_objects_for_tests`) and fold (the stored panel) to one
    /// answer on a real chain.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_derived_panel_bindings_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
        block_daa: u64,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.palw_v2_derived_panel_bindings(state, block, block_daa)
    }

    /// The fold the pipeline runs over a candidate's accepted objects — no attempt, no merged work —
    /// at this processor's own fences and extras, reachable from the sibling test module. The walk's
    /// contract is "what it returns, the transition applies", and past `palw_audit_2026_09_23` the
    /// transition holds a cap (`ClassRegistrationsPerBlockExceeded`) the per-object rehearsal cannot
    /// see, so only a test that folds the walk's output can hold the two together (review of #12).
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_fold_accepted_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
    ) -> Result<kaspa_consensus_core::palw_state_v2::PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
            state,
            state_params,
            self.palw_admission_params_v2.as_ref(),
            point,
            objects,
            kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::None,
            &[],
            kaspa_hashes::Hash64::default(),
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            // The extras the BLOCK fold builds for its objects (the per-model court windows included): under an armed
            // `palw_model_court_window` a registration the node accepts needs the window the node derives for it.
            &self.palw_transition_extras_for_objects(state, point, objects),
        )
        .map(|(next, _, _)| next)
    }

    /// [`Self::palw_v2_fold_accepted_for_tests`] with the delta the transition wrote beside the
    /// state — the record a reorg reverts through `revert_delta_v2`. ADR-0152 v2 F2's suite folds a
    /// conviction through the fold the pipeline runs and then unwinds it, which only the delta can
    /// do: a test that compared roots alone could not tell a reversible write from a lost one.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_fold_accepted_with_delta_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
    ) -> Result<
        (kaspa_consensus_core::palw_state_v2::PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2),
        kaspa_consensus_core::palw_state_v2::PalwStateV2Error,
    > {
        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
            state,
            state_params,
            self.palw_admission_params_v2.as_ref(),
            point,
            objects,
            kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::None,
            &[],
            kaspa_hashes::Hash64::default(),
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for(point),
        )
        .map(|(next, delta, _)| (next, delta))
    }

    /// **A block's OWN attempt, folded as the pipeline folds it** — [`Self::palw_v2_fold_accepted_for_tests`]
    /// with `PalwBlockWorkV3::Attempt(envelope)` and the execution key the pipeline derives from the
    /// carrying header (`palw_execution_key_v1`: `execution_commitment_v3` under the header's own
    /// `execution_anchor_v3`), so a claim a test opens is keyed, deduplicated, reserved and escrowed
    /// exactly as a mined block's own attempt. ADR-0152 v2 F2's suite opens its real producer-built
    /// claims through it. No merged work, no EVM actions and no round permits: a test block carries
    /// none. Returns the delta and the skip list beside the state.
    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_fold_attempt_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
        envelope: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2,
        header: &kaspa_consensus_core::header::Header,
    ) -> Result<
        (
            kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
            kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2,
            Vec<(BlockHash, String)>,
        ),
        kaspa_consensus_core::palw_state_v2::PalwStateV2Error,
    > {
        let (execution_key, job_anchor) = self.palw_attempt_keys_v1(Some(header), &envelope.attempt);
        let mut extras = self.palw_transition_extras_for_objects(state, point, objects).into_legacy_extras();
        extras.own_job_anchor = job_anchor;
        if let Ok(bytes) = borsh::to_vec(header) {
            extras.exec_v2_origin_headers.insert(point.block, bytes);
        }
        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
            state,
            state_params,
            self.palw_admission_params_v2.as_ref(),
            point,
            objects,
            kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::Attempt(envelope),
            &[],
            execution_key,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &extras,
        )
    }

    /// **A block that carries no work of its own and merges one blue's attempt** — the merged work
    /// built by the pipeline's own [`Self::palw_merged_attempt_work_v1`] from the blue's header (or
    /// with none, as when the store cannot serve it), folded at this processor's fences. ADR-0152
    /// v3.1 T-THREAD pins what a merged claim records through it.
    #[cfg(test)]
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_fold_merged_attempt_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        blue: BlockHash,
        blue_header: Option<&kaspa_consensus_core::header::Header>,
        envelope: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2,
        subsidy: u64,
    ) -> Result<
        (
            kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
            kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2,
            Vec<(BlockHash, String)>,
        ),
        kaspa_consensus_core::palw_state_v2::PalwStateV2Error,
    > {
        let merged = [self.palw_merged_attempt_work_v1(blue, blue_header, envelope, subsidy, None, 0)];
        let mut extras = self.palw_transition_extras_for(point).into_legacy_extras();
        if let Some(header) = blue_header
            && let Ok(bytes) = borsh::to_vec(header)
        {
            extras.exec_v2_origin_headers.insert(blue, bytes);
        }
        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
            state,
            state_params,
            self.palw_admission_params_v2.as_ref(),
            point,
            &[],
            kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::None,
            &merged,
            kaspa_hashes::Hash64::default(),
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &extras,
        )
    }

    /// **[`Self::palw_v2_objects_of_block`]'s free-prompt walk over transactions a test hands it**,
    /// at `block_daa` against `state` — the extraction a node runs on an accepted 0x4a carrier,
    /// argument for argument (the bundle's ladder, each class's caps off `state`, the four fences
    /// at the block's DAA, the network's form and this processor's verifier). The pipeline reads
    /// the carriers out of acceptance data, which only a mined, funded carrier produces; this lets
    /// ADR-0152 v2 F2's suite open a real free-prompt claim the way the walk opens it without one.
    /// A change to the walk's arguments must be made here too.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_fp_objects_of_txs_for_tests(
        &self,
        txs: &[kaspa_consensus_core::tx::Transaction],
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block_daa: u64,
    ) -> kaspa_consensus_core::palw_fp_objects_v3::PalwFpExtractionV3 {
        let freeprompt = self.palw_freeprompt_params_v3.as_ref().expect("a network with a free-prompt lane");
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let ladder = self
            .palw_v2_bundle
            .as_ref()
            .map(|bundle| bundle.court.max_step_leaf_count())
            .unwrap_or(kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
        kaspa_consensus_core::palw_fp_objects_v3::palw_fp_objects_from_accepted_txs_by_class_v1(
            txs,
            network_domain,
            freeprompt,
            kaspa_consensus_core::BlockHash::default(),
            self.palw_panel_da_at(block_daa),
            |class_id| kaspa_consensus_core::palw_fp_objects_v3::PalwFpClassCapsV1 {
                step_ladder: state.class_step_ladder_v1(class_id, ladder),
                held: state.class_is_held_v1(class_id),
                derived_work: if self.palw_fp_derived_work_at(block_daa) {
                    match state.fp_work_profile_of(class_id) {
                        Some(profile) => kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Derived(profile),
                        // RFC-0002 Phase H: past the TIR public frame's fence an IR class's graph is its admitted program
                        // under its published layout; below it (or without the layout) it stays unpublished, as before.
                        None => match (
                            self.palw_tir_public_conviction_v1.is_some_and(|fence| fence.is_active(block_daa)),
                            state.tir_class_v1(class_id),
                            kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, *class_id),
                        ) {
                            (true, Some(record), Some(layout)) => {
                                kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Tir(record, layout)
                            }
                            _ => kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Unpublished,
                        },
                    }
                } else {
                    kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Declared
                },
                // RFC-0001 §A.3 / RFC-0003 §I.3: whether the class commits its logits in Q24.
                logits_q24: state.class_commits_q24_logits_v1(class_id),
                // RFC-0001 §2.6 stage 2 and §2.9, at the ACCEPTING block's DAA.
                prefix_state_armed: self.palw_fp_prefix_state.is_some_and(|fence| fence.is_active(block_daa)),
                // RFC-0001 §2.6 stage 2b, at the ACCEPTING block's DAA.
                prefix_inherit_armed: self.palw_fp_prefix_inherit.is_some_and(|fence| fence.is_active(block_daa)),
                // RFC-0001 stage 2b: whether the class's leaves are inheritance-safe (refused by name otherwise).
                prefix_inherit_class_safe: state.class_prefix_inherit_safe_v1(class_id).is_ok(),
                // ADR-0096 Decision 8 and RFC-0001 §2.5, at the ACCEPTING block's DAA.
                constraint_armed: self.palw_fp_decode_constraint.is_some_and(|fence| fence.is_active(block_daa)),
                constraint_v2_armed: self.palw_fp_constraint_v2.is_some_and(|fence| fence.is_active(block_daa)),
                tokenizer: kaspa_consensus_core::palw_fp_tokenizer_v1::PalwFpTokenizerRuleV1::of(
                    self.palw_fp_tokenizer_match.is_some_and(|fence| fence.is_active(block_daa)),
                    state.class_tokenizer_listing_v1(class_id),
                ),
            },
            self.palw_fp_ruleset_caps.is_some_and(|fence| fence.is_active(block_daa)),
            self.palw_held_context_at(block_daa),
            self.palw_prompt_ids_form_at(block_daa),
            self.palw_fp_decode_rules_at(block_daa),
            Self::verify_mldsa87_with_context_bool,
        )
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
    pub(in crate::pipeline::virtual_processor) fn palw_v2_block_fold_for_tests(
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

    /// [`Self::palw_v2_block_fold_for_tests`] carrying the refunds the filter returned, the way the
    /// chain walk hands them to the transition (the 2026-09-23 Position route matrix, P-B1).
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_block_fold_with_refunds_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
        refunds: Vec<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1>,
    ) -> Result<kaspa_consensus_core::palw_state_v2::PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
        let mut extras = self.palw_transition_extras_for(point).into_legacy_extras();
        extras.carrier_market_refunds = refunds;
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
            &extras,
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
    pub(super) fn palw_v2_graded_vector_count(
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        tir_active: bool,
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
                    // RFC-0002 Phase F: an IR drill below `palw_tir_v1` does not decode on an older
                    // build, so it is not graded (nor charged) here either.
                    Ok(Obj::FamilyCertified { evidence }) if tir_active || !evidence.is_tir_v1() => Some(evidence.vector_count()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// P-B1's bookkeeping for [`Self::palw_v2_accepted_objects_and_refunds`]: the previous object's
    /// refund is owed iff `accepted` did not grow while that object was being judged — AND the
    /// queue has a row for it (refunds count against the cap): `queue_held` is the rows the queue
    /// will hold before step 3′ as counted so far (the folded state plus any older promise), and
    /// `reserved` the refund rows already owed. An owed refund reserves its row. A refusal that
    /// cannot be paid back — no room, or a carrier with no P2PKH-ML-DSA-87 output — is logged as the
    /// burn it is.
    pub(super) fn palw_v2_settle_carrier_refund(
        refund_owed: &mut Option<PalwPendingCarrierRefundV1>,
        accepted_now: usize,
        queue_held: usize,
        reserved: &mut usize,
        refunds: &mut Vec<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1>,
        block: BlockHash,
    ) {
        let Some(pending) = refund_owed.take() else { return };
        if accepted_now != pending.accepted_before {
            return;
        }
        let cap = kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PENDING_PAYOUTS;
        match (pending.refund, pending.unrefundable) {
            (Some(refund), _) => {
                let held = queue_held.saturating_add(*reserved);
                if held < cap {
                    info!(
                        "Block {block}: the refused market move of carrier {} on line {} is paid back — {} sompi to {} (P-B1)",
                        refund.carrier, refund.line_id, refund.amount, refund.payee
                    );
                    *reserved += 1;
                    refunds.push(refund);
                } else {
                    warn!(
                        "Block {block}: the refused market move of carrier {} on line {} CANNOT be paid back: the payout queue \
                         holds {held} of {cap} rows with this block's refunds (PALW_V2_MAX_PENDING_PAYOUTS; refunds count \
                         against it), so its {} sompi stay in the sink. A node refuses such a carrier at its mempool and \
                         template; this one was mined anyway (P-B1)",
                        refund.carrier, refund.line_id, refund.amount
                    );
                }
            }
            (None, Some(carrier)) => warn!(
                "Block {block}: the refused market move of carrier {carrier} CANNOT be paid back: the carrier pays no \
                 P2PKH-ML-DSA-87 output to refund to, so its MSK stays in the sink (P-B1)"
            ),
            (None, None) => {}
        }
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
        let (tip_block, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
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
        // RFC-0007's path rule: a claim that licenses by tally takes no receipt-path object, so none is offered for it.
        if kaspa_consensus_core::palw_vertex_v1::palw_vertex_claim_licenses_by_tally_v1(&state, state_params, point.daa_score, &claim)
        {
            return None;
        }
        let record = state.claim(&claim)?;
        if !matches!(record.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
            return None;
        }
        state.panel_duties_of(&claim)?;
        // The door's own predicate: a seat not yet credited, or (ADR-0152 V3S-01, on a licence that
        // awaits its replay) a credited seat not yet counted over the whole job.
        let receipts: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2> = mine
            .iter()
            .filter(|receipt| {
                receipt.claim == claim
                    && matches!(receipt.verdict, kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2::Valid)
                    && kaspa_consensus_core::palw_state_v2::palw_rcore_v2_door_takes_seat_v1(
                        &state,
                        &claim,
                        record,
                        &receipt.seat_bond,
                    )
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

    /// **ADR-0152 SR-10 / Q-7 / V3S-01: the supplementary set a collector offers**, on the tip state at
    /// virtual's point — where the carrying transaction would be accepted, as every assembler here
    /// evaluates ([`Self::palw_v2_supplementary_assemble_on_v1`]).
    pub fn palw_v2_supplementary_assemble_impl(
        &self,
        claim: kaspa_hashes::Hash64,
        v3_candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3],
        v2_candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwSupplementaryOfferV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        self.palw_v2_supplementary_assemble_on_v1(&state, state_params, &point, claim, v3_candidates, v2_candidates)
    }

    /// [`Self::palw_v2_supplementary_assemble_impl`] on a given state and point (the tests fold their
    /// own). The selection is `palw_select_supplementary_offer_v1` — SR-10's V3 set, or on a licence
    /// awaiting its replay the V2 door's full-replay upgrade before any pay set — and every one of its
    /// judges is the chain's: `validate_supplementary_receipts_v3` and `validate_supplementary_receipts_v1`
    /// bound exactly as the acceptance arms bind them (the chain's network domain, the ML-DSA-87
    /// verify), and the fold's own answer (`palw_v2_supplementary_effect_v1`, at this processor's
    /// fences and extras). So the object it returns is the one the gate admits and the door credits,
    /// every seat of it — the property the collector exists for.
    pub(crate) fn palw_v2_supplementary_assemble_on_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        claim: kaspa_hashes::Hash64,
        v3_candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3],
        v2_candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwSupplementaryOfferV1> {
        use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
        // RFC-0007's path rule: a claim that licenses by tally takes no receipt-path object, so none is offered for it.
        if kaspa_consensus_core::palw_vertex_v1::palw_vertex_claim_licenses_by_tally_v1(state, state_params, point.daa_score, &claim) {
            return None;
        }
        if !state_params.rcore_plus_active_at(point.daa_score)
            || !state.claim(&claim).is_some_and(|record| matches!(record.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }))
        {
            return None;
        }
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let extras = self.palw_transition_extras_for(point);
        let validate_v3 = |receipts: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3]| {
            kaspa_consensus_core::palw_panel_v2::validate_supplementary_receipts_v3(
                state,
                state_params,
                point,
                network_domain,
                &claim,
                receipts,
                Self::verify_mldsa87_with_context_bool,
            )
        };
        let validate_v2 = |receipts: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2]| {
            kaspa_consensus_core::palw_panel_v2::validate_supplementary_receipts_v1(
                state,
                state_params,
                point,
                network_domain,
                &claim,
                receipts,
                Self::verify_mldsa87_with_context_bool,
            )
        };
        let effect = |object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2| {
            kaspa_consensus_core::palw_state_v2::palw_v2_supplementary_effect_v1(
                state,
                state_params,
                point,
                object,
                self.palw_unavailable_abstains_at(point.daa_score),
                self.palw_capability_bound_at(point.daa_score),
                self.palw_uncertified_weightless_at(point.daa_score),
                self.palw_da_court_at(point.daa_score),
                &extras,
            )
        };
        kaspa_consensus_core::palw_panel_v2::palw_select_supplementary_offer_v1(
            state,
            &claim,
            v3_candidates,
            v2_candidates,
            validate_v3,
            validate_v2,
            effect,
        )
    }

    pub fn palw_v2_receipt_quorum_assemble_impl(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let panel_params = self.palw_panel_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
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
        self.palw_v2_receipt_quorum_assemble_on_v1(&state, state_params, panel_params, &point, claim, candidates)
    }

    /// [`Self::palw_v2_receipt_quorum_assemble_impl`] on a given state and point — the tests fold their
    /// own (ADR-0152 X22: the V1 door a node offers before S2).
    pub(crate) fn palw_v2_receipt_quorum_assemble_on_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2 as Q;
        // RFC-0007's path rule: a claim that licenses by tally takes no receipt-path object, so none is offered for it.
        if kaspa_consensus_core::palw_vertex_v1::palw_vertex_claim_licenses_by_tally_v1(state, state_params, point.daa_score, &claim) {
            return None;
        }
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
                state,
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
                        state,
                        panel_params,
                        state_params,
                        point,
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

        // The acceptance layer's V1 check, bound as the gate binds it: the greedy loop's judge and,
        // past `palw_rcore_plus`, the backed subset's (below).
        let quorum = |receipts: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2]| {
            kaspa_consensus_core::palw_panel_v2::validate_receipt_quorum_v2_with_policy(
                state,
                panel_params,
                state_params,
                point,
                network_domain,
                &claim,
                receipts,
                verify,
                self.palw_unavailable_abstains_at(point.daa_score),
                // ADR-0147: the same height the fold and the acceptance layer read.
                self.palw_admission_independence_daa(),
            )
        };
        let mut kept: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2> = Vec::new();
        let mut verdict: Option<Q> = None;
        for candidate in candidates {
            let mut attempt = kept.clone();
            attempt.push(candidate.clone());
            match quorum(&attempt) {
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
            // ADR-0152 SR-6 (Phase 2 P2-5): past `palw_rcore_plus` the loop's set — every clean
            // candidate, which is where a backed subset is found if any part of the pool has one
            // (`palw_v2_offered_backed_subset_v1`) — is offered as the fold's backed subset, or not
            // at all. Not the `ProducerDefaulted` arm below, nor the shard parts above.
            Q::Licensed { .. } => self.palw_v2_offered_backed_subset_v1(
                state,
                state_params,
                point,
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: kept },
                |backed| {
                    matches!(
                        backed,
                        kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensed { receipts, .. }
                            if matches!(quorum(receipts), Ok(Q::Licensed { .. }))
                    )
                },
            ),
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
    ///
    /// The set is chosen by `palw_select_coverage_licence_v2` (the 2026-09-24 licence-stall fix),
    /// which asks `validate_receipt_coverage_v2` — bound here exactly as the acceptance arm binds it —
    /// of every set it tries. The greedy loop this replaced dropped every receipt whose addition came
    /// back `CoverageShort`, so its pool stopped at two `Valid`s and this door never opened.
    pub fn palw_v2_receipt_coverage_assemble_impl(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        if !self.palw_verification_v2_at(self.lkg_virtual_state.load().daa_score) {
            return None;
        }
        let state_params = self.palw_state_params_v2.as_ref()?;
        let panel_params = self.palw_panel_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        self.palw_v2_receipt_coverage_assemble_on_v1(&state, state_params, panel_params, &point, claim, candidates)
    }

    /// **ADR-0160 F-B: a collector's batch licence** (node policy): the due claims of `due`, in the
    /// order given (oldest bind first), licensed together over the pooled seats' `windows` —
    /// `palw_assemble_batch_licence_v1` on the tip, each window's root verified here first (a window
    /// that does not verify is left out), each entry kept only where acceptance would take it and the
    /// fold of the equivalent single licence licenses its claim, the object within `max_bytes`. `None`
    /// below `palw_capacity_batch_licence` (at the virtual's DAA) or when nothing is kept.
    pub fn palw_v2_batch_licence_assemble_impl(
        &self,
        windows: &[kaspa_consensus_core::palw_batch_licence_v1::PalwSeatWindowV1],
        due: &[kaspa_hashes::Hash64],
        max_bytes: usize,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let panel_params = self.palw_panel_params_v2.as_ref()?;
        let virtual_state = self.lkg_virtual_state.load();
        if !state_params.capacity_batch_active_at(virtual_state.daa_score) || !self.palw_verification_v2_at(virtual_state.daa_score) {
            return None;
        }
        let (tip_block, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        self.palw_v2_batch_licence_assemble_on_v1(&state, state_params, panel_params, &point, windows, due, max_bytes)
    }

    /// [`Self::palw_v2_batch_licence_assemble_impl`] on a given state and point (the tests fold their
    /// own). The caller has checked the fences.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn palw_v2_batch_licence_assemble_on_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        windows: &[kaspa_consensus_core::palw_batch_licence_v1::PalwSeatWindowV1],
        due: &[kaspa_hashes::Hash64],
        max_bytes: usize,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_batch_licence_v1 as batch;
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let verified: Vec<batch::PalwSeatWindowV1> = windows
            .iter()
            .filter(|window| {
                batch::palw_batch_licence_verify_roots_v1(
                    state,
                    network_domain,
                    std::slice::from_ref(&window.root),
                    Self::verify_mldsa87_with_context_bool,
                )
                .is_ok()
            })
            .cloned()
            .collect();
        let (roots, entries) = batch::palw_assemble_batch_licence_v1(
            state,
            panel_params,
            state_params,
            point,
            network_domain,
            &verified,
            due,
            max_bytes,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_admission_independence_daa(),
            |claim, receipts| {
                self.palw_v2_offered_licence_licenses_v1(
                    state,
                    state_params,
                    point,
                    &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensedV2 {
                        claim: *claim,
                        receipts: receipts.to_vec(),
                    },
                )
            },
        )?;
        Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensedBatchV1 { roots, entries })
    }

    /// [`Self::palw_v2_receipt_coverage_assemble_impl`] on a given state and point — the tests fold
    /// their own (ADR-0152 X22). The caller has checked the Verification V2 fence.
    pub(crate) fn palw_v2_receipt_coverage_assemble_on_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        // RFC-0007's path rule: a claim that licenses by tally takes no receipt-path object, so none is offered for it.
        if kaspa_consensus_core::palw_vertex_v1::palw_vertex_claim_licenses_by_tally_v1(state, state_params, point.daa_score, &claim) {
            return None;
        }
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let verify = |key: &[u8], message: &[u8], sig: &[u8], context: &[u8]| {
            Self::verify_mldsa87_with_context_bool(key, message, sig, context)
        };
        let coverage = |receipts: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3]| {
            kaspa_consensus_core::palw_panel_v2::validate_receipt_coverage_v2(
                state,
                panel_params,
                state_params,
                point,
                network_domain,
                &claim,
                receipts,
                verify,
                self.palw_unavailable_abstains_at(point.daa_score),
                self.palw_admission_independence_daa(),
            )
        };
        let receipts = kaspa_consensus_core::palw_panel_v2::palw_select_coverage_licence_v2(candidates, coverage, |receipts| {
            self.palw_v2_offered_licence_licenses_v1(
                state,
                state_params,
                point,
                &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensedV2 { claim, receipts: receipts.to_vec() },
            )
        })?;
        // ADR-0152 SR-6 (Phase 2 P2-5): past `palw_rcore_plus`, the fold's backed subset of the set
        // the selection found (its largest sound set, so a backed subset is found if one exists).
        // On testnet-12's cut every segment has ONE partial holder beside the full seat, so a
        // coverage set needs every one of its `Valid`s: its backed subset is the whole set or
        // nothing, and the fold's `licenses` above already refused the nothing — here the rule is
        // the V1 door's, kept in one place for a cut where a seat could be spared.
        self.palw_v2_offered_backed_subset_v1(
            state,
            state_params,
            point,
            kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensedV2 { claim, receipts },
            |backed| {
                matches!(
                    backed,
                    kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensedV2 { receipts, .. }
                        if matches!(coverage(receipts), Ok(kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::Licensed { .. }))
                )
            },
        )
    }

    /// **ADR-0152 SR-6 at the V1 and coverage doors (Phase 2 P2-5): offer the fold's backed subset, or
    /// nothing.** Past `Params::palw_rcore_plus` a licence set carrying a `Valid` whose seat cannot post
    /// `lock_{max(k,2)}` licenses on its backed subset when that subset still meets the door's rule,
    /// and is INERT otherwise (`license_rcore_v1`). The two assemblers that keep every clean
    /// candidate would offer such a set whole: an inert one — and X22 asks V1 BEFORE S2
    /// (`palw_licence_offer_order_v1`), so kaspad would resubmit it every replan and never reach the
    /// fast path until the receipt window voided the claim — or a licensing one carrying a `Valid`
    /// the fold neither locks nor credits.
    ///
    /// The fold is asked which carried `Valid`s it licenses on
    /// (`palw_v2_licence_backed_seats_v1`: read off the locks the fold writes, so SR-6's price
    /// arithmetic is not restated here); `None` when it licenses nothing. Those it does not back
    /// are dropped, and a set that changed is put to the door (`door_takes`, the acceptance check
    /// bound as the gate binds it) and to the fold once more. **The whole clean set is the right set to
    /// ask**: backing is monotone in the set — more `Valid`s recount no lower, so the price each is
    /// tested at is no higher (SR-6's fall-once re-test included) — so a subset never backs a seat
    /// the whole set does not, and when the whole set licenses nothing no part of it does. The
    /// backed subset re-tests to itself (same recount, same price), so the second fold agrees.
    ///
    /// **The re-check is kept in release builds, by choice (the P2-5 review's LOW 3).** It is
    /// redundant by the argument above — the review's exhaustive probe over every sub-pool found it
    /// never changes the answer — but the door is a separate predicate from the fold (its own verdict
    /// precedence over what remains: the `Unavailable` obligations, the outsider, the window), and a
    /// carrier the door refuses is not an inert licence but a mempool refusal, on which kaspad's
    /// licence collector drops its funding chain for the rest of the tick. So what is offered is what
    /// the door and the fold were both shown, not what a monotonicity proof says they would say. Its
    /// price — one acceptance check (the ML-DSA verifications) and one more fold (a parent-state
    /// clone) — is paid only when a `Valid` was dropped, and such a set is offered, submitted and
    /// throttled for `COURT_MOVE_REPLAN_DAA`, so it does not recur each tick. What does recur is the
    /// first fold, on a pool that licenses nothing (re-assembled every tick; only submitted claims
    /// are throttled): that is SR-6's own question, the price the coverage and S2 doors already pay
    /// through `palw_v2_offered_licence_licenses_v1`, once per `palw_v1_offer_v1` prefix that reaches
    /// the acceptance quorum.
    ///
    /// Below the fence `object` is returned as it came (byte-identical to the assemblers before
    /// P2-5): a licence there is the whole set's or nothing, and each caller keeps the rule it had.
    pub(super) fn palw_v2_offered_backed_subset_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        door_takes: impl FnOnce(&kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2) -> bool,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2::Valid;
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        if !state_params.rcore_plus_active_at(point.daa_score) {
            return Some(object);
        }
        let backed = kaspa_consensus_core::palw_state_v2::palw_v2_licence_backed_seats_v1(
            state,
            state_params,
            point,
            &object,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for(point),
        )?;
        let keeps = |receipt: &kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2| {
            !matches!(receipt.verdict, Valid) || backed.contains(&receipt.seat_bond)
        };
        let subset = match &object {
            Obj::ReceiptLicensed { claim, receipts } => {
                Obj::ReceiptLicensed { claim: *claim, receipts: receipts.iter().filter(|receipt| keeps(receipt)).cloned().collect() }
            }
            Obj::ReceiptLicensedV2 { claim, receipts } => Obj::ReceiptLicensedV2 {
                claim: *claim,
                receipts: receipts.iter().filter(|signed| keeps(&signed.receipt)).cloned().collect(),
            },
            // Only the two doors that keep every clean candidate come here.
            _ => return None,
        };
        if subset == object {
            return Some(object);
        }
        (door_takes(&subset) && self.palw_v2_offered_licence_licenses_v1(state, state_params, point, &subset)).then_some(subset)
    }

    /// **RFC-0006: the next ready licensing part of a claim drawn per layer shard**, from the cell-masked receipts a node pooled,
    /// at virtual's point. Shards are tried lowest first; each is built greedily — a candidate joins the part if the set stays
    /// sound (`validate_tir_shard_part_v1`), and the part is returned the moment the acceptance validator itself says it
    /// licenses its shard. The set the node offers is the set a block takes.
    pub fn palw_v2_tir_shard_part_assemble_impl(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_tir_shard_v1::PalwSeatReceiptV4],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (tip_block, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        self.palw_v2_tir_shard_part_assemble_on_v1(&state, state_params, &point, claim, candidates)
    }

    /// [`Self::palw_v2_tir_shard_part_assemble_impl`] on a given state and point (the tests fold their own).
    pub(crate) fn palw_v2_tir_shard_part_assemble_on_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_tir_shard_v1::PalwSeatReceiptV4],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_panel_v2::PalwPanelV2Error as E;
        use kaspa_consensus_core::palw_tir_shard_v1::PalwTirShardPartV1;
        if !state_params.tir_shard_active_at(point.daa_score) {
            return None;
        }
        let record = state.tir_shard_claim(&claim)?;
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        for shard in 0..record.s_l {
            if record.progress.is_licensed(u32::from(shard)) {
                continue;
            }
            let mut kept: Vec<kaspa_consensus_core::palw_tir_shard_v1::PalwSeatReceiptV4> = Vec::new();
            for candidate in candidates.iter().filter(|c| c.shard == shard) {
                if kept.iter().any(|k| k.receipt.seat_bond == candidate.receipt.seat_bond) {
                    continue;
                }
                let mut attempt = kept.clone();
                attempt.push(candidate.clone());
                let part = PalwTirShardPartV1 { claim, shard, receipts: attempt.clone() };
                match kaspa_consensus_core::palw_panel_v2::validate_tir_shard_part_v1(
                    state,
                    state_params,
                    point,
                    network_domain,
                    &part,
                    Self::verify_mldsa87_with_context_bool,
                ) {
                    Ok(_) => {
                        return Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::TirShardReceiptLicensed { part });
                    }
                    // A sound set that is not a licence yet: keep the receipt, keep collecting.
                    Err(E::TirShardPartShort(_)) => kept = attempt,
                    // A receipt that poisons any set it is in is dropped.
                    Err(_) => {}
                }
            }
        }
        None
    }

    /// ADR-0133 S2: assemble an `OptimisticLicensed` from V3 receipts when the full-replay seat's
    /// `Valid` is present, past the fence.
    ///
    /// The set is chosen by `palw_select_optimistic_licence_v2` (the 2026-09-24 licence-stall fix):
    /// the full seat's `Valid` first wherever it arrived, then only the riders the door still takes
    /// (`palw_optimistic_licence_admits_v2`, the predicate the acceptance arm calls) and the fold
    /// licenses, or all five when all five validated. The greedy loop this replaced added receipts in arrival order and
    /// dropped the full seat's `Valid` whenever two `Valid`s had arrived before it.
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
        let (tip_block, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let virtual_state = self.lkg_virtual_state.load();
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: tip_block,
            daa_score: virtual_state.daa_score,
            blue_score: virtual_state.ghostdag_data.blue_score,
            subsidy: 0,
        };
        self.palw_v2_optimistic_assemble_on_v1(&state, state_params, panel_params, &point, claim, candidates)
    }

    /// [`Self::palw_v2_optimistic_assemble_impl`] on a given state and point — the tests fold their
    /// own (ADR-0152 X22). The caller has checked the S2 fence.
    pub(crate) fn palw_v2_optimistic_assemble_on_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        claim: kaspa_hashes::Hash64,
        candidates: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        // RFC-0007's path rule: a claim that licenses by tally takes no receipt-path object, so none is offered for it.
        if kaspa_consensus_core::palw_vertex_v1::palw_vertex_claim_licenses_by_tally_v1(state, state_params, point.daa_score, &claim) {
            return None;
        }
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let verify = |key: &[u8], message: &[u8], sig: &[u8], context: &[u8]| {
            Self::verify_mldsa87_with_context_bool(key, message, sig, context)
        };
        let coverage = |receipts: &[kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3]| {
            kaspa_consensus_core::palw_panel_v2::validate_receipt_coverage_v2(
                state,
                panel_params,
                state_params,
                point,
                network_domain,
                &claim,
                receipts,
                verify,
                self.palw_unavailable_abstains_at(point.daa_score),
                self.palw_admission_independence_daa(),
            )
        };
        let receipts = kaspa_consensus_core::palw_panel_v2::palw_select_optimistic_licence_v2(
            state,
            &claim,
            self.palw_admission_independence_daa(),
            candidates,
            coverage,
            |receipts| {
                self.palw_v2_offered_licence_licenses_v1(
                    state,
                    state_params,
                    point,
                    &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::OptimisticLicensed {
                        claim,
                        receipts: receipts.to_vec(),
                    },
                )
            },
        )?;
        Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::OptimisticLicensed { claim, receipts })
    }

    /// **2026-09-24 DoS audit review of #6: offer only a set the fold will license.** A licence set
    /// whose `Valid` signer cannot post its door's price licenses nothing: past the audit fence it
    /// is INERT — it folds, and the claim stays `PanelBound` (an S2 set whose full-replay seat
    /// cannot post the whole-gain price is the case the review found) — and below it the fold
    /// refuses it (`SeatValidLockRefused`) and the rehearsal drops the object. kaspad asked coverage,
    /// then optimistic (`.or_else` in `palw_panel.rs`), so a set the first door offered and the fold
    /// did not take kept the second from ever being tried: the node resubmitted it every replan
    /// until the receipt window voided a claim the other door would have licensed.
    ///
    /// On a Verification V2 chain those two were the only doors kaspad could form while its V1 pool
    /// held only the inner halves of its V3 receipts (signed over the V3 message, which the V1
    /// validators refuse). Past SEAT-R a replaying seat also files its whole-job V2 `Valid`, so the V1
    /// door forms too, and past `palw_rcore_plus` it is asked before S2 (X22); from Phase 2 (P2-5) it
    /// asks the fold as well, through [`Self::palw_v2_offered_backed_subset_v1`].
    ///
    /// The fold is asked directly (`palw_v2_object_licenses_claim_v1`), so this is the fold's
    /// predicate, not a second copy of it. It is asked on every network: it decides only which set
    /// this node offers, never what a block accepts, and it declines only a set the fold refuses or
    /// leaves inert on the state the node would carry it into — on either side of the fence.
    pub(super) fn palw_v2_offered_licence_licenses_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> bool {
        kaspa_consensus_core::palw_state_v2::palw_v2_object_licenses_claim_v1(
            state,
            state_params,
            point,
            object,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for(point),
        )
    }

    /// **The ladder this network froze, ADR-0084 U-08's one accessor.** `max_step_leaf_count` is a
    /// bundle field — inside `palw_ruleset_id_v2`, so it cannot move on a running chain — and it is
    /// DAA-free (`palw_court_params_at_v2` only ever changes the dissection arity). A node with no
    /// bundle has no ladder to apply and falls back to the executor's structural top, which is
    /// exactly as permissive as this path already was.
    pub(super) fn palw_max_step_leaf_count_v1(&self) -> u64 {
        self.palw_v2_bundle
            .as_ref()
            .map(|bundle| bundle.court.max_step_leaf_count())
            .unwrap_or(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES)
    }

    /// **Is this post-genesis `ClassRegistered` signed by the Active bond its carriage names?**
    /// (launch blockers §3; factored out for the 2026-09-24 DoS audit #12 (b)).
    ///
    /// The registrant must hold an ACTIVE bond and have signed the class it is registering together
    /// with the share it is taking, the four fields the signature used to leave open and the
    /// canonical job the pwu rule is derived from (audit M2-6). Asked by the `ClassRegistered` arm of
    /// [`Self::palw_v2_validate_objects`], and — past `palw_audit_2026_09_23` — by the acceptance walk
    /// BEFORE the graph gate runs, so a registration charges a slot of
    /// `PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1` only when its registrant really signed it: a
    /// stranger's unsigned carrier costs one signature check and no slot, and the gate's graph walk
    /// (`verify_class_admission_v9`, ~211 ms for a 1,024-layer held profile in a debug build) runs
    /// at most that many times a block. `Err` for any other object, which callers never pass.
    pub(super) fn palw_v2_class_registration_is_signed(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        // **RFC-0002 Phase F: an IR registration is signed over its own message**, which carries the
        // class (program, layout, tokenizer) in the profile's place, under its own context — so
        // neither form's signature can be lifted onto the other.
        if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredTirV1 {
            class_id,
            share_permille,
            admission,
            activation_daa,
            artifact_root,
            slash_value_per_pwu,
            initial_target,
            pwu_rule,
        } = object
        {
            let registrant = state
                .bond(&admission.registrant_bond)
                .ok_or_else(|| format!("IR class {class_id} is registered under a bond this chain does not have"))?;
            if !matches!(registrant.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                return Err(format!("IR class {class_id} is registered under a bond that is not Active"));
            }
            let message = kaspa_consensus_core::palw_tir_class_v1::palw_tir_class_registration_message_v1(
                kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                ),
                *class_id,
                *share_permille,
                *activation_daa,
                &admission.registrant_bond,
                *artifact_root,
                *slash_value_per_pwu,
                *initial_target,
                pwu_rule,
                &admission.canonical,
                &admission.class,
            );
            if !Self::verify_mldsa87_with_context_bool(
                &registrant.pubkey,
                message.as_byte_slice(),
                &admission.signature,
                kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
            ) {
                return Err(format!("IR class {class_id}'s registration is not signed by the bond it names"));
            }
            return Ok(());
        }
        // **RFC-0003: a generative registration is signed over its own message** (the pipeline class
        // in the profile's place, no canonical job), under its own context.
        if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredGenV1 {
            class_id,
            share_permille,
            admission,
            activation_daa,
            artifact_root,
            slash_value_per_pwu,
            initial_target,
            pwu_rule,
        } = object
        {
            let registrant = state
                .bond(&admission.registrant_bond)
                .ok_or_else(|| format!("generative class {class_id} is registered under a bond this chain does not have"))?;
            if !matches!(registrant.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                return Err(format!("generative class {class_id} is registered under a bond that is not Active"));
            }
            let message = kaspa_consensus_core::palw_gen_class_v1::palw_gen_class_registration_message_v1(
                self.palw_network_domain_v2(),
                *class_id,
                *share_permille,
                *activation_daa,
                &admission.registrant_bond,
                *artifact_root,
                *slash_value_per_pwu,
                *initial_target,
                pwu_rule,
                &admission.class,
            );
            if !Self::verify_mldsa87_with_context_bool(
                &registrant.pubkey,
                message.as_byte_slice(),
                &admission.signature,
                kaspa_consensus_core::palw_gen_class_v1::PALW_GEN_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
            ) {
                return Err(format!("generative class {class_id}'s registration is not signed by the bond it names"));
            }
            return Ok(());
        }
        let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
            class_id,
            share_permille,
            admission,
            activation_daa,
            artifact_root,
            slash_value_per_pwu,
            initial_target,
            pwu_rule,
        } = object
        else {
            return Err("not a class registration".to_string());
        };
        let Some(carriage) = admission.as_ref() else {
            return Err(format!("class {class_id} carries no registrant to have signed it"));
        };
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
            // The four fields the signature used to leave open, and the canonical job the pwu rule
            // is derived from (audit M2-6).
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
        Ok(())
    }

    /// **Does this `ClassRegistered` start at the chain's own target?** (audit M2-12; factored out
    /// in the review of the 2026-09-24 DoS audit #12.)
    ///
    /// `initial_target` seeds the class's own retarget, so a registrant naming a huge one mines its
    /// class for free until the first retarget catches up — and naming a tiny one makes the class
    /// unminable, which is a way to park a share nobody can use. The base class's live target is
    /// what a registration is offered (`palw_v2_registration_terms`), and it is what must arrive.
    /// Asked by the `ClassRegistered` arm of [`Self::palw_v2_validate_objects`], and — past
    /// `palw_audit_2026_09_23` — by the acceptance walk before a registration is charged a slot:
    /// it is one map read, and a replayed registration from before a retarget fails it, so such a
    /// copy is dropped without spending a slot the gate would then waste. `Ok` for any other object
    /// and on a network with no V2 bundle (the arm refuses those on its own terms).
    pub(super) fn palw_v2_class_registration_starts_at_the_chains_target(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        let (class_id, initial_target) = match object {
            kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, initial_target, .. }
            | kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, initial_target, .. }
            | kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredGenV1 { class_id, initial_target, .. } => {
                (class_id, initial_target)
            }
            _ => return Ok(()),
        };
        let Some(bundle) = self.palw_v2_bundle.as_ref() else { return Ok(()) };
        if let Some(base_target) = state.class_target(&bundle.base_class_id)
            && *initial_target != base_target.target
        {
            return Err(format!(
                "class {class_id} registers at target {initial_target}; a post-genesis entrant starts at the                              chain's own ({}) — difficulty is not a registrant's to choose",
                base_target.target
            ));
        }
        Ok(())
    }

    /// `verify_mldsa87_with_context` as a `bool`, which is the shape every PALW verifier callback
    /// takes — one place, so two call sites cannot disagree about what an error means.
    /// ADR-0088: the network domain every registry message is signed under — the same one a class
    /// registration is signed under.
    pub(in crate::pipeline::virtual_processor) fn palw_network_domain_v2(&self) -> kaspa_hashes::Hash64 {
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(self.network_id_bytes.as_slice(), Some(self.genesis.hash))
    }

    /// **ADR-0152 v3.1 J-5: what the identity checks read besides the evidence** — the network's
    /// prompt-id form and the base class — the same two values the fold reads from its extras and
    /// its bundle, so the gate and the fold judge one identity rule.
    /// **Lane LG14-A: `Params::palw_legacy_public_filer_v1` resolved at the block's DAA**, in exactly one place — read off the V2
    /// bundle's mirror, the same field the fold reads (`PalwStateParamsV2::legacy_public_filer_active_at`), so the gate and the fold
    /// cannot disagree about the fence. `false` on every shipped preset.
    pub(in crate::pipeline::virtual_processor) fn palw_legacy_public_filer_at(&self, daa_score: u64) -> bool {
        self.palw_v2_bundle.as_ref().is_some_and(|bundle| bundle.state.legacy_public_filer_active_at(daa_score))
    }

    /// **RFC-0014 §7.4: the session rule a direct proof is judged under at `daa_score`** (the fold's `session_rule_v1`).
    pub(super) fn palw_session_rule_at(&self, daa_score: u64) -> kaspa_consensus_core::palw_offence_attribution_v1::PalwSessionRuleV1 {
        kaspa_consensus_core::palw_offence_attribution_v1::PalwSessionRuleV1::at(self.palw_legacy_public_filer_at(daa_score))
    }

    /// **Lane LG14-A: a dispute object's acceptance** (tags 154–155): the fence, an Active signer bond, and the signer's ML-DSA-87
    /// signature over `(network, kind, signer, payload)`. The claim, the caps, the deposit and the hold are the fold's.
    pub(super) fn palw_legacy_dispute_signature_ok(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
        kind: u8,
        signer: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        payload: &[u8],
        signature: &[u8],
    ) -> Result<(), String> {
        if !self.palw_legacy_public_filer_at(daa_score) {
            return Err("a dispute object is refused: palw_legacy_public_filer_v1 is not in force at this block (LG14-A)".to_string());
        }
        let record = state.bond(signer).ok_or_else(|| "a dispute object is signed by a bond this chain does not have".to_string())?;
        if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err("a dispute object is signed by a bond that is not Active".to_string());
        }
        let message = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_legacy_dispute_message_v1(
            self.palw_network_domain_v2(),
            kind,
            signer,
            payload,
        );
        if !Self::verify_mldsa87_with_context_bool(
            &record.pubkey,
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1,
        ) {
            return Err("a dispute object carries a signature its bond's key does not verify".to_string());
        }
        Ok(())
    }

    pub(super) fn palw_identity_rules_v1(
        &self,
        daa_score: u64,
    ) -> kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 {
        kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 {
            prompt_ids_form: self.palw_prompt_ids_form_at(daa_score),
            base_class_id: self.palw_v2_bundle.as_ref().map(|bundle| bundle.base_class_id).unwrap_or_default(),
            // ADR-0152 N9 (M3): the fold's one predicate, read from the same bundle mirror.
            da_signer_liability: self.palw_v2_bundle.as_ref().is_some_and(|bundle| {
                kaspa_consensus_core::palw_state_v2::palw_da_signer_liability_armed_v1(
                    &bundle.state,
                    kaspa_consensus_core::palw_da_rcore_v1::PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1,
                    daa_score,
                )
            }),
        }
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
    pub(super) fn palw_execution_key_v1(
        &self,
        header: &kaspa_consensus_core::header::Header,
        attempt: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2,
    ) -> kaspa_hashes::Hash64 {
        kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(attempt, self.palw_execution_anchor_v1(header, attempt))
    }

    /// **ADR-0152 v3.1 J-1 (F1): the execution anchor of `attempt` as carried by `header`'s block** —
    /// `execution_anchor_v3(network domain, pre_pow, class, bond, nonce)`, the job the attempt had to
    /// answer (the floor's producer sets `ctx.job_id` to it). Split out of
    /// [`Self::palw_execution_key_v1`], which keys the dedup on the commitment under it; the claim
    /// records the anchor itself as its `job_identity`.
    pub(super) fn palw_execution_anchor_v1(
        &self,
        header: &kaspa_consensus_core::header::Header,
        attempt: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2,
    ) -> kaspa_hashes::Hash64 {
        kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(
            self.palw_network_domain_v2(),
            kaspa_consensus_core::hashing::header::pre_pow_hash_64(header),
            attempt.class_id,
            &attempt.executor_bond,
            header.nonce,
        )
    }

    /// **One block's attempt keys, from its OWN header** — `(execution key, job anchor)`: the key B-4
    /// dedups on and the anchor J-1 records. With no header at hand the key degrades to the attempt id
    /// (a unique value that never false-dedups) and the anchor to `default` — never the attempt id,
    /// which would be an identity the chain made up and a borrowed binding could be convicted
    /// against (SPEC §4.2).
    pub(super) fn palw_attempt_keys_v1(
        &self,
        header: Option<&kaspa_consensus_core::header::Header>,
        attempt: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2,
    ) -> (kaspa_hashes::Hash64, kaspa_hashes::Hash64) {
        match header {
            Some(header) => (self.palw_execution_key_v1(header, attempt), self.palw_execution_anchor_v1(header, attempt)),
            None => (kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(attempt), kaspa_hashes::Hash64::default()),
        }
    }

    /// **A merged attempt's work, as the pipeline carries it into the accepting block's fold** —
    /// the execution key and the job anchor from the merged block's OWN header
    /// ([`Self::palw_attempt_keys_v1`]), beside the subsidy, carve and bits that block recorded. One
    /// construction for the pipeline and for the test that pins what it records.
    pub(super) fn palw_merged_attempt_work_v1<'a>(
        &self,
        blue: BlockHash,
        header: Option<&kaspa_consensus_core::header::Header>,
        envelope: &'a kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2,
        subsidy: u64,
        escrow_carve: Option<kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2>,
        bits: u32,
    ) -> kaspa_consensus_core::palw_state_v2::PalwMergedWorkV1<'a> {
        let (execution_key, job_anchor) = self.palw_attempt_keys_v1(header, &envelope.attempt);
        kaspa_consensus_core::palw_state_v2::PalwMergedWorkV1 {
            carrying_block: blue,
            work: kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::Attempt(envelope),
            // B-4: the execution key from the merged blue's OWN header (pre_pow + nonce). A header
            // the store cannot serve degrades to the attempt id — a unique value that never
            // false-dedups — but a mergeset blue always has one.
            execution_key,
            // B-1: the merged block's OWN subsidy — the pool its escrow is carved from past the deep
            // fence, the SAME value `palw_v2_merged_escrow_withheld` hands the coinbase to withhold
            // (both read it from this one field).
            subsidy,
            // ADR-0126: the carve resolved at the merged block's DAA, read from the same record for
            // the same reason.
            escrow_carve,
            bits,
            // ADR-0152 v3.1 J-1: the job the merged attempt answers, from the blue's own header.
            job_anchor,
        }
    }

    /// ADR-0088: the bond a line's `role` ("owner" or "developer") names, read from the acceptance
    /// state — a founding line without a row answers from its class.
    pub(super) fn palw_model_line_role(
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

    /// RFC-0004: an improvement object's signature, checked against the stored key of the bond it is
    /// attributed to, under the object's own ML-DSA-87 context.
    /// **RFC-0004 A4: a bond-signed material object's signature** — the fence, then the signing bond's
    /// key over `palw_improve_material_message_v1(tag, network, payload, bond)` under the material
    /// context.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn palw_improvement_material_is_signed<T: borsh::BorshSerialize>(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        tag: u8,
        payload: &T,
        bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        signature: &[u8],
        what: &str,
    ) -> Result<(), String> {
        if !self.palw_improvement_at(point.daa_score) {
            return Err(format!("{what} is refused: palw_improvement_v1 is not in force at this block (RFC-0004)"));
        }
        let message = kaspa_consensus_core::palw_improve_material_v1::palw_improve_material_message_v1(
            tag,
            &self.palw_network_domain_v2(),
            payload,
            Some(bond),
        );
        self.palw_improvement_check_bond_signature(
            state,
            bond,
            &message,
            signature,
            kaspa_consensus_core::palw_improve_material_v1::PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1,
            what,
        )
    }

    /// **RFC-0004 A5: a candidate's signature** by its submitter bond, over
    /// `palw_candidate_submission_message_v1` under the candidate context.
    pub(super) fn palw_improvement_candidate_is_signed(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        payload: &kaspa_consensus_core::palw_improve_candidate_v1::PalwCandidateSubmissionV1,
        submitter: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        signature: &[u8],
    ) -> Result<(), String> {
        let message = kaspa_consensus_core::palw_improve_candidate_v1::palw_candidate_submission_message_v1(
            &self.palw_network_domain_v2(),
            payload,
            submitter,
        );
        self.palw_improvement_check_bond_signature(
            state,
            submitter,
            &message,
            signature,
            kaspa_consensus_core::palw_improve_candidate_v1::PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1,
            "a candidate",
        )
    }

    /// **RFC-0001 §2.10: the lister bond's signature over an adapter class listing.**
    pub(super) fn palw_adapter_listing_is_signed(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        payload: &kaspa_consensus_core::palw_adapter_class_v1::PalwAdapterClassListingV1,
        lister: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        signature: &[u8],
    ) -> Result<(), String> {
        let message = kaspa_consensus_core::palw_adapter_class_v1::palw_adapter_listing_message_v1(
            &self.palw_network_domain_v2(),
            payload,
            lister,
        );
        self.palw_improvement_check_bond_signature(
            state,
            lister,
            &message,
            signature,
            kaspa_consensus_core::palw_adapter_class_v1::PALW_ADAPTER_LISTING_MLDSA87_CONTEXT_V1,
            "an adapter class listing",
        )
    }

    pub(super) fn palw_improvement_check_bond_signature(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        message: &kaspa_hashes::Hash64,
        signature: &[u8],
        context: &[u8],
        what: &str,
    ) -> Result<(), String> {
        let record = state.bond(bond).ok_or_else(|| format!("{what} is attributed to a bond this chain does not have"))?;
        if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err(format!("{what} is attributed to a bond that is not Active"));
        }
        if !Self::verify_mldsa87_with_context_bool(&record.pubkey, message.as_byte_slice(), signature, context) {
            return Err(format!("{what} carries a signature the attributed bond's key does not verify"));
        }
        Ok(())
    }

    /// ADR-0088: the signature of a registry object, checked against the stored key of the bond
    /// it is attributed to, under the registry's own ML-DSA-87 context.
    pub(super) fn palw_model_check_bond_signature(
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

    /// **The object an `ObjectChunk` completes, when it completes a group** (G14 lane D): every part present (the pending ones plus
    /// this chunk), the assembled bytes hashing to the declared group id, and a borsh decode that consumes them. `None` for a chunk
    /// that completes nothing, for a group the transition would refuse on its face, and for bytes no object decodes from — the
    /// transition refuses those itself. Bounded by `PALW_OBJECT_CHUNK_MAX_BYTES × PALW_OBJECT_CHUNK_MAX_COUNT`.
    ///
    /// **A-2 uniformity: `None` too for an inner kind whose owning fence is not in force at `daa_score`** — the live build cannot decode
    /// those bytes, so to it the chunk completes a group of undecodable bytes: it is charged as such (the certification cap's
    /// structural count, dormant D14) and the transition refuses it as `ChunkedObjectUndecodable`, the block standing.
    pub(in crate::pipeline::virtual_processor) fn palw_kernel_chunk_inner(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        Self::palw_chunk_inner_any_kind(state, object)
            .filter(|inner| self.palw_lifecycle_kind_fences.kind_in_force_at(inner, daa_score))
    }

    /// [`Self::palw_kernel_chunk_inner`] without the A-2 reading: the decoded inner whatever its kind.
    pub(super) fn palw_chunk_inner_any_kind(
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2 as Obj, palw_object_chunk_group_id_v1};
        let Obj::ObjectChunk { group, index, count, bytes } = object else { return None };
        if *count == 0
            || *count > kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_COUNT
            || *index >= *count
            || bytes.is_empty()
            || bytes.len() > kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES
        {
            return None;
        }
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
        if palw_object_chunk_group_id_v1(&whole) != *group {
            return None;
        }
        borsh::from_slice::<Obj>(&whole).ok()
    }

    /// **G14 GAP 10 (lane K2S): the kernel route's acceptance gate at the mempool and the template.** A `0x4b` carrier of a
    /// `KernelRouteV1` is put through [`Self::palw_kernel_route_object_is_signed`] (the fence, an Active signer, ML-DSA-87, the kernel's
    /// strict decode, the OPV fence) and a `KernelConstraintReceiptV1` through the receipt arm's checks, both at the tip; `Some(why)`
    /// refuses it. `None` for every other transaction, and for an object chunk (judged on the assembled whole by the fold, as before).
    pub(in crate::pipeline::virtual_processor) fn palw_mempool_kernel_route_refusal(
        &self,
        tx: &Transaction,
        virtual_daa_score: u64,
    ) -> Option<String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
            return None;
        }
        let object =
            kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(tx))
                .objects
                .into_iter()
                .next()
                .filter(|carried| self.palw_lifecycle_kind_fences.kind_in_force_at(&carried.object, virtual_daa_score))
                .map(|carried| carried.object)?;
        if !matches!(object, Obj::KernelRouteV1 { .. } | Obj::KernelConstraintReceiptV1 { .. }) {
            return None;
        }
        if !self.palw_kernel_route_at(virtual_daa_score) {
            // A-2: below its fence a kernel-route carrier rides unjudged, as every other new kind does — refusing it here
            // would make a template this release builds differ from one a release that tolerates the kind builds.
            return None;
        }
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (_, state) = self.palw_state_v2_store.read().load_tip_cached(state_params).ok().flatten()?;
        match object {
            Obj::KernelRouteV1 { bytes, signer, signature } => {
                if bytes.len() > kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 {
                    return Some(format!("a {}-byte kernel route object past the carrier", bytes.len()));
                }
                self.palw_kernel_route_object_is_signed(&state, virtual_daa_score, &bytes, &signer, &signature).err()
            }
            Obj::KernelConstraintReceiptV1 { receipt, signature } => {
                let record =
                    match state.kernel_route().and_then(|k| k.bond_key_of(&receipt.seat_bond)).and_then(|key| state.bond(&key)) {
                        Some(record) => record,
                        None => return Some("a kernel receipt is signed by a bond the route never assigned".to_string()),
                    };
                if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                    return Some("a kernel receipt is signed by a bond that is not Active".to_string());
                }
                (!Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    &receipt.signing_message(),
                    &signature,
                    misaka_palw_kernel::receipt::CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1,
                ))
                .then(|| "a kernel receipt carries a signature the seat's key does not verify".to_string())
            }
            _ => None,
        }
    }

    /// **G14 (tag 113, C4 F-C4R3-03): is this chunk of the route's own lane signed by the Active bond that opens its group?**
    pub(super) fn palw_kernel_chunk_is_signed(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
        chunk: &kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkV1,
        signature: &[u8],
    ) -> Result<(), String> {
        if !self.palw_kernel_route_at(daa_score) {
            return Err(
                "a kernel route chunk is refused: palw_probabilistic_constraints_v1 is not in force at this block (G14)".to_string()
            );
        }
        // Target 6 belongs to the independently scheduled funded execution
        // namespace. Check it before even reading the opener or verifying bytes.
        if matches!(chunk.target, kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkTargetV1::FundedProofV11 { .. })
            && !self.palw_kernel_inner_fence_at(
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::FundedExecutionV11,
                daa_score,
            )
        {
            return Err("a funded proof chunk is refused below palw_funded_execution_v11".to_string());
        }
        if matches!(chunk.target, kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkTargetV1::StageProofV14 { .. })
            && !self.palw_kernel_inner_fence_at(
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::StageExecutionV14,
                daa_score,
            )
        {
            return Err("a typed stage proof chunk is refused before its owning stage14 fence".into());
        }
        let record = state
            .bond(&chunk.opener)
            .ok_or_else(|| "a kernel route chunk is signed by a bond this chain does not have".to_string())?;
        if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err("a kernel route chunk is signed by a bond that is not Active".to_string());
        }
        let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_chunk_message_v1(self.palw_network_domain_v2(), chunk);
        if !Self::verify_mldsa87_with_context_bool(
            &record.pubkey,
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1,
        ) {
            return Err("a kernel route chunk carries a signature its opener's key does not verify".to_string());
        }
        Ok(())
    }

    /// **G14 lane D (tag 110): is this kernel route object signed by the bond it names, and is the kernel willing to decode it?**
    /// Resolve each native action's original contract before body decoding,
    /// signature work or generic acceptance budgets. New actions never inherit
    /// authority merely from an active older action with the same outer tag.
    pub(super) fn palw_native_action_active_at_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        bytes: &[u8],
        daa: u64,
    ) -> Option<bool> {
        use kaspa_consensus_core::config::params::ForkActivation;
        use kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1;
        let active = |f: ForkActivation| f != ForkActivation::never() && f.is_active(daa);
        let final27 = || self.palw_native_final_v27.as_ref().is_some_and(|f| active(f.activation)) && self.palw_kernel_opv_at(daa);
        match bytes.get(1).copied()? {
            misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11 => Some(match bytes.get(2).copied() {
                Some(0..=24) => self.palw_kernel_inner_fence_at(PalwKernelInnerFenceV1::FundedExecutionV11, daa),
                Some(32) => {
                    if !final27() {
                        return Some(false);
                    }
                    let frame = misaka_palw_kernel::route::borrowed_native_action_v1(bytes).ok().flatten()?;
                    let part = kaspa_consensus_core::palw_native_paged_proof_v29::borrowed_native_paged_metadata_v29(frame.body)?;
                    match state.kernel_route().and_then(|r| r.native_paged_proof_namespace_v29(part.job, part.seal)) {
                        Some(28) => self.palw_stage_execution_v28.as_ref().is_some_and(|f| active(f.activation)),
                        Some(29) => self.palw_kernel_inner_fence_at(PalwKernelInnerFenceV1::FundedExecutionV11, daa),
                        _ => false,
                    }
                }
                Some(25..=40) => self.palw_kernel_inner_fence_at(PalwKernelInnerFenceV1::FundedExecutionV11, daa) && final27(),
                Some(45 | 46) => {
                    self.palw_kernel_inner_fence_at(PalwKernelInnerFenceV1::FundedExecutionV11, daa)
                        && final27()
                        && self.palw_window_class_v37.is_some_and(active)
                }
                Some(action @ 41..=44) => {
                    if !final27() {
                        return Some(false);
                    }
                    let frame = misaka_palw_kernel::route::borrowed_native_action_v1(bytes).ok().flatten()?;
                    let namespace = if action == 41 {
                        let raw: [u8; 2] = frame.body.get(2..4)?.try_into().ok()?;
                        Some(u16::from_le_bytes(raw))
                    } else {
                        let id = if action == 42 {
                            kaspa_consensus_core::palw_native_initial_v31::borrowed_initial_part_v31(frame.body)?.id
                        } else {
                            frame.body.try_into().ok()?
                        };
                        state.kernel_route().and_then(|r| r.native_initial_namespace_v31(id))
                    };
                    match namespace {
                        Some(28) => self.palw_stage_execution_v28.as_ref().is_some_and(|f| active(f.activation)),
                        Some(29) => self.palw_kernel_inner_fence_at(PalwKernelInnerFenceV1::FundedExecutionV11, daa),
                        _ => false,
                    }
                }
                _ => false,
            }),
            misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14 => Some(match bytes.get(2).copied() {
                Some(0..=18) => self.palw_kernel_inner_fence_at(PalwKernelInnerFenceV1::StageExecutionV14, daa),
                Some(19..=40) => self.palw_stage_execution_v28.as_ref().is_some_and(|f| active(f.activation)) && final27(),
                // Owning39 mining needs its own fence besides native39 (RFC-0002 §II.14, dormant).
                Some(73..=80) => {
                    self.palw_native_mining_v39.as_ref().is_some_and(|f| active(f.activation))
                        && self.palw_native_execution_v39.as_ref().is_some_and(|f| active(f.activation))
                        && final27()
                }
                Some(41..=79) => self.palw_native_execution_v39.as_ref().is_some_and(|f| active(f.activation)) && final27(),
                _ => false,
            }),
            _ => None,
        }
    }

    /// **Below Final27, funded actions 25..=40 ride unjudged** (project rule: a new kind is neither
    /// rejected nor applied below its own fence — rejecting it would split from a release that does not
    /// know it). The predicate is exactly the one that leaves `PalwKernelRouteExtrasV1::native_final`
    /// `None` (Final27 active AND the OPV fence active), and the core fold returns before any state read
    /// or write when that field is `None`, so skipping the signature here can never apply an unsigned body.
    /// Owning39 mining's prerequisite lanes at `daa`: native39 AND Final27 AND the OPV fence (the same conjunction that leaves
    /// `PalwKernelRouteExtrasV1::native_execution39` `Some`). Every preset: `false`.
    pub(super) fn palw_native_mining_v39_lanes_at(&self, daa: u64) -> bool {
        use kaspa_consensus_core::config::params::ForkActivation;
        self.palw_native_execution_v39.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_native_final_v27.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_kernel_opv_at(daa)
    }

    pub(super) fn palw_native_action_unjudged_at_v1(&self, bytes: &[u8], daa: u64) -> bool {
        use kaspa_consensus_core::config::params::ForkActivation;
        let final27 = self
            .palw_native_final_v27
            .as_ref()
            .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_kernel_opv_at(daa);
        let window37 = self.palw_window_class_v37.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa));
        match bytes.get(1).copied() {
            Some(misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11) => match bytes.get(2) {
                Some(25..=40) => !final27,
                // Typed window class (§II.14.60): unjudged below its own fence or below Final27,
                // mirroring the fold's `window_class_v37`/`native_final` early return exactly.
                Some(45 | 46) => !final27 || !window37,
                _ => false,
            },
            Some(misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14) if matches!(bytes.get(2), Some(19..=40)) => {
                !final27
                    || !self
                        .palw_stage_execution_v28
                        .as_ref()
                        .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            }
            Some(misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14) if matches!(bytes.get(2), Some(41..=79)) => {
                !final27
                    || !self
                        .palw_native_execution_v39
                        .as_ref()
                        .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
                    // Mining actions 73–79 also ride unjudged below their own fence (mirrors the fold's early return).
                    // G-N2's action 80 is not unjudged: below the mining fence it stays refused exactly as before (A-2).
                    || matches!(bytes.get(2), Some(73..=79))
                        && !self
                            .palw_native_mining_v39
                            .as_ref()
                            .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            }
            _ => false,
        }
    }

    pub(super) fn palw_kernel_route_object_is_signed(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
        bytes: &[u8],
        signer: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        signature: &[u8],
    ) -> Result<(), String> {
        if !self.palw_kernel_route_at(daa_score) {
            return Err(
                "a kernel route object is refused: palw_probabilistic_constraints_v1 is not in force at this block (G14)".to_string()
            );
        }
        // Consumer-owned opaque namespaces are gated from their fixed tag
        // before signature work or any body allocation. Typed body validation
        // and its expensive work belong to the prepaid native transition.
        if self.palw_native_action_unjudged_at_v1(bytes, daa_score) {
            return Ok(());
        }
        if self.palw_native_action_active_at_v1(state, bytes, daa_score) == Some(false) {
            return Err("a native execution object is refused before its owning fence".into());
        }
        let native = misaka_palw_kernel::route::borrowed_native_action_v1(bytes).map_err(|why| why.to_string())?;
        if let Some(action) = native
            && action.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14
            && matches!(action.action, 39 | 40)
        {
            let owner = if action.action == 39 {
                let p = kaspa_consensus_core::palw_flat_stage_v28::borrowed_invocation_commit_metadata_v32(action.body)
                    .ok_or_else(|| "native Commit32 original bounded part metadata absent".to_string())?;
                state
                    .kernel_route()
                    .and_then(|r| r.native_invocation_commit_owner_v32(p.job, p.group, p.index, p.count, p.total, daa_score))
            } else {
                if action.body.len() != 130 || action.body[..2] != 32u16.to_le_bytes() {
                    return Err("native Commit32 assembly original frame differs".into());
                }
                let job = action.body[2..66].try_into().expect("fixed original job width");
                let group = action.body[66..130].try_into().expect("fixed original group width");
                state.kernel_route().and_then(|r| r.native_invocation_assemble_owner_v32(job, group, daa_score))
            };
            if owner != Some(*signer) {
                return Err("native Commit32 original Funded producer/attempt/phase differs".into());
            }
        }
        if let Some(action) = native
            && action.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14
            && matches!(action.action, 58 | 59)
        {
            let (id, index) = if action.action == 58 {
                let part = kaspa_consensus_core::palw_native_execution_v39::borrowed_source_upload_part_v39(action.body)
                    .ok_or_else(|| "source39 original bounded part metadata absent".to_string())?;
                (part.id, Some(part.index))
            } else {
                let id = action.body.try_into().map_err(|_| "source39 exact original assembly id".to_string())?;
                (id, None)
            };
            if state.kernel_route().and_then(|r| r.native_source_upload_owner_v39(id, index, daa_score)) != Some(*signer) {
                return Err("source39 original owner/attempt/coverage/clock differs".into());
            }
        }
        if let Some(a) = native
            && a.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14
            && matches!(a.action, 68 | 69)
        {
            let (id, index) = if a.action == 68 {
                let part = kaspa_consensus_core::palw_native_execution_v39::borrowed_source_upload_part_v39(a.body)
                    .ok_or_else(|| "metadata39 original bounded part metadata absent".to_string())?;
                (part.id, Some(part.index))
            } else {
                (a.body.try_into().map_err(|_| "metadata39 exact original assembly id".to_string())?, None)
            };
            if state.kernel_route().and_then(|r| r.native_metadata_upload_owner_v39(id, index, daa_score)) != Some(*signer) {
                return Err("metadata39 original owner/attempt/coverage/clock differs".into());
            }
        }
        if let Some(a) = native
            && a.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14
            && a.action == 50
        {
            let part = kaspa_consensus_core::palw_native_job_v39::borrowed_native_proof_part_v39(a.body)
                .ok_or_else(|| "native39 original bounded filing metadata absent".to_string())?;
            if state
                .kernel_route()
                .and_then(|r| r.native_proof_owner_v39(part.job, part.seal, part.index, part.count, part.total, part.mode, daa_score))
                != Some(*signer)
            {
                return Err("native39 original filing owner/attempt/phase differs".into());
            }
        }
        if let Some(a) = native
            && a.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14
            && matches!(a.action, 54 | 55)
        {
            // Owning39 producer compact-post transport: only the original
            // Funded producer, inside its commit window and two attempts.
            let owner = if a.action == 54 {
                let part = kaspa_consensus_core::palw_native_job_v39::borrowed_native_commit_metadata_v39(a.body)
                    .ok_or_else(|| "native39 original bounded commit page metadata absent".to_string())?;
                state
                    .kernel_route()
                    .and_then(|r| r.native_commit_owner_v39(part.job, part.group, part.index, part.count, part.total, daa_score))
            } else {
                let (job, group) = kaspa_consensus_core::palw_native_job_v39::borrowed_native_commit_assembly_v39(a.body)
                    .ok_or_else(|| "native39 exact original commit assembly frame".to_string())?;
                state.kernel_route().and_then(|r| r.native_commit_assemble_owner_v39(job, group, daa_score))
            };
            if owner != Some(*signer) {
                return Err("native39 original commit transport producer/attempt/phase differs".into());
            }
        }
        if let Some(a) = native
            && a.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14
            && a.action == 48
        {
            // Answer39 uses the same fixed fragment frame as a filing part:
            // its two identities denote the pending unit and answer group.
            let part = kaspa_consensus_core::palw_native_job_v39::borrowed_native_proof_part_v39(a.body)
                .filter(|p| p.mode == 0 && p.count <= 4 && p.total <= 128 << 10)
                .ok_or_else(|| "native39 original bounded answer metadata absent".to_string())?;
            if state
                .kernel_route()
                .and_then(|r| r.native_answer_owner_v39(part.job, part.seal, part.index, part.count, part.total, daa_score))
                != Some(*signer)
            {
                return Err("native39 original answer producer/attempt/phase differs".into());
            }
        }
        if let Some(a) = native
            && ((a.tag == misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11 && a.action == 37)
                || (a.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14 && a.action == 32))
        {
            let p = kaspa_consensus_core::palw_native_reply_parts_v29::borrowed_native_reply_metadata_v29(a.body)
                .ok_or_else(|| "native reply29 bounded original unit/part metadata absent".to_string())?;
            let owner = state.kernel_route().and_then(|r| {
                if a.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14 {
                    r.stage_reply_owner_v28(p.pending, p.group, p.index, p.count, p.total, daa_score)
                } else {
                    r.native_text_reply_owner_v29(p.pending, p.index, p.count, p.total, p.group, daa_score)
                }
            });
            if owner != Some(*signer) {
                return Err("native reply29 original producer/attempt/page differs".into());
            }
        }
        if let Some(action) = native
            && action.tag == misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11
            && matches!(action.action, 42 | 43)
        {
            let (id, index) = if action.action == 42 {
                let p = kaspa_consensus_core::palw_native_initial_v31::borrowed_initial_part_v31(action.body)
                    .ok_or_else(|| "initial31 bounded owner/part metadata absent".to_string())?;
                (p.id, Some(p.index))
            } else {
                (action.body.try_into().map_err(|_| "initial31 exact original object identity".to_string())?, None)
            };
            if state.kernel_route().and_then(|r| r.native_initial_owner_v31(id, index, daa_score)) != Some(*signer) {
                return Err("initial31 original live owner/part differs".into());
            }
        }
        if let Some(action) = native
            && action.tag == misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11
            && action.action == 32
        {
            let part = kaspa_consensus_core::palw_native_paged_proof_v29::borrowed_native_paged_metadata_v29(action.body)
                .ok_or_else(|| "a paid29 proof part lacks exact bounded original metadata".to_string())?;
            if part.owner != kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(signer)
                || state
                    .kernel_route()
                    .and_then(|r| r.native_paged_proof_owner_v29(part.job, part.seal, part.owner, part.index, part.count, daa_score))
                    != Some(*signer)
            {
                return Err("a paid29 proof part lacks its original live paid seal owner".into());
            }
        }
        if let Some(action) = native
            && ((action.tag == misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11 && action.action == 15)
                || (action.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14 && action.action == 8)
                || (action.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14 && action.action == 26))
        {
            let pending: [u8; 64] = action
                .body
                .get(..64)
                .ok_or_else(|| "a source answer lacks its fixed pending-unit identity".to_string())?
                .try_into()
                .expect("fixed pending identity extent");
            let owner = state.kernel_route().and_then(|route| {
                if action.tag == misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11 {
                    route.execution_source_answer_owner_v16(&pending, daa_score)
                } else if action.action == 26 {
                    route.stage_source_answer_owner_v28(&pending, daa_score)
                } else {
                    route.stage_source_answer_owner_v14(&pending, daa_score)
                }
            });
            if owner != Some(*signer) {
                return Err("a source answer lacks its original live producer-owned pending unit".into());
            }
        }
        let record =
            state.bond(signer).ok_or_else(|| "a kernel route object is signed by a bond this chain does not have".to_string())?;
        if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err("a kernel route object is signed by a bond that is not Active".to_string());
        }
        let message =
            kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(self.palw_network_domain_v2(), signer, bytes);
        if !Self::verify_mldsa87_with_context_bool(
            &record.pubkey,
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
        ) {
            return Err("a kernel route object carries a signature its bond's key does not verify".to_string());
        }
        if native.is_some() {
            // The exact bounded borrowed frame is canonical. The native fold
            // charges its authenticated source/trace entitlement before any
            // nested payload decoder; signature preflight must not clone it.
            return Ok(());
        }
        let object = misaka_palw_kernel::route::KernelRouteObjectV1::decode(bytes).map_err(|refusal| format!("{refusal}"))?;
        // **A-2 uniformity one level down (`PALW_KERNEL_ROUTE_INNER_KINDS_V1`): an inner kind owned by a fence beyond tag 110's is dropped
        // unless that fence is in force** — below it a build without the kind refuses it at the kernel's decode, the block standing.
        // RFC-0015's registrations under a verification mode (inner 13 / 14) are `palw_panel_free_v1`'s: the network declares the OPV
        // policy or it does not; the ledger refuses the rest (admission, economics, carriers).
        // Inner 75 already dispatches claim certification before legacy model exams in
        // the fold. Mirror that exact existing-row distinction at signature preflight.
        // An unknown id still belongs to the disabled eligibility-exam route.
        let claim_certification_attestation = match &object {
            misaka_palw_kernel::route::KernelRouteObjectV1::AttestEligibilityExamV1 { attestation }
                if self.palw_state_params_v2.as_ref().is_some_and(|p| {
                    p.rfc0018_v1().is_some_and(|m| m.gate_c_at(daa_score))
                }) => {
                    use kaspa_consensus_core::palw_security_certification_v1::{
                        PALW_SECURITY_CERTIFICATION_TABLE_V1, PalwClaimCertificationRowV1,
                        certification_claim_key_v1, certification_native_key_v39,
                    };
                    state.kernel_route_state_v1().is_some_and(|route| {
                        [certification_claim_key_v1(&attestation.exam), certification_native_key_v39(&attestation.exam)]
                            .iter().any(|key| route.aux_row::<PalwClaimCertificationRowV1>(PALW_SECURITY_CERTIFICATION_TABLE_V1, key).is_some())
                    })
                }
            _ => false,
        };
        if let Some(fence) = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_kernel_route_inner_fence_v1(&object)
            && !self.palw_kernel_inner_fence_at(fence, daa_score)
            && !claim_certification_attestation
        {
            return Err(match fence {
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ClaimChallengeV2 => {
                    "a paid Claim V2 object is refused: palw_claim_challenge_v2 is not in force at this block".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelLifetimeV2 => {
                    "a prospective Panel lifetime consent is refused: palw_panel_lifetime_v2 is not in force at this block".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::VerifierPayV1 => {
                    "a drawn-check attestation is refused: palw_verifier_pay_v1 is not in force at this block".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ModelVerificationGateV1 => {
                    "a model verification attestation is refused: palw_model_verification_gate_v1 is not in force at this block".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::HorizonExtensionV1 => {
                    "a horizon extension consent is refused: palw_horizon_extension_v1 is not in force at this block".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::MinimumPriceV1 => {
                    "a minimum-price contract is refused: palw_minimum_price_v1 is not in force at this block".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::BeaconBootstrapV1 => {
                    "a BootstrapExact contribution is refused: palw_beacon_bootstrap_v1 is not in force at this block".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ModelEligibilityV1 => {
                    "an eligibility exam object is refused: palw_model_eligibility_v1 is not in force at this block (RFC-0017)".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ElasticCapacityV1 => {
                    "a capacity attestation is refused: palw_elastic_final_capacity_v1 is not in force at this block (RFC-0016)".into()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelFreeV1 => {
                    "a class registration under a verification mode is refused: palw_panel_free_v1 is not in force at this block (RFC-0015)"
                        .to_string()
                }
                // RFC-0004 Part II: a typed-root object (inner kind 19) or a typed-root proof (`ProsecutionV1::Spec` inside a filing) is
                // dropped unless `palw_typed_roots_v1` is in force (the route's schedule refuses it too: the second lock).
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::TypedRootsV1 => {
                    "a typed-root object is refused: palw_typed_roots_v1 is not in force at this block (RFC-0004 Part II)".to_string()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::BeaconSourceFeeV1 => {
                    "a funded source contract is refused: palw_beacon_source_fee_v1 is not in force at this block".to_string()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::BeaconSourceOptInV1 => {
                    "a source opt-in is refused: palw_beacon_source_opt_in_v1 is not in force at this block".to_string()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelFreeAndTypedRootsV1 => {
                    "a salted typed-root claim is refused: palw_panel_free_v1 and palw_typed_roots_v1 are not both in force at this block"
                        .to_string()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::StageExecutionV14 => {
                    "a typed stage is refused: palw_stage_execution_v14 is not in force at this block".to_string()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::FundedExecutionV11 => {
                    "a funded execution is refused: palw_funded_execution_v11 is not in force at this block".to_string()
                }
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ModelRefutationSealV1 => {
                    "a sealed candidate refutation is refused: palw_panel_free_v1 and palw_model_refutation_seal_v1 are not both in force \
                     at this block (RFC-0002 P0)"
                        .to_string()
                }
            });
        }
        Ok(())
    }

    /// Is a kernel-route inner kind's own fence (`PalwKernelInnerFenceV1`) in force at `daa_score`?
    pub(super) fn palw_kernel_inner_fence_at(
        &self,
        fence: kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1,
        daa_score: u64,
    ) -> bool {
        match fence {
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::StageExecutionV14 => {
                self.palw_stage_execution_v14.as_ref().is_some_and(|f| {
                    f.activation != kaspa_consensus_core::config::params::ForkActivation::never() && f.activation.is_active(daa_score)
                }) && self.palw_kernel_opv_at(daa_score)
            }
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::FundedExecutionV11 => {
                self.palw_funded_execution_v11.as_ref().is_some_and(|f| {
                    f.activation != kaspa_consensus_core::config::params::ForkActivation::never() && f.activation.is_active(daa_score)
                }) && self.palw_kernel_opv_at(daa_score)
            }
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ClaimChallengeV2 => {
                self.palw_state_params_v2.as_ref().is_some_and(|p| p.claim_challenge_v2_at(daa_score).is_some())
            }
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelLifetimeV2 => {
                self.palw_state_params_v2.as_ref().is_some_and(|p| p.panel_lifetime_v2_at(daa_score).is_some())
            }
            // M*-49's drawn-check attestation (inner 39): the verifier-pay fence (the kernel route's own was checked above).
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::VerifierPayV1 => self
                .palw_verifier_pay_v1
                .is_some_and(|f| f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(daa_score)),
            // RFC07/RFC08 economic contracts (inner 67–69): the mirrored dormant fences.
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ModelVerificationGateV1 => self
                .palw_state_params_v2
                .as_ref()
                .is_some_and(|p| p.econ_contracts_v1().model_verification_gate_at(daa_score).is_some()),
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::HorizonExtensionV1 => {
                self.palw_state_params_v2.as_ref().is_some_and(|p| p.econ_contracts_v1().horizon_extension_at(daa_score).is_some())
            }
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::MinimumPriceV1 => {
                self.palw_state_params_v2.as_ref().is_some_and(|p| p.econ_contracts_v1().minimum_price_at(daa_score).is_some())
            }
            // N3 (inner 73, round 3): the mirrored dormant bootstrap fence.
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::BeaconBootstrapV1 => {
                self.palw_state_params_v2.as_ref().is_some_and(|p| p.econ_contracts_v1().beacon_bootstrap_at(daa_score).is_some())
            }
            // RFC-0017 (inner 74/75): the mirrored dormant eligibility fence.
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ModelEligibilityV1 => {
                self.palw_state_params_v2.as_ref().is_some_and(|p| p.model_eligibility_v1_at(daa_score).is_some())
            }
            // RFC-0016 V8 (inner 76): the mirrored dormant elastic-capacity fence.
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ElasticCapacityV1 => {
                self.palw_state_params_v2.as_ref().is_some_and(|p| p.rfc0016_capacity_active_at(daa_score))
            }
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::BeaconSourceFeeV1 => {
                self.palw_beacon_source_fee_v1.is_some_and(|f| {
                    f.activation != kaspa_consensus_core::config::params::ForkActivation::never() && f.activation.is_active(daa_score)
                }) && self.palw_kernel_inner_fence_at(
                    kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::BeaconSourceOptInV1,
                    daa_score,
                )
            }
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelFreeV1 => self.palw_kernel_opv_at(daa_score),
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::TypedRootsV1 => {
                self.palw_kernel_typed_roots_at(daa_score)
            }
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::BeaconSourceOptInV1 => {
                self.palw_beacon_source_opt_in_v1.is_some_and(|f| {
                    f.activation != kaspa_consensus_core::config::params::ForkActivation::never() && f.activation.is_active(daa_score)
                }) && self.palw_kernel_opv_at(daa_score)
            }
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelFreeAndTypedRootsV1 => {
                self.palw_kernel_opv_at(daa_score) && self.palw_kernel_typed_roots_at(daa_score)
            }
            // RFC-0002 P0 (RFC02-T): the sealed candidate refutation needs the OPV fence and its own (never armable) fence.
            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::ModelRefutationSealV1 => {
                self.palw_kernel_opv_at(daa_score)
                    && self
                        .palw_model_refutation_seal_v1
                        .is_some_and(|f| f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(daa_score))
            }
        }
    }

    /// **RFC-0010: the claim-level, VERSIONED anchor — the only door to the lane-A anchor functions below for a CLAIM.**
    ///
    /// Which rule governs a claim's Panel is decided by its ACCEPTANCE (`PalwStateParamsV2::panel_claim_rule_v1`), never by the
    /// block that happens to anchor, bind or retry it. `HistoricalLaneA` claims — everything accepted below
    /// `palw_permissionless_panel_v1`, which is every claim of every shipped preset — take exactly the anchor
    /// [`Self::palw_v2_anchor_fact_of_candidate`] has always derived, genesis-operator privilege (`palw_operator_anchor`,
    /// [`Self::palw_chain_block_as_anchor_v1`], `operator_of_v1`) included, and drain under those rules to their own timeout,
    /// court and DA liability. A `PermissionlessV3` claim has NO lane-A anchor: its Panel is bound by the fold from a
    /// certified, claim-sealed entropy over a public population (`palw_panel_v3_fold_v1`), so it can inherit neither an
    /// operator's attempt as its seed nor a named operator as its binder. `None` here is that answer, not a missing anchor.
    pub(in crate::pipeline::virtual_processor) fn palw_v2_anchor_fact_for_claim_v1(
        &self,
        from: BlockHash,
        claim: &kaspa_consensus_core::palw_state_v2::PalwClaimStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2> {
        match state_params.panel_claim_rule_v1(claim.accepted_daa) {
            kaspa_consensus_core::palw_permissionless_panel_v1::PanelClaimRuleV1::HistoricalLaneA => {
                self.palw_v2_anchor_fact_of_candidate_for_v1(from, claim.bind_base_daa(), panel_params, Some(claim.bond))
            }
            kaspa_consensus_core::palw_permissionless_panel_v1::PanelClaimRuleV1::PermissionlessV3 => None,
        }
    }

    /// The panel's anchor, derived from THIS candidate's chain (Decision 7's sortition input).
    ///
    /// The first chain block at or past `accepted_daa + anchor_delay` that anchors a claim with that
    /// slot ([`Self::palw_chain_block_as_anchor_v1`]), with the predecessor's DAA as the witness that
    /// makes "first at or past" checkable — the same shape as the free-prompt beacon, for the same
    /// reason: a producer that supplies its own anchor picks its own jury.
    ///
    /// **Lane A (`Params::palw_operator_anchor`)** decides which chain block that is and what keys its
    /// draw. Past it (at the chain block's own DAA score) a chain block anchors a claim iff it IS, or
    /// MERGES (blue or red), an operator's attempt at or past the claim's slot. The seed is read off
    /// the earliest such attempt ([`Self::palw_operator_seed_source_v1`]: the one no other of them is
    /// in the DAG past of) — and the draw's DAA-keyed inputs (the draw policy,
    /// the readiness clock, the maturity floor, the capability bound, the shard plan) are resolved at
    /// that attempt's DAA score ([`Self::palw_v2_anchor_fact_with_seed_v1`]). Undisturbed, the anchor
    /// is the operator's attempt itself. When a non-operator displaces it from the selected chain (a
    /// sibling that wins the selected-parent tie-break after reading the attempt's panel off its
    /// published header), the claim binds in the first chain block that merges the displaced attempt
    /// — the very next one unless the displacer also keeps it out of that block's past — on the
    /// displaced attempt's seed and at its DAA. Displacement therefore neither buys a fresh draw nor
    /// holds a claim off its anchor (verification of lane A, 2026-09-26, finding 1: the rule this
    /// replaces bound only at an operator attempt that was itself a chain block, so a bonded
    /// non-operator displacing every operator attempt voided every claim `BindTimeout` at its
    /// backstop, and the displaced attempt's seed was carried to a binding block up to the whole bind
    /// window later, finding 2).
    pub(in crate::pipeline::virtual_processor) fn palw_v2_anchor_fact_of_candidate(
        &self,
        from: BlockHash,
        accepted_daa: u64,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2> {
        self.palw_v2_anchor_fact_of_candidate_for_v1(from, accepted_daa, panel_params, None)
    }

    /// [`Self::palw_v2_anchor_fact_of_candidate`] for a claim whose producer bond is `producer` — past RFC-0018's permissionless
    /// anchor the producer's own contributions are excluded from the claim's seed, and a chain block whose only contributions at
    /// or past the slot are the producer's does not anchor the claim (`palw_permissionless_anchors_claim_v1`).
    pub(in crate::pipeline::virtual_processor) fn palw_v2_anchor_fact_of_candidate_for_v1(
        &self,
        from: BlockHash,
        accepted_daa: u64,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
        producer: Option<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2> {
        let slot = accepted_daa.checked_add(panel_params.anchor_delay())?;
        // The latest chain block recorded at or past the slot that anchors it — the earliest on the
        // chain once the walk is done — with, past lane A, the seed's source.
        let mut candidate: Option<(BlockHash, u64, Option<(u64, BlockHash)>)> = None;
        // RFC-0018 §5: the permissionless anchor's seed where the recorded candidate is one (`None` below its fence).
        let mut permissionless_seed: Option<(u64, kaspa_consensus_core::Hash64)> = None;
        for block in self.reachability_service.default_backward_chain_iterator(from) {
            let header = self.headers_store.get_header(block).ok()?;
            let daa = header.daa_score;
            if daa >= slot {
                match self.palw_chain_block_as_anchor_v1(block, &header)? {
                    // RFC-0018 §5: a permissionless anchor anchors the slot only where its contributions at or past the slot reach
                    // the distinct-bond minimum; its seed is their aggregate, resolved at the earliest of them.
                    Some(anchor) if anchor.reach >= slot && anchor.permissionless.is_some() => {
                        if let Some(seed) = self.palw_permissionless_seed_source_v1(&anchor, slot, producer.as_ref()) {
                            candidate = Some((block, daa, None));
                            permissionless_seed = Some(seed);
                        }
                    }
                    Some(anchor) if anchor.reach >= slot => {
                        candidate = Some((block, daa, self.palw_operator_seed_source_v1(&anchor, slot)?));
                        permissionless_seed = None;
                    }
                    _ => {}
                }
                continue;
            }
            // The first block BELOW the slot is the witness; the last one recorded at or above it
            // is the anchor.
            let (anchor_block, anchor_daa, seed_source) = candidate?;
            if let Some((draw_daa, aggregate)) = permissionless_seed {
                return Some(self.palw_permissionless_anchor_fact_v1(anchor_block, draw_daa, anchor_daa, daa, aggregate));
            }
            return self.palw_v2_anchor_fact_with_seed_v1(anchor_block, anchor_daa, daa, seed_source);
        }
        let (anchor_block, anchor_daa, seed_source) = candidate?;
        if let Some((draw_daa, aggregate)) = permissionless_seed {
            return Some(self.palw_permissionless_anchor_fact_v1(anchor_block, draw_daa, anchor_daa, 0, aggregate));
        }
        self.palw_v2_anchor_fact_with_seed_v1(anchor_block, anchor_daa, 0, seed_source)
    }

    /// **RFC-0018 §5: the permissionless anchor's fact.** The seed (`anchor_execution`) is the aggregate of the per-bond contributions
    /// at or past the slot; the draw's DAA-keyed inputs resolve at the earliest of them (never after the anchor block's own DAA), as
    /// lane A resolves them at its seed attempt. `anchor_block` stays the block that binds (SW-8).
    fn palw_permissionless_anchor_fact_v1(
        &self,
        anchor_block: BlockHash,
        draw_daa: u64,
        anchor_daa: u64,
        predecessor_daa: u64,
        aggregate: kaspa_consensus_core::Hash64,
    ) -> kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2 {
        kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2 {
            anchor_block,
            anchor_daa: draw_daa.min(anchor_daa),
            predecessor_daa,
            anchor_execution: Some(aggregate),
        }
    }

    /// **RFC-0018 §5: the permissionless seed source** for a claim whose slot is `slot` — `(draw DAA, aggregate)`, or `None` where
    /// fewer than the minimum distinct bonds contribute at or past the slot. The producer's own bond is not excluded here (the walk
    /// does not carry the claim's bond: GAP-A2); the two-distinct-bond minimum keeps at least one other bond's unpredictable
    /// execution inside the hash.
    fn palw_permissionless_seed_source_v1(
        &self,
        anchor: &PalwChainAnchorV1,
        slot: u64,
        producer: Option<&kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Option<(u64, kaspa_consensus_core::Hash64)> {
        let contributions = anchor.permissionless.as_ref()?;
        kaspa_consensus_core::palw_rfc0018_v1::palw_permissionless_anchor_seed_v1(
            contributions,
            slot,
            producer,
            &kaspa_consensus_core::palw_rfc0018_v1::PalwPermissionlessAnchorPolicyV1::design_unapproved_v1(),
        )
    }

    /// **RFC-0018 §5: the contributions chain block `block` is or merges** — every attempt header (the block itself and its blue and
    /// genuine red mergeset, not its selected parent) whose envelope decodes and whose executor bond the registry at the block's
    /// selected parent holds under the very key the envelope carries (the header stage verified the signature over that key), each
    /// with the bond's maturity at the attempt's DAA. `admitted` is the header-and-registry approximation of admission (GAP-A1: the
    /// fold's own acceptance of a merged attempt is decided in this same block's transition, so reading it here would be circular).
    /// Reads only chain data every node holds. `None` where a store cannot serve what it reads.
    fn palw_permissionless_contributions_of_chain_block_v1(
        &self,
        block: BlockHash,
        header: &Header,
    ) -> Option<Vec<kaspa_consensus_core::palw_rfc0018_v1::PalwAnchorContributionV1>> {
        // RFC-0018 §5.2 (exact): the contributions are the ADMITTED attempts of committed fold output — the claims of the block's
        // selected-parent state, blue and red carriers alike — so recognition runs one chain block behind admission and no header
        // the fold refused ever contributes.
        let data = self.ghostdag_store.get_data(block).ok()?;
        let committed = self.palw_candidate_state_v2(data.selected_parent)?;
        Some(kaspa_consensus_core::palw_rfc0018_v1::palw_permissionless_contributions_from_state_v1(&committed, header.blue_score))
    }

    /// The header-and-registry reading the committed-output one above replaced (kept for its tests' history; unused).
    #[allow(dead_code)]
    fn palw_permissionless_contributions_of_headers_v0(
        &self,
        block: BlockHash,
        header: &Header,
    ) -> Option<Vec<kaspa_consensus_core::palw_rfc0018_v1::PalwAnchorContributionV1>> {
        let data = self.ghostdag_store.get_data(block).ok()?;
        let registry = self.palw_candidate_state_v2(data.selected_parent)?;
        let window = self.palw_bond_maturity_window_at(&registry, header.daa_score);
        let contribution = |hash: BlockHash, merged_header: &Header| {
            if !kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(merged_header.pow_algo_id) {
                return None;
            }
            let envelope =
                kaspa_consensus_core::palw_attempt_material_v3::native_attempt_base_v3(&merged_header.palw_commitment).ok()?;
            let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(envelope.attempt.executor_bond);
            let record = registry.bond(&bond)?;
            if record.pubkey != envelope.attempt.executor_pubkey {
                return None;
            }
            let execution =
                kaspa_consensus_core::palw_panel_v2::palw_panel_anchor_execution_v1(self.palw_network_domain_v2(), merged_header)?;
            let bond_matured = window.is_none_or(|w| record.registered_daa.saturating_add(w) <= merged_header.daa_score);
            Some(kaspa_consensus_core::palw_rfc0018_v1::PalwAnchorContributionV1 {
                daa_score: merged_header.daa_score,
                block: hash,
                bond,
                execution,
                admitted: true,
                bond_matured,
            })
        };
        let mut contributions = Vec::new();
        contributions.extend(contribution(block, header));
        let classified = data.classify_palw_mergeset_v1(|member| self.ghostdag_manager.is_round_block(member));
        for merged in classified.blues().iter().chain(classified.genuine_reds()).filter(|hash| **hash != data.selected_parent) {
            let merged_header = self.headers_store.get_header(*merged).ok()?;
            contributions.extend(contribution(*merged, &merged_header));
        }
        Some(contributions)
    }

    /// **How chain block `block` stands as a panel anchor — the one answer the anchor walk
    /// ([`Self::palw_v2_anchor_fact_of_candidate`], so the chain's derivation and the acceptance gate
    /// alike), step 4c (`sw8_anchor_delay` and `sw8_anchor_reach`, [`Self::palw_sw8_anchor_for`]) and
    /// the one-state pre-check read**, so they cannot disagree about which block a claim binds in.
    ///
    /// `Some(None)`: the block anchors no claim. `Some(Some(anchor))`: it anchors every claim whose slot
    /// is at or below `anchor.reach` and that no earlier chain block at or past the slot anchored.
    /// `None`: the store cannot serve a GHOSTDAG record or a merged header lane A reads — a node that
    /// cannot read the past names no anchor, as for a missing header.
    ///
    /// * **Below lane A's fence at the block's own DAA score** (every network but an armed
    ///   testnet-12): the block's lane decides alone ([`Self::palw_block_may_anchor_a_panel_v1`]), and
    ///   a block that may anchor reaches its own DAA score — byte for byte the walk testnet-12 launched
    ///   with.
    /// * **Past it:** the operator attempts the block is or merges
    ///   ([`Self::palw_operator_attempts_of_chain_block_v1`]). None, and the block anchors nothing,
    ///   whatever its lane; otherwise it reaches the latest of them, capped at its own DAA score — a
    ///   claim whose slot falls after that attempt (a displaced attempt merged a DAA or two late) is
    ///   not this block's to bind or to void. So a heartbeat or a non-operator's attempt that merges a
    ///   displaced operator attempt anchors the claims that attempt reached: the block binds them, the
    ///   operator's execution seeds them. The block's lane no longer matters there, because the seed
    ///   is never the block's own: lane A requires lane F1 at or below it, so it is the operator
    ///   attempt's execution commitment.
    pub(super) fn palw_chain_block_as_anchor_v1(&self, block: BlockHash, header: &Header) -> Option<Option<PalwChainAnchorV1>> {
        let daa = header.daa_score;
        // **RFC-0018 §5: past `palw_panel_anchor_permissionless_v1` (at the block's own DAA, as lane A) the anchor is permissionless** —
        // it takes precedence over lane A's operator list, which it replaces. A block anchors the slots up to its latest eligible
        // contribution (capped at its own DAA) once at least two distinct bonds contribute; the seed is their aggregate
        // (`palw_permissionless_seed_source_v1`). Below the fence (every shipped network) this branch is never taken.
        if self.palw_panel_anchor_permissionless_at(daa) {
            let contributions = self.palw_permissionless_contributions_of_chain_block_v1(block, header)?;
            let reach = kaspa_consensus_core::palw_rfc0018_v1::palw_permissionless_anchor_reach_v1(&contributions, daa);
            return Some(reach.map(|reach| PalwChainAnchorV1 {
                reach,
                operator_attempts: Some(contributions.iter().map(|c| (c.daa_score, c.block)).collect()),
                permissionless: Some(contributions),
            }));
        }
        match self.palw_operator_anchor.as_ref().filter(|rule| rule.active_at(daa)) {
            Some(rule) => {
                let attempts = self.palw_operator_attempts_of_chain_block_v1(rule, block, header)?;
                let latest = attempts.iter().map(|(attempt_daa, _)| *attempt_daa).max();
                Some(latest.map(|latest| PalwChainAnchorV1 {
                    reach: latest.min(daa),
                    operator_attempts: Some(attempts),
                    permissionless: None,
                }))
            }
            None => Some(self.palw_block_may_anchor_a_panel_v1(header, daa).then_some(PalwChainAnchorV1 {
                reach: daa,
                operator_attempts: None,
                permissionless: None,
            })),
        }
    }

    /// **Lane A: the operator attempts chain block `block` is or merges** — the block itself and every
    /// block its GHOSTDAG merges (blue or red; not its selected parent, a chain block that answers for
    /// itself), each counted iff it is an operator's attempt
    /// ([`kaspa_consensus_core::palw_operator_anchor_v1::PalwOperatorAnchorRuleV1::operator_of_v1`]),
    /// as `(DAA score, hash)`. Reads only chain data every node holds (the block's header, its GHOSTDAG
    /// mergeset and the merged headers), so reorg, IBD and a pruning-proof sync resolve it alike.
    /// `None` where the block's GHOSTDAG record or a merged header is missing.
    pub(super) fn palw_operator_attempts_of_chain_block_v1(
        &self,
        rule: &kaspa_consensus_core::palw_operator_anchor_v1::PalwOperatorAnchorRuleV1,
        block: BlockHash,
        header: &Header,
    ) -> Option<Vec<(u64, BlockHash)>> {
        let mut attempts = Vec::new();
        if rule.operator_of_v1(header).is_some() {
            attempts.push((header.daa_score, block));
        }
        let data = self.ghostdag_store.get_data(block).ok()?;
        let classified = data.classify_palw_mergeset_v1(|member| self.ghostdag_manager.is_round_block(member));
        for merged in classified.blues().iter().chain(classified.genuine_reds()).filter(|hash| **hash != data.selected_parent) {
            let merged_header = self.headers_store.get_header(*merged).ok()?;
            if rule.operator_of_v1(&merged_header).is_some() {
                attempts.push((merged_header.daa_score, *merged));
            }
        }
        Some(attempts)
    }

    /// **Lane A: the seed's source for a claim whose slot is `slot`, at an anchor that is or merges
    /// operator attempts** — `Some(None)` below lane A (the anchor keys the seed itself), `None` where
    /// the reachability store cannot answer.
    ///
    /// The EARLIEST of the operator attempts at or past the slot that the anchor is or merges: one that
    /// no other of them is in the DAG past of, and among several such (concurrent attempts, which only
    /// operators make) the least `(DAA score, hash)`. The order is the DAG's, not the DAA score's:
    /// testnet-12's clock is advanced only by heartbeats (ADR-0138), so an operator attempt produced
    /// after a displaced one — and merging it — carries the same DAA score, and a `(DAA score, hash)`
    /// minimum would hand the seed to whichever of the two hashes lower: a fresh draw on half of the
    /// displacements an operator's next attempt follows. Reachability is chain data every node holds,
    /// so reorg, IBD and a pruning-proof sync resolve it alike.
    pub(super) fn palw_operator_seed_source_v1(&self, anchor: &PalwChainAnchorV1, slot: u64) -> Option<Option<(u64, BlockHash)>> {
        let Some(attempts) = anchor.operator_attempts.as_ref() else {
            return Some(None);
        };
        let eligible: Vec<(u64, BlockHash)> = attempts.iter().filter(|(daa, _)| *daa >= slot).copied().collect();
        let mut earliest: Option<(u64, BlockHash)> = None;
        for &(daa, hash) in &eligible {
            let mut has_an_earlier = false;
            for &(_, other) in &eligible {
                if other != hash && self.reachability_service.try_is_dag_ancestor_of(other, hash).ok()? {
                    has_an_earlier = true;
                    break;
                }
            }
            if !has_an_earlier && earliest.is_none_or(|held| (daa, hash) < held) {
                earliest = Some((daa, hash));
            }
        }
        Some(earliest)
    }

    /// **The highest claim slot chain block `block` anchors**, or `None` where it anchors none (or the
    /// store cannot serve what lane A reads): [`Self::palw_chain_block_as_anchor_v1`]'s answer, as step
    /// 4c and the one-state pre-check read it. Below lane A it is the block's own DAA score on every
    /// block whose lane may anchor.
    pub(in crate::pipeline::virtual_processor) fn palw_anchor_reach_of_v1(&self, block: BlockHash, header: &Header) -> Option<u64> {
        self.palw_chain_block_as_anchor_v1(block, header).flatten().map(|anchor| anchor.reach)
    }

    /// **The anchor fact, with what the panel seed is keyed on** (lane F1, the post-launch panel-seed
    /// fence `Params::palw_panel_seed_execution`, 2026-09-25).
    ///
    /// Past the fence — resolved at the ANCHOR block's DAA score, a fact of the anchor header this
    /// walk already found, so every node (reorg, IBD, a pruning-proof sync) resolves it alike — the
    /// fact carries the anchor ATTEMPT's execution commitment, read off the anchor block's own header
    /// (`palw_panel_anchor_execution_v1`: the key the fold dedups that block's attempt on and its class
    /// ticket is drawn from), and the claim's panel, its segment assignment and its S3 sample are
    /// drawn from `H(execution ‖ claim)`
    /// ([`kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2::panel_seed`]): no signature,
    /// timestamp or nonce inside the bucket moves it. Below the fence `None`, and the seed is the
    /// anchor block, byte for byte the rule testnet-12 launched with.
    ///
    /// `validate_palw_v2` refuses the fence without `palw_rcore_plus` at or below it, so past it
    /// every anchor below lane A is an attempt block ([`Self::palw_block_may_anchor_a_panel_v1`]). A
    /// header the store cannot serve, or an attempt envelope that does not decode (the header stage
    /// refuses one, so a chain block never does), names no anchor: nothing binds, and the claim voids
    /// at its bind window as if its anchor had not arrived — a function of the candidate chain's own
    /// headers.
    ///
    /// **Past lane A's fence at the anchor block's DAA** the seed is read off `seed_source` — the
    /// earliest operator attempt at or past the slot that the anchor is or merges
    /// ([`Self::palw_operator_seed_source_v1`]), the anchor itself unless an operator attempt was
    /// displaced from the chain — and the fact's `anchor_daa`, the DAA score every draw input is
    /// resolved at (the draw policy with its readiness clock, the maturity floor, the capability bound,
    /// the shard plan; by the chain's derivation and by the gate alike), is that attempt's: a
    /// displacement cannot move a seat across a readiness lapse or a maturity boundary. `anchor_block`
    /// stays the block that binds (SW-8's "only in its own anchor block"), and the fences here are
    /// resolved at its DAA, as the walk resolves lane A. `validate_palw_v2` refuses lane A without F1
    /// at or below it, so there the seed is always an execution commitment.
    pub(super) fn palw_v2_anchor_fact_with_seed_v1(
        &self,
        anchor_block: BlockHash,
        anchor_daa: u64,
        predecessor_daa: u64,
        seed_source: Option<(u64, BlockHash)>,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2> {
        let (seed_block, draw_daa) = if self.palw_operator_anchor_at(anchor_daa) {
            // A lane-A anchor anchors a slot only through an operator attempt at or past it, so the
            // walk always hands one in.
            let (source_daa, source) = seed_source?;
            (source, source_daa.min(anchor_daa))
        } else {
            (anchor_block, anchor_daa)
        };
        let anchor_execution = if self.palw_panel_seed_execution_at(anchor_daa) {
            let header = self.headers_store.get_header(seed_block).ok()?;
            Some(kaspa_consensus_core::palw_panel_v2::palw_panel_anchor_execution_v1(self.palw_network_domain_v2(), &header)?)
        } else {
            None
        };
        Some(kaspa_consensus_core::palw_panel_v2::PalwAnchorFactV2 {
            anchor_block,
            anchor_daa: draw_daa,
            predecessor_daa,
            anchor_execution,
        })
    }

    /// **Which chain blocks' LANE may anchor a panel** — the header-only half of the one answer
    /// ([`Self::palw_chain_block_as_anchor_v1`]) below lane A's fence, resolved for a claim whose slot
    /// is `slot`.
    ///
    /// **The anchor has to cost an inference to move.** The anchor block's hash is one of the two
    /// randomness inputs the panel draw runs on, so whoever can cheaply produce blocks at the anchor
    /// slot can re-roll the panel that will judge them.
    ///
    /// * **Below `palw_rcore_plus`: every lane but the receipt-V3 and round lanes**
    ///   (`algo_id_carries_no_chain_position`), byte for byte the walk it always was. That excludes
    ///   the lanes whose headers cost nothing to re-produce (a producer could otherwise mint algo-7
    ///   blocks until one landed at the slot with a hash whose draw it liked) — but NOT the
    ///   heartbeat lane (algo 8, ADR-0066), whose target is the network constant `2⁻²⁴`: a couple of
    ///   seconds of one CPU per try. The comment that stood here said the walk was restricted to the
    ///   attempt lane; it never was, and on a network whose DAA clock ticks on heartbeats (testnet-12,
    ///   ADR-0138/0142) almost every anchor is one.
    /// * **Past `palw_rcore_plus`: the attempt lanes only** (`is_palw_attempt_algo_id`: algo 6 and 9)
    ///   (M4 review, 2026-09-24, finding 2). Under the stake-weighted draw a panel binds only in its
    ///   anchor block (SW-8), so the anchor re-roll is the ONE post-seed lever left, and ADR-0152
    ///   prices it at one block's work per try; an attempt's execution commits to its header's
    ///   pre-PoW hash (`execution_anchor_v3`), so every other header — and so every other anchor —
    ///   costs one more inference, the charge the job anchor makes for the same reason. **That holds
    ///   only where the seed is not the block's identity** (2026-09-25, `wf_72c1a397-e23`): the
    ///   identity also covers the attempt's signature, the timestamp and the nonce inside its bucket,
    ///   none of which the lottery prices, so one win re-rolled the panel for a re-signature. Past
    ///   lane F1's fence (`Params::palw_panel_seed_execution`, post-launch) the seed is the anchor
    ///   attempt's execution commitment and the claim ([`Self::palw_v2_anchor_fact_with_seed_v1`]);
    ///   another panel is another lottery win — while audit P0-10 is open, ~279 junk BLAKE2b draws on
    ///   the testnet-12 floor rather than an inference. What this
    ///   costs is waiting: the anchor is the first ATTEMPT block at or past the slot, which
    ///   `PalwConsensusParamsV2::validate` already holds to `anchor_delay + max_beacon_gap <
    ///   window_bind` (ADR-0077's lattice promise, written for this reading; 20 + 400 < 600 on the
    ///   RC lattice); a slot no attempt block reaches before the bind window lapses voids
    ///   `BindTimeout` at the window's backstop, without forfeit (S0), and a network with no attempts
    ///   has no claims to bind. The fold's step 4c voids a claim its anchor block did not bind at
    ///   that block, on this same lane rule ([`Self::palw_sw8_anchor_for`]).
    ///
    /// **Past lane A's fence (`Params::palw_operator_anchor`, testnet-12's post-launch stopgap,
    /// 2026-09-26) this predicate is not asked**: while P0-10 is open whoever may produce the anchor
    /// re-rolls a claim's panel by drawing fresh wins, so there a chain block anchors a claim iff it
    /// is, or merges, an OPERATOR's attempt at or past the claim's slot — whatever its own lane — and
    /// the seed is that operator attempt's execution ([`Self::palw_chain_block_as_anchor_v1`]). The
    /// attacker cannot hold the seed's position, and the claim it fixed before the slot buys it one
    /// draw on an execution it neither picks nor predicts.
    ///
    /// The R-core+ fence is resolved at the SLOT: `palw_rcore_plus` is genesis-only
    /// (`validate_palw_rcore_plus_v1`), so the slot, the anchor and the block give one answer on every
    /// chain that can run.
    pub(in crate::pipeline::virtual_processor) fn palw_block_may_anchor_a_panel_v1(&self, header: &Header, slot: u64) -> bool {
        if self.palw_rcore_plus_at(slot) {
            kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id)
        } else {
            !kaspa_consensus_core::pow_layer0::algo_id_carries_no_chain_position(header.pow_algo_id)
        }
    }

    /// **ADR-0152 SW-8 / DL-1 (M4): `PalwTransitionExtrasV1::{sw8_anchor_delay, sw8_anchor_reach}` for
    /// the block `point` names** — the panel's `anchor_delay` and the highest slot the block anchors,
    /// iff `palw_rcore_plus` is active at the block and the block anchors any claim
    /// ([`Self::palw_anchor_reach_of_v1`], the same per-block answer the anchor walk reads, at the
    /// same fences); `None` otherwise. So the fold's step 4c voids, in a block, exactly the claims
    /// whose anchor block it is by the walk's own rule and which it did not bind. A block whose
    /// header the store does not hold (none that the pipeline folds) answers `None`: its claims wait
    /// out the bind window's backstop.
    pub(super) fn palw_sw8_anchor_for(&self, point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2) -> Option<(u64, u64)> {
        self.palw_sw8_chain_anchor_v1(point).map(|(anchor_delay, _, anchor)| (anchor_delay, anchor.reach))
    }

    /// [`Self::palw_sw8_anchor_for`]'s one reading of the block — `(anchor_delay, the block's DAA score,
    /// how it stands as an anchor)` — which `palw_transition_extras_for` shares with lane F2's draw
    /// points ([`Self::palw_sw8_draw_points_of_anchor_v1`]), so the block's header and mergeset are read
    /// once per extras.
    pub(super) fn palw_sw8_chain_anchor_v1(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Option<(u64, u64, PalwChainAnchorV1)> {
        if !self.palw_rcore_plus_at(point.daa_score) {
            return None;
        }
        let panel = self.palw_panel_params_v2.as_ref()?;
        let header = self.headers_store.get_header(point.block).ok()?;
        let anchor = self.palw_chain_block_as_anchor_v1(point.block, &header)??;
        Some((panel.anchor_delay(), header.daa_score, anchor))
    }

    /// **Lane F2 (`Params::palw_floor_refusal_retry`): `PalwTransitionExtrasV1::sw8_draw` for the block
    /// `point` names** — `None` unless the fence is in force at the block (the bundle's mirror) and the
    /// block anchors past `palw_rcore_plus` ([`Self::palw_sw8_anchor_for`]'s answer, from the same
    /// [`Self::palw_chain_block_as_anchor_v1`]).
    ///
    /// One draw point per range of slots the block anchors, each with the DAA the claim's anchor fact
    /// reads its draw at ([`Self::palw_v2_anchor_fact_with_seed_v1`]'s `draw_daa`): below lane A the
    /// block's own, for every slot up to its reach; past lane A, for the slots up to each distinct DAA
    /// of the operator attempts the block is or merges, the DAA of the attempt
    /// [`Self::palw_operator_seed_source_v1`] names for them, capped at the block's. And at that DAA
    /// every input the derivation resolves there: the draw policy, the bond-maturity window and the
    /// second clock's raw depth (the window is widened over the fold's own base), the capability bound
    /// and the shard fence. So the fold's retry verdict (step 2e′) asks the draw's question on the
    /// draw's inputs. Chain data only (headers, GHOSTDAG, reachability), so every node — reorg, IBD, a
    /// pruning-proof sync — carries one answer.
    ///
    /// `palw_transition_extras_for` builds the same value from its one anchor reading
    /// ([`Self::palw_sw8_chain_anchor_v1`], [`Self::palw_sw8_draw_points_of_anchor_v1`]); this spelling is the
    /// tests' reading of it.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_sw8_draw_inputs_for(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwSw8DrawInputsV1> {
        if !self.palw_floor_refusal_retry_at(point.daa_score) {
            return None;
        }
        self.palw_sw8_draw_points_v1(point)
    }

    /// Lane F2: is the floor-refusal retry in force at `daa_score` (the bundle's mirror, the one the fold
    /// reads)? `false` on every shipped preset.
    pub(super) fn palw_floor_refusal_retry_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.floor_refusal_retry_active_at(daa_score))
    }

    /// [`Self::palw_sw8_draw_inputs_for`] without its fence: the draw points of any block that anchors
    /// past `palw_rcore_plus` — what lane A's displacement test holds to the anchor walk's own fact.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_sw8_draw_points_v1(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwSw8DrawInputsV1> {
        let (_, block_daa, anchor) = self.palw_sw8_chain_anchor_v1(point)?;
        self.palw_sw8_draw_points_of_anchor_v1(block_daa, &anchor)
    }

    /// [`Self::palw_sw8_draw_points_v1`] of a block already read: its DAA score and its anchor answer.
    pub(super) fn palw_sw8_draw_points_of_anchor_v1(
        &self,
        block_daa: u64,
        anchor: &PalwChainAnchorV1,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwSw8DrawInputsV1> {
        let panel = self.palw_panel_params_v2.as_ref()?;
        let reach = anchor.reach;
        // The slot ranges and their draw DAAs, as the anchor walk would name them.
        let ranges: Vec<(u64, u64)> = if self.palw_operator_anchor_at(block_daa) {
            let attempts = anchor.operator_attempts.as_ref()?;
            let mut thresholds: Vec<u64> = attempts.iter().map(|(daa, _)| (*daa).min(reach)).collect();
            thresholds.sort_unstable();
            thresholds.dedup();
            let mut ranges = Vec::with_capacity(thresholds.len());
            for max_slot in thresholds {
                let (source_daa, _) = self.palw_operator_seed_source_v1(anchor, max_slot)??;
                ranges.push((max_slot, source_daa.min(block_daa)));
            }
            ranges
        } else {
            vec![(reach, block_daa)]
        };
        let points = ranges
            .into_iter()
            .map(|(max_slot, draw_daa)| kaspa_consensus_core::palw_panel_v2::PalwSw8DrawPointV1 {
                max_slot,
                draw_daa,
                policy: self.palw_panel_draw_policy_at(draw_daa),
                bond_maturity_window: self.palw_bond_maturity_at(draw_daa),
                settled_anchor_depth: self.palw_settled_anchor_depth_at(draw_daa),
                capability_proof: self.palw_capability_bound_at(draw_daa),
                shard_licensing: self.palw_shard_licensing_at(draw_daa),
            })
            .collect();
        Some(kaspa_consensus_core::palw_panel_v2::PalwSw8DrawInputsV1 { seat_count: panel.seat_count(), points })
    }

    /// [`Self::palw_sw8_anchor_for`]'s `anchor_delay`: `Some` iff the block `point` names anchors a
    /// panel past `palw_rcore_plus` — what the processor tests read back as a block's lane answer.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_sw8_anchor_delay_for(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Option<u64> {
        self.palw_sw8_anchor_for(point).map(|(anchor_delay, _)| anchor_delay)
    }

    /// **ADR-0065 D4, resolved in exactly one place.** Every consumer — the receipt tally, the
    /// object-acceptance rehearsal, the assembler and the fold — must get the same answer for the
    /// same block, because a tally that says "no quorum" beside a fold that still charges the
    /// dissenting seat is two rules wearing one name.
    pub(super) fn palw_unavailable_abstains_at(&self, daa_score: u64) -> bool {
        self.palw_unavailable_abstains.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0069 Decision 7, resolved in exactly one place**, and resolved at the BLOCK's own
    /// acceptance DAA rather than at the evaluating node's tip.
    ///
    /// Fork choice compares candidate chains, so a rule that read the reader's point of view would
    /// make one block worth different amounts to two nodes — the same defect
    /// `PalwClassFactsViewV1` exists to close for the class target one line over.
    pub(super) fn palw_uncertified_weightless_at(&self, daa_score: u64) -> bool {
        self.palw_uncertified_weightless.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0084 U-08, resolved in exactly one place, at the BLOCK's own DAA.** The step ladder
    /// a court close is adjudicated at: the ruleset's past the fence, `2^22` before it. Read at
    /// the block's acceptance DAA and not the tip's for the reason the weight fences above give —
    /// two nodes grading one close proof must read one ladder.
    /// **ADR-0087 Decision 6, resolved in exactly one place, at the BLOCK's own DAA.**
    pub(super) fn palw_model_market_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_market.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0088 Decision 11, resolved in exactly one place, at the BLOCK's own DAA.**
    pub(in crate::pipeline::virtual_processor) fn palw_model_lines_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_lines.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(in crate::pipeline::virtual_processor) fn palw_model_immutable_v1_at(&self, daa_score: u64) -> bool {
        self.palw_model_immutable_v1.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0095 §4.11 as corrected, resolved at the BLOCK's own DAA like every other fence.
    pub(in crate::pipeline::virtual_processor) fn palw_model_benefits_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_benefits.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0114, resolved at the BLOCK's own DAA like every other fence.
    pub(in crate::pipeline::virtual_processor) fn palw_model_leg_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_leg_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0120, resolved at the BLOCK's own DAA like every other fence.
    pub(in crate::pipeline::virtual_processor) fn palw_model_seed_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_seed_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0162, resolved at the BLOCK's own DAA like every other fence — as the height it came into
    /// force at, where it is in force there (`PalwTransitionExtrasV1::model_virtual_v1`).
    pub(in crate::pipeline::virtual_processor) fn palw_model_virtual_v1_from_at(&self, daa_score: u64) -> Option<u64> {
        self.palw_model_virtual_v1.filter(|fence| fence.is_active(daa_score)).map(|fence| fence.daa_score())
    }

    /// ADR-0124 Decisions 1–5, resolved at one DAA — the BLOCK's for the fold and the receipt
    /// door, the claim's ANCHOR for the draw.
    pub(in crate::pipeline::virtual_processor) fn palw_panel_economy_active_at(&self, daa_score: u64) -> bool {
        self.palw_panel_economy.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0124 Decision 6, resolved at the BLOCK's own DAA like every other fence.
    pub(in crate::pipeline::virtual_processor) fn palw_work_priced_reward_active_at(&self, daa_score: u64) -> bool {
        self.palw_work_priced_reward.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0130: the seat exposure floor's reward multiple at one DAA, in permille — `0` where the
    /// floor is dormant.** The claim's ANCHOR for the draw (inside `palw_panel_draw_policy_at`), the
    /// BLOCK's for the fold's reservation (`palw_transition_extras_at`); a binding the chain derives
    /// at its anchor block reads one number at both.
    pub(in crate::pipeline::virtual_processor) fn palw_panel_reward_multiple_permille_at(&self, daa_score: u64) -> u32 {
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
    /// Model attempts grow their escrow; recovery floor claims retain their old bounded carve.
    /// Coinbases withhold the entire new base and burn any unallocated difference.
    pub(super) fn palw_escrow_for_carrier_at(
        &self,
        carrier: BlockHash,
        earning_daa: u64,
        paying_daa: u64,
    ) -> Option<kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2> {
        if self.dns_retired_at(earning_daa.min(paying_daa)) {
            let is_floor = self
                .headers_store
                .get_header(carrier)
                .ok()
                .and_then(|h| kaspa_consensus_core::palw_attempt_material_v3::native_attempt_base_v3(&h.palw_commitment).ok())
                .is_some_and(|e| self.palw_state_params_v2.as_ref().is_some_and(|p| e.attempt.class_id == p.base_class_id()));
            if is_floor {
                return kaspa_consensus_core::config::params::palw_overlay_escrow_carve_at_v1(
                    self.palw_overlay_carve,
                    earning_daa,
                    paying_daa,
                );
            }
        }
        self.palw_escrow_carve_at(earning_daa, paying_daa)
    }

    /// **ADR-0126 Decision 3: the carve a claim escrows** for an attempt carried at
    /// `attempt_daa_score` and paid by the block at `paying_daa_score`, resolved at the lower of the
    /// two ([`kaspa_consensus_core::config::params::palw_overlay_escrow_carve_at_v1`]) so it never
    /// outgrows the worker base of the split its payer withholds it from. The fold's escrow and the
    /// coinbase's withhold read the same resolved value. `None` below the fence, where the bundle's
    /// own `worker_carve_permille` applies.
    pub(in crate::pipeline::virtual_processor) fn palw_escrow_carve_at(
        &self,
        attempt_daa_score: u64,
        paying_daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2> {
        if self.dns_retired_at(attempt_daa_score.min(paying_daa_score)) {
            return Some(kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2::new(920).expect("92% fits the fixed schedule"));
        }
        kaspa_consensus_core::config::params::palw_overlay_escrow_carve_at_v1(
            self.palw_overlay_carve,
            attempt_daa_score,
            paying_daa_score,
        )
    }

    /// **Lane accept-order**: whether a merging block at `daa_score` applies its mergeset parents-first.
    pub(in crate::pipeline::virtual_processor) fn palw_lane_accept_parents_first_at(&self, daa_score: u64) -> bool {
        self.palw_lane_accept_parents_first.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0125: the execution lane's shape where it is open at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_execution_lane_at(
        &self,
        daa_score: u64,
    ) -> Option<kaspa_consensus_core::config::params::PalwExecutionLaneV1> {
        self.palw_execution_lane.filter(|lane| lane.activation.is_active(daa_score))
    }

    /// **ADR-0125 / ADR-0125 semantic amendment: the round blocks of a mergeset** — members carrying the round lane's id where the
    /// lane is open at their own DAA score. Headers only, so the template (which holds no verdicts)
    /// and the validating walk compute the same set; nothing is read where the lane is not configured.
    pub(in crate::pipeline::virtual_processor) fn palw_round_blocks_of(&self, ghostdag_data: &GhostdagData) -> BlockHashSet {
        let Some(lane) = self.palw_execution_lane else {
            return BlockHashSet::default();
        };
        ghostdag_data
            .unordered_mergeset_without_selected_parent()
            .filter(|member| {
                self.headers_store.get_header(*member).is_ok_and(|header| {
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
    pub(in crate::pipeline::virtual_processor) fn palw_round_verdicts_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        ghostdag_data: &GhostdagData,
        daa_score: u64,
    ) -> Option<super::super::utxo_validation::PalwRoundVerdictsV1> {
        self.palw_round_verdicts_at_v1(state, ghostdag_data, daa_score, Default::default())
    }

    pub(super) fn palw_round_verdicts_at_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        ghostdag_data: &GhostdagData,
        daa_score: u64,
        accepting_block: BlockHash,
    ) -> Option<super::super::utxo_validation::PalwRoundVerdictsV1> {
        use kaspa_consensus_core::palw_exec_v2::PalwExecV2Envelope;
        use kaspa_consensus_core::palw_execution_lane_v1::{
            PalwExecEnvelopeV1, PalwExecPermitUseV1, palw_execution_permit_of_v2, palw_execution_span_v1,
        };
        let lane = self.palw_execution_lane_at(daa_score)?;
        // Route-matrix #2: past ADR-0151's bundle with the quanta armed, a permit is a ticket.
        let tickets_only = self.palw_round_permits_are_tickets_at(daa_score);
        let span_daa = lane.schedule_span_daa_at(daa_score);
        let span_now = palw_execution_span_v1(daa_score, span_daa);
        let mut verdicts = super::super::utxo_validation::PalwRoundVerdictsV1 {
            round_blocks: self.palw_round_blocks_of(ghostdag_data),
            fee_budget: self.palw_state_params_v2.as_ref().and_then(|params| {
                state.round_fee_admission_v1(
                    params,
                    &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                        block: Default::default(),
                        daa_score,
                        blue_score: ghostdag_data.blue_score,
                        subsidy: self.coinbase_manager.calc_block_subsidy(daa_score),
                    },
                    self.palw_escrow_for_carrier_at(accepting_block, daa_score, daa_score),
                )
            }),
            ..Default::default()
        };
        // **RFC-0008 v2: the covered set is judged in the canonical order — `(round, permit index, hash)` — so two blocks of one permit,
        // or more payees than a coinbase may pay, have a deterministic winner** (the header rule that bounds a v1 mergeset does not bound
        // an anchor's closure). Slices carry no round and sort last; they are skipped below. Past the fence only: below it (and where it
        // is not armed) the order, the checks and the telemetry are the build before RFC-0008 v2's, byte for byte — the header rule
        // already forbids a duplicate permit and more than the payee bound in a v1 mergeset, but virtual's own mergeset is not a header
        // the rule judged, so the new checks are not merely redundant there (the X8R review).
        let exec_v2 = self.palw_exec_v2_active_at(daa_score);
        let mut ordered: Vec<BlockHash> = verdicts.round_blocks.iter().copied().collect();
        ordered.sort();
        if exec_v2 {
            ordered.sort_by_key(|block| {
                let coords = self
                    .headers_store
                    .get_header(*block)
                    .ok()
                    .and_then(|header| kaspa_consensus_core::palw_exec_v2::palw_exec_lane_coords_v1(&header.palw_commitment).ok());
                match coords {
                    Some(kaspa_consensus_core::palw_exec_v2::PalwExecLaneCoordsV1::Permit { round, permit_index, .. }) => {
                        (0u8, round, permit_index, *block)
                    }
                    _ => (1u8, 0, 0, *block),
                }
            });
        }
        let mut taken: std::collections::BTreeSet<(u64, u64, u16)> = std::collections::BTreeSet::new();
        let mut payees: std::collections::BTreeSet<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2> =
            std::collections::BTreeSet::new();
        for block in ordered {
            use kaspa_consensus_core::palw_exec_view_v1::{PalwRoundLineageV1, PalwRoundRefusalV1 as Refusal};
            // **RFC-0008 v2: an `EXEC_SLICE` carrier holds no permit and is judged by no permit verdict** — the fold judges it, by the
            // six admission rules, against its running state. It is a lane member (in `round_blocks`, so its body is inert) and never
            // `permitted`, so it pays and accepts nothing here.
            if exec_v2
                && self.headers_store.get_header(block).is_ok_and(|header| {
                    PalwExecV2Envelope::is_v2_carriage(&header.palw_commitment)
                        && PalwExecV2Envelope::decode(&header.palw_commitment)
                            .is_ok_and(|envelope| envelope.subtype == kaspa_consensus_core::palw_exec_v2::PalwExecSubtypeV2::Slice)
                })
            {
                continue;
            }
            let judged = (|| -> Result<(PalwExecPermitUseV1, PalwRoundLineageV1, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2), Refusal> {
                let header = self.headers_store.get_header(block).map_err(|_| Refusal::EnvelopeUndecodable)?;
                // The envelope this block's height admits: v2's `EXEC_TX` permit, or the v1 round envelope (a block merged below
                // the fence). Both name a round, a permit index, a bond and the key that signed; the checks below are one function
                // of those four.
                let envelope = if exec_v2 && PalwExecV2Envelope::is_v2_carriage(&header.palw_commitment) {
                    let v2 = PalwExecV2Envelope::decode(&header.palw_commitment).map_err(|_| Refusal::EnvelopeUndecodable)?;
                    let permit = v2.tx_permit.ok_or(Refusal::EnvelopeUndecodable)?;
                    PalwExecEnvelopeV1 {
                        version: kaspa_consensus_core::palw_execution_lane_v1::PALW_EXEC_ENVELOPE_VERSION_V1,
                        network_domain: v2.network_domain,
                        round: permit.round,
                        permit_index: permit.permit_index,
                        bond: v2.executor_bond,
                        pubkey: v2.pubkey,
                        signature: Vec::new(),
                    }
                } else {
                    PalwExecEnvelopeV1::decode(&header.palw_commitment).map_err(|_| Refusal::EnvelopeUndecodable)?
                };
                let anchor = self.ghostdag_store.get_selected_parent(block).map_err(|_| Refusal::SpanOutsideWindow)?;
                let span = palw_execution_span_v1(
                    self.headers_store.get_daa_score(anchor).map_err(|_| Refusal::SpanOutsideWindow)?,
                    span_daa,
                );
                if span > span_now || span + 1 < span_now {
                    return Err(Refusal::SpanOutsideWindow);
                }
                // §7.3: a permit proven signed twice is granted to no block.
                if state.round_equivocated(span, envelope.round, envelope.permit_index) {
                    return Err(Refusal::PermitEquivocated);
                }
                let schedule = state.round_schedule(span).ok_or(Refusal::NoSchedule)?;
                // §7.2: the width of the anchor's span — the width the schedule was drawn at.
                let permit = palw_execution_permit_of_v2(
                    schedule,
                    envelope.round,
                    lane.width_of_span_len(span, span_daa),
                    envelope.permit_index,
                    &envelope.bond,
                    tickets_only,
                )
                .ok_or(Refusal::PermitNotGranted)?;
                let bond = state.bond(&envelope.bond).ok_or(Refusal::BondNotActiveOrKeyMismatch)?;
                if !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
                    || bond.pubkey != envelope.pubkey
                {
                    return Err(Refusal::BondNotActiveOrKeyMismatch);
                }
                let transactions = self.block_transactions_store.get(block).map_err(|_| Refusal::PayoutMismatch)?;
                let coinbase = self
                    .coinbase_manager
                    .deserialize_coinbase_payload(&transactions.first().ok_or(Refusal::PayoutMismatch)?.payload)
                    .map_err(|_| Refusal::PayoutMismatch)?;
                if coinbase.miner_data.script_public_key
                    != kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&bond.payout_payload.as_bytes())
                {
                    return Err(Refusal::PayoutMismatch);
                }
                if state.round_permit_used(span, envelope.round, envelope.permit_index)
                    || (exec_v2 && taken.contains(&(span, envelope.round, envelope.permit_index)))
                {
                    return Err(Refusal::PermitAlreadyUsed);
                }
                // The coinbase pays each permitted bond one aggregate output, so the distinct payees one block may pay are bounded.
                if exec_v2
                    && !payees.contains(&envelope.bond)
                    && payees.len() >= kaspa_consensus_core::palw_execution_lane_v1::PALW_EXEC_MAX_BONDS_PER_MERGESET_V1
                {
                    return Err(Refusal::PayeeBound);
                }
                let lineage = kaspa_consensus_core::palw_exec_view_v1::palw_round_lineage_v1(state, schedule, permit.quantum_id);
                Ok((PalwExecPermitUseV1 { span, round: envelope.round, permit_index: envelope.permit_index }, lineage, envelope.bond))
            })();
            match judged {
                Ok((used, lineage, bond)) => {
                    verdicts.fee_recipients.insert(block, bond);
                    taken.insert((used.span, used.round, used.permit_index));
                    if let Some(bond) = self
                        .headers_store
                        .get_header(block)
                        .ok()
                        .and_then(|header| kaspa_consensus_core::palw_exec_v2::palw_exec_lane_coords_v1(&header.palw_commitment).ok())
                        .and_then(|coords| match coords {
                            kaspa_consensus_core::palw_exec_v2::PalwExecLaneCoordsV1::Permit { bond, .. } => Some(bond),
                            _ => None,
                        })
                    {
                        payees.insert(bond);
                    }
                    verdicts.permitted.insert(block);
                    verdicts.uses.push(used);
                    verdicts.judged.push((block, Ok(lineage)));
                }
                Err(refusal) => verdicts.judged.push((block, Err(refusal))),
            }
        }
        verdicts.uses.sort();
        Some(verdicts)
    }

    /// **Lane SCAN: write each round block's verdict into the node's telemetry ledger.** Node-local
    /// and read by no consensus path: the ledger answers `getBlock`'s lane class and
    /// `getPalwRoundLane`'s health.
    pub(super) fn palw_record_round_verdicts(&self, verdicts: &super::super::utxo_validation::PalwRoundVerdictsV1) {
        use kaspa_consensus_core::palw_exec_view_v1::{PalwRoundBlockRecordV1, PalwRoundOutcomeV1, palw_round_lane_telemetry_v1};
        let ledger = palw_round_lane_telemetry_v1();
        for (block, verdict) in &verdicts.judged {
            let Ok(header) = self.headers_store.get_header(*block) else { continue };
            let coords =
                kaspa_consensus_core::palw_exec_v2::palw_exec_lane_coords_v1(&header.palw_commitment).ok().and_then(|coords| {
                    match coords {
                        kaspa_consensus_core::palw_exec_v2::PalwExecLaneCoordsV1::Permit { round, permit_index, bond } => {
                            Some((round, permit_index, bond))
                        }
                        kaspa_consensus_core::palw_exec_v2::PalwExecLaneCoordsV1::Slice { .. } => None,
                    }
                });
            ledger.record(PalwRoundBlockRecordV1 {
                hash: *block,
                daa_score: header.daa_score,
                timestamp_ms: header.timestamp,
                round: coords.map(|c| c.0).unwrap_or(0),
                permit_index: coords.map(|c| c.1).unwrap_or(0),
                bond: coords.map(|c| c.2),
                outcome: match verdict {
                    Ok(lineage) => PalwRoundOutcomeV1::Exec(*lineage),
                    Err(refusal) => PalwRoundOutcomeV1::Refused(*refusal),
                },
            });
        }
    }

    /// **The bind's Valid-lock question, for the draw** (the 2026-09-23 route-matrix audit's #3):
    /// what one `Valid` signature on `claim` must lock and the clocks a bond's free collateral is
    /// read at, from the BINDING block's fold inputs (`extras`, `now_daa`) — the ones the fold's
    /// `require_panel_lock_eligible` reads, so the draw never seats a bond the bind would refuse.
    /// `None` below `palw_audit_2026_09_23` or where the lock ledger is not armed: testnet-11 arms
    /// the ledger without the fence, and its derived panels must stay the ones it always drew.
    pub(in crate::pipeline::virtual_processor) fn palw_panel_valid_lock_of_v1(
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        extras: &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1,
        // The claim's id: the buyback bound `s` of L-1's `G_res` reads the claim's root through it.
        claim_id: &Hash64,
        claim: &kaspa_consensus_core::palw_state_v2::PalwClaimStateV2,
        now_daa: u64,
        // The panel's seat count: what `duty_bind` divides the claim's commitment by.
        seat_count: usize,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwPanelValidLockV1> {
        // The one pricing, shared with lane F2's retry verdict (the fold's step 2e′), so the draw and the
        // verdict cannot read one claim's lock two ways.
        kaspa_consensus_core::palw_state_v2::palw_panel_valid_lock_v1(
            state,
            state_params,
            extras,
            claim_id,
            claim,
            now_daa,
            seat_count,
        )
    }

    /// **ADR-0124's draw policy at a claim's anchor** — the deep fence's weighting and the panel
    /// economy's floor and ceiling, resolved at ONE point so the assembler and the acceptance layer
    /// recompute one identical panel. `economy` is `None` while the fence is dormant at the anchor.
    ///
    /// **ADR-0152 SW-1 (M4): `stake` is `Some(PalwPanelStakeDrawV1::V1)` iff `palw_rcore_plus` is
    /// active at THIS anchor's DAA** — the one resolver, so the chain's derivation, the acceptance
    /// gate and every shadow reader draw one panel. It weights exactly two draws, the flat class draw
    /// and ADR-0147's outsider seat (SW-5); **the admission jury never reads it** (SW-A4:
    /// `palw_admission_jury_v1` / `admission_jury_seated` stay ADR-0147's unweighted ticket, past the
    /// fence and below it). Where it is `Some` the draw also refuses under SW-10's eligible-stake floor
    /// (`InsufficientEligibleStake`), a panel binds only in its own anchor block (SW-8,
    /// `validate_panel_bound_v2_with_policy`), that anchor is an attempt block
    /// (`palw_block_may_anchor_a_panel_v1`; past lane A, a block that is or merges an operator's
    /// attempt, `palw_chain_block_as_anchor_v1`), and a claim the anchor block does not bind voids there
    /// (the fold's step 4c); `None` — testnet-11, devnet, mainnet, and testnet-12 with the fence off —
    /// is ADR-0130's operator lottery and ADR-0147's outsider ticket, byte for byte (T88).
    ///
    /// **What arming it leans on (the pure half's review, 2026-09-24).** SW-10 counts every seat the
    /// one ledger has filled as saturated, and the ledger does not ask WHY a seat is full: an honest
    /// seat's own work — its locks, its duties, its own claims' reservations — counts exactly as an
    /// attacker's load does. So how much of a seat one duty takes decides whether the floor ever
    /// fires on an honest network, and that price is S's, not this draw's: **S-3's A-4 duty pricing**
    /// (the one-ledger committed amount a seat's duty on a claim reserves; S-SPEC §3.3 L-4b adds the
    /// one-ledger eligibility to `PalwPanelValidLockV1`). Without A-4, a 2M duty priced at the claim's
    /// full exposure (~179k MSK) leaves every 130k operator in the 2M base but never eligible, and a
    /// 2M class held by such operators beside the genesis seats would refuse every draw on the floor.
    /// Integrated (rcore/int-1): S-3's `duty_bind` is what a seat reserves at bind, and its L-4b
    /// filter (`PalwPanelValidLockV1::rcore`, `committed + max(duty_bind, lock_2)` under the 500‰
    /// ceiling, filled at the binding block by `palw_panel_valid_lock_of_v1`) is this draw's
    /// eligibility test in place of the economy's headroom — the one ledger decides WHETHER a bond is
    /// drawn, the posted stake HOW OFTEN; SW-10's base is the same list before that filter.
    ///
    /// **The margin on testnet-12 (M4 review finding 1).** testnet-12's producers are its eight
    /// genesis cards, so a floor claim's executor is one of the eight and may not sit. SW-10 counts
    /// the executor's own capped weight on both sides of the floor where it could sit but for being
    /// the executor (`palw_panel_stake_executor_bonds_judging_v1`), so a genesis card's claim is
    /// measured over all eight: with the other seven unloaded it binds at 8/8; with ONE other seat
    /// saturated (or the executor itself — the busiest producer — its load unread) at 7/8 = 875‰; with
    /// two other seats saturated it refuses (6/8). Without the term the seven alone would refuse at
    /// one (6/7 = 857‰), and one working card would halt every other card's binding. At genesis
    /// nothing is loaded and the 2M class is C7-held to one claim, which
    /// `t12_genesis_binds_under_the_stake_draw` pins on a real chain and the pure half's
    /// `t94_a_genesis_executor_keeps_the_one_saturated_seat_tolerance` pins at the draw. With A-4
    /// landed its duty price is what moves SW-10's margin; T94's saturation cases, re-read under the
    /// one-ledger filter, are
    /// `t94_under_the_one_ledger_filter_free_stake_decides_whether_and_posted_stake_how_often`.
    pub(in crate::pipeline::virtual_processor) fn palw_panel_draw_policy_at(
        &self,
        anchor_daa: u64,
    ) -> kaspa_consensus_core::palw_panel_v2::PalwPanelDrawPolicyV1 {
        let economy = if self.palw_panel_economy_active_at(anchor_daa) {
            self.palw_state_params_v2.as_ref().map(|state| kaspa_consensus_core::palw_panel_economy_v1::PalwSeatEconomyV1 {
                panel_floor_sompi: kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_at_v1(
                    state.min_collateral_sompi(),
                    state.legacy_public_filer_active_at(anchor_daa),
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
            // ADR-0152 SW-1: the stake-weighted draw's terms past `palw_rcore_plus` at THIS anchor's
            // DAA (the 1,000,000 MSK cap and the 875‰ floor); `None` everywhere else.
            // Lane PL part E (ADR-0166): past `palw_seat_availability` at THIS anchor the class seats' race weight is scaled by the
            // seat's availability factor, read at the anchor's DAA.
            stake: self.palw_rcore_plus_at(anchor_daa).then(|| kaspa_consensus_core::palw_panel_v2::PalwPanelStakeDrawV1 {
                availability_at: self
                    .palw_state_params_v2
                    .as_ref()
                    .is_some_and(|state| state.seat_availability_active_at(anchor_daa))
                    .then_some(anchor_daa),
                ..kaspa_consensus_core::palw_panel_v2::PalwPanelStakeDrawV1::V1
            }),
        }
    }

    /// **RFC-0010: what the permissionless Panel's draw reads that the transition cannot see**, resolved at the PRE-ENTROPY
    /// CHECKPOINT — the block's selected parent, never the block itself or anything it carries. `None` unless the fence is in
    /// force at the block (the V2 bundle's mirror, `PalwStateParamsV2::panel_v3`) with R-core+, which is every shipped
    /// preset: `validate_palw_permissionless_panel_v1` refuses every armed height, because no beacon scheme is approved
    /// (`approved_panel_beacon_policies_v1` is empty) and every Final on this chain is Panel-licensed.
    pub(super) fn palw_panel_v3_inputs_for(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwPanelV3InputsV1> {
        let state = self.palw_state_params_v2.as_ref()?;
        let mirror = state.panel_v3()?;
        if point.daa_score < mirror.from_daa || !state.rcore_plus_active_at(point.daa_score) {
            return None;
        }
        // The checkpoint's DAA: the selected parent's. A block whose GHOSTDAG record the store cannot serve resolves at its own.
        let checkpoint_daa = self
            .ghostdag_store
            .get_data(point.block)
            .ok()
            .and_then(|data| self.headers_store.get_header(data.selected_parent).ok())
            .map(|header| header.daa_score)
            .unwrap_or(point.daa_score);
        let (approved_beacons, beacon_source) = (
            kaspa_consensus_core::palw_panel_beacon_v1::approved_panel_beacon_policies_v1(),
            kaspa_consensus_core::palw_state_v2::PalwPanelV3BeaconSourceV1::Chain,
        );
        #[cfg(test)]
        let (approved_beacons, beacon_source) = self.panel_v3_test_beacon.lock().clone().unwrap_or((approved_beacons, beacon_source));
        Some(kaspa_consensus_core::palw_state_v2::PalwPanelV3InputsV1 {
            draw: self.palw_panel_draw_policy_at(checkpoint_daa),
            capability_proof: self.palw_capability_bound_at(checkpoint_daa),
            floor_class: state.base_class_id(),
            approved_beacons,
            beacon_source,
        })
    }

    /// **ADR-0089 Decision 9, resolved in exactly one place, at the BLOCK's own DAA.**
    pub(in crate::pipeline::virtual_processor) fn palw_model_evm_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_evm.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0089: the three fences the executor reads, at one DAA.
    #[cfg(feature = "evm")]
    pub(in crate::pipeline::virtual_processor) fn palw_evm_market_fences_at(
        &self,
        daa_score: u64,
    ) -> kaspa_consensus_core::evm::model_market::PalwEvmMarketFencesV1 {
        kaspa_consensus_core::evm::model_market::PalwEvmMarketFencesV1 {
            market_active: self.palw_model_market_active_at(daa_score),
            lines_active: self.palw_model_lines_active_at(daa_score),
            evm_active: self.palw_model_evm_active_at(daa_score),
            leg_v2_active: self.palw_model_leg_v2_active_at(daa_score),
            seed_v2_active: self.palw_model_seed_v2_active_at(daa_score),
            audit_2026_09_23_active: self.palw_audit_2026_09_23_at(daa_score),
            virtual_v1_from: self.palw_model_virtual_v1_from_at(daa_score),
        }
    }

    /// **RFC-0009 L2: the root a header commits for `state`** — the ONE spelling the chain walk, the template, the proof server, the
    /// pruning-point import and the native-settlement walk share (`palw_fork_choice_commitment_v1`). Below the fence (every shipped
    /// network) it is `state.state_root()`.
    pub(crate) fn palw_committed_root_v1(&self, state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2) -> Hash64 {
        // RFC-0018 GAP-R4: the form the state's own point selects — flat, leaf v1, rule-E leaf v2, or (rule E with rule E v2 / Gate C)
        // leaf v3 — under the node's own ruleset. Equal to `palw_committed_state_root_v1` wherever rule E is not in force (every
        // shipped network: the commitment fence itself is `None` there, so this is `state.state_root()`).
        kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::palw_committed_state_root_v2(state, &self.native_redemption_params_v33)
    }

    /// **G14 lane D: `Params::palw_probabilistic_constraints_v1` resolved at the block's DAA**, in exactly one place.
    pub(in crate::pipeline::virtual_processor) fn palw_kernel_route_at(&self, daa_score: u64) -> bool {
        self.palw_probabilistic_constraints_v1
            .is_some_and(|fence| fence != kaspa_consensus_core::config::params::ForkActivation::never() && fence.is_active(daa_score))
    }

    /// **Lane DA16: `Params::palw_provider_court_v1` resolved at the block's DAA**, in exactly one place (the kernel route's fence must be in
    /// force too: the court's rows are the route's).
    pub(in crate::pipeline::virtual_processor) fn palw_provider_court_at(&self, daa_score: u64) -> bool {
        self.palw_kernel_route_at(daa_score)
            && self.palw_provider_court_v1.is_some_and(|fence| {
                fence != kaspa_consensus_core::config::params::ForkActivation::never() && fence.is_active(daa_score)
            })
    }

    /// **Lane LG14-B: `Params::palw_legacy_held_da_v2` resolved at the block's DAA**, in exactly one place. The legacy held DA rides
    /// R-core+'s DA court, so `palw_rcore_plus` must be in force too.
    pub(in crate::pipeline::virtual_processor) fn palw_legacy_held_da_v2_at(&self, daa_score: u64) -> bool {
        self.palw_legacy_held_da_v2
            .is_some_and(|fence| fence != kaspa_consensus_core::config::params::ForkActivation::never() && fence.is_active(daa_score))
    }

    /// **Lane LG14-B: a legacy held object's acceptance** (tags 157–159): the fence, the named bond on this chain, its ML-DSA-87
    /// signature over the object's message under its context, and the ruleset's close ceiling on what it carries. The claim's roots,
    /// the units, the answers and the verdict are the fold's.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn palw_legacy_held_signature_ok(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
        claim: &kaspa_hashes::Hash64,
        signer: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        message: &kaspa_hashes::Hash64,
        signature: &[u8],
        context: &[u8],
        bytes: u64,
    ) -> Result<(), String> {
        if !self.palw_legacy_held_da_v2_at(daa_score) {
            return Err(format!(
                "claim {claim}: a legacy held object is refused: palw_legacy_held_da_v2 is not in force at this block (LG14-B)"
            ));
        }
        let court = self
            .palw_court_params_v2
            .as_ref()
            .ok_or_else(|| "a legacy held object on a network with no V2 court parameters".to_string())?;
        if bytes > court.max_close_bytes() {
            return Err(format!(
                "claim {claim}: the legacy held object carries {bytes} bytes and this ruleset prices a close at {}",
                court.max_close_bytes()
            ));
        }
        let record =
            state.bond(signer).ok_or_else(|| format!("a legacy held object names bond {signer:?} this chain does not have"))?;
        if !Self::verify_mldsa87_with_context_bool(&record.pubkey, message.as_byte_slice(), signature, context) {
            return Err(format!("claim {claim}: the legacy held object is not signed by the bond it names"));
        }
        Ok(())
    }

    /// **Lane BUDGET: `Params::palw_model_bond_allocation_v1` resolved at the block's DAA**, in exactly one place (the bond budget's fence
    /// must be in force too: an assignment's rows are the budget's).
    pub(in crate::pipeline::virtual_processor) fn palw_model_bond_allocation_at(&self, daa_score: u64) -> bool {
        let at = |fence: Option<kaspa_consensus_core::config::params::ForkActivation>| {
            fence.is_some_and(|f| f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(daa_score))
        };
        at(self.palw_bond_budget_v1) && at(self.palw_model_bond_allocation_v1)
    }

    /// **Lane BUDGET: a capital assignment's acceptance** (tag 140): the allocation fence, an Active signer bond, and the bond's ML-DSA-87
    /// signature over `(network, bond, assignments, sequence)`. The rules of ADR-0177 D3 (canonical entries, registered models, the
    /// capital, the sequence) are the fold's.
    pub(super) fn palw_capital_assignment_signature_ok(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
        bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        assignments: &[(kaspa_hashes::Hash64, u64)],
        sequence: u64,
        signature: &[u8],
    ) -> Result<(), String> {
        if !self.palw_model_bond_allocation_at(daa_score) {
            return Err(
                "a capital assignment is refused: palw_model_bond_allocation_v1 is not in force at this block (ADR-0177)".to_string()
            );
        }
        let record =
            state.bond(bond).ok_or_else(|| "a capital assignment is signed by a bond this chain does not have".to_string())?;
        if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err("a capital assignment is signed by a bond that is not Active".to_string());
        }
        let message = kaspa_consensus_core::palw_bond_budget_v1::palw_capital_assignment_message_v1(
            self.palw_network_domain_v2(),
            bond,
            assignments,
            sequence,
        );
        if !Self::verify_mldsa87_with_context_bool(
            &record.pubkey,
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_bond_budget_v1::PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1,
        ) {
            return Err("a capital assignment carries a signature its bond's key does not verify".to_string());
        }
        Ok(())
    }

    /// **Lane DA16: a provider-court object's acceptance** (tags 150–153): the court's fence, an Active signer bond, and the signer's
    /// ML-DSA-87 signature over `(network, kind, signer, payload)`. The rows, the units and the verdicts are the fold's.
    pub(super) fn palw_provider_court_signature_ok(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
        kind: u8,
        signer: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        payload: &[u8],
        signature: &[u8],
    ) -> Result<(), String> {
        if !self.palw_provider_court_at(daa_score) {
            return Err("a provider-court object is refused: palw_provider_court_v1 is not in force at this block (DA16)".to_string());
        }
        let record =
            state.bond(signer).ok_or_else(|| "a provider-court object is signed by a bond this chain does not have".to_string())?;
        if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err("a provider-court object is signed by a bond that is not Active".to_string());
        }
        let message = kaspa_consensus_core::palw_provider_court_v1::palw_provider_court_message_v1(
            self.palw_network_domain_v2(),
            kind,
            signer,
            payload,
        );
        if !Self::verify_mldsa87_with_context_bool(
            &record.pubkey,
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_provider_court_v1::PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1,
        ) {
            return Err("a provider-court object carries a signature its bond's key does not verify".to_string());
        }
        Ok(())
    }

    /// **RFC-0009 G-EXPIRY / G-RULESET: `Params::palw_signed_registration_v1` resolved at the block's DAA**, in exactly one place.
    pub(in crate::pipeline::virtual_processor) fn palw_signed_registration_at(&self, daa_score: u64) -> bool {
        self.palw_signed_registration_v1
            .is_some_and(|fence| fence != kaspa_consensus_core::config::params::ForkActivation::never() && fence.is_active(daa_score))
    }

    /// **Unwrap a signed-registration envelope (tag 108)** into the class registration it wraps, or say why it is dropped: the fence in
    /// force, the block inside the envelope's validity window, the fork-id fired digest at the block's DAA (and at the window's start)
    /// being the one the signer named, the wrapped object a BOUGHT class registration of the signer's own bond, and the signer's
    /// ML-DSA-87 signature over `(network, fork digest, valid_from, valid_until, signer, registration)`. The wrapped registration then
    /// meets every rule it would meet carried bare — including its own signature.
    ///
    /// **F-C4R3-01(b)**: the digest is `fork_id_v1(params, daa).fired`, not `consensus_params_id` — two builds that differ only in a
    /// fence not yet fired (a rollout) compute the same digest and agree on the envelope; a fence that fires inside the window expires it.
    pub(super) fn palw_signed_registration_unwrap(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
        envelope: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, String> {
        use kaspa_consensus_core::palw_onboarding_v1::palw_envelope_fork_digest_v1;
        use kaspa_consensus_core::palw_state_v2::{PalwBondStatusV2, PalwConsensusObjectV2 as Obj, palw_class_registration_buyer_v1};
        let Obj::SignedRegistrationV1 { registration, valid_from_daa, valid_until_daa, fork_digest, signer, signature } = envelope
        else {
            return Err("not a signed registration envelope".to_string());
        };
        if !self.palw_signed_registration_at(daa_score) {
            return Err("palw_signed_registration_v1 is not in force at this block".to_string());
        }
        if daa_score > *valid_until_daa {
            return Err(format!("expired: valid until DAA {valid_until_daa}, the block is at {daa_score} (G-EXPIRY)"));
        }
        if daa_score < *valid_from_daa {
            return Err(format!("not yet valid: valid from DAA {valid_from_daa}, the block is at {daa_score}"));
        }
        let genesis = self.genesis.hash;
        if palw_envelope_fork_digest_v1(genesis, &self.palw_fence_schedule, *valid_from_daa) != *fork_digest {
            return Err("signed under another fork schedule than this chain's at its own DAA (G-RULESET)".to_string());
        }
        if palw_envelope_fork_digest_v1(genesis, &self.palw_fence_schedule, daa_score) != *fork_digest {
            return Err("a fence fired since the envelope was signed: it expires across it (G-RULESET)".to_string());
        }
        if palw_class_registration_buyer_v1(registration) != Some(*signer) {
            return Err("the envelope wraps no bought class registration of its signer".to_string());
        }
        let record = state.bond(signer).ok_or_else(|| "the envelope is signed by a bond this chain does not have".to_string())?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err("the envelope is signed by a bond that is not Active".to_string());
        }
        let bytes = borsh::to_vec(registration.as_ref()).map_err(|e| e.to_string())?;
        let message = kaspa_consensus_core::palw_onboarding_v1::palw_signed_registration_message_v1(
            self.palw_network_domain_v2(),
            *fork_digest,
            *valid_from_daa,
            *valid_until_daa,
            signer,
            &bytes,
        );
        if !Self::verify_mldsa87_with_context_bool(
            &record.pubkey,
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_onboarding_v1::PALW_SIGNED_REGISTRATION_MLDSA87_CONTEXT_V1,
        ) {
            return Err("the envelope's signature does not verify under its signer's key".to_string());
        }
        Ok((**registration).clone())
    }

    /// **An onboarding object's acceptance** (tags 104-107, 109): the route's fence, an Active signer bond, and the signer's ML-DSA-87
    /// signature over `(network, kind, signer, payload)`.
    pub(super) fn palw_onboarding_signature_ok(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
        kind: u8,
        signer: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        payload: &[u8],
        signature: &[u8],
    ) -> Result<(), String> {
        if !self.palw_kernel_route_at(daa_score) {
            return Err(
                "an onboarding object is refused: palw_probabilistic_constraints_v1 is not in force at this block (G14)".to_string()
            );
        }
        let record =
            state.bond(signer).ok_or_else(|| "an onboarding object is signed by a bond this chain does not have".to_string())?;
        if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
            return Err("an onboarding object is signed by a bond that is not Active".to_string());
        }
        let message =
            kaspa_consensus_core::palw_onboarding_v1::palw_onboarding_message_v1(self.palw_network_domain_v2(), kind, signer, payload);
        if !Self::verify_mldsa87_with_context_bool(
            &record.pubkey,
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_MLDSA87_CONTEXT_V1,
        ) {
            return Err("an onboarding object carries a signature its bond's key does not verify".to_string());
        }
        Ok(())
    }

    /// **RFC-0015: the network's OPV fence, resolved in ONE place** — `None` where `Params::palw_panel_free_v1` is absent (every preset,
    /// and `Some(never())`, which the params collapse). The policy is a genesis constant, so whether the network declares it is a
    /// property of the ruleset and never of the block; the fence's activation decides from which DAA it acts.
    pub(super) fn palw_kernel_opv_fence(&self) -> Option<&kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1> {
        self.palw_panel_free_v1
            .as_ref()
            .filter(|fence| fence.activation != kaspa_consensus_core::config::params::ForkActivation::never())
    }

    /// Is the OPV fence in force at `daa_score` (a registration under the mode, tags 13 / 14, may be carried)?
    pub(in crate::pipeline::virtual_processor) fn palw_kernel_opv_at(&self, daa_score: u64) -> bool {
        self.palw_kernel_opv_fence().is_some_and(|fence| fence.activation.is_active(daa_score))
    }

    /// **RFC-0004 Part II: the typed-roots fence's activation, resolved in ONE place** — a genesis constant of the route (the height from
    /// which its schedule holds `K2-TR-v1` Active), `None` where `Params::palw_typed_roots_v1` is absent or `never()`.
    pub(super) fn palw_kernel_typed_roots_activation(&self) -> Option<u64> {
        self.palw_typed_roots_v1
            .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|fence| fence.daa_score())
    }

    /// Is the typed-roots fence in force at `daa_score` (a `Spec` object, inner kind 19, may be carried)?
    pub(in crate::pipeline::virtual_processor) fn palw_kernel_typed_roots_at(&self, daa_score: u64) -> bool {
        self.palw_kernel_typed_roots_activation().is_some_and(|at| daa_score >= at)
    }

    /// What the kernel route's fold is handed where the fence is in force: the network, the ruleset and the artifact attestations.
    /// **There is no on-chain artifact attestation yet** (onboarding conformance + availability: GAP), so outside a test the list is
    /// empty and a kernel class can never register; a test's hook (`kernel_route_test_attest_artifact_v1`, `cfg(test)`) fills it.
    pub(super) fn palw_kernel_route_extras_at(
        &self,
        daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteExtrasV1> {
        self.palw_kernel_route_at(daa_score).then(|| {
            let mut ruleset = [0u8; 64];
            ruleset[..32].copy_from_slice(&self.palw_kernel_route_ruleset_id.as_bytes());
            let native_mass = (self.palw_kernel_inner_fence_at(
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::FundedExecutionV11,
                daa_score,
            ) || self.palw_kernel_inner_fence_at(
                kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::StageExecutionV14,
                daa_score,
            ) || self.palw_stage_execution_v28.as_ref().is_some_and(|f| {
                f.activation != kaspa_consensus_core::config::params::ForkActivation::never() && f.activation.is_active(daa_score)
            }) && self.palw_native_final_v27.as_ref().is_some_and(|f| {
                f.activation != kaspa_consensus_core::config::params::ForkActivation::never() && f.activation.is_active(daa_score)
            }) && self.palw_kernel_opv_at(daa_score))
            .then(|| self.native_carriage_mass.clone());
            let native_final = self
                .palw_native_final_v27
                .as_ref()
                .filter(|f| {
                    f.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                        && f.activation.is_active(daa_score)
                        && self.palw_kernel_opv_at(daa_score)
                })
                .cloned();
            let native_service = native_mass.as_ref().map(|mass| {
                native_final
                    .as_ref()
                    .and_then(|f| {
                        kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(
                            mass,
                            &f.policy.queue_policy,
                        )
                        .ok()
                    })
                    .unwrap_or_else(|| kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::probe(mass))
            });
            kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: self.palw_network_domain_v2(),
                native_carriage_mass: native_mass,
                native_carriage_service: native_service,
                native_final,
                stage_execution_v28: self
                    .palw_stage_execution_v28
                    .as_ref()
                    .filter(|f| {
                        f.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                            && f.activation.is_active(daa_score)
                            && self.palw_kernel_opv_at(daa_score)
                            && self.palw_native_final_v27.as_ref().is_some_and(|n| {
                                n.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                                    && n.activation.is_active(daa_score)
                            })
                    })
                    .cloned(),
                native_execution39: self
                    .palw_native_execution_v39
                    .as_ref()
                    .filter(|f| {
                        f.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                            && f.activation.is_active(daa_score)
                            && self.palw_kernel_opv_at(daa_score)
                            && self.palw_native_final_v27.as_ref().is_some_and(|n| {
                                n.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                                    && n.activation.is_active(daa_score)
                            })
                    })
                    .cloned(),
                // Owning39 mining: its own fence AND native39 AND Final27 AND OPV (every preset: None).
                native_mining39: self
                    .palw_native_mining_v39
                    .as_ref()
                    .filter(|f| {
                        f.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                            && f.activation.is_active(daa_score)
                            && self.palw_native_mining_v39_lanes_at(daa_score)
                    })
                    .cloned(),
                stage_execution: self
                    .palw_stage_execution_v14
                    .as_ref()
                    .filter(|f| {
                        f.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                            && f.activation.is_active(daa_score)
                            && self.palw_kernel_opv_at(daa_score)
                    })
                    .cloned(),
                funded_execution: self
                    .palw_funded_execution_v11
                    .as_ref()
                    .filter(|f| {
                        f.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                            && f.activation.is_active(daa_score)
                            && self.palw_kernel_opv_at(daa_score)
                    })
                    .cloned(),
                ruleset_digest: kaspa_hashes::Hash64::from_bytes(ruleset),
                chain_genesis: self.genesis.hash,
                #[cfg(test)]
                attested_artifacts: kernel_route_test_attestations_v1(daa_score),
                #[cfg(not(test))]
                attested_artifacts: Vec::new(),
                // Tests run with a four-adjudication block so the cross-object budget is reachable with a handful of carriers.
                #[cfg(test)]
                max_adjudications_per_block: Some(4),
                #[cfg(not(test))]
                max_adjudications_per_block: None,
                // RFC-0008 v2 (X8R): the composed run's seam (`exec_v2_test_admit_class_v1`); nothing outside a test.
                #[cfg(test)]
                test_admitted_classes: exec_v2_test_admitted_classes_v1(daa_score),
                #[cfg(not(test))]
                test_admitted_classes: Vec::new(),
                opv: self.palw_kernel_opv_fence().map(|fence| kaspa_consensus_core::palw_kernel_route_v1::PalwKernelOpvExtrasV1 {
                    policy: fence.opv_policy(),
                    denied_classes: fence.denied_classes.clone(),
                    min_effective_bits: fence.min_effective_bits,
                    sampled_conformance_gates_reward: fence.sampled_conformance_gates_reward,
                    #[cfg(test)]
                    test_eligible: kernel_route_test_opv_eligible_list_v1(),
                    #[cfg(not(test))]
                    test_eligible: Vec::new(),
                    reward_gate: fence.activation.is_active(daa_score).then(|| {
                        kaspa_consensus_core::palw_kernel_route_v1::PalwRewardGateTermsV1 {
                            fence_activation_daa: fence.activation.daa_score(),
                        }
                    }),
                    // RFC-0015 §8.3 (lead default 2026-10-15): every network with an OPV policy budgets every Panel-free claim.
                    budget_required: true,
                }),
                typed_roots: self.palw_kernel_typed_roots_activation(),
                beacon_source: self.palw_beacon_source_opt_in_v1.filter(|f| {
                    f.activation != kaspa_consensus_core::config::params::ForkActivation::never() && f.activation.is_active(daa_score)
                }),
                beacon_source_fee: self.palw_beacon_source_fee_v1.filter(|f| {
                    f.activation != kaspa_consensus_core::config::params::ForkActivation::never() && f.activation.is_active(daa_score)
                }),
                // DA16: the provider court's activation, where its (never armable) fence is in force at this block.
                provider_court: self
                    .palw_provider_court_at(daa_score)
                    .then(|| self.palw_provider_court_v1.map(|f| f.daa_score()))
                    .flatten(),
                commitment_scope: self.palw_court_commitment_scope_v1.filter(|at| at.is_active(daa_score)).map(|at| at.daa_score()),
                // G14R round 3: M*-49 / O2's dormant fence (refused when armed; a test arms it through the Config seam).
                verifier_pay: self
                    .palw_verifier_pay_v1
                    .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never())
                    .map(|f| f.daa_score()),
                // RFC-0002 P0 (RFC02-T): the protected candidate refutation's dormant fence, where it is in force at this block.
                model_refutation_seal: self
                    .palw_model_refutation_seal_v1
                    .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(daa_score))
                    .map(|f| f.daa_score()),
                window_class_v37: self
                    .palw_window_class_v37
                    .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(daa_score))
                    .map(|f| f.daa_score()),
            }
        })
    }

    /// Derive each carried fused class's exact finite window before the block fold. Missing or
    /// uncomputable values are intentionally omitted; the transition then refuses that registration
    /// under the active fence instead of silently reverting to the network-wide window.
    pub(super) fn palw_class_court_windows_for_objects(
        &self,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
        daa_score: u64,
    ) -> std::collections::BTreeMap<kaspa_hashes::Hash64, u64> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        let mut windows = std::collections::BTreeMap::new();
        if !self.palw_model_court_window_active_at(daa_score) || !self.palw_kary_court_active_at(daa_score) {
            return windows;
        }
        let Some(bundle) = self.palw_v2_bundle.as_ref() else { return windows };
        let Some(Ok(court)) = self.palw_court_params_at(daa_score) else { return windows };
        for object in objects {
            match object {
                Obj::ClassRegistered { class_id, admission: Some(carriage), .. }
                    if kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&carriage.profile) =>
                {
                    if let Ok(window) = kaspa_consensus_core::palw_class_admission_v2::palw_class_court_window_for_shape_v1(
                        bundle.state.window_court(),
                        true,
                        self.palw_held_context_at(daa_score),
                        &court,
                        &carriage.profile,
                    ) {
                        windows.insert(*class_id, window);
                    }
                }
                Obj::ClassRegisteredTirV1 { class_id, admission, .. } => {
                    if let Ok(window) = kaspa_consensus_core::palw_class_admission_v2::palw_court_window_for_history_v1(
                        bundle.state.window_court(),
                        true,
                        self.palw_held_context_at(daa_score),
                        &court,
                        admission.class.layout.max_context as u64,
                        admission.class.layout.h_tile.max(1),
                    ) {
                        windows.insert(*class_id, window);
                    }
                }
                _ => {}
            }
        }
        windows
    }

    /// **ADR-0072 Decision 8's free-prompt half, resolved in exactly one place, at the BLOCK's own
    /// DAA.** The retention obligation a free-prompt claim records is a function of the block that
    /// accepted it, so two nodes folding one block must read one answer — the reason every fence
    /// beside this one reads the block's score and not the tip's.
    pub(super) fn palw_fp_da_pins_at(&self, daa_score: u64) -> bool {
        self.palw_fp_da_pins.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_objective_offence_at(&self, daa_score: u64) -> bool {
        self.palw_objective_offence.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_execution_quanta_at(&self, daa_score: u64) -> bool {
        self.palw_execution_quanta.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_economic_safety_at(&self, daa_score: u64) -> bool {
        self.palw_economic_safety.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **Whether a round's permits are its schedule's tickets alone at `daa_score`** (the 2026-09-23
    /// route-matrix audit's #2): ADR-0151's bundle and the quanta both armed. Every reader of a
    /// schedule's permits — the verdict, the equivocation check and the producer's view — asks this
    /// one question, so an armed mint that issued nothing grants nothing at all of them, and a network
    /// that arms the quanta alone (testnet-11 at 7,800) keeps its lottery fallback byte for byte.
    pub(in crate::pipeline::virtual_processor) fn palw_round_permits_are_tickets_at(&self, daa_score: u64) -> bool {
        self.palw_economic_safety_at(daa_score) && self.palw_execution_quanta_at(daa_score)
    }

    pub(super) fn palw_objective_offence_daa(&self) -> Option<u64> {
        self.palw_objective_offence
            .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|fence| fence.daa_score())
    }

    pub(super) fn palw_seat_gate_possession_daa(&self) -> Option<u64> {
        self.palw_seat_gate_possession
            .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|fence| fence.daa_score())
    }

    /// **ADR-0107, resolved in exactly one place, at the BLOCK's own DAA** — the block that
    /// crosses an epoch boundary decides that boundary's growth, so two nodes folding it must
    /// read one answer.
    pub(super) fn palw_share_growth_final_at(&self, daa_score: u64) -> bool {
        self.palw_share_growth_final.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0123's budget release at `daa_score` — the one resolver every budget reader shares.
    pub(in crate::pipeline::virtual_processor) fn palw_epoch_budget_release_at(&self, daa_score: u64) -> bool {
        self.palw_epoch_budget_release.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// Resolve both epoch-budget fences at one block's DAA. Keeping them together at the
    /// admission boundary makes it impossible for a caller to transpose the two policies while
    /// forwarding adjacent booleans.
    pub(super) fn palw_epoch_budget_fences_at(
        &self,
        daa_score: u64,
    ) -> kaspa_consensus_core::palw_admission_v2::PalwEpochBudgetFencesV1 {
        kaspa_consensus_core::palw_admission_v2::PalwEpochBudgetFencesV1 {
            native_material_ticket_v3: None,
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
            // ADR-0152 SR-7: the RAW second-clock depth, so item 8 reads the one committed ledger at
            // the escaped depth exactly as the fold's ceiling does (read only past `palw_rcore_plus`).
            settled_anchor_depth: self.palw_settled_anchor_depth_at(daa_score),
            // Lane V02 (post-launch): the extras pair the fold's held charge reads, so item 8 counts a
            // held dissection at its charge past `palw_final_lock_full_collateral` as `gate_room` does.
            held_accuser_charge_active: self.palw_held_accuser_charge_at(daa_score),
            // Lane bind-deadlock: enforced here; only the binder re-check
            // (`palw_v2_anchor_at_ceiling_binder_v1`) waives it.
            exposure_ceiling_waived: false,
            emission_waived: false,
        }
    }

    /// **Lane V02: whether a block at `daa_score` folds held dissections at their charge** — exactly
    /// `extras.offence_attribution_active && extras.held_context_ladder.is_some()` of
    /// [`Self::palw_transition_extras_at`] (the ladder is `Some` iff the held fence is active and the
    /// court params exist), the pair the fold's `held_charge_block_active_v1` reads.
    pub(in crate::pipeline::virtual_processor) fn palw_held_accuser_charge_at(&self, daa_score: u64) -> bool {
        self.palw_offence_attribution_at(daa_score) && self.palw_held_context_at(daa_score) && self.palw_court_params_v2.is_some()
    }

    /// **ADR-0149 §5: the floor class's derived draw as the registry will write it** — its entry in
    /// [`Self::palw_known_model_works_v1`], the map `palw_model_registry_fold_at` hands the fold as
    /// `genesis_works` and `step_model_registry` copies into the floor's row. One table, so the
    /// admission, the producer's facts and the fold price the floor's row-less attempts alike.
    pub(super) fn palw_base_known_draw_v1(&self) -> Option<u128> {
        let base = self.palw_state_params_v2.as_ref()?.base_class_id();
        self.palw_known_model_works_v1().get(&base).map(|work| work.economic_ccu_per_claim).filter(|draw| *draw > 0)
    }

    /// ADR-0132 S: whether the single lottery is in force at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_verification_v2_at(&self, daa_score: u64) -> bool {
        self.palw_verification_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(in crate::pipeline::virtual_processor) fn palw_verification_s3_at(&self, daa_score: u64) -> bool {
        self.palw_verification_s3.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(in crate::pipeline::virtual_processor) fn palw_verification_s2_at(&self, daa_score: u64) -> bool {
        self.palw_verification_s2.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0133 §11.2: whether the whole-artifact possession proof is in force at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_readiness_v2_at(&self, daa_score: u64) -> bool {
        self.palw_readiness_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(in crate::pipeline::virtual_processor) fn palw_single_lottery_at(&self, daa_score: u64) -> bool {
        self.palw_single_lottery.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **The fused terminal's responder-coverage rule, resolved in exactly one place** — the reason
    /// its neighbours give: the object-acceptance rehearsal, the fold and the state walk must agree
    /// about whether it is in force at THIS block, because one folding what another does not is two
    /// rules wearing one name.
    pub(super) fn palw_court_responder_coverage_at(&self, daa_score: u64) -> bool {
        self.palw_court_responder_coverage.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0084 §7.5, resolved in exactly one place, at the BLOCK's acceptance DAA.**
    ///
    /// `pub(super)` so `tests.rs` can assert that this reads `palw_court_ladder` and not its
    /// neighbour: the 2026-09-06 merge left two ladder fences on `Params` and
    /// `git grep palw_refutation_leaf_cap` showed this site had no test at all, so nothing in the
    /// tree observed which field a card had to arm (mainnet audit 2026-09-06, M-15).
    pub(in crate::pipeline::virtual_processor) fn palw_court_step_ladder_at(
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
    pub(super) fn palw_bond_withdrawal_delay_at(&self, daa_score: u64) -> u64 {
        match (self.palw_bond_params_v2.as_ref(), self.palw_v2_bundle.as_ref()) {
            (Some(_), Some(bundle)) => {
                kaspa_consensus_core::config::params::palw_v2_bond_withdrawal_delay_at_v1(bundle, self.palw_da_court, daa_score)
            }
            (Some(bond_params), None) => bond_params.withdrawal_delay_daa(),
            (None, _) => 0,
        }
    }

    pub(super) fn palw_da_court_at(&self, daa_score: u64) -> bool {
        self.palw_da_court.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0103, resolved in exactly one place.**
    pub(in crate::pipeline::virtual_processor) fn palw_held_context_at(&self, daa_score: u64) -> bool {
        self.palw_held_context.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **RFC-0001 §A.4, resolved in exactly one place**: which free-prompt job versions the
    /// extraction walk admits at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_fp_decode_rules_at(
        &self,
        daa_score: u64,
    ) -> kaspa_consensus_core::palw_freeprompt_v3::PalwFpDecodeRulesV1 {
        kaspa_consensus_core::palw_freeprompt_v3::PalwFpDecodeRulesV1::at(
            self.palw_fp_decode_rules
                .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
                .map(|fence| fence.daa_score()),
            daa_score,
        )
    }

    /// **ADR-0099 Decision 5 / ADR-0100, resolved in exactly one place**, for the DA court's
    /// reason. The fold does not read this: it reads the ladder [`Self::palw_transition_extras_at`]
    /// carries, which is `Some` exactly when this is true.
    pub(super) fn palw_shard_court_at(&self, daa_score: u64) -> bool {
        self.palw_shard_court.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_memory_routing_court_v2_at(&self, daa_score: u64) -> bool {
        self.palw_memory_routing_court_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0191 step B: `palw_tir_designated_answers_v1`'s mirror, the same shape as tag 244's fence.
    pub(super) fn palw_tir_designated_answers_v1_at(&self, daa_score: u64) -> bool {
        self.palw_tir_designated_answers_v1.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0100 Decision 4, resolved in exactly one place.**
    pub(super) fn palw_shard_licensing_at(&self, daa_score: u64) -> bool {
        self.palw_shard_licensing.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0102, resolved in exactly one place.**
    pub(super) fn palw_token_lift_at(&self, daa_score: u64) -> bool {
        self.palw_token_lift.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_kimi_k3_at(&self, daa_score: u64) -> bool {
        self.palw_kimi_k3.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **RFC-0002 Phase F, resolved in exactly one place.**
    pub(super) fn palw_tir_at(&self, daa_score: u64) -> bool {
        self.palw_tir_v1.is_some_and(|fence| fence.activation.is_active(daa_score))
    }

    /// **RFC-0003, resolved in exactly one place.**
    pub(super) fn palw_gen_at(&self, daa_score: u64) -> bool {
        self.palw_gen_v1.is_some_and(|fence| fence.activation.is_active(daa_score))
    }

    /// **Is the task-head profile in force at `daa_score`?** (`Params::palw_task_heads_v1`) — never on a shipped preset.
    pub(super) fn palw_task_heads_at(&self, daa_score: u64) -> bool {
        self.palw_task_heads_v1.is_some_and(|fence| fence.activation.is_active(daa_score))
    }

    /// **RFC-0003 §I.4: is the free-prompt lane open for tensor claims at `daa_score`?** Both fences in
    /// force — `palw_gen_v1` (the registry, the court) and `palw_fp_job_v5` (the lane's job versions
    /// past V4): `palw_gen_lane_open_v1`, resolved here in exactly one place. The extraction walk reads
    /// it at the accepting block; below it no tensor claim becomes an object.
    pub(in crate::pipeline::virtual_processor) fn palw_gen_lane_open_at(&self, daa_score: u64) -> bool {
        self.palw_gen_at(daa_score)
            && self
                .palw_fp_job_v5
                .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
                .is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **RFC-0002 Phase F's second IR fence, read off the bundle's mirror** (`tir_fence2_from_daa`,
    /// which `validate_palw_v2` holds equal to `Params::palw_tir_fence2`).
    pub(super) fn palw_tir_fence2_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.tir_fence2_active_at(daa_score))
    }

    /// **RFC-0006's layer-sharded panels, read off the bundle's mirror** (`tir_shard_from_daa`, which `validate_palw_v2` holds
    /// equal to `Params::palw_tir_shard_v1`).
    pub(super) fn palw_tir_shard_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.tir_shard_active_at(daa_score))
    }

    /// **Whether a claim's panel is drawn per LAYER shard, and into how many** (RFC-0006) — the ONE decision the binding and its
    /// validation share, beside [`Self::palw_stratified_shard_count`]: the fence at the claim's ANCHOR (the panel is a pure
    /// function of the claim) and a plan its IR class declared.
    pub(super) fn palw_tir_shard_count(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        class_id: &kaspa_hashes::Hash64,
        anchor_daa: u64,
    ) -> Option<u16> {
        if !self.palw_tir_shard_at(anchor_daa) {
            return None;
        }
        state.tir_shard_plan(class_id).map(|plan| plan.s_l)
    }

    /// **RFC-0004's improvement fence, read off the bundle's mirror** (`improve_from_daa`, which
    /// `validate_palw_v2` holds equal to `Params::palw_improvement_v1`).
    pub(super) fn palw_improvement_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.improve_active_at(daa_score))
    }

    /// **Pipeline-claim data availability** (spec 17 §17.14), resolved in exactly one place
    /// (`PalwStateParamsV2::pipeline_da_active_at`): the acceptance walk, the gate and the fold must agree.
    pub(super) fn palw_pipeline_da_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.pipeline_da_active_at(daa_score))
    }

    /// **RFC-0003 decision 22's held leaf challenge, read off the bundle's mirror**
    /// (`held_close_chunks_from_daa`, which `validate_palw_v2` holds equal to `Params::palw_held_close_chunks_v1`).
    pub(super) fn palw_fp_decode_constraint_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.fp_decode_constraint_active_at(daa_score))
    }

    pub(super) fn palw_adapter_class_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.adapter_class_active_at(daa_score))
    }

    pub(super) fn palw_held_close_chunks_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.held_close_chunks_active_at(daa_score))
    }

    /// **RFC-0007 Part I's verification vertex, read off the bundle's mirror**
    /// (`vertex_from_daa`, which `validate_palw_v2` holds equal to `Params::palw_verification_vertex_v1`).
    /// **Lane PA, the beacon of `palw_audit_1004_v1`**: the hash of the selected-chain block
    /// [`kaspa_consensus_core::palw_audit_1004_v1::PALW_AUDIT_1004_BEACON_DEPTH_V1`] below (and including) `selected_parent` — fixed by the
    /// parents the accepting block builds on, so its producer cannot grind it. `None` below the fence and when the chain is shorter
    /// than the depth (the first blocks after genesis keep the seed they had).
    pub(super) fn palw_audit_1004_draw_seed_source(&self, selected_parent: BlockHash, daa_score: u64) -> Option<kaspa_hashes::Hash64> {
        if !self.palw_state_params_v2.as_ref().is_some_and(|params| params.audit_1004_active_at(daa_score)) {
            return None;
        }
        let mut current = selected_parent;
        for _ in 1..kaspa_consensus_core::palw_audit_1004_v1::PALW_AUDIT_1004_BEACON_DEPTH_V1 {
            current = self.ghostdag_store.get_selected_parent(current).ok()?;
        }
        Some(current)
    }

    pub(super) fn palw_vertex_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.vertex_active_at(daa_score))
    }

    /// **RFC-0008 v2's unified EXEC payload, read off the bundle's mirror** (`exec_v2_from_daa`). `false` on every shipped preset.
    pub(super) fn palw_exec_v2_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.exec_v2_active_at(daa_score))
    }

    /// **RFC-0007 Part IV.1's audit mesh, read off the bundle's mirror** (`audit_mesh_from_daa`).
    pub(super) fn palw_audit_mesh_at(&self, daa_score: u64) -> bool {
        self.palw_state_params_v2.as_ref().is_some_and(|params| params.audit_mesh_active_at(daa_score))
    }

    /// **RFC-0002 Phase H, resolved in exactly one place.**
    pub(super) fn palw_tir_only_at(&self, daa_score: u64) -> bool {
        self.palw_tir_only_v1.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **P0a, resolved in exactly one place.**
    pub(super) fn palw_gdn_key_heads_at(&self, daa_score: u64) -> bool {
        self.palw_gdn_key_heads.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0093 Decision 6, resolved in exactly one place.**
    pub(super) fn palw_fused_dissectable_at(&self, daa_score: u64) -> bool {
        self.palw_fused_dissectable.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0093 Decision 8, resolved in exactly one place.**
    pub(super) fn palw_attn_anchored_root_at(&self, daa_score: u64) -> bool {
        self.palw_attn_anchored_root.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_audit_2026_09_11_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_11.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_audit_2026_09_11_deep_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_11_deep.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(super) fn palw_audit_2026_09_23_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_23.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0152 v2 F2, resolved in exactly one place, at the BLOCK's own DAA.** Which route a
    /// false `Valid` takes — the V1 `PanelFalseValid` or `PanelFalseValidV2` — is decided by the
    /// object gate and again by the fold, and the two must read one answer: a gate that admitted
    /// the V2 kind under a fold that still read the V1 rule would drop every conviction it let in.
    pub(in crate::pipeline::virtual_processor) fn palw_offence_attribution_at(&self, daa_score: u64) -> bool {
        self.palw_offence_attribution.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0152-adjacent: the Activation Pool's terms at `daa_score`, resolved in exactly one
    /// place** — `Some` past `Params::palw_activation_pool` (genesis on testnet-12), `None` on every
    /// other network.
    pub(in crate::pipeline::virtual_processor) fn palw_activation_pool_at(
        &self,
        daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_activation_pool_v1::PalwActivationPoolTermsV1> {
        self.palw_activation_pool.filter(|pool| pool.activation.is_active(daa_score)).map(|pool| pool.terms)
    }

    /// **ADR-0152 R-core+, resolved in exactly one place.** `false` on every network but
    /// testnet-12, where it is armed at genesis (a crossing is refused at startup), so "at the anchor"
    /// and "at the block" give one answer on every chain that can run; the callers still name the
    /// DAA the rule is resolved at (SW-1: the anchor's; SW-8's one-state derivation: the block's).
    pub(in crate::pipeline::virtual_processor) fn palw_rcore_plus_at(&self, daa_score: u64) -> bool {
        self.palw_rcore_plus.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **Lane F1 (the panel seed, post-launch), resolved in exactly one place** — at a claim's
    /// ANCHOR block's DAA score ([`Self::palw_v2_anchor_fact_with_seed_v1`]). `false` on every
    /// shipped preset (the fence is dormant until armed at a post-launch height).
    pub(in crate::pipeline::virtual_processor) fn palw_panel_seed_execution_at(&self, anchor_daa: u64) -> bool {
        self.palw_panel_seed_execution.is_some_and(|fence| fence.is_active(anchor_daa))
    }

    /// **RFC-0018 §5 (`palw_panel_anchor_permissionless_v1`), at a candidate anchor's own DAA** — the V2 state params' mirror.
    /// `false` on every shipped preset.
    pub(in crate::pipeline::virtual_processor) fn palw_panel_anchor_permissionless_at(&self, anchor_daa: u64) -> bool {
        self.palw_state_params_v2
            .as_ref()
            .and_then(|params| params.rfc0018_v1())
            .is_some_and(|mirror| mirror.panel_anchor_permissionless_at(anchor_daa))
    }

    /// **Lane A (the operator anchor, post-launch), at a candidate anchor's own DAA.** `false` on every
    /// shipped preset.
    pub(in crate::pipeline::virtual_processor) fn palw_operator_anchor_at(&self, anchor_daa: u64) -> bool {
        self.palw_operator_anchor.as_ref().is_some_and(|rule| rule.active_at(anchor_daa))
    }

    /// The second clock's depth where the fence carries it; `None` is the DAA-only rule.
    pub(super) fn palw_settled_anchor_depth_at(&self, daa_score: u64) -> Option<u64> {
        if self.palw_audit_2026_09_23_at(daa_score) { self.palw_settled_anchor_depth } else { None }
    }

    /// **Lane bind-deadlock (the anchor at the ceiling, post-launch), at a chain block's own DAA.**
    /// `false` on every shipped preset.
    pub(in crate::pipeline::virtual_processor) fn palw_anchor_at_ceiling_at(&self, daa_score: u64) -> bool {
        self.palw_anchor_at_ceiling.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// The second clock's depth at `daa_score` AFTER the liveness escape
    /// (`palw_second_clock_depth_v1`, 2026-09-24 DoS audit): `None` once `state` has settled no
    /// anchor for `2 × window_court`. Read against the state the caller judges on, so the fold, the
    /// spend gate and the draw see one clock.
    pub(super) fn palw_second_clock_depth_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
    ) -> Option<u64> {
        let depth = self.palw_settled_anchor_depth_at(daa_score);
        match self.palw_state_params_v2.as_ref() {
            Some(params) => kaspa_consensus_core::palw_state_v2::palw_second_clock_depth_v1(
                depth,
                state.recent_anchor_daas(),
                daa_score,
                params.window_court(),
            ),
            None => depth,
        }
    }

    /// **ADR-0065 D1's window on both clocks** — `Params::palw_bond_maturity` widened so that the
    /// maturity floor is no later than the second clock's (`palw_settled_anchor_floor_daa_v1`): a
    /// bond may judge only once its window has elapsed AND the chain has settled `depth` anchors
    /// since it registered. ONE place, read by the validator, the assembler and the registry
    /// warning alike, so the three recompute one identical panel. Below the fence, or with fewer
    /// than `depth` anchors before the anchor (the bootstrap waiver), it is the window itself.
    pub(in crate::pipeline::virtual_processor) fn palw_bond_maturity_window_at(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        anchor_daa: u64,
    ) -> Option<u64> {
        // The one reading, shared with lane F2's retry verdict (the fold's step 2e′), which is handed the
        // window and the raw depth at the draw's DAA and reads the anchor ring off its own base.
        kaspa_consensus_core::palw_state_v2::palw_bond_maturity_window_on_state_v1(
            state,
            self.palw_state_params_v2.as_ref().map(|params| params.window_court()),
            anchor_daa,
            self.palw_bond_maturity_at(anchor_daa),
            self.palw_settled_anchor_depth_at(anchor_daa),
        )
    }

    /// **Whether a claim's panel is drawn per shard, and into how many** — the ONE decision the
    /// binding and its validation share: the fence at the claim's ANCHOR (the panel is a pure
    /// function of the claim, the reason ADR-0065 D1 and ADR-0071 SA-3 are read there too) and a
    /// plan its class declared.
    pub(super) fn palw_stratified_shard_count(
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
    pub(super) fn palw_chunk_cap_charge_at(&self, daa_score: u64) -> bool {
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
    pub(super) fn palw_chunk_completes_a_certification_v1(
        parts: Option<&std::collections::BTreeMap<u8, Vec<u8>>>,
        index: u8,
        count: u8,
        bytes: &[u8],
        group: &kaspa_consensus_core::Hash64,
        tir_active: bool,
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
            // RFC-0002 Phase F: below `palw_tir_v1` an IR drill is bytes an older build cannot decode,
            // so it is none here either — it is graded, and charged, only past the fence.
            Ok(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::FamilyCertified { evidence })
                if tir_active || !evidence.is_tir_v1() =>
            {
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
    ///
    /// **Lane maturity (post-launch, 2026-09-26)** brings the same rule forward: past
    /// `palw_bond_maturity_early` the window is `palw_bond_maturity`'s even below that fence's own
    /// height — one OR over one window (`palw_bond_maturity_window_in_force_v1`), so the two fences
    /// never disagree and there is no gap where the early one hands over. Below both it is `None`,
    /// byte for byte the released rule.
    pub(super) fn palw_bond_maturity_at(&self, daa_score: u64) -> Option<u64> {
        kaspa_consensus_core::config::params::palw_bond_maturity_window_in_force_v1(
            self.palw_bond_maturity,
            self.palw_bond_maturity_early,
            daa_score,
        )
    }

    /// **ADR-0071 SA-1..SA-4, resolved in exactly one place**, for the D1 reason: a panel is
    /// accepted only if it equals the derived one exactly, and the fold's declaration refusals
    /// have to be the acceptance layer's. A node that resolved this differently would propose
    /// panels its peers refuse and compute a state root its peers do not.
    pub(super) fn palw_capability_bound_at(&self, daa_score: u64) -> bool {
        self.palw_capability_bound.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0135: whether the model registry governs classes at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_model_registry_at(&self, daa_score: u64) -> bool {
        self.palw_model_registry.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0137: whether the work target is in force at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_work_target_at(&self, daa_score: u64) -> bool {
        self.palw_work_target.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0143: whether an artifact root has one recorded owner at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_artifact_root_ownership_at(&self, daa_score: u64) -> bool {
        self.palw_artifact_root_ownership.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// The 2026-09-19 audit: whether one operator identity backs one bond at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_operator_id_unique_at(&self, daa_score: u64) -> bool {
        self.palw_operator_id_unique.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0147 (the 2026-09-19 audit's F3): whether the block-level rules of independent admission
    /// apply at `daa_score`.
    pub(in crate::pipeline::virtual_processor) fn palw_admission_independence_at(&self, daa_score: u64) -> bool {
        self.palw_admission_independence.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0147: the fence's HEIGHT, which the claim-level rule compares against a claim's own
    /// `accepted_daa` — the draw, the binding and every licensing path read one claim at different
    /// chain points and must give it one answer. `None` where the fence is not configured.
    pub(in crate::pipeline::virtual_processor) fn palw_admission_independence_daa(&self) -> Option<u64> {
        self.palw_admission_independence
            .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|fence| fence.daa_score())
    }

    /// ADR-0145 §5/§6 at the FOLDING block's DAA — candidate-scoped, like every rule on this lane:
    /// two nodes folding one block must price it identically however far either has synced.
    pub(in crate::pipeline::virtual_processor) fn palw_fp_derived_work_at(&self, daa_score: u64) -> bool {
        self.palw_fp_derived_work.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0148: the chain's free-prompt pricing** — the canonical-work fence's height and the
    /// floor. The admission that draws a quantum's ticket and the producer's finder that looks for a
    /// winning one read the same value the fold priced the claim with.
    pub(in crate::pipeline::virtual_processor) fn palw_fp_pricing(
        &self,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwFpPricingV1> {
        self.palw_state_params_v2
            .as_ref()
            .map(|state| kaspa_consensus_core::palw_state_v2::PalwFpPricingV1::of(state, self.palw_canonical_work_daa))
    }

    /// ADR-0137: `W₀` for a block of `daa_score` paying `subsidy`, where the work target is in
    /// force — the block's escrow at the payout's rate; `None` below the fence.
    /// **The audit's #5: what a free-prompt claim accepted at `daa_score` prices its receipt rights
    /// from** — the fold's `fp_receipt_rights_inputs_v1` from the node's side: the block's worker carve
    /// and `W₀`, past `palw_audit_2026_09_23` and while the work target is in force.
    pub(in crate::pipeline::virtual_processor) fn palw_fp_receipt_rights_inputs_at(
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

    pub(in crate::pipeline::virtual_processor) fn palw_work_target_floor_for(&self, daa_score: u64, subsidy: u64) -> Option<u128> {
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
    pub(in crate::pipeline::virtual_processor) fn palw_known_model_works_v1(
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
    pub(super) fn palw_canonical_class_work_v1(
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
    pub(super) fn palw_canonical_class_work_v1(
        &self,
        _class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1> {
        None
    }

    /// ADR-0135: the fold's registry input at `daa_score` — the globals, the lane's span clock and
    /// the genesis classes' work — or `None` below the fence (and where no lane is scheduled: the
    /// registry steps at span boundaries, and `validate_palw_v2` refuses a registry without a lane).
    pub(in crate::pipeline::virtual_processor) fn palw_model_registry_fold_at(
        &self,
        daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1> {
        if !self.palw_model_registry_at(daa_score) {
            return None;
        }
        let lane = self.palw_execution_lane_at(daa_score)?;
        // The network's globals, from its bundle in one place (`palw_registry_globals_of_bundle_v1`):
        // the panel's seat count, not the global constant's (a devnet with three-seat panels needs
        // five ready seats, testnet-11 seven), and the readiness-V2 horizon every judge of a row reads
        // (`Params::palw_readiness_v2_max_age_spans`: 24 spans on testnet-12, eight elsewhere — user
        // decision 2026-09-25, readiness capacity option (a)).
        let globals = self
            .palw_v2_bundle
            .as_ref()
            .map(kaspa_consensus_core::palw_model_registry_v1::palw_registry_globals_of_bundle_v1)
            .unwrap_or(kaspa_consensus_core::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1);
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
            // Which freshness rule a readiness row is judged by at this DAA — the same resolution
            // the fold's extras carry (`readiness_v2_active`), for the readers holding only the fold.
            readiness_v2_active: self.palw_readiness_v2_at(daa_score),
            // Lane maturity-ext (post-launch, 2026-09-26): ADR-0065 D1 on the ready count and the
            // admission jury, from lane maturity's fence (MODE-folded) on — `palw_bond_maturity`'s
            // window and the second clock's raw depth at this DAA, the pair `palw_bond_maturity_window_at`
            // reads for the draw. `None` below the fence and on every shipped preset.
            bond_maturity: kaspa_consensus_core::palw_model_registry_v1::palw_bond_maturity_fold_v1(
                self.palw_bond_maturity,
                self.palw_bond_maturity_early,
                self.palw_settled_anchor_depth_at(daa_score),
                daa_score,
            ),
        })
    }

    /// ADR-0132 Upgrade C: the fold's payout input at the block `point` names — the fence's numbers
    /// where it is active at the block's DAA, with the block's compact `bits` (`0` where the store
    /// holds no header for it, which prices the network draw at one).
    /// **ADR-0137 (shadow): the work target's fold input.** The rate is the payout fence's where
    /// it is armed at `daa_score` and the shadow constant otherwise; the clamp is the class DAA's;
    /// the works are the registry's (the genesis classes from the canonical table, the registered
    /// ones from their carriages). `None` off ConsensusV2, where there is no work to price.
    pub(in crate::pipeline::virtual_processor) fn palw_work_target_fold_for(
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

    pub(in crate::pipeline::virtual_processor) fn palw_economic_payout_fold_for(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> Option<kaspa_consensus_core::palw_economic_payout_v1::PalwEconomicPayoutFoldV1> {
        let payout = self.palw_economic_payout.filter(|payout| payout.activation.is_active(point.daa_score))?;
        let bits = self.headers_store.get_header(point.block).map(|header| header.bits).unwrap_or(0);
        Some(payout.fold_v1(bits))
    }

    /// ADR-0135: the draw's readiness policy at an anchor — a seat judges a non-base class only
    /// with a possession proof no older than the readiness age.
    pub(in crate::pipeline::virtual_processor) fn palw_readiness_policy_at(
        &self,
        anchor_daa: u64,
    ) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwReadinessPolicyV1> {
        let fold = self.palw_model_registry_fold_at(anchor_daa)?;
        if !fold.governs_at(anchor_daa) {
            return None;
        }
        let state = self.palw_state_params_v2.as_ref()?;
        // **The readiness-age sweep (2026-09-24): the draw judges a row by the registry's rule** —
        // readiness V2's horizon (the fold's globals: eight spans, 24 on testnet-12) and no V1 row —
        // but only past the 2026-09-23 audit fence, since
        // the draw is consensus (the panel a claim is bound to). Below it (testnet-11) the old rule
        // stands byte for byte: any row, the V1 age.
        let readiness_v2 = self.palw_audit_2026_09_23_at(anchor_daa) && self.palw_readiness_v2_at(anchor_daa);
        Some(
            kaspa_consensus_core::palw_model_registry_v1::PalwReadinessPolicyV1::at(
                &fold,
                anchor_daa,
                state.base_class_id(),
                readiness_v2,
            )
            // Lane MU (ADR-0173): past `palw_audit_1004_v1` the draw reads possession of the root the claim named.
            .with_root_keyed(state.audit_1004_active_at(anchor_daa)),
        )
    }

    /// **ADR-0075 SA-1/SA-2, resolved in exactly one place.** `false` on every shipped preset.
    ///
    /// Two sites read it and they must agree: `calculate_utxo_state` decides whether to REMEMBER
    /// what each 0x4b carrier paid, and `palw_v2_accepted_objects` decides whether to SPEND that
    /// memory. If the first said no and the second said yes, every carrier would look like it paid
    /// nothing and no certification could ever be graded — the fail-closed direction, but a
    /// liveness halt all the same, and the reverse would collect fees nobody reads.
    pub(in crate::pipeline::virtual_processor) fn palw_certification_rent_at(&self, daa_score: u64) -> bool {
        self.palw_certification_rent.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0082 Decision 3, resolved in exactly one place.** `false` on every shipped preset.
    ///
    /// Two things read it and they must agree: the acceptance filter, which admits a dissection
    /// move only past the fence, and [`Self::palw_court_params_at`], which decides the arity the
    /// children of every round are cut at. A node that resolved them differently would accept a
    /// root claim its peers refused, or deal a different number of children from the same range.
    pub(in crate::pipeline::virtual_processor) fn palw_kary_court_active_at(&self, daa_score: u64) -> bool {
        self.palw_kary_court.is_some_and(|fence| fence.is_active(daa_score))
    }

    pub(in crate::pipeline::virtual_processor) fn palw_model_court_window_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_court_window.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0077 Decision 16's `PanelDa` at this block, resolved in exactly one place.** `false` on
    /// every shipped preset; `true` from block one on a genesis that arms it.
    pub(in crate::pipeline::virtual_processor) fn palw_panel_da_at(&self, daa_score: u64) -> bool {
        self.palw_panel_da.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **ADR-0081 Decision 3's form at this block, resolved in exactly one place.** `Flat` on every
    /// shipped preset; the tiled Merkle root from block one on a genesis that arms it
    /// (`validate_palw_v2` refuses any other height, so this never differs from
    /// `Params::palw_prompt_ids_form_v1`).
    pub(in crate::pipeline::virtual_processor) fn palw_prompt_ids_form_at(
        &self,
        daa_score: u64,
    ) -> kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1 {
        match self.palw_prompt_ids_merkle {
            Some(fence) if fence.is_active(daa_score) => kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            _ => kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        }
    }

    /// **ADR-0077 Phase B's fence at this block, resolved in exactly one place.** `false` on
    /// testnet-11 and the hash-lineage presets; armed from genesis on devnet and on a carded
    /// mainnet.
    pub(in crate::pipeline::virtual_processor) fn palw_context_ladder_at(&self, daa_score: u64) -> bool {
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
    pub(in crate::pipeline::virtual_processor) fn palw_court_params_at(
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
    pub(super) fn palw_warn_if_maturity_outruns_the_registry(
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
    pub(super) fn palw_v2_check_attempt_admission(
        &self,
        header: &Header,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        bootstrap_state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
    ) -> Result<Option<kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2>, String> {
        self.palw_v2_check_attempt_admission_waiving_v1(header, state, state_params, point, bootstrap_state, false)
    }

    /// [`Self::palw_v2_check_attempt_admission`] with admission item 8 (the exposure ceiling) waived
    /// when `waive_ceiling` — lane bind-deadlock's binder re-check
    /// ([`Self::palw_v2_anchor_at_ceiling_binder_v1`]) and nothing else passes `true`.
    pub(super) fn palw_v2_check_attempt_admission_waiving_v1(
        &self,
        header: &Header,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        bootstrap_state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
        waive_ceiling: bool,
    ) -> Result<Option<kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2>, String> {
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
        let envelope = kaspa_consensus_core::palw_attempt_material_v3::native_attempt_base_v3(&header.palw_commitment)
            .map_err(|e| e.to_string())?;
        let native_ready =
            if kaspa_consensus_core::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3::is_carriage(&header.palw_commitment) {
                if !kaspa_consensus_core::palw_attempt_material_v3::palw_native_custody_required_v3(
                    state_params,
                    &envelope.attempt.class_id,
                    header.daa_score,
                ) {
                    return Err("PAV3 native custody form below its monetary fence or for the exempt BASE floor class".into());
                }
                let ready = kaspa_consensus_core::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3::decode_wire(
                    &header.palw_commitment,
                )?;
                let anchor = self.palw_execution_anchor_v1(header, &ready.base.attempt);
                ready.verify_prior_with_recovery_v4(
                    state,
                    state_params,
                    anchor,
                    header.daa_score,
                    Self::verify_mldsa87_with_context_bool,
                )?;
                Some((ready.class_ticket(anchor), ready))
            } else {
                if kaspa_consensus_core::palw_attempt_material_v3::palw_native_custody_required_v3(
                    state_params,
                    &envelope.attempt.class_id,
                    header.daa_score,
                ) {
                    return Err("new native work requires the owned signed PAV3 custody form".into());
                }
                None
            };
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
            // Lane bind-deadlock: item 8 waived for a binder's re-check only.
            {
                let mut fences = self.palw_epoch_budget_fences_at(point.daa_score);
                fences.native_material_ticket_v3 = native_ready.as_ref().map(|(ticket, _)| *ticket);
                fences.exposure_ceiling_waived = waive_ceiling;
                fences
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(Some(envelope))
    }

    /// **Lane bind-deadlock: the binder — a chain block whose own attempt admission refused on its
    /// bond's exposure ceiling alone, and which is the anchor of a due claim** (`Params::palw_anchor_at_ceiling`,
    /// at the block's own DAA).
    ///
    /// Past `palw_rcore_plus` a claim binds only in its anchor block (SW-8) and only an attempt block
    /// anchors; an attempt a full bond cannot back was disqualified here, so a fleet whose producers all
    /// stood at their ceilings had no anchor at all and its `Provisional` claims — the very room that
    /// held it there — waited for the `BindTimeout` backstop. `Some(envelope)` — the walk keeps the block
    /// and hands the fold its attempt — exactly when:
    ///
    /// 1. the fence is active at the block's DAA;
    /// 2. **past lane A's fence (`Params::palw_operator_anchor`) at the block's DAA, the block's OWN
    ///    attempt is an operator's** (`PalwOperatorAnchorRuleV1::operator_of_v1`). Lane A lets a chain
    ///    block anchor through an operator attempt it merely MERGES, whatever its own lane; that
    ///    capability is the merged attempt's, and any chain block that merges it — a heartbeat, which
    ///    has no ceiling — carries it, so a non-operator at its ceiling is never the one block that can
    ///    bind those claims and is disqualified as before. What lane A leaves stuck is the operator's
    ///    own attempt at its ceiling: disqualified, it is popped from the sink search and merged by no
    ///    template, so the claims due at it wait for an operator with room — none, in the deadlock;
    /// 3. the block anchors a panel, and the parent holds a claim `Provisional` with its slot at or
    ///    below the highest slot the block anchors ([`Self::palw_sw8_anchor_for`]: R-core+ at the block
    ///    and the one per-block answer the anchor walk and step 4c read — below lane A the block's lane,
    ///    reaching its own DAA; past it the operator attempts it is or merges, reaching the latest of
    ///    them, which for an operator's own attempt is its own DAA) — so this block IS that claim's
    ///    anchor and step 4c binds or voids it here; a block with nothing to anchor stays disqualified,
    ///    so binders are bounded by the anchor events claims create;
    /// 4. the full admission passes with item 8 waived — signature, pins, pwu, producer floor, budget,
    ///    identity, class lottery: an anchor still costs a won draw by a registered bond's key.
    ///
    /// The fold then runs the attempt through `apply_attempt`, whose own ceiling (finding 17,
    /// `AttemptExposureCeiling`, on the state this block's objects leave) skips it: no claim, no
    /// reservation, the worker carve withheld and burned (`palw_v2_skipped_own_attempt_carve`). If this
    /// block's own objects freed the room first, the fold admits the claim on the ceiling it measures
    /// — the invariant `committed ≤ ceiling` is the fold's, whichever block the claim rides.
    pub(super) fn palw_v2_anchor_at_ceiling_binder_v1(
        &self,
        header: &Header,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        bootstrap_state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
    ) -> Option<kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2> {
        if !self.palw_anchor_at_ceiling_at(point.daa_score) {
            return None;
        }
        // (2) Past lane A only an operator's own attempt binds at its ceiling.
        if self
            .palw_operator_anchor
            .as_ref()
            .is_some_and(|rule| rule.active_at(point.daa_score) && rule.operator_of_v1(header).is_none())
        {
            return None;
        }
        // (3) Due at the slots this block anchors — `reach`, as step 4c caps it — not at its own DAA.
        let (anchor_delay, reach) = self.palw_sw8_anchor_for(point)?;
        if kaspa_consensus_core::palw_state_v2::palw_claims_provisional_past_their_anchor_slot_v1(
            state,
            reach.min(point.daa_score),
            anchor_delay,
        )
        .is_empty()
        {
            return None;
        }
        self.palw_v2_check_attempt_admission_waiving_v1(header, state, state_params, point, bootstrap_state, true).ok().flatten()
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
    pub(in crate::pipeline::virtual_processor) fn palw_v2_merged_works(
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
        if ghostdag_data.unordered_mergeset_without_selected_parent().next().is_none() {
            return (works, skips);
        }
        let non_daa = mergeset_non_daa;
        // Lane accept-order: past its fence, parents-first at the accepting block's DAA score.
        let merged: Vec<BlockHash> = self.acceptance_ordered_mergeset_without_selected_parent(ghostdag_data, point.daa_score);
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
                    self.palw_escrow_for_carrier_at(*blue, header.daa_score, point.daa_score),
                    // ADR-0132 Upgrade C: the merged block's own `bits`, the lottery its forward faced.
                    header.bits,
                    header.clone(),
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

    pub(super) fn palw_v2_check_receipt_spend(
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
        // **RFC-0002 §II.14: the owning39 native receipt block (`PNR9`).** Valid only past `palw_native_mining_v39` with its lanes;
        // the full stateful gate (live certified receipt, unspent quantum, the producer's key and signature, the strict budget draw
        // under this block's carve and the 0.51 export) runs here against the PARENT state, and again in the fold. The shared
        // receipt-spend view it returns names the receipt as `claim_id` and carries fold version 39, so the B-5 dedup, the
        // entitlement and the coinbase's worker share treat it exactly as any receipt block.
        if kaspa_consensus_core::palw_native_receipt_v39::palw_native_receipt_carriage_is_v39(&header.palw_commitment) {
            use kaspa_consensus_core::config::params::ForkActivation;
            if !self
                .palw_native_mining_v39
                .as_ref()
                .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(header.daa_score))
                || !self.palw_native_mining_v39_lanes_at(header.daa_score)
            {
                return Err("a PNR9 native receipt block below palw_native_mining_v39".to_string());
            }
            let envelope =
                kaspa_consensus_core::palw_native_receipt_v39::PalwNativeReceiptSpendEnvelopeV39::decode(&header.palw_commitment)?;
            let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                self.network_id_bytes.as_slice(),
                Some(self.genesis.hash),
            );
            let pre_pow_hash = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
            let carve =
                state_params.worker_carve_at(point.subsidy, self.palw_escrow_for_carrier_at(point.block, point.daa_score, point.daa_score));
            kaspa_consensus_core::palw_native_receipt_v39::check_native_receipt_spend_admission_v39(
                state,
                state_params,
                point,
                network_domain,
                pre_pow_hash,
                header.timestamp,
                header.nonce,
                carve,
                &envelope,
                |key, message, sig, context| kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false),
            )?;
            return Ok(Some(envelope.to_fold_envelope()));
        }
        let Some(freeprompt) = self.palw_freeprompt_params_v3.as_ref() else {
            return Err("a receipt-lane block on a network with no free-prompt bundle".to_string());
        };
        // **RFC-0009: which receipt carriage is this?** The magic says. A `PFS4` header is a V4 (public redemption) spend and is valid
        // only past `palw_receipt_spend_v4`; a `PFS3` one is the V3 spend exactly as it always was. Both reach the fold through the
        // same V3-shaped view (`(claim, quantum, executor bond)` is all the fold reads), so everything downstream is shared.
        let v4 = if kaspa_consensus_core::palw_receipt_v4::palw_receipt_v4_carriage_is_v4(&header.palw_commitment) {
            if !self.palw_receipt_spend_v4.is_some_and(|fence| fence.is_active(header.daa_score)) {
                return Err(kaspa_consensus_core::palw_receipt_v4::PalwReceiptV4Error::BelowFence.to_string());
            }
            Some(
                kaspa_consensus_core::palw_receipt_v4::PalwReceiptSpendEnvelopeV4::decode(&header.palw_commitment)
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let envelope = match &v4 {
            Some(v4) => v4.to_fold_envelope(),
            None => PalwReceiptSpendEnvelopeV3::decode(&header.palw_commitment).map_err(|e| e.to_string())?,
        };
        let claim = state
            .claim(&envelope.spend.claim_id)
            .ok_or_else(|| format!("claim {} does not exist at this chain point", envelope.spend.claim_id))?;
        let kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
            return Err(format!("claim {} is not certified at this chain point", envelope.spend.claim_id));
        };
        // **RFC-0018 18-A (Gate C)**: a governed free-prompt claim licenses no receipt block until `SecurityCertified`, and its lane's
        // clock starts at certification — the ONE predicate the fold, the admission items and the spendable RPC read, asked here against
        // the PARENT state so an own block is disqualified and a merged one unentitled (no coinbase worker share) for the reason the
        // fold would refuse it. Below Gate C's fence (every shipped network): `final_daa`, unchanged.
        let final_daa =
            kaspa_consensus_core::palw_rfc0018_v1::palw_fp_receipt_clock_v1(state_params, state, &envelope.spend.claim_id, claim, final_daa)
                .map_err(|e| e.to_string())?;
        // 2026-09-24 DoS audit #5: the fold's own refusal (`ReceiptRightsForfeited`), asked here at
        // the same fence against the PARENT state, so an own block is disqualified — and a merged one
        // unentitled — for the reason the fold would refuse it. A conviction carried by this very
        // block's objects is not in the parent state; the fold refuses the spend at step 4 instead
        // (`WrongPhase` for the named claim, `ReceiptRightsForfeited` for a sibling) and the block is
        // disqualified the same way on every node — deliberately not skipped, because a skipped
        // spend's block would still be paid its worker share (see the fold's step 4).
        if self.palw_audit_2026_09_23_at(point.daa_score) && state.palw_execution_root_is_forfeited_v1(&claim.execution_root) {
            return Err(format!(
                "claim {}'s receipt rights are forfeit: its execution {} was convicted",
                envelope.spend.claim_id, claim.execution_root
            ));
        }
        // **Lane BUDGET (ADR-0176 D3, hook H-5): the fold's strict budget draw**, asked here against the PARENT state, so a receipt block
        // whose commitment's reservation cannot pay it (one block, this block's carve) is refused for the reason the fold would refuse
        // it. `true` for every unbudgeted claim — every claim of every network that never armed `palw_bond_budget_v1` — and the carve
        // is not even read then.
        if !state.bond_budget_spend_fits_v1(state_params, &envelope.spend.claim_id, point.daa_score, || {
            state_params.worker_carve_at(point.subsidy, self.palw_escrow_for_carrier_at(point.block, point.daa_score, point.daa_score))
        }) {
            return Err(format!("claim {}'s bond budget cannot pay this receipt block (ADR-0176)", envelope.spend.claim_id));
        }
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
        if let Some(v4) = &v4 {
            kaspa_consensus_core::palw_receipt_v4::check_palw_receipt_spend_admission_full_v5(
                state,
                point,
                network_domain,
                pre_pow_hash,
                header.timestamp,
                header.nonce,
                freeprompt.receipt_maturity_daa(),
                freeprompt.receipt_use_window_daa(),
                &beacon,
                v4,
                |key, message, sig, context| kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false),
                self.palw_fp_pricing().as_ref(),
                Some(state_params),
            )
            .map_err(|e| e.to_string())?;
            let _ = state_params;
            return Ok(Some(envelope));
        }
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
            Some(state_params),
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
    pub(in crate::pipeline::virtual_processor) fn palw_beacon_fact_of_candidate(
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
    pub(super) fn palw_beacon_fold_k_at(&self, slot: u64) -> u8 {
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
    pub(super) fn palw_v2_derived_panel_bindings(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
        block_daa: u64,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
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
        // **ADR-0152 SW-8: past `palw_rcore_plus`, one state.** See
        // [`Self::palw_v2_derived_panel_bindings_on_one_state_v1`].
        if self.palw_panel_one_state_at(block_daa) {
            return self.palw_v2_derived_panel_bindings_on_one_state_v1(state, block, block_daa, panel_params, min_collateral);
        }
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
            if let Ok(object) = self.palw_v2_derive_panel_binding_v1(
                state,
                block,
                block_daa,
                &binding_extras,
                panel_params,
                min_collateral,
                claim_id,
                claim,
            ) {
                out.push(object);
            }
        }
        out
    }

    /// **ADR-0152 SW-8: does the chain derive this block's panels on ONE state?** Past
    /// `palw_rcore_plus` at the BLOCK's DAA, on a chain whose acceptance walk folds the real step 3
    /// (the 2026-09-11 audit's A-1, `palw_audit_2026_09_11`: the pre-object base the gate validates
    /// against). testnet-12 arms both at genesis; everywhere else this is `false` and the derivation
    /// reads the parent state exactly as it always did. `validate_palw_rcore_plus_v1` refuses
    /// `palw_rcore_plus` without `palw_audit_2026_09_11` at or below it (M4 review finding 4), so on
    /// any ruleset a node can run the second conjunct is implied by the first; it stays spelled here
    /// because without it the walk rehearses each object through a whole-block transition while the
    /// derivation would read a different state, and a binding bound only in its anchor block could
    /// then be dropped with no retry.
    pub(super) fn palw_panel_one_state_at(&self, block_daa: u64) -> bool {
        self.palw_rcore_plus_at(block_daa) && self.palw_audit_2026_09_11_at(block_daa)
    }

    /// **ADR-0152 SW-8: the derived bindings on the ONE state acceptance reads.**
    ///
    /// Below the fence the chain derived every binding on the PARENT state while the acceptance walk
    /// validated each against the fold's pre-object base (`palw_v2_pre_object_base_v1`: the parent
    /// after this block's step 2 — its payouts, sweeps, retargets and redraws) advanced by the
    /// objects accepted before it. Where the two differed — a sweep that moved a seat's load, a
    /// binding earlier in the block that reserved a seat's duty — the derived panel met
    /// `PanelMismatch`, was dropped, and was re-derived by a later block on a later state: a second
    /// draw of the same seed, on a state an attacker had time to shape (the draft's retry path).
    ///
    /// Here the derivation starts from that same base, at the same chain point the walk uses (the
    /// block's own blue score and subsidy, read off its header), and advances it binding by binding
    /// in claim-id order exactly as the walk will: each derived `PanelBound` meets the gate
    /// (`palw_v2_validate_objects`) and then the fold's one object (`palw_v2_apply_one_object_v1`,
    /// with this block's own-attempt reservation), and only a binding both take is emitted and folded
    /// in before the next claim is drawn. The derived bindings lead the block's objects, so the walk
    /// meets them first, on the same base, in the same order — build = accept = fold (T89) — and the
    /// second claim bound in a block sees the first one's duties.
    ///
    /// Together with the stake draw's bind-only-in-the-anchor-block rule (the policy's `stake`, see
    /// [`Self::palw_v2_derive_panel_binding_v1`]), no later state exists on which a failed or
    /// dropped draw could be retried: a claim this block does not bind voids `BindTimeout` at the end
    /// of this very block (the fold's step 4c, `PalwTransitionExtrasV1::sw8_anchor_delay`; S0: no
    /// forfeit; a free-prompt claim starts its abandon hold there) — unless the fold re-anchors it at
    /// its next slot: lane F1's V03(1) where its class could not seat a panel, and lane F2's
    /// floor-refusal retry where its stake draw refused for eligibility at every seed on this block's
    /// pre-object base (`palw_sw8_thin_draws_v1`, asked below on the same base for the log line). The
    /// same draw on a LATER state is never retried: the retry is a new anchor, a new seed, a new draw.
    ///
    /// **Most blocks bind nothing, and cost nothing here** (M4 review finding 6). Past
    /// `palw_rcore_plus` a panel anchors only on an attempt block
    /// ([`Self::palw_block_may_anchor_a_panel_v1`]; past lane A only on a block that is or merges an
    /// operator's attempt, [`Self::palw_anchor_reach_of_v1`]), and step 2 cannot make a claim this block's
    /// anchor (a redraw re-bases on this block, so its slot is later; the sweeps only remove claims),
    /// so a block binds only claims its PARENT already holds `Provisional` with their slot at or
    /// below this block's DAA (`palw_claims_provisional_past_their_anchor_slot_v1`). The pre-object
    /// base — a state clone and step 2 — is built only when this is an attempt block with such a
    /// claim; the candidates are then re-read on the base, which is what the loop draws.
    ///
    /// The cost is one more pre-object base and one more rehearsal per binding than the walk alone
    /// pays, on testnet-12's anchor blocks only; the walk still validates every binding itself, so a
    /// node whose derivation ever disagreed with its walk would drop the binding rather than
    /// disqualify a block — and say so, since under SW-8 the claim's only chance to bind has passed.
    pub(super) fn palw_v2_derived_panel_bindings_on_one_state_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
        block_daa: u64,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
        min_collateral: u64,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        use kaspa_consensus_core::palw_state_v2::palw_claims_provisional_past_their_anchor_slot_v1;
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Vec::new() };
        let Ok(header) = self.headers_store.get_header(block) else {
            // Unreachable for a block under validation (its header is stored before its body is
            // processed); a node that cannot read it derives nothing rather than guessing a point.
            return Vec::new();
        };
        // Finding 6's pre-check, on the parent: a block that anchors, with a claim whose slot it
        // reaches, or nothing to derive. The reach is the block's own DAA score, except past lane A,
        // where a block anchors only the slots at or below the latest operator attempt it is or merges
        // (`palw_anchor_reach_of_v1`, the answer the walk and step 4c read).
        let anchor_delay = panel_params.anchor_delay();
        let Some(reach) = self.palw_anchor_reach_of_v1(block, &header) else {
            return Vec::new();
        };
        if palw_claims_provisional_past_their_anchor_slot_v1(state, reach, anchor_delay).is_empty() {
            return Vec::new();
        }
        // The walk's chain point, spelled as the pipeline spells it for this block.
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block,
            daa_score: block_daa,
            blue_score: header.blue_score,
            subsidy: self.coinbase_manager.calc_block_subsidy(block_daa),
        };
        let own_attempt_class = Self::palw_own_attempt_class_of_header_v1(&header);
        let extras = self.palw_transition_extras_for(&point);
        let mut folded = match kaspa_consensus_core::palw_state_v2::palw_v2_pre_object_base_retained_v1(
            state,
            state_params,
            &point,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &extras,
        ) {
            Ok(base) => base,
            // The walk accepts nothing on a block whose pre-object fold fails (the block is
            // disqualified whatever it carries), so there is nothing to bind here either.
            Err(_) => return Vec::new(),
        };
        // Claim-id order, over the claims the base holds `Provisional` with their slot at or below
        // this block's reach (a binding moves only its own claim, so the list cannot change under the
        // loop; every other `Provisional` claim's anchor is a later block, so it could not bind here).
        let provisional = palw_claims_provisional_past_their_anchor_slot_v1(&folded, reach, anchor_delay);
        // **Lane F2: which unbound claims the fold's step 4c re-anchors instead of voiding** — its step
        // 2e′ verdict, asked here of the same function on the same pre-object base with the same extras
        // (empty below `palw_floor_refusal_retry` and wherever the block carries no draw points), so the
        // line below says which way a refused claim went.
        let floor_retries = kaspa_consensus_core::palw_state_v2::palw_sw8_thin_draws_v1(
            &folded,
            state_params,
            &extras,
            block_daa,
            anchor_delay,
            reach.min(block_daa),
        );
        let mut out = Vec::new();
        // RFC-0018 §5.2: past the permissionless anchor a block anchors only the claims a contribution of ANOTHER bond reaches — the
        // per-claim predicate step 4c asks, on the same committed contributions.
        let permissionless = self
            .palw_panel_anchor_permissionless_at(block_daa)
            .then(|| kaspa_consensus_core::palw_rfc0018_v1::palw_permissionless_contributions_from_state_v1(state, header.blue_score));
        for claim_id in provisional {
            let Some(claim) = folded.claim(&claim_id) else { continue };
            if let Some(contributions) = permissionless.as_ref()
                && !kaspa_consensus_core::palw_rfc0018_v1::palw_permissionless_anchors_claim_v1(
                    contributions,
                    claim.bind_base_daa().saturating_add(anchor_delay),
                    &claim.bond,
                )
            {
                continue;
            }
            let object = match self.palw_v2_derive_panel_binding_v1(
                &folded,
                block,
                block_daa,
                &extras,
                panel_params,
                min_collateral,
                &claim_id,
                claim,
            ) {
                Ok(object) => object,
                // SW-8/SW-10: a draw refused IN its anchor block is final for this panel — say so, since
                // the claim now voids `BindTimeout` at the end of this block (step 4c) and an operator
                // should see why. Past lane F2's fence a draw that refused for eligibility at every seed
                // is re-anchored instead, and the line says so.
                Err(Some(why)) => {
                    if floor_retries.contains(&claim_id) {
                        info!(
                            "Block {block}: claim {claim_id}'s panel is not bound — its draw refused in its anchor block ({why}), \
                             and the eligible population refuses it at every seed on the anchor block's pre-object base: \
                             re-anchored at its next slot (floor), DAA {}, keeping its reservation and escrow; it voids only \
                             at its backstop, DAA {} (lane F2, palw_floor_refusal_retry)",
                            block_daa.saturating_add(anchor_delay),
                            claim.accepted_daa.saturating_add(state_params.window_bind()),
                        );
                    } else {
                        info!(
                            "Block {block}: claim {claim_id}'s panel is not bound — its draw refused in its anchor block ({why}); \
                             under the stake-weighted draw a claim binds only there, so it voids in this block, without forfeit \
                             (ADR-0152 SW-8/SW-10) — or, past palw_registry_resilience with its class unable to seat a panel, \
                             is re-anchored at its next slot (lane F1, V03)"
                        );
                    }
                    continue;
                }
                Err(None) => continue,
            };
            // Finding 5: under SW-8 this block is the claim's one chance to bind, so a derived
            // binding the gate or the fold refuses is reported as the draw's refusal is — the claim
            // voids at the end of this block (step 4c), and an operator should see why.
            if let Err(why) = self.palw_v2_validate_objects(&folded, state_params, &point, std::slice::from_ref(&object)) {
                info!(
                    "Block {block}: claim {claim_id}'s panel is not bound — its derived binding was refused by the gate in its \
                     anchor block ({why}); under the stake-weighted draw a claim binds only there, so it voids BindTimeout in \
                     this block, without forfeit (ADR-0152 SW-8)"
                );
                continue;
            }
            let applied = kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_retained_v1(
                &folded,
                state_params,
                &point,
                &object,
                self.palw_unavailable_abstains_at(point.daa_score),
                self.palw_capability_bound_at(point.daa_score),
                self.palw_uncertified_weightless_at(point.daa_score),
                self.palw_da_court_at(point.daa_score),
                &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
                    own_attempt_class,
                    ..self.palw_transition_extras_for(&point).into_legacy_extras()
                },
            );
            match applied {
                Ok(next) => {
                    folded = next;
                    out.push(object);
                }
                Err(why) => info!(
                    "Block {block}: claim {claim_id}'s panel is not bound — its derived binding was refused by the fold in its \
                     anchor block ({why}); under the stake-weighted draw a claim binds only there, so it voids BindTimeout in \
                     this block, without forfeit (ADR-0152 SW-8)"
                ),
            }
        }
        out
    }

    /// **This block's own attempt's class, read off its header** (2026-09-24 DoS audit review of #11):
    /// the class the fold reserves one claim of on the room while step 3 folds the block's objects.
    /// `None` for a block that carries no attempt or an undecodable one (admission refuses that).
    /// The one spelling the block pipeline and the one-state derivation (SW-8) both read.
    pub(super) fn palw_own_attempt_class_of_header_v1(header: &Header) -> Option<kaspa_hashes::Hash64> {
        kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id)
            .then(|| {
                kaspa_consensus_core::palw_attempt_material_v3::native_attempt_base_v3(&header.palw_commitment)
                    .ok()
                    .map(|envelope| envelope.attempt.class_id)
            })
            .flatten()
    }

    /// **One claim's derived binding at `block`**, on `state` — the per-claim body both derivations
    /// share (the parent-state one below `palw_rcore_plus`, the one-state one past it), so the two
    /// cannot differ in anything but the state they read.
    ///
    /// `Err(None)`: nothing to bind here (not `Provisional`, no anchor yet, or — under the stake
    /// draw — this block is not the claim's anchor block). `Err(Some(why))`: the claim's anchor block
    /// is this one and its draw refused (`why`); only the stake draw reports it, since below it the
    /// claim simply tries again at the next block.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn palw_v2_derive_panel_binding_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
        block_daa: u64,
        binding_extras: &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
        min_collateral: u64,
        claim_id: &kaspa_hashes::Hash64,
        claim: &kaspa_consensus_core::palw_state_v2::PalwClaimStateV2,
    ) -> Result<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, Option<String>> {
        use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
        if !matches!(claim.phase, PalwClaimPhaseV2::Provisional) {
            return Err(None);
        }
        // The anchor is the first chain block at or past `accepted_daa + anchor_delay`. Until
        // one exists the claim simply waits — that delay is what stops a producer from
        // mining until it likes its own jury.
        // Same base as the validator's, for the same reason — see the sibling call site.
        // RFC-0010: through the versioned door — a V3-rule claim has no lane-A anchor (and the list this binder walks never offers one).
        let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Err(None) };
        let Some(anchor) = self.palw_v2_anchor_fact_for_claim_v1(block, claim, state_params, panel_params) else {
            return Err(None);
        };
        // C-02 (deep fence) and ADR-0124 (the panel economy): the whole draw policy, from the
        // same anchor as the acceptance layer's sibling call — so this assembler builds the
        // exact panel that layer recomputes.
        let mut policy = self.palw_panel_draw_policy_at(anchor.anchor_daa);
        // **ADR-0152 SW-8: under the stake draw a panel binds only in its own anchor block**
        // (`validate_panel_bound_v2_with_policy` refuses any other), so no later block draws it again.
        if policy.stake.is_some() && anchor.anchor_block != block {
            return Err(None);
        }
        if let Some(state_params) = self.palw_state_params_v2.as_ref() {
            policy.valid_lock = Self::palw_panel_valid_lock_of_v1(
                state,
                state_params,
                binding_extras,
                claim_id,
                claim,
                block_daa,
                panel_params.seat_count() as usize,
            );
        }
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
        // ADR-0100 Decision 4: a class with a plan draws per shard, or not at all — a flat
        // panel of a sharded class would ask shard seats to judge a whole model.
        // Lane F1, the panel seed: for a claim anchored past `Params::palw_panel_seed_execution`
        // `H(anchor attempt's execution ‖ claim)`, below it the anchor block — the one value the gate
        // recomputes and the fold stores.
        let seed = anchor.panel_seed(claim_id);
        let drawn = match (
            self.palw_tir_shard_count(state, &claim.class_id, anchor.anchor_daa),
            self.palw_stratified_shard_count(state, &claim.class_id, anchor.anchor_daa),
        ) {
            // RFC-0006: an IR class with a layer-shard plan draws per shard, with the whole draw policy (stake race, readiness,
            // route-matrix lock) the flat draw reads — the same function the acceptance layer recomputes it with.
            (Some(s_l), _) => kaspa_consensus_core::palw_panel_v2::derive_tir_shard_panel_v1(
                state,
                panel_params,
                claim_id,
                seed,
                min_collateral,
                maturity_floor,
                capability_bound,
                policy,
                s_l,
            ),
            (None, Some(shard_count)) => kaspa_consensus_core::palw_panel_v2::derive_stratified_panel_v2(
                state,
                panel_params,
                claim_id,
                seed,
                min_collateral,
                maturity_floor,
                capability_bound,
                shard_count,
            ),
            (None, None) => kaspa_consensus_core::palw_panel_v2::derive_panel_v2_with_policy(
                state,
                panel_params,
                claim_id,
                seed,
                min_collateral,
                maturity_floor,
                capability_bound,
                policy,
            ),
        };
        let seats = drawn.map_err(|why| policy.stake.map(|_| why.to_string()))?;
        // ADR-0147: nothing to ask here. The outsider is IN the derived panel — the draw policy
        // carries the fence — and the acceptance layer demands that panel exactly, so a node
        // proposes only what its own fold will take. The identity test that stood here
        // compared fields a registrant writes for itself, and is gone with the rule it served.
        Ok(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::PanelBound { claim: *claim_id, anchor: seed, seats })
    }

    /// **RFC-0003 §I.4.4: the tensor claims of an accepted set** — the free-prompt walk's twin for
    /// version-10 payloads ([`kaspa_consensus_core::palw_gen_claim_v1::palw_fp_gen_objects_from_accepted_txs_v1`]),
    /// at `block_daa` against the parent `state`: this network's domain and `PanelDa` arming, each
    /// commitment bounded by its CLASS's ladder (the bundle's, or the one the class recorded under the held
    /// regime), the network's form and this processor's verifier. Empty below `palw_gen_v1` or
    /// `palw_fp_job_v5` ([`Self::palw_gen_lane_open_at`]): no tensor claim becomes an object. The one place
    /// the walk's arguments are written — the extraction and the tests' helper both read it.
    pub(in crate::pipeline::virtual_processor) fn palw_v2_gen_objects_of_txs(
        &self,
        txs: &[Transaction],
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block_daa: u64,
    ) -> kaspa_consensus_core::palw_fp_objects_v3::PalwFpExtractionV3 {
        if !self.palw_gen_lane_open_at(block_daa) {
            return Default::default();
        }
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let ladder = self
            .palw_v2_bundle
            .as_ref()
            .map(|bundle| bundle.court.max_step_leaf_count())
            .unwrap_or(kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
        kaspa_consensus_core::palw_gen_claim_v1::palw_fp_gen_objects_from_accepted_txs_v1(
            txs,
            network_domain,
            self.palw_panel_da_at(block_daa),
            |class_id| state.class_step_ladder_v1(class_id, ladder),
            self.palw_prompt_ids_form_at(block_daa),
            Self::verify_mldsa87_with_context_bool,
        )
    }

    /// **LIVE-R1 (2026-10-08): name the rule that refused, in the wedge warning.** Node-local and read-only:
    /// nothing here is asked by a verdict.
    ///
    /// The settled-sink warning above used to say only `reason DominanceViolation` — the DNS gate's word —
    /// for every refusal, so an operator (and H1's devnet report) read a PALW deep-reorg refusal as the
    /// DNS veto. On a V2 network a non-extension candidate is judged by `dns_reorg_outcome`'s V2 arm
    /// before any DNS rule, and that arm answers `DominanceViolation` for a strict economic loss, a deep
    /// all-economic tie, and a candidate this node cannot weigh alike. This spells out which, with the
    /// three keys of both tips, and what would end it. Empty off V2 and for an extension. Runs once per
    /// search and only after a refusal — the same two weighings the gate itself just made.
    pub(crate) fn palw_reorg_refusal_explained_v1(&self, sink: BlockHash, refused: BlockHash) -> String {
        let extension = matches!(self.reachability_service.try_is_chain_ancestor_of(sink, refused), Ok(true));
        if self.palw_state_params_v2.is_none() || extension {
            return String::new();
        }
        let keys = |o: &kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1| {
            format!("frontier {}, safe {}, live {}", o.safe_frontier_blue_score, o.safe_weight, o.live_total)
        };
        match (self.palw_candidate_order_v2(sink), self.palw_candidate_order_v2(refused)) {
            (Some(incumbent), Some(challenger)) => {
                let economic = |o: &kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1| {
                    (o.safe_frontier_blue_score, o.safe_weight, o.live_total)
                };
                let verdict = match economic(&challenger).cmp(&economic(&incumbent)) {
                    std::cmp::Ordering::Less => "a strict economic LOSS for the heavier candidate",
                    std::cmp::Ordering::Equal => "an all-economic tie deeper than the shallow window",
                    std::cmp::Ordering::Greater => "an economic win refused by the frontier-provenance or stake rule",
                };
                format!(
                    "; this is the PALW deep-reorg rule (palw_reorg_strict_economic_win), not DNS: {verdict} — this sink ({}) vs the \
                     candidate ({}). Blue work does not move this rule; this node follows that chain only once it strictly \
                     out-weighs this one on these keys (a later Final or licence there), or after a resync",
                    keys(&incumbent),
                    keys(&challenger)
                )
            }
            (Some(_), None) => "; PALW deep-reorg rule, not DNS: this node cannot weigh the candidate".to_owned(),
            (None, _) => "; PALW deep-reorg rule, not DNS: this node cannot weigh its own sink".to_owned(),
        }
    }

    /// **lane: rcore/f1-strictwin-tie — may GHOSTDAG decide this all-economic tie past
    /// `palw_reorg_strict_economic_win`?** `true` exactly when (1) `candidate` is heavier than
    /// `prev_sink` in GHOSTDAG's own order — blue work, then hash, the [`SortableBlock`] order the sink
    /// search pops in — (2) `candidate` does not LOWER the sink's DAA (`daa(candidate) ≥
    /// incumbent_daa`), and (3) the reorg is SHALLOW: the incumbent's selected chain above its common
    /// chain ancestor with `candidate` spans at most [`PALW_REORG_SHALLOW_TIE_DAA_V1`] DAA ticks
    /// (`incumbent_daa − daa(ancestor) ≤ D`). The question `palw_reorg_strict_economic_win_v1` asks,
    /// and only on an all-economic tie.
    ///
    /// **Why a depth, and why on the incumbent's side.** A rule that keeps the incumbent on a tie is a
    /// rule about arrival order, so two honest nodes racing a slot keep different sinks; a rule that
    /// hands every tie to GHOSTDAG hands it to whoever piles more blue work in private, which is the
    /// heartbeat double spend the fence closed. Honest races are resolved within a propagation delay —
    /// well inside one slot, two ticks with the lead cap's burst — while a double spend must reverse a
    /// payment someone has watched confirm. The depth is read on the INCUMBENT's own chain (this node's
    /// history since the fork), which a challenger cannot shorten: blocks it gets merged into that
    /// chain only lengthen it. So a shallow tie is decided the way every honest node decides it, and a
    /// payment deeper than `D` ticks under this node's sink keeps the fence's protection whatever blue
    /// work is piled against it in one release. The depth alone does not pin the sink's DAA — a denser
    /// tying branch that still holds the payment could move the sink one tick DOWN first, and a second
    /// release reach under the lowered window (measured: three ticks, and a branch weighted without
    /// ticks would not stop there) — which is what (2) is for: a tie never lowers the sink's DAA, so no
    /// chain of releases reaches deeper than `D` ticks under the highest sink a tie produced
    /// (`a_shallow_tie_never_lowers_the_sink_so_chained_releases_stop_at_two`).
    ///
    /// Conservative on every read it cannot make — an unreadable header or GHOSTDAG row, a reachability
    /// miss, a chain that runs past [`PALW_REORG_SHALLOW_TIE_WALK_V1`] blocks before leaving the depth —
    /// is `false` (the tie keeps the incumbent, the strict answer). Reads only this node's committed
    /// stores, so every node holding the same DAG answers the same.
    ///
    /// **The one copy** (rcore/cap-s1). rcore/cap-weight (3aa4abec4) carried this body without (2) as
    /// `palw_capacity_shallow_ghostdag_win_v1` for ADR-0160 F-W; cap-s1 dropped that copy, so past F-W —
    /// which requires strict-win at or below it — this function answers, (2) included, and F-W is never
    /// armable on a build whose rule lacks (2) (`PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1`, read
    /// below and by `Params::validate_palw_capacity_weight_cap_v1`).
    ///
    /// [`PALW_REORG_SHALLOW_TIE_DAA_V1`]: kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1
    /// [`PALW_REORG_SHALLOW_TIE_WALK_V1`]: kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_WALK_V1
    pub(crate) fn palw_reorg_shallow_ghostdag_win_v1(&self, candidate: BlockHash, prev_sink: BlockHash, incumbent_daa: u64) -> bool {
        use kaspa_consensus_core::palw_fork_authority_v2::{
            PALW_REORG_SHALLOW_TIE_DAA_V1, PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1, PALW_REORG_SHALLOW_TIE_WALK_V1,
        };
        let heavier = match (self.ghostdag_store.get_blue_work(candidate), self.ghostdag_store.get_blue_work(prev_sink)) {
            (Ok(c), Ok(p)) => SortableBlock::new(candidate, c) > SortableBlock::new(prev_sink, p),
            _ => false,
        };
        if !heavier {
            return false;
        }
        // **A shallow tie never LOWERS the sink's DAA** (the ratchet `strict_win_honest_race` measured:
        // two chained releases reversed three ticks without this). The window is read at the incumbent's
        // DAA, so a tie-flip onto a tying branch with FEWER ticks would move the window itself down and a
        // second release could reach under it; keeping the incumbent there pins the window to the highest
        // DAA a tie ever moved the sink to. Siblings on one parent set share one DAA, so an honest slot
        // race moves the sink sideways, never back.
        let Ok(candidate_daa) = self.headers_store.get_daa_score(candidate) else { return false };
        if PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1 && candidate_daa < incumbent_daa {
            return false;
        }
        let floor = incumbent_daa.saturating_sub(PALW_REORG_SHALLOW_TIE_DAA_V1);
        let mut block = prev_sink;
        for _ in 0..PALW_REORG_SHALLOW_TIE_WALK_V1 {
            let Ok(daa) = self.headers_store.get_daa_score(block) else { return false };
            if daa < floor {
                return false;
            }
            match self.reachability_service.try_is_chain_ancestor_of(block, candidate) {
                Ok(true) => return true,
                Ok(false) => {}
                Err(_) => return false,
            }
            match self.ghostdag_store.get_selected_parent(block) {
                Ok(parent) if parent != block && parent != kaspa_consensus_core::blockhash::ORIGIN => block = parent,
                _ => return false,
            }
        }
        false
    }

    /// ADR-0125: the round blocks among `tips`, where the lane is configured.
    pub(in crate::pipeline::virtual_processor) fn palw_round_tips(&self, tips: &[BlockHash]) -> Vec<BlockHash> {
        if self.palw_execution_lane.is_none() {
            return Vec::new();
        }
        tips.iter().copied().filter(|tip| self.ghostdag_manager.is_round_block(*tip)).collect()
    }

    /// ADR-0125: a round block stands for its anchor — its selected parent, never itself a round
    /// block — wherever the chain's own blocks are being chosen among.
    pub(in crate::pipeline::virtual_processor) fn palw_project_round_block(&self, block: BlockHash) -> BlockHash {
        if self.palw_execution_lane.is_some() && self.ghostdag_manager.is_round_block(block) {
            self.ghostdag_store.get_selected_parent(block).unwrap_or(block)
        } else {
            block
        }
    }

    /// ADR-0125: [`Self::palw_project_round_block`] over a set, reduced back to an antichain (an
    /// anchor is often an ancestor of another candidate — the sink above all).
    pub(super) fn palw_project_round_blocks(&self, blocks: Vec<BlockHash>) -> Vec<BlockHash> {
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
    pub(super) fn palw_add_round_parents(
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
        // **RFC-0008 v2: past `palw_exec_payload_v2` a chain block names no EXEC block as a parent** — the lane reaches the chain
        // through the anchor trailer of the coinbase. So virtual takes no round parent once its OWN DAA score is at the fence (the score
        // the header rule judges the template at). A round tip adds no DAA term (it is outside the DAA set, and its past brings no chain
        // block virtual's selected chain had not: its anchor is on that chain), so virtual's score is the one its non-round parents
        // give. That score is computed only within one mergeset of the fence (a child's score is at most its selected parent's plus its
        // mergeset): further below, nothing is asked, and from the fence on, nothing is taken. The X8R review replaced the coarse
        // `selected + mergeset_size_limit` cut-off, which stopped the v1 lane up to 181 DAA (~8 hours on testnet-12) early on an
        // upgraded node. (Dormant on every shipped preset: `palw_exec_v2` is `None`.)
        if let Some(fence) = self.palw_exec_v2 {
            let selected_daa = self.headers_store.get_daa_score(selected_parent).unwrap_or_default();
            if fence.is_active(selected_daa) {
                return;
            }
            if fence.is_active(selected_daa.saturating_add(self.mergeset_size_limit.get(selected_daa)).saturating_add(1)) {
                let base = self.ghostdag_manager.ghostdag(&virtual_parents[..]);
                match self.window_manager.block_daa_window(&base) {
                    Ok(window) if !fence.is_active(window.daa_score) => {}
                    _ => return,
                }
            }
        }
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
            for member in ghostdag.unordered_mergeset_without_selected_parent() {
                if !self.ghostdag_manager.is_round_block(member) {
                    continue;
                }
                match envelope_of(member) {
                    Some(envelope) => members.push((envelope.round, envelope.permit_index, envelope.bond)),
                    None => malformed = true,
                }
            }
            if malformed
                || ghostdag.mergeset_size() as u64 - members.len() as u64
                    > self.mergeset_size_limit.get(self.headers_store.get_daa_score(selected_parent).unwrap())
            {
                continue;
            }
            if palw_execution_mergeset_rule_v1(None, &members, width, lane.max_per_mergeset).is_err() {
                continue;
            }
            virtual_parents.push(tip);
        }
    }

    /// **The stamp a template built on a virtual carries — or the lead cap's refusal of it.** One
    /// computation for the template builder and for [`Self::palw_lead_cap_virtual_parents`], so the
    /// policy leaves out exactly what the builder would refuse.
    ///
    /// `max(now, median + 1)`, raised to the slot where the clock floor is armed and the block steps
    /// (H5; `clock` is the virtual's own clock decision, `None` only where neither the floor nor the
    /// cap is armed). Where `lead_capped` and the block steps (`granted`), a stamp more than
    /// [`PALW_CLOCK_LEAD_CAP_MS`](kaspa_consensus_core::palw_clock_cursor_v1::PALW_CLOCK_LEAD_CAP_MS)
    /// past `now` is `ClockStepTemplateTooFarAhead`: every header stage, this node's included, would
    /// refuse the block (`ClockLeadTooFarAhead`), so the builder waits instead. The heartbeat adapter
    /// then stamps a beat at `max(template, slot)` and hands that stamp back as `earliest`, and the
    /// miner mints no beat while `earliest` is past its own clock.
    pub(super) fn palw_template_stamp_v1(
        &self,
        clock: Option<kaspa_consensus_core::palw_clock_cursor_v1::PalwClockStepV1>,
        lead_capped: bool,
        past_median_time: u64,
        now: u64,
    ) -> Result<u64, RuleError> {
        let proposed = u64::max(past_median_time + 1, now);
        let timestamp = match clock {
            Some(clock) if self.palw_clock_floor.is_some() => clock.floor_stamp(proposed),
            _ => proposed,
        };
        if lead_capped
            && clock.is_some_and(|clock| clock.granted)
            && let Err(latest) = kaspa_consensus_core::palw_clock_cursor_v1::palw_clock_lead_admits_v1(timestamp, now)
        {
            return Err(RuleError::ClockStepTemplateTooFarAhead(timestamp, latest, timestamp - latest));
        }
        Ok(timestamp)
    }

    /// **Would a template built on a virtual with this GHOSTDAG data step the clock past the lead
    /// cap?** — [`Self::palw_template_stamp_v1`]'s refusal, asked of a candidate virtual. `false`
    /// wherever the cap is not in force, the virtual steps nothing, or a window cannot be computed
    /// (the builder then answers for itself).
    pub(super) fn palw_virtual_steps_past_lead_cap(&self, ghostdag_data: &GhostdagData, now: u64) -> bool {
        let Some(fence) = self.palw_clock_lead_cap else {
            return false;
        };
        let Ok(window) = self.window_manager.block_daa_window(ghostdag_data) else {
            return false;
        };
        if !fence.is_active(window.daa_score) || !window.clock.granted {
            return false;
        }
        let Ok((past_median_time, _)) = self.window_manager.calc_past_median_time(ghostdag_data) else {
            return false;
        };
        self.palw_template_stamp_v1(Some(window.clock), true, past_median_time, now).is_err()
    }

    /// **The lead cap's local policy on virtual's parents** (`palw_clock_lead_cap`; the lead-cap
    /// review's HIGH): a granted beat whose step every template would have to stamp past this node's
    /// clock plus 132 s is not merged YET.
    ///
    /// A virtual whose mergeset carries a granted beat steps the clock, so every template built on it
    /// — every lane's — is a step, stamped `max(now, median + 1, slot)`. A producer can hold the
    /// past-median time more than 132 s ahead with blocks that move no clock (they keep the full
    /// tolerance), and a beat granted BELOW that median — its own past carries none of the push:
    /// withheld and released after it, or merely relayed late — is still a valid block and a tip.
    /// Merged, it would make every template past the cap: the builder would refuse them all and the
    /// node would produce nothing, in any lane, until wall time caught up — up to the tolerance less
    /// the cap after the last push, and longer while the push is renewed, the pusher the only
    /// producer meanwhile. So virtual leaves such a beat out, as it leaves out a tip past the merge
    /// depth: a local choice of which tips to merge, never a verdict. The beat stays a valid tip, and
    /// the first virtual resolved once wall time reaches the step's stamp less the cap merges it (if
    /// the merge depth has passed it first, it is never merged, and the next beat takes the slot).
    /// Every lane keeps building non-step blocks meanwhile, and what stalls is the DAA clock alone —
    /// which the push holds anyway, since any step is stamped above the median: DAA-denominated
    /// windows keep their wall length and no readiness row lapses.
    ///
    /// Left out, in order, until a template on the rest steps nothing past the cap: every parent
    /// that brings a heartbeat into the mergeset (the sink excepted), then every parent but the sink
    /// (the case of a sink that is itself the granted beat, merging the push beside it). A sink that
    /// is a granted beat stepping past the cap on its own past is left as it is — the builder then
    /// refuses until wall time catches up; that takes a push inside the beat's own median window
    /// that the beat, stamped within the cap of the node that admitted it, did not already exceed.
    ///
    /// Its warnings are rate-limited to one a minute (a push is renewed block by block, and the
    /// virtual is resolved at every block); the limiter is log state only, read by nothing else.
    pub(in crate::pipeline::virtual_processor) fn palw_lead_cap_virtual_parents(
        &self,
        virtual_parents: Vec<BlockHash>,
        virtual_ghostdag_data: GhostdagData,
        pruning_point: BlockHash,
    ) -> (Vec<BlockHash>, GhostdagData) {
        /// The wall-clock millisecond this policy last warned at. Log state only.
        static LAST_WARNED_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let warn_due = |now: u64| {
            use std::sync::atomic::Ordering::Relaxed;
            let last = LAST_WARNED_MS.load(Relaxed);
            now >= last.saturating_add(60_000) && LAST_WARNED_MS.compare_exchange(last, now, Relaxed, Relaxed).is_ok()
        };
        if self.palw_clock_lead_cap.is_none() || virtual_parents.len() < 2 {
            return (virtual_parents, virtual_ghostdag_data);
        }
        let is_beat = |hash: BlockHash| {
            self.headers_store
                .get_header(hash)
                .is_ok_and(|header| header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID)
        };
        let sink = virtual_ghostdag_data.selected_parent;
        let merged_beats: Vec<BlockHash> =
            virtual_ghostdag_data.unordered_mergeset_without_selected_parent().filter(|hash| is_beat(*hash)).collect();
        // No beat in the mergeset: nothing is granted, and there is nothing to decide.
        if merged_beats.is_empty() && !is_beat(sink) {
            return (virtual_parents, virtual_ghostdag_data);
        }
        // The builder's clock (the wall clock outside a test), so the policy leaves out exactly what the builder would refuse.
        let now = self.template_now();
        if !self.palw_virtual_steps_past_lead_cap(&virtual_ghostdag_data, now) {
            return (virtual_parents, virtual_ghostdag_data);
        }
        let _prune_guard = self.pruning_lock.blocking_read();
        let without_beats: Vec<BlockHash> = virtual_parents
            .iter()
            .copied()
            .filter(|parent| {
                *parent == sink || !merged_beats.iter().any(|beat| self.reachability_service.is_dag_ancestor_of(*beat, *parent))
            })
            .collect();
        let mut narrower = Vec::with_capacity(2);
        if without_beats.len() < virtual_parents.len() {
            narrower.push(without_beats);
        }
        narrower.push(vec![sink]);
        for kept in narrower {
            let (kept, kept_ghostdag_data) = self.remove_bounded_merge_breaking_parents(kept, pruning_point);
            if !self.palw_virtual_steps_past_lead_cap(&kept_ghostdag_data, now) {
                let left_out = virtual_parents.iter().filter(|parent| !kept.contains(parent)).count();
                if warn_due(now) {
                    warn!(
                        "[palw-clock] virtual leaves {left_out} tip(s) unmerged for now: merged, a beat grants a clock step every \
                         template would stamp more than 132 s past this node's clock (the past-median time stands that far ahead — \
                         palw_clock_lead_cap); non-step blocks are built meanwhile and the DAA clock waits for wall time"
                    );
                } else {
                    debug!("[palw-clock] virtual leaves {left_out} tip(s) unmerged for now (palw_clock_lead_cap)");
                }
                return (kept, kept_ghostdag_data);
            }
        }
        if warn_due(now) {
            warn!(
                "[palw-clock] the sink {sink} is a beat whose clock step every template would stamp more than 132 s past this \
                 node's clock (palw_clock_lead_cap): templates wait for wall time"
            );
        }
        (virtual_parents, virtual_ghostdag_data)
    }

    /// **ADR-0142: the clock's decision for a block built on `parents`** — the cursor its window
    /// derives at its selected parent's score and whether its mergeset carries a granted beat.
    ///
    /// Computed exactly as the header stage computes it for the block those parents make (GHOSTDAG,
    /// then `block_daa_window`, whose `clock` is the half of `palw_clock_step_v1` the DAA score does
    /// not read), so a template, the adapter and the validator cannot answer differently. Headers and
    /// GHOSTDAG data are immutable once written, so the answer for a template's parents is the same
    /// whenever it is asked — a template that has gone stale is still judged against its own slot.
    pub(crate) fn palw_clock_step_for_parents(
        &self,
        parents: &[BlockHash],
    ) -> Result<kaspa_consensus_core::palw_clock_cursor_v1::PalwClockStepV1, RuleError> {
        let ghostdag = self.ghostdag_manager.ghostdag(parents);
        Ok(self.window_manager.block_daa_window(&ghostdag)?.clock)
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

    /// **Lane SCAN: the executions the sink's PALW state holds** — see
    /// [`kaspa_consensus_core::palw_exec_view_v1::palw_recent_executions_v1`]. Read at the sink, like
    /// every `palw_*` node read.
    pub fn palw_recent_executions_v1(
        &self,
        limit: usize,
    ) -> Option<(Vec<kaspa_consensus_core::palw_exec_view_v1::PalwExecutionRowV1>, usize)> {
        let sink = self.virtual_stores.read().state.get().unwrap().ghostdag_data.selected_parent;
        let (_at, state) = self.palw_v2_state_at(sink)?;
        Some(kaspa_consensus_core::palw_exec_view_v1::palw_recent_executions_v1(&state, limit))
    }

    /// **RFC-0009 L2: the fork-choice opening of `block`'s post-state** — the state this node weighs `block` by
    /// ([`Self::palw_candidate_state_v2_checked`], the same walk the deep-reorg gate takes), its opening and the root it is committed as.
    /// `Err` names why this node cannot weigh the block; it never answers from a guess. The walk composes this node's own deltas, and a
    /// block gets a delta only when its chain validation passed — its header's committed root included (a mismatch disqualifies the block
    /// and every chain descendant before the transition runs) — so an opening is served only for a block whose whole selected chain this
    /// node validated. That is what an issuer signs when it attests a served root.
    pub fn palw_fork_choice_opening_v1_impl(
        &self,
        block: BlockHash,
        extras: &kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceExtrasRequestV2,
    ) -> Result<kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceServedEntryV1, String> {
        use kaspa_consensus_core::palw_fork_choice_commitment_v1::{PalwForkChoiceOpeningV1, PalwForkChoiceServedEntryV1};
        if self.palw_state_params_v2.is_none() {
            return Err("this network has no PALW V2 state".to_string());
        }
        let header = self.headers_store.get_header(block).map_err(|_| format!("the node holds no header for block {block}"))?;
        let sink_daa = self.virtual_stores.read().state.get().map(|v| v.daa_score).unwrap_or(0);
        if header.daa_score.saturating_add(PALW_STATE_PROOF_MAX_LAG_DAA) < sink_daa {
            return Err(format!(
                "block {block} is {} DAA behind the sink, more than the {PALW_STATE_PROOF_MAX_LAG_DAA} this node rebuilds state for",
                sink_daa - header.daa_score
            ));
        }
        let state = self.palw_candidate_state_v2_checked(block).map_err(|fault| match fault {
            PalwWeighFaultV2::NoOpinion => format!("this node cannot weigh block {block}: it holds no PALW history to it"),
            PalwWeighFaultV2::StoreUnreadable => format!("this node's own PALW store would not read for block {block}"),
            PalwWeighFaultV2::NativeCapacityUnavailable => format!("this node cannot admit native state memory for block {block}"),
        })?;
        let opening = PalwForkChoiceOpeningV1::of(&state).ok_or_else(|| "the genesis state has no fork-choice leaf".to_string())?;
        if opening.leaf.block != block {
            return Err(format!("the state this node walked to names block {}, not {block}", opening.leaf.block));
        }
        let committed_root = self.palw_committed_root_v1(&state);
        let committed_form = kaspa_consensus_core::palw_fork_choice_commitment_v1::palw_fork_choice_committed_at_v1(
            self.palw_fork_choice_commitment_v1,
            Some(opening.leaf.daa_score),
        );
        // RFC09: the point's own committed form also selects the opening grammar.
        let params = &self.native_redemption_params_v33;
        let committed_leaf = kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::palw_committed_leaf_bytes_v2(&state, params).unwrap_or_default();
        use kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::{PalwCommitFormV1, palw_rule_e_commit_form_v1};
        let form = palw_rule_e_commit_form_v1(params, opening.leaf.daa_score);
        let rule_e_window = extras.rule_e_window_above_blue.and_then(|from| {
            match form {
                PalwCommitFormV1::RuleEV2 => kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v2::prove_window_suffix_v1(&state, params, from),
                PalwCommitFormV1::RuleEV3 => kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::prove_window_suffix_v3(&state, params, from),
                _ => None,
            }.map(|w| borsh::to_vec(&w).expect("a window opening serializes"))
        });
        let registry_openings = if form == PalwCommitFormV1::RuleEV2 {
            extras.registry_bonds.iter()
                .take(kaspa_consensus_core::palw_fork_choice_commitment_v1::PALW_FORK_CHOICE_MAX_REGISTRY_BONDS_V2)
                .map(|bond| borsh::to_vec(&kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v2::prove_registry_membership_v1(&state, bond))
                    .expect("a registry opening serializes"))
                .collect()
        } else if form == PalwCommitFormV1::RuleEV3 && !extras.registry_bonds.is_empty() {
            let entries = kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::palw_rule_e_registry_entries_v3(&state);
            extras
                .registry_bonds
                .iter()
                .take(kaspa_consensus_core::palw_fork_choice_commitment_v1::PALW_FORK_CHOICE_MAX_REGISTRY_BONDS_V2)
                .map(|bond| {
                    borsh::to_vec(&kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::palw_rule_e_open_registry_v3(&entries, bond))
                        .expect("a registry opening serializes")
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(PalwForkChoiceServedEntryV1 {
            header: (*header).clone(),
            opening,
            committed_root,
            committed_form,
            committed_leaf,
            rule_e_window,
            registry_openings,
        })
    }

    /// Op 203's body: the openings of `blocks` (or of the sink and every tip), with the node's sink and tips.
    pub fn palw_fork_choice_openings_v1_impl(
        &self,
        blocks: &[BlockHash],
        sink: BlockHash,
        tips: Vec<BlockHash>,
        extras: &kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceExtrasRequestV2,
    ) -> Result<kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceServedV1, String> {
        use kaspa_consensus_core::palw_fork_choice_commitment_v1::{
            PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1, PalwForkChoiceServedV1,
        };
        if self.palw_state_params_v2.is_none() {
            return Err("this network has no PALW V2 state".to_string());
        }
        if blocks.len() > PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1 {
            return Err(format!("at most {PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1} blocks per request (got {})", blocks.len()));
        }
        let mut named: Vec<BlockHash> =
            if blocks.is_empty() { std::iter::once(sink).chain(tips.iter().copied()).collect() } else { blocks.to_vec() };
        let mut seen = std::collections::HashSet::new();
        named.retain(|b| seen.insert(*b));
        named.truncate(PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1);
        let entries = named.into_iter().map(|block| (block, self.palw_fork_choice_opening_v1_impl(block, extras))).collect();
        // The gate's inputs as this node holds them (`dns_bft_gate_refusal` reads exactly these): the rollout stage and the
        // confirmed anchor. An unreadable or unwritten DNS state is reported as nothing confirmed in Bootstrap — what the gate reads it as.
        // Past the RFC-0012 retirement at the sink the gate abstains (`dns_bft_gate_refusal` returns before reading the row), so the
        // facts served are the abstaining ones — never the row frozen at the fence (lane R12, 2026-10-11; ported 2026-10-15).
        let retired = self.headers_store.get_daa_score(sink).is_ok_and(|daa| self.dns_retired_at(daa));
        let dns_gate = self.dns_params.as_ref().map(|_| {
            let state = if retired { None } else { self.dns_state_store.read().get().ok() };
            kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwDnsGateFactV1 {
                stage_active: state.as_ref().is_some_and(|s| s.rollout_stage == DnsRolloutStage::Active),
                confirmed_anchor: state
                    .as_ref()
                    .filter(|s| s.last_dns_confirmed_anchor != Hash64::default())
                    .map(|s| (s.last_dns_confirmed_anchor, s.last_dns_confirmed_anchor_daa_score)),
            }
        });
        Ok(PalwForkChoiceServedV1 { sink, tips, entries, dns_gate })
    }
}

impl VirtualStateProcessor {
    pub(crate) fn palw_monetary_material_release_delta_v1(
        &self,
        root: Hash64,
        consent: Hash64,
        tail: Hash64,
        block: BlockHash,
    ) -> Result<bool, String> {
        let (_, delta) = self.palw_state_v2_store.read().delta_of_retained_v1(block).map_err(|e| e.to_string())?;
        if delta.point.block != block {
            return Err("material release delta substitutes its carrying block".into());
        }
        for entry in &delta.entries {
            if let kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::KernelRouteRow {
                table: 120,
                key,
                old: Some(bytes),
                new: None,
            } = entry
                && key.as_slice() == root.as_byte_slice()
            {
                let row: kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1 =
                    borsh::from_slice(bytes).map_err(|e| e.to_string())?;
                if row.terms.root == root && row.consent == consent && row.tail_block == Some(tail) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
