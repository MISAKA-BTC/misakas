//! **G14 item 1 / RFC-0035 A §3, §7, §9.1: the funded-original coordinator's public inputs** (DORMANT).
//!
//! The coordinator itself lives in the native fold (`palw_state_v2::palw_tir_public_fold_v1`, file
//! `palw_funded_original_coordinator_fold_v1.rs`), at the one pre-write seam: inside `apply_object_with_pair_v1`'s paired
//! checkpoint, after `prepare_paired_original_v1`, before the first `reserve_for_claim`. This module holds only what crosses a crate
//! boundary or a test seam:
//!
//! * [`PalwFundedOriginalPairingV1`] — the transaction-local pairing of an accepted TFO1 carrier with its OWN actual transaction.
//!   Constructed only by [`palw_funded_original_pairing_v1`] from the actual accepted [`Transaction`] (literal payload equality,
//!   output index, amount, payer P2PKH; RFC-0035 §8). It carries no `paid`/`complete` flag and grants nothing: the coordinator
//!   re-inspects the same transaction and re-verifies the payer's signature itself.
//! * [`PalwFundedOriginalScheduleV1`] — the values the funded original needs that are NOT adopted (fee/capital/retention/settlement
//!   profiles and amounts, the native requirement-derivation budgets, and the role contracts the actor catalog names as missing).
//!   Its fields are private and its only constructor is `#[cfg(test)]`: no production build can hold a value, so every production
//!   resolution is a typed [`PalwFundedOriginalMissingV1`] (RFC-0035 §4: no amount is adopted by structure). Once amounts are
//!   adopted, only a production resolver for this struct needs to be written; the coordinator does not change.
//!
//! Never: a ZK/succinct proof, a caller-supplied `paid` flag, a profile hash promoted to policy, a zero-filled missing term.
use crate::Hash64;
use crate::palw_tir_funded_original_envelope_v1::{PalwTirFundedOriginalEnvelopeV1, is_funded_original_carrier_v1};
use crate::tx::{Transaction, TransactionOutpoint};

/// **Why a funded original is refused because a contract or value does not exist** (typed-missing; never a default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFundedOriginalMissingV1 {
    /// No adopted fee / capital / retention / settlement schedule (RFC-0035 §4, ADR-0202 承認の範囲). Every production resolution.
    Schedule,
    /// The native requirement-derivation budgets are part of the unadopted schedule.
    RequirementBudgets,
    /// No court service book in rooted state: the shared service namespace the source graph is scoped to does not exist.
    SharedServiceNamespace,
    /// The actor catalog's canonical whole-branch actor assignment has no adopted contract.
    CanonicalWholeBranchActorAssignment,
    /// The shared service operator's signed consent has no adopted contract.
    SharedServiceOperatorConsent,
    /// Fee / capital / retention / recovery bodies.
    FeeCapitalRetentionRecoveryBodies,
    /// A semantic stage's material provider consent.
    SemanticMaterialProviderConsent,
    /// The native registration policy of the class is absent.
    NativeRegistrationPolicy,
    /// The adopted X3 hard-deadline mirror does not govern the accepting DAA.
    HardDeadline,
    /// The native network domain is not resolved at this block.
    NetworkDomain,
    /// G14 item 1 (lane r5): the all-branch admission values (service profile, per-branch packet/heap/retention costs and
    /// capital) are not adopted.
    AdmissionSchedule,
}

/// Bit for each role contract a schedule adopts (test-only values; see [`PalwFundedOriginalScheduleV1`]).
pub const PALW_FUNDED_ROLE_WHOLE_BRANCH_V1: u8 = 1;
pub const PALW_FUNDED_ROLE_SERVICE_OPERATOR_V1: u8 = 2;
pub const PALW_FUNDED_ROLE_FEE_CAPITAL_BODIES_V1: u8 = 4;
pub const PALW_FUNDED_ROLE_SEMANTIC_PROVIDER_V1: u8 = 8;

/// **The unadopted values, as one parameter.** Private fields; the ONLY constructor is `#[cfg(test)]`, so a production build holds
/// no value and [`palw_funded_original_schedule_v1`] always answers [`PalwFundedOriginalMissingV1::Schedule`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwFundedOriginalScheduleV1 {
    fee_profile_id: Hash64,
    capital_profile_id: Hash64,
    retention_profile_id: Hash64,
    settlement_policy_id: Hash64,
    fee_sompi: u64,
    capital_sompi: u64,
    numerical: crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3,
    input: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
    source: SourceBudgetV1,
    adopted_roles: u8,
    admission: Option<crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1>,
}

/// `SourceGraphBudgetV3` has no `PartialEq`; its four bounds are kept here and rebuilt on read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
struct SourceBudgetV1 {
    max_queries: u64,
    max_roles: u64,
    max_vertices: u64,
    max_edges: u64,
}

