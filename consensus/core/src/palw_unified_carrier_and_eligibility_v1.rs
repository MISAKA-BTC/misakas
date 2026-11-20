//! **RFC-0048, RFC-0049, RFC-0050, RFC-0055 (`UAI1`), & RFC-0058 (`SWC1` §10–§11) — Unified Consensus Eligibility Gate (`MatureEligible` / `EligibilitySnapshot`), Same-Batch Refutation Priority, and Global 1 BPS Carrier Control (`UnifiedCarrierV1`).**
//!
//! This module implements:
//! 1. **`Swc1EligibilitySnapshotV1` & Unified Consumer Gate (`RFC-0048`, `RFC-0058 §10.2–§10.4`)**:
//!    Ensures that unverified candidates (`Registered..VerificationPending`), unmatured claims
//!    (`VerifiedPendingMaturity`), refuted/aborted claims, and `LateEvidenceStopsNewUses` claims
//!    contribute **zero** to DAA work samples, safe/live weight, safe frontier, settlement depth,
//!    Rule E participation, settled anchor, SWC1 work rewards, and `ExecCredit` mint/reserve/consume.
//! 2. **Transitive Dependency Closure & `Q` Hold Propagation (`RFC-0049`, `RFC-0050`)**:
//!    Propagates refutations, `Q` holds, and late-evidence stops across the transitive dependency
//!    closure to invalidate dependent Originals and `ExecCredit` records immediately.
//! 3. **Deterministic 5-Phase Same-Batch Priority (`apply_canonical_batch_v1`, `RFC-0050 §6`, `RFC-0058 §11.2`)**:
//!    Enforces wire-order-independent execution:
//!    (1) record notices/materials/responses/evidence ->
//!    (2) apply refutations, `Q` holds, dependency revocations, credit invalidations ->
//!    (3) evaluate finite case terminals, expiries, refunds, and maturity promotions ->
//!    (4) derive `EligibilitySnapshot` and execute mint/reserve/permit/consume ->
//!    (5) commit effects and reorg undo journal.
//! 4. **Global 1 BPS Outer Carrier Control (`UnifiedCarrierV1` & `GlobalPermitBookV1`, `RFC-0055 §9`)**:
//!    Releases 1 permit every `1,000 ms` on `RateClock` (`global_slot = rate_clock_ms / 1_000`) with
//!    bounded slot lifetime `L` across all carrier kinds (`RealAttempt`, `ExecCarrier`, `BaseBlock`,
//!    `Heartbeat`, `CourtOnlyCarrier`), with one-time consumption per `(network, policy_generation, global_slot)`
//!    and full reorg undo/replay support.

use crate::palw_an1_input_binding_and_panel_court_v1::{
    An1BatchOutcomeV1, An1CourtEvidenceV1, An1ErrorV1, An1FinalUndoEntryV1, An1LedgerStateV1, An1PanelReviewOutcomeV1,
};
use crate::palw_claim_admission_difficulty_v1::Cad1FlowVectorV1;
use crate::palw_staged_work_commitments_v1::{
    SWC1_MAX_FIELD_BYTES_V1, Swc1ErrorV1, Swc1LedgerStateV1, Swc1RightKindV1, Swc1RightsUseRecordV1, Swc1VerificationStageV1,
    lp_append_checked_v1, swc1_h256_v1,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Outer carrier slot interval on `RateClock` (`1,000 ms` = 1 BPS across all carrier types, `RFC-0055 §9.1`).
pub const UAI1_CARRIER_SLOT_MS_V1: u64 = 1_000;

/// Domain tag for `EligibilitySnapshot` commitment (`RFC-0058 §10.3`).
pub const TAG_SWC1_ELIGIBILITY_SNAPSHOT_V1: &[u8] = b"MISAKA/PALW/SWC1/ELIGIBILITY-SNAPSHOT";
pub const TAG_SWC1_DEP_CLOSURE_V1: &[u8] = b"MISAKA/PALW/SWC1/DEP-CLOSURE";

/// Outer carrier kinds governed by the unified 1 BPS permit book (`RFC-0055 §9.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum UnifiedCarrierKindV1 {
    RealAttempt = 1,
    ExecCarrier = 2,
    BaseBlock = 3,
    Heartbeat = 4,
    CourtOnlyCarrier = 5,
}

/// Outer carrier envelope (`UnifiedCarrierV1`, `RFC-0055 §9.2`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedCarrierV1 {
    pub version: u16,
    pub network_id: [u8; 32],
    pub policy_generation: u64,
    pub global_slot: u64,
    pub carrier_kind: UnifiedCarrierKindV1,
    pub header_id: [u8; 32],
    pub rate_clock_ms: u64,
    pub payload_wire_bytes: u64,
    pub payload_flow: Cad1FlowVectorV1,
}

