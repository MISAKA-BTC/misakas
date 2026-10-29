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
//!   Its fields are private. Production resolution returns the pinned estimate only while
//!   [`crate::palw_state_v2::PalwTransitionExtrasV1::tir_public_conviction_active`] is set (the funded original has no fence of
//!   its own; it is the sub-route of `palw_tir_public_conviction_v1`). Below that fence, and while it is unset, resolution stays
//!   [`PalwFundedOriginalMissingV1::Schedule`]. No new amount is introduced.
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

/// Lane VAL estimates, wired by [`palw_funded_original_schedule_v1`] only while the conviction fence is active:
/// `crate::palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1`. Private fields. Below that fence the production
/// resolver still answers [`PalwFundedOriginalMissingV1::Schedule`].
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
    /// **Production constructor (lane VAL, 2026-10-26)**: refuses unset profile ids, a zero or unbounded fee/capital, a budget
    /// outside `1..=2^40` and an unknown role bit. [`palw_funded_original_schedule_v1`] returns the value built by
    /// `palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1` while the conviction fence is active. No preset sets
    /// that fence, so below it the resolver still refuses `Schedule`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_v1(
        profiles: [Hash64; 4],
        fee_sompi: u64,
        capital_sompi: u64,
        numerical: crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3,
        input: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
        source: crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3,
        adopted_roles: u8,
        admission: crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1,
    ) -> Result<Self, &'static str> {
        const MAX: u64 = 1 << 40;
        const MAX_SOMPI: u64 = 1_000_000 * 100_000_000;
        let in_range = |v: u64| v > 0 && v <= MAX;
        if profiles.iter().any(|p| *p == Hash64::from_bytes([0; 64])) {
            return Err("funded original schedule: profile ids must be set");
        }
        if fee_sompi == 0 || capital_sompi == 0 || fee_sompi > MAX_SOMPI || capital_sompi > MAX_SOMPI {
            return Err("funded original schedule: fee and capital must be in 1..=1,000,000 MSK");
        }
        let n = numerical;
        let i = input;
        if ![n.max_original_leaves, n.max_roots, n.max_paths, n.max_edges, n.max_path_bytes].into_iter().all(in_range)
            || ![i.max_original_leaves, i.max_total_edges, i.max_total_coordinates, i.max_root_memberships].into_iter().all(in_range)
            || ![source.max_queries, source.max_roles, source.max_vertices, source.max_edges].into_iter().all(in_range)
        {
            return Err("funded original schedule: every budget must be in 1..=2^40");
        }
        let known = PALW_FUNDED_ROLE_WHOLE_BRANCH_V1
            | PALW_FUNDED_ROLE_SERVICE_OPERATOR_V1
            | PALW_FUNDED_ROLE_FEE_CAPITAL_BODIES_V1
            | PALW_FUNDED_ROLE_SEMANTIC_PROVIDER_V1;
        if adopted_roles & !known != 0 {
            return Err("funded original schedule: unknown role bit");
        }
        Ok(Self {
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
            admission: Some(admission),
        })
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

/// **Resolve the schedule at this block.** A test build may inject a value through
/// `PalwTransitionExtrasV1::funded_original_test_schedule_v1` (a `#[cfg(test)]` field). Otherwise, while
/// `tir_public_conviction_active` is set, the pinned estimate
/// ([`crate::palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1`]) is the schedule. Below that fence, and while
/// the fence is unset, the answer stays [`PalwFundedOriginalMissingV1::Schedule`].
pub fn palw_funded_original_schedule_v1(
    extras: &crate::palw_state_v2::PalwTransitionExtrasV1,
) -> Result<PalwFundedOriginalScheduleV1, PalwFundedOriginalMissingV1> {
    #[cfg(test)]
    if let Some(schedule) = extras.funded_original_test_schedule_v1 {
        return Ok(schedule);
    }
    if extras.tir_public_conviction_active {
        return Ok(crate::palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1());
    }
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
    palw_funded_original_pairing_payload_v1(tx, &tx.payload)
}

/// RFC38 outer FPA1, with the actual signed transaction retained as the payer carrier.
/// Only an accepting caller past the accounting fence may use this structural pairing.
pub fn palw_funded_original_answer_pairing_v1(tx: &Transaction) -> Result<PalwFundedOriginalPairingV1, String> {
    let view = crate::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_view_v1(&tx.payload, u32::MAX as usize)
        .ok_or("invalid FPA1 accounting envelope")?;
    palw_funded_original_pairing_payload_v1(tx, view.inner)
}
pub fn palw_funded_original_pairing_payload_v1(tx: &Transaction, payload: &[u8]) -> Result<PalwFundedOriginalPairingV1, String> {
    let envelope = PalwTirFundedOriginalEnvelopeV1::decode_v1(payload)?;
    let original = envelope.original_v1()?;
    let claim = original.original_fp_v1()?.claim_id();
    if claim != envelope.funding.terms.claim || original.pledge.claim != claim {
        return Err("TFO1 terms, pledge and literal original name different claims".into());
    }
    let outpoint = envelope.inspect_carrier_v1(tx)?.outpoint();
    Ok(PalwFundedOriginalPairingV1 { carrier: tx.clone(), envelope, claim, outpoint })
}