impl PalwFundedOriginalScheduleV1 {
    /// **TEST ONLY.** Injects values for the unadopted schedule so the coordinator's success path can be exercised. Absent from
    /// every non-test build: there is no production constructor, deserializer or `Default`.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn test_only_v1(
        profiles: [Hash64; 4],
        fee_sompi: u64,
        capital_sompi: u64,
        numerical: crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3,
        input: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
        source: crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3,
        adopted_roles: u8,
    ) -> Self {
        Self {
            fee_profile_id: profiles[0],
            capital_profile_id: profiles[1],
            retention_profile_id: profiles[2],
            settlement_policy_id: profiles[3],
            fee_sompi,
            capital_sompi,
            numerical,
            input,
            source: SourceBudgetV1 {
                max_queries: source.max_queries,
                max_roles: source.max_roles,
                max_vertices: source.max_vertices,
                max_edges: source.max_edges,
            },
            adopted_roles,
            admission: None,
        }
    }
    /// **TEST ONLY.** Adds the all-branch admission values (lane r5). Without them the coordinator refuses `AdmissionSchedule`.
    #[cfg(test)]
    pub fn with_admission_v1(mut self, admission: crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1) -> Self {
        self.admission = Some(admission);
        self
    }
    pub fn admission_v1(&self) -> Option<crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1> {
        self.admission
    }
    pub fn fee_sompi(&self) -> u64 {
        self.fee_sompi
    }
    pub fn capital_floor_sompi(&self) -> u64 {
        self.capital_sompi
    }
    pub fn fee_profile_id(&self) -> Hash64 {
        self.fee_profile_id
    }
    pub fn capital_profile_id(&self) -> Hash64 {
        self.capital_profile_id
    }
    pub fn retention_profile_id(&self) -> Hash64 {
        self.retention_profile_id
    }
    pub fn settlement_policy_id(&self) -> Hash64 {
        self.settlement_policy_id
    }
    /// The amount the escrow must cover: fee + capital, checked. `None` on overflow (refused by the caller, never clamped).
    pub fn required_amount_sompi(&self) -> Option<u64> {
        self.fee_sompi.checked_add(self.capital_sompi)
    }
    pub fn numerical_budget(&self) -> crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3 {
        self.numerical
    }
    pub fn input_budget(&self) -> crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 {
        self.input
    }
    pub fn source_budget(&self) -> crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3 {
        crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3 {
            max_queries: self.source.max_queries,
            max_roles: self.source.max_roles,
            max_vertices: self.source.max_vertices,
            max_edges: self.source.max_edges,
        }
    }
    pub fn adopts_role(&self, bit: u8) -> bool {
        self.adopted_roles & bit == bit
    }
}

/// **Resolve the schedule at this block.** Production: always `Err(Schedule)` (nothing adopted). A test build may inject a value
/// through `PalwTransitionExtrasV1::funded_original_test_schedule_v1` (a `#[cfg(test)]` field).
pub fn palw_funded_original_schedule_v1(
    extras: &crate::palw_state_v2::PalwTransitionExtrasV1,
) -> Result<PalwFundedOriginalScheduleV1, PalwFundedOriginalMissingV1> {
    #[cfg(test)]
    if let Some(schedule) = extras.funded_original_test_schedule_v1 {
        return Ok(schedule);
    }
    let _ = extras;
    Err(PalwFundedOriginalMissingV1::Schedule)
}

/// **An accepted TFO1 carrier paired with its own actual transaction.** Private fields; built only by
/// [`palw_funded_original_pairing_v1`]. Carries the transaction itself so the coordinator re-runs the existing TFO1 inspection
/// (`inspect_carrier_v1`) and `prepare_funding_v1` against the actual bytes, never against a copied DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFundedOriginalPairingV1 {
    carrier: Transaction,
    envelope: PalwTirFundedOriginalEnvelopeV1,
    claim: Hash64,
    outpoint: TransactionOutpoint,
}

impl PalwFundedOriginalPairingV1 {
    pub fn carrier(&self) -> &Transaction {
        &self.carrier
    }
    pub fn envelope(&self) -> &PalwTirFundedOriginalEnvelopeV1 {
        &self.envelope
    }
    /// The literal original's claim the payer's terms bind.
    pub fn claim(&self) -> Hash64 {
        self.claim
    }
    /// The carrier's own funding outpoint (actual txid + signed index).
    pub fn outpoint(&self) -> TransactionOutpoint {
        self.outpoint
    }
}

/// **Pair an accepted TFO1 carrier with itself**: framing, bounded decode, literal original, and the existing own-output inspection
/// (payload equality, index, amount, payer P2PKH). No input amounts, fee or acceptance flag are read, so none can be misreported.
/// The payer's signature is NOT checked here (the coordinator checks it with the native expected scope); a pairing is not consent.
pub fn palw_funded_original_pairing_v1(tx: &Transaction) -> Result<PalwFundedOriginalPairingV1, String> {
    if !is_funded_original_carrier_v1(tx) {
        return Err("not a TFO1 carrier".into());
    }
    let envelope = PalwTirFundedOriginalEnvelopeV1::decode_v1(&tx.payload)?;
    let original = envelope.original_v1()?;
    let claim = original.original_fp_v1()?.claim_id();
    if claim != envelope.funding.terms.claim || original.pledge.claim != claim {
        return Err("TFO1 terms, pledge and literal original name different claims".into());
    }
    let outpoint = envelope.inspect_carrier_v1(tx)?.outpoint();
    Ok(PalwFundedOriginalPairingV1 { carrier: tx.clone(), envelope, claim, outpoint })
}