/// Errors returned by the unified eligibility gate, 5-phase batch executor, and 1 BPS permit book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnifiedEligibilityErrorV1 {
    Swc1(Swc1ErrorV1),
    InvalidVersion(u16),
    OriginalNotFound([u8; 32]),
    OriginalNotMatureEligible { original_id: [u8; 32], stage: Swc1VerificationStageV1 },
    OriginalHeldInQuarantineQ([u8; 32]),
    DependencyClosureInvalid { original_id: [u8; 32], invalid_ancestor: [u8; 32] },
    SnapshotStateCutMismatch { expected: [u8; 32], got: [u8; 32] },
    SnapshotDependencyRootMismatch { expected: [u8; 32], got: [u8; 32] },
    SnapshotCaseRootMismatch { expected: [u8; 32], got: [u8; 32] },
    SnapshotRevocationGenMismatch { expected: u64, got: u64 },
    SnapshotBeforeMaturity { current_daa: u64, maturity_cut_daa: u64 },
    SnapshotPastExpiry { current_daa: u64, expiry_daa: u64 },
    ExecCreditNotFound([u8; 32]),
    ExecCreditNotSpendable { credit_id: [u8; 32], state: ExecCreditLifecycleStateV1 },
    CarrierPermitFutureSlot { global_slot: u64, current_slot: u64 },
    CarrierPermitExpiredSlot { global_slot: u64, current_slot: u64, lifetime_slots: u64 },
    CarrierPermitAlreadyUsed { network_id: [u8; 32], policy_generation: u64, global_slot: u64, existing_header_id: [u8; 32] },
    CarrierFlowExceedsCap { header_id: [u8; 32] },
    An1(An1ErrorV1),
}

impl From<Swc1ErrorV1> for UnifiedEligibilityErrorV1 {
    fn from(e: Swc1ErrorV1) -> Self {
        Self::Swc1(e)
    }
}

impl From<An1ErrorV1> for UnifiedEligibilityErrorV1 {
    fn from(e: An1ErrorV1) -> Self {
        Self::An1(e)
    }
}

/// Canonical `EligibilitySnapshot` (`RFC-0058 §10.3`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1EligibilitySnapshotV1 {
    pub policy_id: [u8; 32],
    pub source_work_key: [u8; 32],
    pub source_original: [u8; 32],
    pub canonical_state_cut: [u8; 32],
    pub dependency_root: [u8; 32],
    pub case_root: [u8; 32],
    pub maturity_cut_daa: u64,
    pub original_expiry_daa: u64,
    pub revocation_generation: u64,
    pub purpose: Swc1RightKindV1,
}

impl Swc1EligibilitySnapshotV1 {
    pub fn snapshot_id(&self) -> Result<[u8; 32], UnifiedEligibilityErrorV1> {
        let mut buf = Vec::with_capacity(280);
        lp_append_checked_v1(&mut buf, TAG_SWC1_ELIGIBILITY_SNAPSHOT_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&self.policy_id);
        buf.extend_from_slice(&self.source_work_key);
        buf.extend_from_slice(&self.source_original);
        buf.extend_from_slice(&self.canonical_state_cut);
        buf.extend_from_slice(&self.dependency_root);
        buf.extend_from_slice(&self.case_root);
        buf.extend_from_slice(&self.maturity_cut_daa.to_le_bytes());
        buf.extend_from_slice(&self.original_expiry_daa.to_le_bytes());
        buf.extend_from_slice(&self.revocation_generation.to_le_bytes());
        buf.push(self.purpose as u8);
        Ok(swc1_h256_v1(&buf))
    }
}

/// Lifecycle state of an `ExecCredit` derived from a `MatureEligible` Original (`RFC-0050`, `RFC-0058 §11.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ExecCreditLifecycleStateV1 {
    Active,
    Reserved,
    Consumed,
    Invalidated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecCreditRecordV1 {
    pub credit_id: [u8; 32],
    pub source_original: [u8; 32],
    pub source_work_key: [u8; 32],
    pub dependency_closure: BTreeSet<[u8; 32]>,
    pub minted_at_daa: u64,
    pub expiry_daa: u64,
    pub state: ExecCreditLifecycleStateV1,
}

/// Actions submitted in a canonical block/batch (`RFC-0050 §6`, `RFC-0058 §11.2`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanonicalBatchActionV1 {
    /// Phase 1: Record notice, material request/response, or objective evidence.
    RecordCourtNoticeOrMaterial { original_id: [u8; 32], case_digest: [u8; 32] },
    /// Phase 2: Refute an Original, place it in `Q` hold, or apply late evidence stop.
    RefuteOrQuarantineOriginal { original_id: [u8; 32], place_in_q_hold: bool, late_evidence_only: bool },
    /// Phase 3: Promote `VerifiedPendingMaturity` -> `MatureEligible` or expire open case.
    EvaluateMaturityOrTerminal { original_id: [u8; 32] },
    /// Phase 4a: Consume a consensus right (`DaaWorkSample`, `SafeWeight`, `WorkReward`, `RuleEParticipation`, etc.).
    ConsumeConsensusRight { purpose: Swc1RightKindV1, source_original: [u8; 32], sub_key: [u8; 32], expiry_daa: u64 },
    /// Phase 4b: Mint an `ExecCredit` from a `MatureEligible` Original.
    MintExecCredit { credit_id: [u8; 32], source_original: [u8; 32], expiry_daa: u64 },
    /// Phase 4c: Reserve or consume an existing `ExecCredit`.
    SpendExecCredit { credit_id: [u8; 32], consume_final: bool },
}

