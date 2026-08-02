use super::BlockBodyProcessor;
use crate::{
    errors::{BlockProcessResult, RuleError},
    model::stores::{ghostdag::GhostdagStoreReader, headers::HeaderStoreReader, statuses::StatusesStoreReader},
    processes::{
        transaction_validator::{
            TransactionValidator,
            errors::TxRuleError,
            tx_validation_in_header_context::{LockTimeArg, LockTimeType},
        },
        window::WindowManager,
    },
};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_database::prelude::StoreResultExt;
use kaspa_txscript::script_class::ScriptClass;
use once_cell::unsync::Lazy;
use std::sync::Arc;

fn palw_certificate_matches_lifecycle(
    header_version: u16,
    lifecycle_certificate: Option<kaspa_hashes::Hash64>,
    named: kaspa_hashes::Hash64,
) -> bool {
    header_version < kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION || lifecycle_certificate == Some(named)
}

#[inline]
fn palw_v4_parent_completion_required(header_version: u16, parent_count: usize) -> bool {
    header_version >= kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION && parent_count != 0
}

impl BlockBodyProcessor {
    pub fn validate_body_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        self.check_parent_bodies_exist(block)?;
        self.ensure_palw_v4_parent_provenance(block)?;
        self.check_palw_overlay_activation(block)?;
        self.check_coinbase_blue_score_and_subsidy(block)?;
        self.check_palw_ticket(block)?;
        self.check_block_transactions_in_context(block)
    }

    /// Header-v4 consumes selected-parent lifecycle facts produced only after UTXO acceptance. The
    /// body worker therefore asks the single virtual worker to finish every direct parent and sleeps
    /// on a bounded completion channel. Header-v3 and older return before allocating or sending
    /// anything, preserving their existing scheduling and persisted bytes.
    ///
    /// Every failure here is [`RuleError::PalwParentProvenanceUnavailable`], which
    /// [`BlockBodyProcessor::error_marks_block_invalid`] keeps OUT of the persisted `StatusInvalid`
    /// set — deliberately, and load-bearing for IBD liveness. Not one of these outcomes is a verdict
    /// on this block's own body; each is a property of the local node's point of view at this instant:
    ///  * the selected parent is `StatusDisqualifiedFromChain` — a *cache* of a past UTXO validation
    ///    (`resolve_virtual` re-runs it on reorg, see "Revalidating previously disqualified
    ///    selected-chain block"), so it can flip back to valid;
    ///  * the parent is `StatusInvalid` — possibly a STALE mark from an older binary under different
    ///    rules; cascading it is exactly what `--reset-invalid-marks` recovery has to undo;
    ///  * the parent is still header-only / UTXO-pending, or sits below THIS node's virtual finality
    ///    point — pure delivery-ordering and point-of-view conditions;
    ///  * the virtual worker is shutting down, or a store read failed — node lifecycle, not consensus.
    ///
    /// Marking the block invalid for any of them persists a point-of-view answer as a permanent one:
    /// the block is then never re-requested (the body-sync list retains only header-only blocks) and
    /// its children fail forever on parents that can no longer be satisfied — the 2026-07-29
    /// testnet-200 dead loop, whose first link was a v4 child of a beacon-disqualified block getting
    /// `StatusInvalid` written for a condition that was never its own fault. Rejecting without marking
    /// keeps the block re-requestable, so the node recovers on its own once the parent resolves.
    fn ensure_palw_v4_parent_provenance(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        if !palw_v4_parent_completion_required(block.header.version, block.header.direct_parents().len()) {
            return Ok(());
        }
        let unavailable = |m: String| RuleError::PalwParentProvenanceUnavailable(m);
        let selected_parent = self
            .ghostdag_store
            .get_selected_parent(block.hash())
            .map_err(|error| unavailable(format!("Header-v4 selected-parent lookup failed: {error}")))?;
        let (result_tx, result_rx) = crossbeam_channel::bounded(1);
        self.sender
            .send(crate::pipeline::deps_manager::VirtualStateProcessingMessage::EnsurePalwParents {
                parents: block.header.direct_parents().to_vec(),
                selected_parent,
                daa_score: block.header.daa_score,
                result: result_tx,
            })
            .map_err(|_| unavailable("virtual worker stopped before Header-v4 parent completion".to_string()))?;
        result_rx
            .recv()
            .map_err(|_| unavailable("virtual worker dropped Header-v4 parent completion".to_string()))?
            .map_err(unavailable)
    }

    /// Keep the reserved PALW subnetworks consensus-inert until the PALW hard fork. Isolation
    /// validation deliberately knows no block DAA score, so it can decode future PALW payloads for
    /// relay/mempool policy; block acceptance must still preserve the legacy `SubnetworksDisabled`
    /// result before activation. Without this contextual fence, an upgraded node could accept a
    /// pre-fork block that every legacy node rejects.
    fn check_palw_overlay_activation(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        // ADR-MA §17.1: the Compute Set registry band (0x40-0x44) has its OWN activation fence,
        // independent of the PALW overlay fence below. Every shipped preset keeps it at
        // `u64::MAX`, so registry transactions stay block-invalid everywhere today.
        if block.header.daa_score < self.palw_compute_registry_activation_daa_score
            && let Some(tx) = block.transactions.iter().find(|tx| tx.subnetwork_id.is_palw_compute_registry())
        {
            return Err(RuleError::TxInContextFailed(tx.id(), TxRuleError::SubnetworksDisabled(tx.subnetwork_id.clone())));
        }
        if block.header.daa_score >= self.palw_activation_daa_score {
            return Ok(());
        }
        if let Some(tx) = block.transactions.iter().find(|tx| tx.subnetwork_id.palw_tx_kind().is_some()) {
            return Err(RuleError::TxInContextFailed(tx.id(), TxRuleError::SubnetworksDisabled(tx.subnetwork_id.clone())));
        }
        Ok(())
    }

    /// ADR-0039 §14.2/§18.1 — the store-resolved acceptance check for an algo-4 (PALW) block: resolve
    /// the leaf/certificate the header names from the PALW overlay stores and enforce the five
    /// store+epoch-resolvable clauses (nullifier / proof-type / leaf-active / cert-active / interval)
    /// via the shared [`verify_palw_ticket_store_facts`]. This is the block-processing side of the
    /// `verify_palw_ticket ↔ PalwStore` bridge; body validation (not header validation) is the correct
    /// stage because the binding lives in body-derived, accepted-overlay state.
    ///
    /// PALW is inert on mainnet, testnet-10, simnet and devnet, where the activation score is
    /// `u64::MAX`. The three PALW presets use an activation score of 0. `palw_compute_work_scale = 0`
    /// limits fork-choice credit rather than admission; header validation enforces the component-work
    /// and compute-cap rules after GHOSTDAG.
    ///
    /// **C5 flip (§14.4 decision B), safe subset.** The check now resolves the batch lifecycle against the
    /// **past-relative overlay view carried at the block's selected parent** (`view(SP)`, built by
    /// `commit_palw_overlay_view` in SP's own body-commit batch), advanced to this block's epoch, rather
    /// than against any tip-global / virtual-commit state. This closes the consensus split the C4 panel
    /// proved for the batch gate: whether the batch is present / Active / certified / non-revoked / in-
    /// window is now a **pure function of the block's past** (view(SP) is guaranteed present by the body-
    /// DAG downward closure, and `advance_epoch(epoch(B))` is deterministic). The immutable per-leaf/-cert
    /// CONTENT is read from the content-addressed `DbPalwStore` (write-once, fork-safe), feeding the pure
    /// clauses 1–5 via [`verify_palw_ticket_store_facts`].
    ///
    /// The work-credit closure the panel required is preserved: `ghostdag()` credits an algo-4 block's
    /// compute work at HEADER stage, but body-DAG downward closure (`check_parent_bodies_exist` +
    /// `body_tips_store`) keeps a body-invalid ticket's block out of every body-valid past, so its
    /// header-credited work never reaches an authoritative sink.
    ///
    /// Clauses 6 and 9 are enforced here. Both are derived from the finality-buried anchor through
    /// `resolve_palw_lagged_anchor`, using only headers and reachability from the block's past.
    ///
    /// Related construction and validation properties:
    ///  * the header-stage difficulty check is lane-aware:
    ///    `HeaderProcessor::calculate_palw_lane_difficulty_bits` filters the DAA window to the header's
    ///    own lane;
    ///  * the mining template constructs algo-4 headers:
    ///    `ConsensusApi::palw_build_algo4_template` derives `bits` through that same shared helper, so
    ///    construction == validation holds for the field.
    fn check_palw_ticket(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        use kaspa_consensus_core::palw::verify_palw_ticket_store_facts;
        use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA;
        let header = &block.header;
        if header.daa_score < self.palw_activation_daa_score || header.pow_algo_id != POW_ALGO_ID_PALW_REPLICA {
            return Ok(());
        }
        let reject = |m: String| RuleError::PalwTicketInvalid(m);
        let epoch = header.daa_score / self.palw_epoch_length_daa.max(1);
        let a = &self.palw_batch_admission;

        // Past-relative coordinate: the batch-lifecycle facts as of the selected parent's view. `view(SP)`
        // is written in SP's own body-commit batch (block-keyed, guaranteed present for a body-valid SP),
        // and B resolves against it BEFORE folding B's own mergeset deltas (a batch certified in B's own
        // mergeset is not yet block-eligible for B's ticket). It is `advance_epoch`d to B's epoch so the
        // Certified→Active transition is evaluated at epoch(B), not frozen at epoch(SP) (review finding).
        let sp = self.ghostdag_store.get_selected_parent(block.hash()).map_err(|e| reject(format!("no selected parent: {e:?}")))?;

        // The clause-6 finality-buried anchor + the K5 lagged buried samples, resolved FIRST: the view's
        // activation gate and clauses 6/9/10 all consume them (one anchor resolve + one sampling walk per
        // algo-4 block). Fail-closed if no lag-ready buried anchor exists yet in this history.
        let dns_params = self
            .dns_params
            .as_ref()
            .ok_or_else(|| reject("PALW active without DNS params (re-genesis misconfiguration)".to_string()))?;
        let anchor =
            crate::processes::palw::resolve_palw_lagged_anchor(&self.headers_store, &self.reachability_service, dns_params, sp)
                .ok_or_else(|| reject("no finality-buried DNS anchor in this block's past".to_string()))?;
        // Read the buried anchor's header once — its frozen facts feed clause 6, and its retained
        // palw_beacon_seed (the LAGGED R_E, authenticated at the anchor's own virtual stage, SLICE 2) is
        // the eligibility beacon for clause 9.
        let anchor_header =
            self.headers_store.get_header(anchor.anchor_hash).map_err(|e| reject(format!("anchor header read failed: {e:?}")))?;
        let samples = self.palw_buried_epoch_samples(anchor.anchor_hash);

        let mut view = self
            .palw_overlay_view_store
            .view(sp)
            .map_err(|e| reject(format!("overlay view read failed: {e:?}")))?
            .map(|v| (*v).clone())
            .ok_or_else(|| {
                // Bug report #6 layer 2: above the suture fence an algo-4 block can now REACH this
                // check with a disqualified selected parent, which has no accepted-view row. That
                // is a point-of-view condition, so it must not poison the block — report it as
                // provenance-unavailable (rejected now, never marked invalid, re-requested later)
                // and let the algo-3 backbone re-validate the selected parent first. Below the
                // fence the provenance gate still refuses such a block earlier, so this arm keeps
                // its original meaning and its original error.
                if header.daa_score >= self.palw_suture_disqualified_selected_parent_daa_score {
                    RuleError::PalwParentProvenanceUnavailable(format!(
                        "selected parent {sp} has no accepted PALW overlay view yet (its own UTXO \
                         classification is unresolved from this node's virtual state); the algo-4 \
                         ticket cannot be checked until it resolves"
                    ))
                } else {
                    reject("no PALW overlay view at selected parent (batch not resolvable in this block's past)".to_string())
                }
            })?;
        // K5 (§11.3): the Certified→Active flip is gated on the SAME lagged signal the view builder
        // (`commit_palw_overlay_view`) gates on — both compute it from this block's selected parent, so
        // the carried view and this in-memory advance can never diverge on an activation net.
        view.advance_epoch_gated(
            epoch,
            a.registration_lead_epochs,
            a.audit_window_epochs,
            kaspa_consensus_core::palw::palw_lagged_activation_open(&samples),
        );
        // Fork-relative batch gate (§18.2): present, Active, certified, non-revoked, windows open at epoch(B).
        // ADR-0040 SS-04: `header.daa_score` carries the §9.5 non-retroactive revocation cutoff. It is
        // the same consensus-derived value clause 5 pins the target interval to (see the TGT-01 note
        // below), so the revocation gate inherits that unforgeability rather than adding a new one.
        let lifecycle = view.resolvable_batch(&header.palw_batch_id, epoch, header.daa_score).ok_or_else(|| {
            reject(format!("batch {:?} not block-eligible at epoch {epoch} in this block's past", header.palw_batch_id))
        })?;
        // Header-v4's pruning sidecar is self-contained around the lifecycle's canonical certificate
        // reference. Allowing another same-batch certificate would make an archival node accept bytes
        // a fresh pruned node was never required to import. V3 preserves the historical multi-cert
        // semantics byte-for-byte; the exact pin is a re-genesis-only v4 rule.
        if !palw_certificate_matches_lifecycle(header.version, lifecycle.cert_hash, header.palw_epoch_certificate_hash) {
            return Err(reject(format!(
                "Header-v4 certificate {} is not the selected-parent lifecycle certificate for batch {}",
                header.palw_epoch_certificate_hash, header.palw_batch_id
            )));
        }

        // Content-addressed leaf/cert blob (write-once, fork-safe) → the pure clause-1..5 binding.
        let resolved = crate::processes::palw::resolve_palw_binding(
            header.palw_batch_id,
            header.palw_leaf_index,
            header.palw_epoch_certificate_hash,
            header.palw_target_daa_interval,
            &*self.palw_store,
        )
        .map_err(|e| reject(format!("{e:?}")))?;

        // Static-audit finding C-03 — the header must name the SAME Compute Set the leaf was
        // produced under.
        //
        // Everything downstream of the header trusts `header.palw_compute_set_id` on its own word:
        // pre-PoW resolves the per-set descriptor and difficulty from it, UTXO acceptance checks
        // that the header's policy/plan fork-locally govern IT, and GHOSTDAG credits compute work
        // through ITS weight factor. Nothing compared it to the leaf, so a producer could compute
        // under a cheap set A, declare an expensive set B in the header, and collect B's
        // difficulty allocation and weight — with a valid whole-header authorization, because the
        // leaf authority signing the header is the same party doing the substitution. The honest
        // template builder copies the leaf's value across (`palw_mint.rs`), which is precisely why
        // this was invisible: a template builder is not a validator.
        //
        // SCOPED to Header-v5, and that scope is the rule, not a convenience. Below
        // `PALW_COMPUTE_SET_HEADER_VERSION` the three registry references are consensus-FORCED to
        // zero (`pre_ghostdag_validation`, NonZeroPalwHeaderFieldsBeforeActivation) while an
        // Object-V2 leaf's set id is required NON-zero — so a blanket equality would reject every
        // honest pre-registry algo-4 block, which is precisely what it did when first written. It
        // is also unnecessary there: with the header's id pinned to zero there is no per-set
        // difficulty to borrow, no policy/plan to name and no weight factor to inflate, because
        // every consumer of the field is inert below the fence.
        //
        // Enforced HERE rather than in the resolver so the error names the mismatch. On v5 the
        // compare is total: a v2 leaf's set id is non-zero by `validate_public_leaf`, and a v5
        // header only reaches this point after pre-PoW resolved a registered descriptor for its own
        // id (so its id is non-zero too).
        if header.version >= kaspa_consensus_core::constants::PALW_COMPUTE_SET_HEADER_VERSION
            && header.palw_compute_set_id != resolved.receipt_v3_compute_set_id
        {
            return Err(reject(format!(
                "clause 14: header names compute set {} but leaf {}:{} was produced under {} — a leaf may not be \
                 mined under another set's difficulty allocation, policy or compute-work weight",
                header.palw_compute_set_id, header.palw_batch_id, header.palw_leaf_index, resolved.receipt_v3_compute_set_id
            )));
        }

        let cert_active = resolved.cert_activation_epoch <= epoch && epoch < resolved.cert_expiry_epoch;

        // ADR-0040 **P1-7 (TGT-01) — REFUTED. The interval is already consensus-derived.**
        //
        // The audit reported that `palw_target_daa_interval` is "self-reported by the header rather than
        // derived by consensus", voiding I-3. Verified against the enforced rule, that is wrong: clause 5
        // in `verify_palw_ticket_store_facts` requires `h_daa_score == binding.target_daa_interval`, and
        // `daa_score` is itself consensus-validated post-GHOSTDAG as a function of the block's past. So a
        // miner cannot name a favourable interval — declaring anything other than its own DAA score is
        // rejected, and it does not choose its DAA score.
        //
        // A derivation from `slot_digest` WAS implemented here and then removed, because it added a
        // SECOND interval rule that contradicted clause 5: the slot value is unrelated to `daa_score`, so
        // every honest block failed with `IntervalMismatch`. The lesson is the finding itself — the gap
        // was in the audit's model, not in the code, and "fixing" it would have broken a correct rule.
        // ADR-0040 TGT-02: `target_daa_interval` / `slot_digest` were the unused derivation helpers
        // behind that reverted attempt. They had zero production callers, so they were DELETED rather
        // than left as a second, drifting source of truth — the interval is pinned to `daa_score` here
        // and nowhere else.
        // Clauses 1–5 (nullifier / proof-type / leaf-active / cert-active / interval).
        verify_palw_ticket_store_facts(
            &header.palw_ticket_nullifier,
            header.palw_proof_type,
            header.daa_score,
            &resolved.binding,
            cert_active,
            epoch,
        )
        .map_err(|rej| reject(format!("{rej:?}")))?;

        // Clause 6 (chain_commit, C6 SLICE 3) — PURE FUNCTION OF THE PAST, no virtual read: the header's
        // chain_commit must equal the value derived from the FINALITY-BURIED DNS anchor resolved from this
        // block's selected-parent chain (headers + reachability only, resolved above). This is the
        // fork-binding that stops a miner from choosing chain_commit as a re-roll nonce (I-4). Design
        // departure (recorded): the anchor is selected by BURIAL alone (the re-genesis band gate requires
        // lag > reorg horizon), not by the stake-depth DNS confirmation, which needs the virtual-only bond
        // view and is DNS-liveness, orthogonal to fork-binding.
        let anchor_facts = kaspa_consensus_core::palw::BeaconDnsAnchor {
            hash: anchor.anchor_hash,
            blue_score: anchor.anchor_blue_score,
            daa_score: anchor.anchor_daa_score,
            overlay_root: anchor_header.overlay_commitment_root,
        };
        let expected_chain_commit = kaspa_consensus_core::palw::chain_commit(
            &anchor_facts.hash,
            &kaspa_consensus_core::palw::dns_finality_certificate_hash_v1(&anchor_facts),
            header.palw_target_daa_interval,
            self.palw_network_id,
        );
        if header.palw_chain_commit != expected_chain_commit {
            return Err(reject("clause 6: chain_commit does not match the finality-buried DNS anchor".to_string()));
        }

        // Clause 9 (eligibility DRAW, C6 SLICE 4) — the PALW "PoW", a PURE FUNCTION OF THE PAST: the
        // lagged R_E is the buried anchor's own retained palw_beacon_seed (present on every node, pruning-
        // surviving, reorg-stable, seed-authenticated at the anchor's virtual stage). The draw digest binds
        // the consensus-derived chain_commit (clause 6 already forced header==expected), the target
        // interval, the leaf identity, and the ticket nullifier; acceptance requires
        // Uint512(digest) <= target_512(bits) AND nonce == low64(nullifier) — so the nonce is pinned to the
        // ticket (I-3, no grinding) and the draw cannot be steered via a chosen bits (clause 6 fixed
        // chain_commit; the header bits gate is the lane-difficulty slice). One leaf, one draw.
        let eligibility_digest = kaspa_consensus_core::palw::eligibility_hash(
            self.palw_network_id,
            &anchor_header.palw_beacon_seed,
            &expected_chain_commit,
            header.palw_target_daa_interval,
            &header.palw_batch_id,
            header.palw_leaf_index,
            &resolved.leaf_hash,
            &header.palw_ticket_nullifier,
        );
        if !kaspa_consensus_core::palw::palw_eligibility_win(
            &eligibility_digest,
            header.bits,
            header.nonce,
            &header.palw_ticket_nullifier,
        ) {
            return Err(reject("clause 9: eligibility draw not satisfied".to_string()));
        }

        // Clause 10 (K5, ADR-0039 §11.3) — the LAGGED HALT INDICATOR, a pure function of the past: the
        // buried per-epoch seed-carry run below the clause-6 anchor certifies (hash-chain argument —
        // Healthy always advances the seed, degraded/halted carry it verbatim) that no Healthy epoch
        // occurred across the run; a run longer than the grace window certifies the newest BURIED epoch
        // was Halted, and the compute lane is closed to algo-4 blocks. NOT the block's own epoch mode —
        // it trails by ~the attestation lag: it admits ~lag epochs at halt onset (their provider pay is
        // zeroed by the ReplicaPalwHalted reward gate and their weight is bounded by the permanent 4H
        // cap) and keeps rejecting for ~lag epochs after a Healthy recovery (fail-closed; the algo-4
        // template constructor MUST consult `palw_template_lane_open` on the SAME carry run or it would
        // build self-rejecting blocks). Full teeth: body-invalid ⇒ unmergeable — unlike the
        // chain-candidacy-only S2 `PalwLaneHalted` rule this clause layers with.
        if kaspa_consensus_core::palw::palw_seed_carry_run(&samples) > self.palw_beacon_grace_epochs {
            return Err(reject("clause 10: buried beacon-seed carry run exceeds grace (lane halted, lagged)".to_string()));
        }

        // Clause 13 (ADR-0045 D3-b) — the MINT-TIME PCPB presence/equality re-check, fail-closed.
        //
        // Clauses 11/12 already ran, in full, at the leaf's ACCEPTANCE (the leaf-chunk arm re-derives
        // the challenge and re-runs the dispatch evidence before `insert_leaf`). This clause does not
        // repeat that work — the heavy witnesses live only on the wire and are never persisted — it
        // re-asserts the two facts a mint depends on:
        //
        //   * the leaf's committed snapshot/assignment roots still EQUAL the on-chain commitment of
        //     its anchor epoch, and
        //   * the self arm's A-commit anchor is still registered at-or-before its declared epoch.
        //
        // Why that is sufficient rather than a weaker second gate: `anchor − k`, `anchor + Δ` and the
        // anchor's own registration all sit DEEPER than the chunk's acceptance. A reorg that changes
        // any of them necessarily unwinds the chunk acceptance too, and the new chain re-runs clauses
        // 11/12 in the new context. A reorg that does not unwind acceptance cannot move these values.
        // The equality here is the defence line for that argument, plus the bounded-window rule: a
        // ticket whose anchor epoch has aged out of the retained window stops minting (fail-closed)
        // rather than resolving against a substituted live value.
        {
            let leaf = &resolved.pcpb;
            let anchor = if leaf.dispatch_kind == kaspa_consensus_core::palw::PALW_DISPATCH_KIND_SELF_SERIAL {
                leaf.a_commit_epoch
            } else {
                leaf.issued_epoch
            };
            let snapshot_epoch = anchor
                .checked_sub(self.palw_snapshot_lag_epochs)
                .ok_or_else(|| reject("clause 13: leaf anchor epoch predates the snapshot lag".to_string()))?;
            let commitment = self
                .palw_pcpb_store
                .snapshot_at(snapshot_epoch)
                .map_err(|e| reject(format!("clause 13: provider snapshot read failed: {e:?}")))?
                .ok_or_else(|| reject(format!("clause 13: no retained provider snapshot for epoch {snapshot_epoch} (fail-closed)")))?;
            if commitment.snapshot_root != leaf.provider_snapshot_root || commitment.assignment_root != leaf.assignment_proof_root {
                return Err(reject("clause 13: leaf-committed PCPB roots disagree with the on-chain snapshot".to_string()));
            }
            let draw_epoch = anchor
                .checked_add(self.palw_post_commit_delta_epochs)
                .ok_or_else(|| reject("clause 13: leaf anchor epoch + Δ overflows".to_string()))?;
            if self
                .palw_beacon_store_for_pcpb
                .beacon_seed_at(draw_epoch)
                .map_err(|e| reject(format!("clause 13: beacon seed read failed: {e:?}")))?
                .is_none()
            {
                return Err(reject(format!("clause 13: no retained post-commit beacon seed for epoch {draw_epoch} (fail-closed)")));
            }
            if leaf.dispatch_kind == kaspa_consensus_core::palw::PALW_DISPATCH_KIND_SELF_SERIAL {
                let registered = self
                    .palw_pcpb_store
                    .acommit_epoch(&leaf.a_commit)
                    .map_err(|e| reject(format!("clause 13: A-commit registry read failed: {e:?}")))?;
                if registered.is_none_or(|row| row > leaf.a_commit_epoch) {
                    return Err(reject("clause 13: the leaf's A-commit anchor is not registered at-or-before its epoch".to_string()));
                }
            }
        }

        // ADR-0040 **P1-6 (AUTH-01/02/03)** — per-block ticket authorization.
        //
        // Without this, a winning algo-4 header discloses its raw `ticket_nullifier` and any OBSERVER
        // can restamp the same winning draw onto unlimited competing blocks with different parents,
        // transactions and payout. On a shared network that is a consensus-level DoS surface pointed at
        // other people's nodes, which is why it gates T-shared rather than merely activation.
        //
        // The authorization rides in THIS block's own body (subnetwork 0x38), because it authorizes this
        // block and so must be verifiable here rather than after acceptance.
        {
            use kaspa_consensus_core::palw::{PALW_AUTHORIZATION_MLDSA87_CONTEXT, PalwBlockAuthorizationV1};
            use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION;
            use kaspa_txscript::verify_mldsa87_with_context;

            // ADR-0040 (AUTH-TXSHAPE) — the authorization must be the LAST transaction, not merely
            // present somewhere. `authed_root` is computed over the FILTERED transaction list, so moving
            // the authorization among n transactions leaves `authed_root` (and therefore the signature)
            // untouched while permuting the real merkle leaf order — n distinct block hashes per
            // authorization, at zero cost on a lane with no proof-of-work. Pinning the index is the
            // other half of pinning the transaction's bytes; together they make the real
            // `hash_merkle_root` a deterministic function of (authed tx list, authorization payload).
            //
            // Both in-tree producers already append it last, after the template is finalized. Any future
            // template code that sorts or reorders transactions must exclude the authorization from the
            // sort and re-append it.
            let auth_tx = match block.transactions.last() {
                Some(tx) if tx.subnetwork_id == SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION => tx,
                _ => {
                    return Err(reject(
                        "clause 7: algo-4 block must carry its ticket authorization as its LAST transaction".to_string(),
                    ));
                }
            };
            let auth = <PalwBlockAuthorizationV1 as borsh::BorshDeserialize>::try_from_slice(&auth_tx.payload)
                .map_err(|_| reject("clause 7: malformed ticket authorization payload".to_string()))?;

            // `palw_authorization_hash` commits to the authorization object, so the header cannot name
            // one authorization and the body carry another.
            if auth.hash() != header.palw_authorization_hash {
                return Err(reject("clause 7: authorization does not match header.palw_authorization_hash".to_string()));
            }
            // ADR-0040 (AUTH-02, second half) — the authorization transaction must be in CANONICAL
            // SHAPE. Without this the tx is a free variation axis of its own: it is permitted to carry
            // zero inputs (see `check_transaction_inputs_count`), and a zero-input transaction is
            // VACUOUSLY finalized for any `lock_time`, so 2^64 distinct authorization txids — hence 2^64
            // distinct `hash_merkle_root`s, hence 2^64 distinct block hashes — all share one
            // `authed_root` and one signature. Pinning the shape makes the real merkle root a
            // DETERMINISTIC FUNCTION of `authed_root` plus the authorization blob, so it has exactly one
            // legal value. The payload is compared against its own borsh re-encoding so a non-canonical
            // encoding of the same struct is rejected too.
            //
            // The AUTHORITATIVE statement of this rule is `check_palw_block_authorization_shape` in
            // transaction validation-in-isolation (which also covers the mempool/BBT surface and carries
            // the per-field rationale); it is RESTATED here, where the authorization is consumed, so
            // clause 7 does not silently depend on a rule stated in another file. The two must agree —
            // if you widen one, widen the other.
            if auth_tx.version != kaspa_consensus_core::constants::TX_VERSION
                || !auth_tx.inputs.is_empty()
                || !auth_tx.outputs.is_empty()
                || auth_tx.lock_time != 0
                || auth_tx.gas != 0
                || auth_tx.mass() != 0
                || borsh::to_vec(&auth).map(|v| v != auth_tx.payload).unwrap_or(true)
            {
                return Err(reject("clause 7: ticket authorization transaction is not in canonical shape".to_string()));
            }
            // The authorization must be ABOUT this block — TOTALLY, not over a list of fields. The bound
            // value is the block's OWN header preimage (see `palw_header_preimage_commitment`), so every
            // header field is covered, including the five that are only ever checked at the virtual/UTXO
            // stage and therefore go unchecked entirely on a block that never becomes a chain candidate.
            //
            // The bound merkle root deliberately EXCLUDES the authorization transaction itself — the
            // authorization cannot commit to a root that contains the authorization, that is circular.
            // Excluding exactly one identifiable transaction, whose shape is pinned just above, keeps the
            // binding total over everything the miner actually chooses.
            let authed_txs: Vec<_> =
                block.transactions.iter().filter(|tx| tx.subnetwork_id != SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION).cloned().collect();
            if authed_txs.len() + 1 != block.transactions.len() {
                return Err(reject("clause 7: exactly one ticket authorization transaction is permitted".to_string()));
            }
            let authed_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(authed_txs.iter());
            if !auth.binds_header(self.palw_network_id, header, &authed_root) {
                return Err(reject("clause 7: authorization does not bind this header".to_string()));
            }
            // AUTH-03: the signing key must be the authority the LEAF declared. Without this the field
            // `leaf.ticket_authority_pk_hash` names an authority nothing checks.
            if !auth.binds_leaf_authority(&resolved.ticket_authority_pk_hash) {
                return Err(reject("clause 7: authorization key is not the leaf's declared ticket authority".to_string()));
            }
            let digest = auth.signing_hash(self.palw_network_id);
            if !matches!(
                verify_mldsa87_with_context(
                    &auth.authority_public_key,
                    digest.as_bytes().as_slice(),
                    &auth.signature,
                    PALW_AUTHORIZATION_MLDSA87_CONTEXT,
                ),
                Ok(true)
            ) {
                return Err(reject("clause 7: ticket authorization signature invalid".to_string()));
            }
        }

        // Clause 8 (compute headroom) is enforced post-GHOSTDAG in header validation; the lane-bits half
        // of clause 7 lands with the lane-aware header difficulty gate (its own slice).
        Ok(())
    }

    fn check_block_transactions_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        // Use lazy evaluation to avoid unnecessary work, as most of the time we expect the txs not to have lock time.
        let lazy_pmt_res = Lazy::new(|| self.window_manager.calc_past_median_time_for_known_hash(block.hash()));

        for tx in block.transactions.iter() {
            let lock_time_arg = match TransactionValidator::get_lock_time_type(tx) {
                LockTimeType::Finalized => LockTimeArg::Finalized,
                LockTimeType::DaaScore => LockTimeArg::DaaScore(block.header.daa_score),
                // We only evaluate the pmt calculation when actually needed
                LockTimeType::Time => LockTimeArg::MedianTime((*lazy_pmt_res).clone()?),
            };
            if let Err(e) = self.transaction_validator.validate_tx_in_header_context(tx, lock_time_arg) {
                return Err(RuleError::TxInContextFailed(tx.id(), e));
            };
        }
        Ok(())
    }

    fn check_parent_bodies_exist(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let statuses_read_guard = self.statuses_store.read();
        let mut invalid: Vec<BlockHash> = Vec::new();
        let mut missing: Vec<BlockHash> = Vec::new();
        for parent in block.header.direct_parents().iter().copied() {
            match statuses_read_guard.get(parent).optional().unwrap() {
                None => missing.push(parent),
                Some(BlockStatus::StatusInvalid) => invalid.push(parent),
                Some(s) if !s.has_block_body() => missing.push(parent),
                Some(_) => {}
            }
        }
        // An invalid-marked parent is a permanent local condition, not a delivery-ordering gap: during
        // IBD the body-sync list retains only header-only blocks, so an invalid parent is never
        // re-requested and every retry fails here again for the same hashes. Reporting it as
        // MissingParents makes that loop undiagnosable from the logs; name the real obstruction.
        if !invalid.is_empty() {
            return Err(RuleError::InvalidParentBodies(invalid));
        }
        if !missing.is_empty() {
            return Err(RuleError::MissingParents(missing));
        }

        Ok(())
    }

    fn check_coinbase_blue_score_and_subsidy(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        match self.coinbase_manager.deserialize_coinbase_payload(&block.transactions[0].payload) {
            Ok(data) => {
                if data.blue_score != block.header.blue_score {
                    return Err(RuleError::BadCoinbasePayloadBlueScore(data.blue_score, block.header.blue_score));
                }

                let expected_subsidy = self.coinbase_manager.calc_block_subsidy(block.header.daa_score);

                if data.subsidy != expected_subsidy {
                    return Err(RuleError::WrongSubsidy(expected_subsidy, data.subsidy));
                }

                // kaspa-pq PQ-only invariant: the coinbase payload's miner script must itself be
                // ML-DSA P2PKH. The block's own coinbase OUTPUTS are PQ-class-checked in isolation,
                // but the payload miner script is a SEPARATE field that descendant blocks read into
                // their reward fan-out (`expected_coinbase_transaction` pays this block's merged
                // reward to `reward_data.script_public_key` = this miner script). A non-PQ script
                // here would force every descendant's coinbase to carry a non-PQ output, which the
                // PQ output-class rule rejects — a reward-path / liveness poison rather than a
                // mintable non-PQ UTXO. Reject it at the source. Gated by the PQ script policy,
                // active from genesis on every kaspa-pq network (`pq_activation_daa_score = 0`).
                if self.transaction_validator.resolved_script_policy(block.header.daa_score).pq_only
                    && !ScriptClass::from_script(&data.miner_data.script_public_key).is_pq_standard()
                {
                    return Err(RuleError::NonPqCoinbasePayloadScript);
                }

                Ok(())
            }
            Err(e) => Err(RuleError::BadCoinbasePayload(e)),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        config::ConfigBuilder,
        consensus::test_consensus::TestConsensus,
        constants::TX_VERSION,
        errors::RuleError,
        model::stores::ghostdag::GhostdagStoreReader,
        processes::{transaction_validator::errors::TxRuleError, window::WindowManager},
    };
    use kaspa_consensus_core::{
        BlockHash, // PR-9.5e: block ids are Hash64
        api::ConsensusApi,
        coinbase::MinerData,
        config::params::MAINNET_PARAMS,
        dns_finality::p2pkh_mldsa87_spk,
        merkle::calc_hash_merkle_root,
        subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_BEACON_COMMIT},
        tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint},
    };
    use kaspa_core::assert_match;

    #[test]
    fn header_v4_rejects_valid_same_batch_alternate_certificate() {
        // Both hashes may identify independently quorum-verified certificates for the same batch;
        // Header-v3 retains that historical choice, while v4 pins the lifecycle reference so the
        // pruning sidecar is a complete and consensus-equivalent blob set.
        let lifecycle = kaspa_hashes::Hash64::from_bytes([0x41; 64]);
        let alternate = kaspa_hashes::Hash64::from_bytes([0x42; 64]);
        assert!(super::palw_certificate_matches_lifecycle(
            kaspa_consensus_core::constants::PALW_HEADER_VERSION,
            Some(lifecycle),
            alternate
        ));
        assert!(!super::palw_certificate_matches_lifecycle(
            kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION,
            Some(lifecycle),
            alternate
        ));
        assert!(super::palw_certificate_matches_lifecycle(
            kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION,
            Some(lifecycle),
            lifecycle
        ));
    }

    #[tokio::test]
    async fn validate_body_in_context_test() {
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| p.deflationary_phase_daa_score = p.genesis.daa_score + 2)
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        consensus.add_header_only_block_with_parents(1.into(), vec![config.genesis.hash]).await.unwrap();

        {
            let block = consensus.build_block_with_parents_and_transactions(2.into(), vec![1.into()], vec![]);
            // We expect a missing parents error since the parent is header only.
            assert_match!(body_processor.validate_body_in_context(&block.to_immutable()), Err(RuleError::MissingParents(_)));
        }

        let valid_block = consensus.build_block_with_parents_and_transactions(3.into(), vec![config.genesis.hash], vec![]);
        consensus.validate_and_insert_block(valid_block.to_immutable()).virtual_state_task.await.unwrap();
        {
            let mut block = consensus.build_block_with_parents_and_transactions(2.into(), vec![3.into()], vec![]);
            block.transactions[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            // kaspa-pq: pre-deflationary subsidy == year-1 per-block subsidy at 10 BPS (table[0].div_ceil(10)).
            assert_match!(
                consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 205972571);

            // The second time we send an invalid block we expect it to be a known invalid.
            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::KnownInvalid)
            );
        }

        {
            let mut block = consensus.build_block_with_parents_and_transactions(4.into(), vec![3.into()], vec![]);
            block.transactions[0].payload[0..8].copy_from_slice(&(100_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::BadCoinbasePayloadBlueScore(_, _))
            );
        }

        {
            let mut block = consensus.build_block_with_parents_and_transactions(5.into(), vec![3.into()], vec![]);
            block.transactions[0].payload = vec![];
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::BadCoinbasePayload(_))
            );
        }

        let valid_block_child = consensus.build_block_with_parents_and_transactions(6.into(), vec![3.into()], vec![]);
        consensus.validate_and_insert_block(valid_block_child.clone().to_immutable()).virtual_state_task.await.unwrap();
        {
            // The block DAA score is 2 (>= deflationary_phase_daa_score), so the subsidy comes from
            // the decay table: month 0 => table[0].div_ceil(10) = 370_468_345 sompi.
            let mut block = consensus.build_block_with_parents_and_transactions(7.into(), vec![6.into()], vec![]);
            block.transactions[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());
            assert_match!(consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 205972571);
        }

        {
            // Check that the same daa score as the block's daa score or higher fails, but lower passes.
            let tip_daa_score = valid_block_child.header.daa_score + 1;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 8.into(), tip_daa_score + 1, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 9.into(), tip_daa_score, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 10.into(), tip_daa_score - 1, 0, true).await;

            let valid_block_child_gd = consensus.ghostdag_store().get_data(valid_block_child.header.hash).unwrap();
            let (valid_block_child_gd_pmt, _) = consensus.window_manager().calc_past_median_time(&valid_block_child_gd).unwrap();
            let past_median_time = valid_block_child_gd_pmt + 1;

            // Check that the same past median time as the block's or higher fails, but lower passes.
            let tip_daa_score = valid_block_child.header.daa_score + 1;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 11.into(), past_median_time + 1, 0, false)
                .await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 12.into(), past_median_time, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 13.into(), past_median_time - 1, 0, true)
                .await;

            // We check that if the transaction is marked as finalized it'll pass for any lock time.
            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                14.into(),
                past_median_time + 1,
                u64::MAX,
                true,
            )
            .await;

            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 15.into(), tip_daa_score + 1, u64::MAX, true)
                .await;
        }

        consensus.shutdown(wait_handles);
    }

    /// A parent carrying a local `StatusInvalid` mark must surface as `InvalidParentBodies` — the
    /// operator-diagnosable variant — and must NOT mark the child invalid in turn.
    ///
    /// This is the IBD ordering: every header lands before any body is validated, so when a body
    /// later invalidates the parent, the child's header is already stored and only the BODY stage
    /// ever sees the invalid parent. Before this variant existed the child failed as generic
    /// `MissingParents`, which the body-sync list can never satisfy (invalid blocks are not
    /// header-only, so they are never re-requested) — an infinite, undiagnosable IBD retry loop
    /// whenever a database carries stale invalid-marks from an older binary. The no-cascade half
    /// is what keeps `--reset-invalid-marks` recovery complete: only the originally-marked blocks
    /// need re-evaluation, not an ever-growing cone of descendants.
    #[tokio::test]
    async fn invalid_parent_bodies_is_reported_and_does_not_cascade() {
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| p.deflationary_phase_daa_score = p.genesis.daa_score + 2)
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();

        // Headers first (the IBD header phase): parent 1 on genesis, child 2 on 1. Both accepted.
        consensus.add_header_only_block_with_parents(1.into(), vec![config.genesis.hash]).await.unwrap();
        consensus.add_header_only_block_with_parents(2.into(), vec![1.into()]).await.unwrap();

        // Body phase: the parent's body violates a rule (wrong subsidy) and is marked StatusInvalid.
        {
            let mut parent = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
            parent.transactions[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            parent.header.hash_merkle_root = calc_hash_merkle_root(parent.transactions.iter());
            assert_match!(
                consensus.validate_and_insert_block(parent.to_immutable()).virtual_state_task.await,
                Err(RuleError::WrongSubsidy(_, _))
            );
        }

        // The child's body is itself valid, but its parent is now invalid-marked: the distinct
        // variant fires and names the poisoned parent.
        let child = consensus.build_block_with_parents_and_transactions(2.into(), vec![1.into()], vec![]);
        assert_match!(
            consensus.validate_and_insert_block(child.clone().to_immutable()).virtual_state_task.await,
            Err(RuleError::InvalidParentBodies(parents)) if parents == vec![1.into()]
        );

        // No cascade: resubmission fails the same way — NOT with KnownInvalid — proving the child
        // was not marked invalid by the failure above.
        assert_match!(
            consensus.validate_and_insert_block(child.to_immutable()).virtual_state_task.await,
            Err(RuleError::InvalidParentBodies(_))
        );

        consensus.shutdown(wait_handles);
    }

    /// The full `--reset-invalid-marks` recovery arc over a poisoned status: a parent marked
    /// `StatusInvalid` (here by a genuine body rejection standing in for an older binary's stale
    /// verdict) blocks its child's body forever; after the reset pass the parent is header-only
    /// again, a re-offered VALID body for it is accepted, and the child follows — the exact
    /// unstick path for a node whose database was poisoned by rules that have since changed.
    #[tokio::test]
    async fn reset_invalid_marks_unsticks_a_poisoned_body_sync() {
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| p.deflationary_phase_daa_score = p.genesis.daa_score + 2)
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();

        consensus.add_header_only_block_with_parents(1.into(), vec![config.genesis.hash]).await.unwrap();
        consensus.add_header_only_block_with_parents(2.into(), vec![1.into()]).await.unwrap();

        // Poison: the parent's body is rejected and the mark persists.
        {
            let mut parent = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
            parent.transactions[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            parent.header.hash_merkle_root = calc_hash_merkle_root(parent.transactions.iter());
            assert_match!(
                consensus.validate_and_insert_block(parent.to_immutable()).virtual_state_task.await,
                Err(RuleError::WrongSubsidy(_, _))
            );
        }
        let child = consensus.build_block_with_parents_and_transactions(2.into(), vec![1.into()], vec![]);
        assert_match!(
            consensus.validate_and_insert_block(child.clone().to_immutable()).virtual_state_task.await,
            Err(RuleError::InvalidParentBodies(_))
        );

        // Recovery: the pass clears exactly the one mark, back to header-only (its header is stored).
        assert_eq!(consensus.consensus_clone().reset_invalid_marks(), (1, 0));

        // A valid body for the parent is now accepted again, and the child unsticks.
        let parent = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
        consensus.validate_and_insert_block(parent.to_immutable()).virtual_state_task.await.unwrap();
        consensus.validate_and_insert_block(child.to_immutable()).virtual_state_task.await.unwrap();

        consensus.shutdown(wait_handles);
    }

    /// kaspa-pq PQ-only invariant (audit Finding A): the coinbase **payload** miner script must
    /// be ML-DSA P2PKH, independently of the coinbase outputs. A non-PQ payload miner script
    /// would flow into descendant blocks' reward fan-out and force a non-PQ reward output (which
    /// the PQ output-class rule rejects) — a reward-path / liveness poison. Verifies the source
    /// block is rejected at body validation even though its coinbase carries no outputs (so the
    /// isolation output-class check cannot catch it).
    #[tokio::test]
    async fn coinbase_payload_miner_script_must_be_pq() {
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        // Control: an ML-DSA P2PKH miner script in the coinbase payload passes body validation.
        let good = consensus.build_utxo_valid_block_with_parents(
            1.into(),
            vec![config.genesis.hash],
            MinerData::new(p2pkh_mldsa87_spk(&[0x07; 64]), vec![]),
            vec![],
        );
        body_processor.validate_body_in_context(&good.to_immutable()).unwrap();

        // Finding A: a non-PQ miner script (here a bare OP_TRUE) in the payload is rejected,
        // although the genesis-child coinbase has no outputs for the output-class rule to flag.
        let non_pq_spk = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51]));
        let bad = consensus.build_utxo_valid_block_with_parents(
            2.into(),
            vec![config.genesis.hash],
            MinerData::new(non_pq_spk, vec![]),
            vec![],
        );
        assert_match!(body_processor.validate_body_in_context(&bad.to_immutable()), Err(RuleError::NonPqCoinbasePayloadScript));

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn palw_overlay_subnetworks_are_fenced_before_activation() {
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();
        let palw_tx = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_PALW_BEACON_COMMIT, 0, vec![]);
        let block = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![palw_tx]);

        assert_match!(
            body_processor.check_palw_overlay_activation(&block.to_immutable()),
            Err(RuleError::TxInContextFailed(_, TxRuleError::SubnetworksDisabled(id)))
                if id == SUBNETWORK_ID_PALW_BEACON_COMMIT
        );

        consensus.shutdown(wait_handles);
    }

    /// ADR-MA §17.1: the Compute Set registry band (0x40-0x44) has its OWN fence, independent of
    /// the PALW overlay fence — a registry tx is block-invalid below
    /// `palw_compute_registry_activation_daa_score` even on a PALW-ACTIVE preset, and every
    /// shipped preset keeps that fence at `u64::MAX`.
    #[tokio::test]
    async fn compute_registry_subnetworks_are_fenced_before_activation() {
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL;
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();
        let registry_tx = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL, 0, vec![]);
        let block = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![registry_tx]);

        assert_match!(
            body_processor.check_palw_overlay_activation(&block.to_immutable()),
            Err(RuleError::TxInContextFailed(_, TxRuleError::SubnetworksDisabled(id)))
                if id == SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL
        );

        consensus.shutdown(wait_handles);
    }

    #[test]
    fn parent_completion_message_is_v4_only_and_genesis_free() {
        assert!(!super::palw_v4_parent_completion_required(kaspa_consensus_core::constants::PALW_HEADER_VERSION, 2));
        assert!(!super::palw_v4_parent_completion_required(kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION, 0));
        assert!(super::palw_v4_parent_completion_required(kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION, 1));
    }

    async fn check_for_lock_time_and_sequence(
        consensus: &TestConsensus,
        parent: BlockHash,
        block_hash: BlockHash,
        lock_time: u64,
        sequence: u64,
        should_pass: bool,
    ) {
        // The block DAA score is 2, so the subsidy should be calculated according to the deflationary stage.
        let block = consensus.build_block_with_parents_and_transactions(
            block_hash,
            vec![parent],
            vec![Transaction::new(
                TX_VERSION,
                vec![TransactionInput::new(TransactionOutpoint::new(1.into(), 0), vec![], sequence, 0)],
                vec![],
                lock_time,
                SUBNETWORK_ID_NATIVE,
                0,
                vec![],
            )],
        );

        if should_pass {
            consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.unwrap();
        } else {
            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::TxInContextFailed(_, e)) if matches!(e, TxRuleError::NotFinalized(_)));
        }
    }
}