/// Undo journal entry for global 1 BPS carrier permits (`RFC-0055 §9.3`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermitUndoJournalEntryV1 {
    pub key: ([u8; 32], u64, u64),
    pub header_id: [u8; 32],
}

/// Global 1 BPS outer carrier permit book (`GlobalPermitBookV1`, `RFC-0055 §9`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalPermitBookV1 {
    pub slot_ms: u64,
    pub max_slot_lifetime_l: u64,
    pub per_carrier_flow_cap: Cad1FlowVectorV1,
    /// Consumed permits `(network_id, policy_generation, global_slot) -> header_id`.
    pub used_permits: BTreeMap<([u8; 32], u64, u64), [u8; 32]>,
    /// Undo stack for reorg rollback.
    pub undo_journal: Vec<PermitUndoJournalEntryV1>,
}

impl GlobalPermitBookV1 {
    pub fn new(max_slot_lifetime_l: u64, per_carrier_flow_cap: Cad1FlowVectorV1) -> Self {
        Self {
            slot_ms: UAI1_CARRIER_SLOT_MS_V1,
            max_slot_lifetime_l,
            per_carrier_flow_cap,
            used_permits: BTreeMap::new(),
            undo_journal: Vec::new(),
        }
    }

    /// Validate and consume 1 outer carrier permit for `carrier` on `RateClock` (`RFC-0055 §9.2`).
    pub fn consume_carrier_permit(
        &mut self,
        carrier: &UnifiedCarrierV1,
    ) -> Result<PermitUndoJournalEntryV1, UnifiedEligibilityErrorV1> {
        if carrier.version != 1 {
            return Err(UnifiedEligibilityErrorV1::InvalidVersion(carrier.version));
        }
        let current_slot = carrier.rate_clock_ms / self.slot_ms;
        if carrier.global_slot > current_slot {
            return Err(UnifiedEligibilityErrorV1::CarrierPermitFutureSlot { global_slot: carrier.global_slot, current_slot });
        }
        if current_slot.saturating_sub(carrier.global_slot) > self.max_slot_lifetime_l {
            return Err(UnifiedEligibilityErrorV1::CarrierPermitExpiredSlot {
                global_slot: carrier.global_slot,
                current_slot,
                lifetime_slots: self.max_slot_lifetime_l,
            });
        }
        if !carrier.payload_flow.fits_within(&self.per_carrier_flow_cap) {
            return Err(UnifiedEligibilityErrorV1::CarrierFlowExceedsCap { header_id: carrier.header_id });
        }
        let key = (carrier.network_id, carrier.policy_generation, carrier.global_slot);
        if let Some(&existing_hdr) = self.used_permits.get(&key) {
            if existing_hdr == carrier.header_id {
                return Ok(PermitUndoJournalEntryV1 { key, header_id: existing_hdr });
            }
            return Err(UnifiedEligibilityErrorV1::CarrierPermitAlreadyUsed {
                network_id: carrier.network_id,
                policy_generation: carrier.policy_generation,
                global_slot: carrier.global_slot,
                existing_header_id: existing_hdr,
            });
        }
        self.used_permits.insert(key, carrier.header_id);
        let entry = PermitUndoJournalEntryV1 { key, header_id: carrier.header_id };
        self.undo_journal.push(entry.clone());
        Ok(entry)
    }

    /// Roll back a carrier permit during a DAG reorg (`RFC-0055 §9.3`).
    pub fn undo_carrier_permit(&mut self, entry: &PermitUndoJournalEntryV1) {
        if self.used_permits.get(&entry.key) == Some(&entry.header_id) {
            self.used_permits.remove(&entry.key);
        }
        if self.undo_journal.last() == Some(entry) {
            self.undo_journal.pop();
        }
    }
}

/// Unified Consensus Eligibility & Batch Execution Engine (`RFC-0048 / 0049 / 0050 / 0055 / 0058`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedConsensusEligibilityEngineV1 {
    pub policy_id: [u8; 32],
    pub canonical_state_cut: [u8; 32],
    pub swc1_ledgers: Swc1LedgerStateV1,
    /// Direct parent dependencies per Original (`child -> parents`).
    pub original_dependencies: BTreeMap<[u8; 32], BTreeSet<[u8; 32]>>,
    /// Quarantine hold set `Q` (`RFC-0048 / 0049`).
    pub quarantine_hold_q: BTreeSet<[u8; 32]>,
    /// Case state root per Original.
    pub case_roots: BTreeMap<[u8; 32], [u8; 32]>,
    /// Active/reserved/consumed/invalidated `ExecCredit` records (`RFC-0050`).
    pub exec_credits: BTreeMap<[u8; 32], ExecCreditRecordV1>,
    /// Global 1 BPS outer carrier permit book (`RFC-0055`).
    pub permit_book: GlobalPermitBookV1,
    /// `PALW-AN1` atomic slot/work/base-work ledger (`RFC-0058 §16.6`, `RFC-0059 §8`).
    pub an1_ledger: An1LedgerStateV1,
}

impl UnifiedConsensusEligibilityEngineV1 {
    pub fn new(
        policy_id: [u8; 32],
        canonical_state_cut: [u8; 32],
        swc1_ledgers: Swc1LedgerStateV1,
        permit_book: GlobalPermitBookV1,
    ) -> Self {
        Self {
            policy_id,
            canonical_state_cut,
            swc1_ledgers,
            original_dependencies: BTreeMap::new(),
            quarantine_hold_q: BTreeSet::new(),
            case_roots: BTreeMap::new(),
            exec_credits: BTreeMap::new(),
            permit_book,
            an1_ledger: An1LedgerStateV1::default(),
        }
    }

    /// Compute transitive dependency closure (including `original_id` itself) (`RFC-0049 §4`).
    pub fn transitive_dependency_closure(&self, original_id: &[u8; 32]) -> BTreeSet<[u8; 32]> {
        let mut closure = BTreeSet::new();
        let mut stack = vec![*original_id];
        while let Some(cur) = stack.pop() {
            if closure.insert(cur)
                && let Some(parents) = self.original_dependencies.get(&cur)
            {
                for p in parents {
                    if !closure.contains(p) {
                        stack.push(*p);
                    }
                }
            }
        }
        closure
    }

    /// Compute canonical `dependency_root` over `transitive_dependency_closure(original_id)`.
    pub fn dependency_root_for(&self, original_id: &[u8; 32]) -> Result<[u8; 32], UnifiedEligibilityErrorV1> {
        let closure = self.transitive_dependency_closure(original_id);
        let mut buf = Vec::with_capacity(36 + closure.len() * 32);
        lp_append_checked_v1(&mut buf, TAG_SWC1_DEP_CLOSURE_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&(closure.len() as u32).to_le_bytes());
        for item in closure {
            buf.extend_from_slice(&item);
        }
        Ok(swc1_h256_v1(&buf))
    }

    /// Derive a fresh `Swc1EligibilitySnapshotV1` from canonical native state (`RFC-0058 §10.3`).
    ///
    /// Succeeds ONLY if `original_id` is `MatureEligible`, not in `Q`, and every ancestor in its
    /// transitive dependency closure is `MatureEligible` and not in `Q`.
    pub fn derive_eligibility_snapshot(
        &self,
        original_id: [u8; 32],
        purpose: Swc1RightKindV1,
        current_daa: u64,
        original_expiry_daa: u64,
    ) -> Result<Swc1EligibilitySnapshotV1, UnifiedEligibilityErrorV1> {
        let orig =
            self.swc1_ledgers.original_state.get(&original_id).ok_or(UnifiedEligibilityErrorV1::OriginalNotFound(original_id))?;
        if !orig.stage.is_consensus_eligible() {
            return Err(UnifiedEligibilityErrorV1::OriginalNotMatureEligible { original_id, stage: orig.stage });
        }
        if self.quarantine_hold_q.contains(&original_id) {
            return Err(UnifiedEligibilityErrorV1::OriginalHeldInQuarantineQ(original_id));
        }
        let closure = self.transitive_dependency_closure(&original_id);
        for anc in &closure {
            if self.quarantine_hold_q.contains(anc) {
                return Err(UnifiedEligibilityErrorV1::DependencyClosureInvalid { original_id, invalid_ancestor: *anc });
            }
            let anc_rec = self
                .swc1_ledgers
                .original_state
                .get(anc)
                .ok_or(UnifiedEligibilityErrorV1::DependencyClosureInvalid { original_id, invalid_ancestor: *anc })?;
            if !anc_rec.stage.is_consensus_eligible() {
                return Err(UnifiedEligibilityErrorV1::DependencyClosureInvalid { original_id, invalid_ancestor: *anc });
            }
        }
        let maturity_cut_daa = orig.maturity_cut_daa.unwrap_or(0);
        if current_daa < maturity_cut_daa {
            return Err(UnifiedEligibilityErrorV1::SnapshotBeforeMaturity { current_daa, maturity_cut_daa });
        }
        if current_daa > original_expiry_daa {
            return Err(UnifiedEligibilityErrorV1::SnapshotPastExpiry { current_daa, expiry_daa: original_expiry_daa });
        }
        let dependency_root = self.dependency_root_for(&original_id)?;
        let case_root = self.case_roots.get(&original_id).copied().unwrap_or([0u8; 32]);
        Ok(Swc1EligibilitySnapshotV1 {
            policy_id: self.policy_id,
            source_work_key: orig.work_key,
            source_original: original_id,
            canonical_state_cut: self.canonical_state_cut,
            dependency_root,
            case_root,
            maturity_cut_daa,
            original_expiry_daa,
            revocation_generation: self.swc1_ledgers.global_revocation_generation,
            purpose,
        })
    }

    /// Re-verify a `Swc1EligibilitySnapshotV1` at a consensus consumer and consume the right once (`RFC-0058 §10.4`).
    pub fn verify_and_consume_consensus_eligibility(
        &mut self,
        snapshot: &Swc1EligibilitySnapshotV1,
        sub_key: [u8; 32],
        current_daa: u64,
    ) -> Result<Swc1RightsUseRecordV1, UnifiedEligibilityErrorV1> {
        let fresh =
            self.derive_eligibility_snapshot(snapshot.source_original, snapshot.purpose, current_daa, snapshot.original_expiry_daa)?;
        if snapshot.canonical_state_cut != fresh.canonical_state_cut {
            return Err(UnifiedEligibilityErrorV1::SnapshotStateCutMismatch {
                expected: fresh.canonical_state_cut,
                got: snapshot.canonical_state_cut,
            });
        }
        if snapshot.dependency_root != fresh.dependency_root {
            return Err(UnifiedEligibilityErrorV1::SnapshotDependencyRootMismatch {
                expected: fresh.dependency_root,
                got: snapshot.dependency_root,
            });
        }
        if snapshot.case_root != fresh.case_root {
            return Err(UnifiedEligibilityErrorV1::SnapshotCaseRootMismatch { expected: fresh.case_root, got: snapshot.case_root });
        }
        if snapshot.revocation_generation != fresh.revocation_generation {
            return Err(UnifiedEligibilityErrorV1::SnapshotRevocationGenMismatch {
                expected: fresh.revocation_generation,
                got: snapshot.revocation_generation,
            });
        }
        let record = self.swc1_ledgers.consume_right_once(
            snapshot.purpose,
            snapshot.source_original,
            sub_key,
            current_daa,
            self.canonical_state_cut,
        )?;
        Ok(record)
    }

    /// Invalidate all `ExecCredit` rows whose `dependency_closure` intersects `invalidated_original` (`RFC-0049 / 0050`).
    fn invalidate_dependent_exec_credits(&mut self, invalidated_original: &[u8; 32]) {
        for credit in self.exec_credits.values_mut() {
            if credit.source_original == *invalidated_original || credit.dependency_closure.contains(invalidated_original) {
                credit.state = ExecCreditLifecycleStateV1::Invalidated;
            }
        }
    }

    /// Execute a canonical batch of actions in strict 5-phase priority order (`RFC-0050 §6`, `RFC-0058 §11.2`).
    ///
    /// Regardless of the input order in `actions`, phase 2 (refutations, `Q` holds, dependency revocations,
    /// and credit invalidations) ALWAYS executes before phase 4 (mint/reserve/permit/consume), so a
    /// refutation in the same block preempts any reward, weight, or credit use.
    pub fn apply_canonical_batch_v1(
        &mut self,
        actions: &[CanonicalBatchActionV1],
        current_daa: u64,
        maturity_window_daa: u64,
        next_state_cut: [u8; 32],
    ) -> Vec<Result<(), UnifiedEligibilityErrorV1>> {
        let mut results: Vec<Option<Result<(), UnifiedEligibilityErrorV1>>> = vec![None; actions.len()];

        // Phase 1: Record notices, material requests/responses, and objective evidence
        for (idx, act) in actions.iter().enumerate() {
            if let CanonicalBatchActionV1::RecordCourtNoticeOrMaterial { original_id, case_digest } = act {
                self.case_roots.insert(*original_id, *case_digest);
                results[idx] = Some(Ok(()));
            }
        }

        // Phase 2: Apply refutations, Q holds, dependency revocations, and ExecCredit invalidations
        for (idx, act) in actions.iter().enumerate() {
            if let CanonicalBatchActionV1::RefuteOrQuarantineOriginal { original_id, place_in_q_hold, late_evidence_only } = act {
                if *place_in_q_hold {
                    self.quarantine_hold_q.insert(*original_id);
                }
                let target_stage = if *late_evidence_only {
                    Swc1VerificationStageV1::LateEvidenceStopsNewUses
                } else {
                    Swc1VerificationStageV1::Refuted
                };
                let res = self
                    .swc1_ledgers
                    .transition_original_stage(*original_id, target_stage, current_daa, maturity_window_daa)
                    .map_err(UnifiedEligibilityErrorV1::from);
                self.invalidate_dependent_exec_credits(original_id);
                results[idx] = Some(res);
            }
        }

        // Phase 3: Evaluate finite case terminals and maturity promotions
        for (idx, act) in actions.iter().enumerate() {
            if let CanonicalBatchActionV1::EvaluateMaturityOrTerminal { original_id } = act {
                let res = if self.quarantine_hold_q.contains(original_id) {
                    Err(UnifiedEligibilityErrorV1::OriginalHeldInQuarantineQ(*original_id))
                } else {
                    self.swc1_ledgers
                        .transition_original_stage(
                            *original_id,
                            Swc1VerificationStageV1::MatureEligible,
                            current_daa,
                            maturity_window_daa,
                        )
                        .map_err(UnifiedEligibilityErrorV1::from)
                };
                results[idx] = Some(res);
            }
        }

        // Phase 4: Derive EligibilitySnapshot and execute mint / reserve / permit / consume
        for (idx, act) in actions.iter().enumerate() {
            match act {
                CanonicalBatchActionV1::ConsumeConsensusRight { purpose, source_original, sub_key, expiry_daa } => {
                    let res = self
                        .derive_eligibility_snapshot(*source_original, *purpose, current_daa, *expiry_daa)
                        .and_then(|snap| self.verify_and_consume_consensus_eligibility(&snap, *sub_key, current_daa).map(|_| ()));
                    results[idx] = Some(res);
                }
                CanonicalBatchActionV1::MintExecCredit { credit_id, source_original, expiry_daa } => {
                    let res = self
                        .derive_eligibility_snapshot(*source_original, Swc1RightKindV1::ExecCreditMint, current_daa, *expiry_daa)
                        .and_then(|snap| {
                            self.verify_and_consume_consensus_eligibility(&snap, *credit_id, current_daa)?;
                            let closure = self.transitive_dependency_closure(source_original);
                            self.exec_credits.insert(
                                *credit_id,
                                ExecCreditRecordV1 {
                                    credit_id: *credit_id,
                                    source_original: *source_original,
                                    source_work_key: snap.source_work_key,
                                    dependency_closure: closure,
                                    minted_at_daa: current_daa,
                                    expiry_daa: *expiry_daa,
                                    state: ExecCreditLifecycleStateV1::Active,
                                },
                            );
                            Ok(())
                        });
                    results[idx] = Some(res);
                }
                CanonicalBatchActionV1::SpendExecCredit { credit_id, consume_final } => {
                    let res = (|| {
                        let credit =
                            self.exec_credits.get(credit_id).ok_or(UnifiedEligibilityErrorV1::ExecCreditNotFound(*credit_id))?.clone();
                        if !matches!(credit.state, ExecCreditLifecycleStateV1::Active | ExecCreditLifecycleStateV1::Reserved) {
                            return Err(UnifiedEligibilityErrorV1::ExecCreditNotSpendable {
                                credit_id: *credit_id,
                                state: credit.state,
                            });
                        }
                        let purpose =
                            if *consume_final { Swc1RightKindV1::ExecCreditConsume } else { Swc1RightKindV1::ExecCreditReserve };
                        let snap =
                            self.derive_eligibility_snapshot(credit.source_original, purpose, current_daa, credit.expiry_daa)?;
                        self.verify_and_consume_consensus_eligibility(&snap, *credit_id, current_daa)?;
                        self.exec_credits.get_mut(credit_id).expect("checked").state =
                            if *consume_final { ExecCreditLifecycleStateV1::Consumed } else { ExecCreditLifecycleStateV1::Reserved };
                        Ok(())
                    })();
                    results[idx] = Some(res);
                }
                _ => {}
            }
        }

        // Phase 5: Commit canonical state cut
        self.canonical_state_cut = next_state_cut;
        results.into_iter().map(|r| r.unwrap_or(Ok(()))).collect()
    }

    /// Execute a `PALW-AN1` canonical batch (`material_notices` -> `court` refutations -> `finalize`)
    /// with same-batch refutation priority, atomic one-time consumption (`slot`, `attempt_id`,
    /// `ComputationalWorkKey`, and optional `BaseWorkKey`), and reorg undo journal support
    /// (`RFC-0058 §16.6`, `RFC-0059 §10`, `AN1-W06`..`AN1-W08`).
    pub fn execute_an1_atomic_batch_and_snapshot_v1(
        &mut self,
        slot: &str,
        evidences: &[An1CourtEvidenceV1],
        request_final: bool,
        now: u64,
        material_notices: &[(String, An1PanelReviewOutcomeV1)],
        next_state_cut: [u8; 32],
    ) -> Result<(An1BatchOutcomeV1, [u8; 32]), UnifiedEligibilityErrorV1> {
        let prev_cut = self.canonical_state_cut;
        let outcome = self.an1_ledger.apply_batch(slot, evidences, request_final, now, material_notices)?;
        self.canonical_state_cut = next_state_cut;
        Ok((outcome, prev_cut))
    }

    /// Roll back the last finalized `PALW-AN1` claim and restore canonical state cut during a DAG reorg (`RFC-0058 §16.6`, `AN1-W08`).
    pub fn rollback_last_an1_final_and_restore_cut_v1(&mut self, prev_state_cut: [u8; 32]) -> Option<An1FinalUndoEntryV1> {
        let entry = self.an1_ledger.rollback_last_final()?;
        self.canonical_state_cut = prev_state_cut;
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_staged_work_commitments_v1::{
        SWC1_STAGE_JOB_V1, Swc1BeaconSourceProfileV1, Swc1CanonicalOriginalFieldsV1, Swc1CanonicalWorkFieldsV1, Swc1JobRegistrationV1,
        Swc1TypedDigestV1, swc1_job_for_seed_actual_input_v1, swc1_job_seed_v1,
    };

    #[test]
    fn unified_eligibility_same_batch_refutation_priority_and_one_bps_carrier_book() {
        let mut ledgers = Swc1LedgerStateV1::new();
        let reg = Swc1JobRegistrationV1 {
            version: 1,
            network_id: [0x11; 32],
            policy_id: [0x22; 32],
            job_epoch: 1,
            route_family: 1,
            model_id: [0x33; 32],
            runtime_profile_id: [0x44; 32],
            canonical_input_commitment: [0x55; 32],
            decode_policy_id: [0x66; 32],
            tool_policy_id: [0x77; 32],
            seed_derivation_profile_id: 1,
            capital_lock_ref: [0x88; 32],
            capital_sequence: 1,
            preseal_fee_commitment: [0x99; 32],
            max_ready_window_ms: 60_000,
            locked_capital_sompi: 5_000_000,
            paid_preseal_fee_sompi: 50_000,
        };
        let opp_ids = ledgers.register_opportunities_batch(&[reg.clone()], 1_000, 1_000_000, 10_000).unwrap();
        let jf = ledgers
            .freeze_stage(SWC1_STAGE_JOB_V1, reg.network_id, reg.policy_id, 1, [0u8; 32], [0xAA; 32], &opp_ids, 2_000, 5_000, 1)
            .unwrap();
        let msg = ledgers.stage_use[&(SWC1_STAGE_JOB_V1, 1)].descriptor.beacon_message().unwrap();
        let r_job = Swc1TypedDigestV1::new64([0xBB; 64]);
        ledgers
            .record_stage_seed(
                SWC1_STAGE_JOB_V1,
                1,
                jf,
                msg,
                r_job.clone(),
                2_500,
                Swc1BeaconSourceProfileV1::PostCloseDagCutHashV1 { confirm_delay_ms: 100 },
            )
            .unwrap();
        let js = swc1_job_seed_v1(&reg.network_id, &reg.policy_id, &jf, &r_job, &opp_ids[0]).unwrap();
        let actual_in = swc1_job_for_seed_actual_input_v1(&reg, &js).unwrap();
        let wf = Swc1CanonicalWorkFieldsV1 {
            version: 1,
            network_id: reg.network_id,
            policy_id: reg.policy_id,
            route_family: reg.route_family,
            model_id: reg.model_id,
            runtime_profile_id: reg.runtime_profile_id,
            actual_input_hash: actual_in,
            decode_policy_id: reg.decode_policy_id,
            tool_policy_id: reg.tool_policy_id,
            step_count: 16,
            leaf_count: 256,
            execution_shape_id: [0xCC; 32],
            obligation_set_root: [0xDD; 32],
        };
        let wk = wf.work_key().unwrap();
        let of = Swc1CanonicalOriginalFieldsV1 {
            version: 1,
            opportunity_id: opp_ids[0],
            job_freeze_id: jf,
            job_seed: js,
            work_key: wk,
            producer_principal_id: [0xEE; 32],
            trace_root: [0x01; 32],
            output_root: [0x02; 32],
            transcript_root: [0x03; 32],
            resource_descriptor_id: [0x04; 32],
            readiness_root: [0x05; 32],
            escrow_lock_id: [0x06; 32],
            ready_at_ms: 3_000,
            hard_expiry_ms: 90_000,
        };
        let (orig_id, _) = ledgers.commit_originals_batch(&[(wf, of)]).unwrap()[0];
        ledgers.original_state.get_mut(&orig_id).unwrap().stage = Swc1VerificationStageV1::TicketEligible;
        ledgers.transition_original_stage(orig_id, Swc1VerificationStageV1::VerificationPending, 100, 20).unwrap();
        ledgers.transition_original_stage(orig_id, Swc1VerificationStageV1::VerifiedPendingMaturity, 110, 20).unwrap();
        ledgers.transition_original_stage(orig_id, Swc1VerificationStageV1::MatureEligible, 130, 20).unwrap();

        let permit_book = GlobalPermitBookV1::new(5, Cad1FlowVectorV1::from_array([1_000; 10]));
        let mut engine = UnifiedConsensusEligibilityEngineV1::new([0x22; 32], [0xA0; 32], ledgers, permit_book);

        // Even if MintExecCredit is placed BEFORE RefuteOrQuarantineOriginal in the wire batch,
        // Phase 2 executes the refutation first and Phase 4 rejects the MintExecCredit!
        let batch = vec![
            CanonicalBatchActionV1::MintExecCredit { credit_id: [0xF1; 32], source_original: orig_id, expiry_daa: 500 },
            CanonicalBatchActionV1::RefuteOrQuarantineOriginal {
                original_id: orig_id,
                place_in_q_hold: true,
                late_evidence_only: false,
            },
        ];
        let outcomes = engine.apply_canonical_batch_v1(&batch, 140, 20, [0xA1; 32]);
        assert!(outcomes[1].is_ok());
        assert!(outcomes[0].is_err());
        assert!(!engine.exec_credits.contains_key(&[0xF1; 32]));

        // Test 1 BPS GlobalPermitBookV1 across REAL and EXEC carriers + reorg undo
        let c_real = UnifiedCarrierV1 {
            version: 1,
            network_id: [0x11; 32],
            policy_generation: 1,
            global_slot: 10,
            carrier_kind: UnifiedCarrierKindV1::RealAttempt,
            header_id: [0xB1; 32],
            rate_clock_ms: 10_500,
            payload_wire_bytes: 512,
            payload_flow: Cad1FlowVectorV1::default(),
        };
        let c_exec_conflict =
            UnifiedCarrierV1 { header_id: [0xB2; 32], carrier_kind: UnifiedCarrierKindV1::ExecCarrier, ..c_real.clone() };
        let undo_entry = engine.permit_book.consume_carrier_permit(&c_real).unwrap();
        // Second carrier in the same global_slot = 10 is rejected (1 BPS global limit across all carrier kinds)
        assert!(matches!(
            engine.permit_book.consume_carrier_permit(&c_exec_conflict),
            Err(UnifiedEligibilityErrorV1::CarrierPermitAlreadyUsed { global_slot: 10, .. })
        ));
        // Reorg undo releases slot 10 so the competing branch's carrier can take it
        engine.permit_book.undo_carrier_permit(&undo_entry);
        assert!(engine.permit_book.consume_carrier_permit(&c_exec_conflict).is_ok());
    }

    #[test]
    fn unified_an1_batch_refutation_priority_atomic_consumption_and_reorg_w06_to_w08() {
        use crate::palw_an1_input_binding_and_panel_court_v1::{
            An1PanelReviewOutcomeV1, An1RegistrationContextV1, An1TargetThreshold257V1, An1TreeV1, an1_build_claim_v1,
            an1_deterministic_weights_v1, an1_encode_iv_record_v1, an1_job_tree_v1, an1_model_tree_v1, an1_panel_review_v1,
            an1_profile_id_hex_v1,
        };

        let permit_book = GlobalPermitBookV1::new(5, Cad1FlowVectorV1::from_array([1_000; 10]));
        let mut engine = UnifiedConsensusEligibilityEngineV1::new([0x22; 32], [0xA0; 32], Swc1LedgerStateV1::new(), permit_book);

        let weights = an1_deterministic_weights_v1(23);
        let job_tokens = vec![17i64, 25];
        let ctx = An1RegistrationContextV1 {
            accounting_version: "AN1-TEST-USEFUL-ONCE-1".to_owned(),
            beacon_key: "fixture-key-1".to_owned(),
            beacon_round: 13,
            chain: "an1-independent-test-chain".to_owned(),
            deadline: 300,
            epoch: 7,
            job_root: an1_job_tree_v1(&job_tokens).unwrap().root_hex(),
            model_root: an1_model_tree_v1(&weights).unwrap().root_hex(),
            nonce: "22".repeat(32),
            owner: "11".repeat(32),
            panel_event: 100,
            profile_id: an1_profile_id_hex_v1(),
            target: An1TargetThreshold257V1::MaxTwoPow256,
        };
        let sigma = [0x33u8; 32];
        let slot = engine.an1_ledger.register(&ctx, &job_tokens, 10, &ctx.owner, "bond-1").unwrap();

        // AN1-W06: Corrupted trace with 4 rubber-stamp approvals and 1 honest refutation in the SAME batch
        let (mut bad_claim, mut bad_material) = an1_build_claim_v1(&ctx, &sigma, &job_tokens, &weights).unwrap();
        bad_material.trace_records[50] = an1_encode_iv_record_v1(50, 999_999);
        bad_claim.trace_root = An1TreeV1::build("trace", &bad_claim.attempt, &bad_material.trace_records, None).unwrap().root_hex();
        engine.an1_ledger.seal(&slot, &bad_claim, 25, &ctx.owner).unwrap();
        let candidates: Vec<String> = (1..=6).map(|i| format!("panel-{i}")).collect();
        let selected = engine.an1_ledger.select_panel(&slot, &[0x44u8; 32], 100, &candidates).unwrap();
        for p in &selected[..4] {
            engine.an1_ledger.approve(&slot, p).unwrap();
        }
        {
            let row = engine.an1_ledger.rows.get_mut(&slot).unwrap();
            row.materials_ready = true;
            row.closed = true;
            row.delivered = true;
        }
        let native = engine.an1_ledger.native(&slot).unwrap();
        let honest_review = an1_panel_review_v1(&native, Some(&bad_material));
        let An1PanelReviewOutcomeV1::Refuted { court, .. } = honest_review else {
            panic!("expected Refuted outcome from honest full replay");
        };
        let (outcome, _) = engine.execute_an1_atomic_batch_and_snapshot_v1(&slot, &[court], true, 120, &[], [0xA2; 32]).unwrap();
        assert!(!outcome.is_final);
        assert!(engine.an1_ledger.rows[&slot].refuted);

        // AN1-W08: Honest slot finalizes once and rolls back cleanly on reorg
        let mut ctx2 = ctx.clone();
        ctx2.nonce = "55".repeat(32);
        let slot2 = engine.an1_ledger.register(&ctx2, &job_tokens, 12, &ctx2.owner, "bond-2").unwrap();
        let (good_claim, _) = an1_build_claim_v1(&ctx2, &sigma, &job_tokens, &weights).unwrap();
        engine.an1_ledger.seal(&slot2, &good_claim, 26, &ctx2.owner).unwrap();
        let selected2 = engine.an1_ledger.select_panel(&slot2, &[0x44u8; 32], 100, &candidates).unwrap();
        engine.an1_ledger.approve(&slot2, &selected2[0]).unwrap();
        {
            let row2 = engine.an1_ledger.rows.get_mut(&slot2).unwrap();
            row2.materials_ready = true;
            row2.closed = true;
            row2.delivered = true;
        }
        let before_final = engine.clone();
        let (outcome2, prev_cut) = engine.execute_an1_atomic_batch_and_snapshot_v1(&slot2, &[], true, 125, &[], [0xA3; 32]).unwrap();
        assert!(outcome2.is_final);
        assert!(engine.an1_ledger.consumed_slots.contains(&slot2));

        let undone = engine.rollback_last_an1_final_and_restore_cut_v1(prev_cut).unwrap();
        assert_eq!(undone.slot, slot2);
        assert!(!engine.an1_ledger.consumed_slots.contains(&slot2));
        assert_eq!(engine, before_final);
        assert!(engine.rollback_last_an1_final_and_restore_cut_v1([0; 32]).is_none());
        assert_eq!(engine, before_final);
    }
}
