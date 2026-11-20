//! **RFC-0055 (`UAI1`) & RFC-0056 (`CAD1`) — Unified Pre-Admission Compact Gate, Global Service Calendar, and `D_admit` Controller.**
//!
//! This module implements:
//! 1. **Pre-Admission Compact Gate (`CAD1` / `SWC1` §18)**:
//!    Evaluates `Preseal` / `WorkTicket <= T(D_e)` in `O(1)` compact metadata before any heavy trace,
//!    geometry, actor-graph, or planner decoding across all new-original routes (`Attempt/PAV3`,
//!    `FreePrompt`, `TFP1 Plain`, `TFO1/TFO2/TFO3 Funded`, `EvaluationClaim`, `MultiOriginalBatch`,
//!    and detailed `Quote RPC`), while exempting continuations of already-accepted claims.
//! 2. **10-Dimensional Flow Calendar & Stock Accounting (`UAI1` / `CAD1` §9–§10)**:
//!    Tracks per-epoch worst-case flow reserves (`deterministic_work`, `signature_checks`, `hash_bytes`,
//!    `parse_and_expand_bytes`, `wire_bytes`, `io_read_bytes`, `io_write_bytes`, `io_operations`,
//!    `control_operations`, `carrier_space`) and stock caps (`retained_bytes`, `live_cases`,
//!    `live_obligations`, `active_locks`, `preseal_count`), comparing empty-book vs active-book fit
//!    to separate true `NoCapacity` (`known_congestion`) from intrinsic candidate failures.
//! 3. **120,000ms Epoch `D_(b+2)` Controller (`CAD1` §12)**:
//!    Computes `U = max(U_future, U_spent, U_stock)` in 1024-basis-point integer ceil arithmetic and
//!    updates `D_(b+2)` (`ceil(9*D/8)` on high/congestion, 3-epoch `floor(8*D/9)` on low, hold on
//!    incomplete/unavailable observation).

use crate::palw_an1_input_binding_and_panel_court_v1::{An1ErrorV1, An1TargetThreshold257V1, An1TimingPlanV1, an1_ticket_bytes_v1};
use crate::palw_staged_work_commitments_v1::{
    SWC1_MAX_DIFFICULTY_D_V1, SWC1_MAX_FIELD_BYTES_V1, Swc1ErrorV1, Swc1TypedDigestV1, lp_append_checked_v1, nr_append_checked_v1,
    swc1_difficulty_threshold_be_v1, swc1_h256_v1, swc1_ticket_meets_difficulty_v1, swc1_work_ticket_bytes_v1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Canonical admission epoch duration on `RateClock` (`120,000 ms`, `RFC-0056 §4.3`).
pub const CAD1_EPOCH_MS_V1: u64 = 120_000;

/// Fixed-point utilization scale (`1024 = 100%`, `RFC-0056 §12.1`).
pub const CAD1_SCALE_U_V1: u64 = 1024;
pub const CAD1_U_TARGET_V1: u64 = 640;
pub const CAD1_U_HIGH_V1: u64 = 768;
pub const CAD1_U_LOW_V1: u64 = 384;
pub const CAD1_W_DOWN_EPOCHS_V1: u32 = 3;

/// Domain separation tags (`RFC-0056 §5.1`).
pub const TAG_CAD1_SLOT_V1: &[u8] = b"MISAKA/PALW/CAD1/SLOT";
pub const TAG_CAD1_PRESEAL_V1: &[u8] = b"MISAKA/PALW/CAD1/PRESEAL";
pub const TAG_CAD1_TICKET_V1: &[u8] = b"MISAKA/PALW/CAD1/TICKET";

/// Route family identifier for new-original ingress (`RFC-0056 §5.2, §14.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Cad1IngressRouteFamilyV1 {
    AttemptPav3 = 1,
    FreePromptPlain = 2,
    Tfp1PlainOriginal = 3,
    TfoFundedOriginal = 4,
    EvaluationClaim = 5,
    MultiOriginalBatchItem = 6,
    QuoteRpcDetailed = 7,
}

impl Cad1IngressRouteFamilyV1 {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::AttemptPav3),
            2 => Some(Self::FreePromptPlain),
            3 => Some(Self::Tfp1PlainOriginal),
            4 => Some(Self::TfoFundedOriginal),
            5 => Some(Self::EvaluationClaim),
            6 => Some(Self::MultiOriginalBatchItem),
            7 => Some(Self::QuoteRpcDetailed),
            _ => None,
        }
    }
}

/// Ingress classification distinguishing new-original candidates (gated by `D_admit`) from
/// continuations of already-accepted claims (never gated by `D_admit`, `RFC-0056 §4.2, §14.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cad1IngressRouteKindV1 {
    NewOriginalCandidate(Cad1IngressRouteFamilyV1),
    ContinuationOfAcceptedClaim,
}

/// Disposition assigned to a candidate in a target epoch (`RFC-0056 §11.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Cad1CandidateDispositionV1 {
    Ineligible,
    Accepted,
    NoCapacity,
    InvalidQualification,
    UnsupportedShape,
    InsufficientFunds,
    MissingMaterial,
    IntrinsicDeadlineFailure,
    IntrinsicShapeFailure,
    ExpiredUnvisited,
    SeedUnavailable,
}

impl Cad1CandidateDispositionV1 {
    /// Only `NoCapacity` on a qualified candidate that fits an empty book sets `known_congestion` (`RFC-0056 §11.3`).
    #[inline]
    pub const fn is_known_congestion(self) -> bool {
        matches!(self, Self::NoCapacity)
    }

    /// True if this candidate represents qualified demand (`Accepted` or `NoCapacity`).
    #[inline]
    pub const fn is_qualified_demand(self) -> bool {
        matches!(self, Self::Accepted | Self::NoCapacity)
    }
}

/// Errors produced by `CAD1` / `UAI1` admission and difficulty control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cad1ErrorV1 {
    Swc1(Swc1ErrorV1),
    InvalidVersion(u16),
    InvalidRouteFamily(u8),
    InvalidPolicyBounds(&'static str),
    NodeBelowCommonCheckerProfile(&'static str),
    TargetEpochNotCollecting { target_epoch: u64, current_epoch: u64 },
    TargetEpochAlreadyFrozen(u64),
    TargetEpochNotFrozen(u64),
    TargetEpochSeedAlreadySet(u64),
    TargetEpochSeedWindowViolation { arrived_ms: u64, freeze_ms: u64, ready_cut_ms: u64 },
    TargetEpochSeedAnchorMismatch,
    TargetEpochPresealCapExceeded { target_epoch: u64, cap: u64 },
    CapitalLockInsufficient { locked_sompi: u64, required_sompi: u64 },
    PresealFeeInsufficient { fee_sompi: u64, required_sompi: u64 },
    CapitalSequenceNotMonotone { capital_lock_ref: [u8; 32], sequence: u64, highwater: u64 },
    TrialSlotAlreadyUsed { trial_slot_key: [u8; 32], existing_preseal_id: [u8; 32] },
    DuplicateBatchId([u8; 32]),
    PrefixOrderViolation { expected_next: [u8; 32], got: [u8; 32] },
    CandidateNotEligible([u8; 32]),
    HeavyDecodeInvokedOnIneligibleCandidate([u8; 32]),
    PlannerSimulationFailed([u8; 32]),
    An1(An1ErrorV1),
}

impl From<Swc1ErrorV1> for Cad1ErrorV1 {
    fn from(e: Swc1ErrorV1) -> Self {
        Self::Swc1(e)
    }
}

impl From<An1ErrorV1> for Cad1ErrorV1 {
    fn from(e: An1ErrorV1) -> Self {
        Self::An1(e)
    }
}

/// Canonical 20-field compact `Preseal` commitment (`RFC-0056 §5.2`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1PresealFieldsV1 {
    pub version: u16,
    pub network_id: [u8; 32],
    pub policy_id: [u8; 32],
    pub target_epoch: u64,
    pub route_family: u8,
    pub trial_slot_key: [u8; 32],
    pub capital_lock_ref: [u8; 32],
    pub capital_sequence: u64,
    pub preseal_fee_commitment: [u8; 32],
    pub producer_principal_id: [u8; 32],
    pub model_id: [u8; 32],
    pub runtime_profile_id: [u8; 32],
    pub job_input_commitment: [u8; 32],
    pub trace_root: [u8; 32],
    pub output_root: [u8; 32],
    pub transcript_root: [u8; 32],
    pub resource_descriptor_id: [u8; 32],
    pub readiness_root: [u8; 32],
    pub escrow_lock_id: [u8; 32],
    pub ready_at_ms: u64,
    pub hard_expiry_ms: u64,
}

impl Cad1PresealFieldsV1 {
    /// Compute `TrialSlotKey` (`MISAKA/PALW/CAD1/SLOT`, `RFC-0056 §5.1`).
    pub fn compute_trial_slot_key(&self) -> Result<[u8; 32], Cad1ErrorV1> {
        let mut buf = Vec::with_capacity(240);
        lp_append_checked_v1(&mut buf, TAG_CAD1_SLOT_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        nr_append_checked_v1(&mut buf, "network", 1, &self.network_id)?;
        nr_append_checked_v1(&mut buf, "policy", 1, &self.policy_id)?;
        nr_append_checked_v1(&mut buf, "capital-lock", 1, &self.capital_lock_ref)?;
        nr_append_checked_v1(&mut buf, "capital-seq", 1, &self.capital_sequence.to_le_bytes())?;
        nr_append_checked_v1(&mut buf, "job-input", 1, &self.job_input_commitment)?;
        Ok(swc1_h256_v1(&buf))
    }

    /// Encode canonical `Preseal` fields in exact `RFC-0056 §5.2` order.
    pub fn encode(&self) -> Result<Vec<u8>, Cad1ErrorV1> {
        if self.version != 1 {
            return Err(Cad1ErrorV1::InvalidVersion(self.version));
        }
        if Cad1IngressRouteFamilyV1::from_u8(self.route_family).is_none() {
            return Err(Cad1ErrorV1::InvalidRouteFamily(self.route_family));
        }
        let mut out = Vec::with_capacity(800);
        out.extend_from_slice(&self.version.to_le_bytes());
        nr_append_checked_v1(&mut out, "network", 1, &self.network_id)?;
        nr_append_checked_v1(&mut out, "policy", 1, &self.policy_id)?;
        nr_append_checked_v1(&mut out, "target-epoch", 1, &self.target_epoch.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "route-family", 1, &[self.route_family])?;
        nr_append_checked_v1(&mut out, "trial-slot", 1, &self.trial_slot_key)?;
        nr_append_checked_v1(&mut out, "capital-lock", 1, &self.capital_lock_ref)?;
        nr_append_checked_v1(&mut out, "capital-seq", 1, &self.capital_sequence.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "preseal-fee", 1, &self.preseal_fee_commitment)?;
        nr_append_checked_v1(&mut out, "producer", 1, &self.producer_principal_id)?;
        nr_append_checked_v1(&mut out, "model", 1, &self.model_id)?;
        nr_append_checked_v1(&mut out, "runtime-profile", 1, &self.runtime_profile_id)?;
        nr_append_checked_v1(&mut out, "job-input", 1, &self.job_input_commitment)?;
        nr_append_checked_v1(&mut out, "trace-root", 1, &self.trace_root)?;
        nr_append_checked_v1(&mut out, "output-root", 1, &self.output_root)?;
        nr_append_checked_v1(&mut out, "transcript-root", 1, &self.transcript_root)?;
        nr_append_checked_v1(&mut out, "resource-desc", 1, &self.resource_descriptor_id)?;
        nr_append_checked_v1(&mut out, "readiness-root", 1, &self.readiness_root)?;
        nr_append_checked_v1(&mut out, "escrow-lock", 1, &self.escrow_lock_id)?;
        nr_append_checked_v1(&mut out, "ready-at-ms", 1, &self.ready_at_ms.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "hard-expiry-ms", 1, &self.hard_expiry_ms.to_le_bytes())?;
        Ok(out)
    }

    /// Compute `PresealId = H256(LP("MISAKA/PALW/CAD1/PRESEAL") || encode)`.
    pub fn preseal_id(&self) -> Result<[u8; 32], Cad1ErrorV1> {
        let encoded = self.encode()?;
        let mut buf = Vec::with_capacity(4 + TAG_CAD1_PRESEAL_V1.len() + encoded.len());
        lp_append_checked_v1(&mut buf, TAG_CAD1_PRESEAL_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&encoded);
        Ok(swc1_h256_v1(&buf))
    }
}

/// Compute `CAD1` ticket bytes (`H256`, big-endian `uint256_be`, `RFC-0056 §6.1`).
pub fn cad1_ticket_bytes_v1(
    network_id: &[u8; 32],
    policy_id: &[u8; 32],
    target_epoch: u64,
    seed_anchor: &Swc1TypedDigestV1,
    seed_value: &Swc1TypedDigestV1,
    trial_slot_key: &[u8; 32],
    preseal_id: &[u8; 32],
) -> Result<[u8; 32], Cad1ErrorV1> {
    seed_anchor.validate()?;
    seed_value.validate()?;
    let mut buf = Vec::with_capacity(380);
    lp_append_checked_v1(&mut buf, TAG_CAD1_TICKET_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    nr_append_checked_v1(&mut buf, "network", 1, network_id)?;
    nr_append_checked_v1(&mut buf, "policy", 1, policy_id)?;
    nr_append_checked_v1(&mut buf, "target-epoch", 1, &target_epoch.to_le_bytes())?;
    nr_append_checked_v1(&mut buf, "seed-anchor", seed_anchor.version, &seed_anchor.bytes)?;
    nr_append_checked_v1(&mut buf, "seed-value", seed_value.version, &seed_value.bytes)?;
    nr_append_checked_v1(&mut buf, "trial-slot", 1, trial_slot_key)?;
    nr_append_checked_v1(&mut buf, "preseal", 1, preseal_id)?;
    Ok(swc1_h256_v1(&buf))
}

/// Ticket derivation mode supporting standalone `CAD1` (`PresealId`), `SWC1` (`WorkTicket` + `OriginalId`, `RFC-0056 §18`),
/// and `PALW-AN1` input-bound tickets (`H("AN1-ticket", s, input_root, output_root, trace_root)` ranked by `(ticket, slot_id)`, `RFC-0056 §17`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cad1TicketInputV1 {
    Cad1Preseal {
        network_id: [u8; 32],
        policy_id: [u8; 32],
        target_epoch: u64,
        seed_anchor: Swc1TypedDigestV1,
        seed_value: Swc1TypedDigestV1,
        trial_slot_key: [u8; 32],
        preseal_id: [u8; 32],
    },
    Swc1WorkTicket {
        network_id: [u8; 32],
        policy_id: [u8; 32],
        ticket_epoch: u64,
        original_freeze_id: [u8; 32],
        r_ticket: Swc1TypedDigestV1,
        opportunity_id: [u8; 32],
        work_key: [u8; 32],
        original_id: [u8; 32],
    },
    An1InputBoundTicket {
        attempt_s: [u8; 32],
        input_root: [u8; 32],
        output_root: [u8; 32],
        trace_root: [u8; 32],
        slot_id: [u8; 32],
    },
}

impl Cad1TicketInputV1 {
    pub fn ticket_bytes_and_candidate_id(&self) -> Result<([u8; 32], [u8; 32]), Cad1ErrorV1> {
        match self {
            Self::Cad1Preseal { network_id, policy_id, target_epoch, seed_anchor, seed_value, trial_slot_key, preseal_id } => {
                let ticket =
                    cad1_ticket_bytes_v1(network_id, policy_id, *target_epoch, seed_anchor, seed_value, trial_slot_key, preseal_id)?;
                Ok((ticket, *preseal_id))
            }
            Self::Swc1WorkTicket {
                network_id,
                policy_id,
                ticket_epoch,
                original_freeze_id,
                r_ticket,
                opportunity_id,
                work_key,
                original_id,
            } => {
                let ticket = swc1_work_ticket_bytes_v1(
                    network_id,
                    policy_id,
                    *ticket_epoch,
                    original_freeze_id,
                    r_ticket,
                    opportunity_id,
                    work_key,
                    original_id,
                )?;
                Ok((ticket, *original_id))
            }
            Self::An1InputBoundTicket { attempt_s, input_root, output_root, trace_root, slot_id } => {
                let ticket = an1_ticket_bytes_v1(attempt_s, input_root, output_root, trace_root);
                Ok((ticket, *slot_id))
            }
        }
    }
}

/// 10-dimensional per-epoch flow resource vector (`RFC-0055 §5.2`, `RFC-0056 §9.1`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1FlowVectorV1 {
    pub deterministic_work: u64,
    pub signature_checks: u64,
    pub hash_bytes: u64,
    pub parse_and_expand_bytes: u64,
    pub wire_bytes: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub io_operations: u64,
    pub control_operations: u64,
    pub carrier_space: u64,
}

impl Cad1FlowVectorV1 {
    pub const fn as_array(&self) -> [u64; 10] {
        [
            self.deterministic_work,
            self.signature_checks,
            self.hash_bytes,
            self.parse_and_expand_bytes,
            self.wire_bytes,
            self.io_read_bytes,
            self.io_write_bytes,
            self.io_operations,
            self.control_operations,
            self.carrier_space,
        ]
    }

    pub const fn from_array(a: [u64; 10]) -> Self {
        Self {
            deterministic_work: a[0],
            signature_checks: a[1],
            hash_bytes: a[2],
            parse_and_expand_bytes: a[3],
            wire_bytes: a[4],
            io_read_bytes: a[5],
            io_write_bytes: a[6],
            io_operations: a[7],
            control_operations: a[8],
            carrier_space: a[9],
        }
    }

    pub fn checked_add(&self, rhs: &Self) -> Option<Self> {
        let lhs_a = self.as_array();
        let rhs_a = rhs.as_array();
        let mut out = [0u64; 10];
        for i in 0..10 {
            out[i] = lhs_a[i].checked_add(rhs_a[i])?;
        }
        Some(Self::from_array(out))
    }

    pub fn saturating_add(&self, rhs: &Self) -> Self {
        let lhs_a = self.as_array();
        let rhs_a = rhs.as_array();
        let mut out = [0u64; 10];
        for i in 0..10 {
            out[i] = lhs_a[i].saturating_add(rhs_a[i]);
        }
        Self::from_array(out)
    }

    pub fn fits_within(&self, cap: &Self) -> bool {
        let lhs_a = self.as_array();
        let cap_a = cap.as_array();
        for i in 0..10 {
            if lhs_a[i] > cap_a[i] {
                return false;
            }
        }
        true
    }

    /// Compute `max_r ceil(1024 * used[r] / cap[r])` across all 10 flow dimensions (`RFC-0056 §12.1`).
    pub fn max_utilization_bps_1024(&self, cap: &Self) -> Option<u64> {
        let used_a = self.as_array();
        let cap_a = cap.as_array();
        let mut max_u = 0u64;
        for i in 0..10 {
            if cap_a[i] == 0 {
                return None;
            }
            let num = (used_a[i] as u128).saturating_mul(CAD1_SCALE_U_V1 as u128);
            let den = cap_a[i] as u128;
            let ceil_ratio = num.div_ceil(den) as u64;
            if ceil_ratio > max_u {
                max_u = ceil_ratio;
            }
        }
        Some(max_u)
    }
}

/// Stock usage and stock budget caps (`RFC-0056 §9.2`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1StockVectorV1 {
    pub retained_bytes: u64,
    pub live_cases: u64,
    pub live_obligations: u64,
    pub active_locks: u64,
    pub preseal_count: u64,
}

impl Cad1StockVectorV1 {
    pub const fn as_array(&self) -> [u64; 5] {
        [self.retained_bytes, self.live_cases, self.live_obligations, self.active_locks, self.preseal_count]
    }

    pub fn fits_within(&self, cap: &Self) -> bool {
        let u = self.as_array();
        let c = cap.as_array();
        for i in 0..5 {
            if u[i] > c[i] {
                return false;
            }
        }
        true
    }

    pub fn checked_add(&self, rhs: &Self) -> Option<Self> {
        Some(Self {
            retained_bytes: self.retained_bytes.checked_add(rhs.retained_bytes)?,
            live_cases: self.live_cases.checked_add(rhs.live_cases)?,
            live_obligations: self.live_obligations.checked_add(rhs.live_obligations)?,
            active_locks: self.active_locks.checked_add(rhs.active_locks)?,
            preseal_count: self.preseal_count.checked_add(rhs.preseal_count)?,
        })
    }

    pub fn max_utilization_bps_1024(&self, cap: &Self) -> Option<u64> {
        let u = self.as_array();
        let c = cap.as_array();
        let mut max_u = 0u64;
        for i in 0..5 {
            if c[i] == 0 {
                return None;
            }
            let num = (u[i] as u128).saturating_mul(CAD1_SCALE_U_V1 as u128);
            let den = c[i] as u128;
            let ceil_ratio = num.div_ceil(den) as u64;
            if ceil_ratio > max_u {
                max_u = ceil_ratio;
            }
        }
        Some(max_u)
    }
}

/// Worst-case per-candidate resource descriptor (`ClaimResourceDescriptor`, `RFC-0055 §6.1`, `RFC-0056 §10.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1ClaimResourceDescriptorV1 {
    pub descriptor_id: [u8; 32],
    /// Flow cost charged in the admission epoch `e` itself.
    pub immediate_flow: Cad1FlowVectorV1,
    /// Worst-case future flow schedule indexed by `delta_epoch` (`0..=max_horizon_epochs`) for positive/negative court obligations.
    pub future_epoch_flows: BTreeMap<u64, Cad1FlowVectorV1>,
    /// Stock footprint held while the claim is live.
    pub stock_footprint: Cad1StockVectorV1,
    /// Required escrow in sompi covering worst-case court and verification obligations.
    pub required_escrow_sompi: u64,
    /// Maximum legal lifetime in milliseconds from `ready_at_ms` to `hard_expiry_ms`.
    pub max_lifetime_ms: u64,
}

/// Admission and difficulty controller policy (`AdmissionPolicy`, `RFC-0056 §4.3, §12`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1AdmissionPolicyV1 {
    pub policy_id: [u8; 32],
    pub epoch_ms: u64,
    pub g_ready_ms: u64,
    pub d_init: u128,
    pub d_min: u128,
    pub d_max: u128,
    pub min_capital_lock_sompi: u64,
    pub min_preseal_fee_sompi: u64,
    pub per_preseal_control_cost: Cad1FlowVectorV1,
    pub epoch_flow_cap: Cad1FlowVectorV1,
    pub stock_cap: Cad1StockVectorV1,
    pub max_future_horizon_epochs: u64,
}

impl Cad1AdmissionPolicyV1 {
    pub fn validate(&self, node_flow_cap: &Cad1FlowVectorV1, node_stock_cap: &Cad1StockVectorV1) -> Result<(), Cad1ErrorV1> {
        if self.epoch_ms == 0 || self.g_ready_ms == 0 || self.g_ready_ms >= self.epoch_ms {
            return Err(Cad1ErrorV1::InvalidPolicyBounds("epoch_ms and g_ready_ms must satisfy 0 < g_ready_ms < epoch_ms"));
        }
        if self.d_min == 0 || self.d_init < self.d_min || self.d_init > self.d_max || self.d_max > SWC1_MAX_DIFFICULTY_D_V1 {
            return Err(Cad1ErrorV1::InvalidPolicyBounds("difficulty bounds must satisfy 1 <= d_min <= d_init <= d_max <= 2^120 - 1"));
        }
        if self.min_capital_lock_sompi == 0 || self.min_preseal_fee_sompi == 0 {
            return Err(Cad1ErrorV1::InvalidPolicyBounds("min_capital_lock_sompi and min_preseal_fee_sompi must be positive"));
        }
        if self.per_preseal_control_cost.control_operations == 0 {
            return Err(Cad1ErrorV1::InvalidPolicyBounds("per_preseal_control_cost must charge nonzero control_operations"));
        }
        if !self.epoch_flow_cap.fits_within(node_flow_cap) || !self.stock_cap.fits_within(node_stock_cap) {
            return Err(Cad1ErrorV1::NodeBelowCommonCheckerProfile("node capacity profile is below CommonCheckerProfileV1"));
        }
        Ok(())
    }
}

/// Candidate submitted for target epoch `e` (`RFC-0056 §8, §11`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1RegisteredCandidateV1 {
    pub candidate_id: [u8; 32],
    pub preseal: Cad1PresealFieldsV1,
    pub swc1_work_key: Option<[u8; 32]>,
    pub swc1_opportunity_id: Option<[u8; 32]>,
    pub locked_capital_sompi: u64,
    pub paid_preseal_fee_sompi: u64,
    pub available_escrow_sompi: u64,
    pub qualification_valid: bool,
    pub shape_supported: bool,
    pub material_ready_on_chain: bool,
    pub resource_descriptor: Cad1ClaimResourceDescriptorV1,
}

/// Canonical rank `(ticket_be, candidate_id)` (`RFC-0056 §6.3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Cad1RankKeyV1 {
    pub ticket_be: [u8; 32],
    pub candidate_id: [u8; 32],
}

/// Per-epoch observation inputs for the `D_(b+2)` difficulty controller (`RFC-0056 §12.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1EpochObservationV1 {
    pub epoch: u64,
    pub u_future_bps_1024: u64,
    pub u_spent_bps_1024: u64,
    pub u_stock_bps_1024: u64,
    pub u_max_bps_1024: u64,
    pub qualified_demand: u64,
    pub known_congestion: bool,
    pub observation_complete: bool,
    pub unavailable: bool,
}

impl Cad1EpochObservationV1 {
    pub fn compute(
        epoch: u64,
        future_peak_flow: &Cad1FlowVectorV1,
        spent_flow: &Cad1FlowVectorV1,
        stock_peak: &Cad1StockVectorV1,
        policy: &Cad1AdmissionPolicyV1,
        qualified_demand: u64,
        known_congestion: bool,
        observation_complete: bool,
        unavailable: bool,
    ) -> Self {
        let u_future = future_peak_flow.max_utilization_bps_1024(&policy.epoch_flow_cap);
        let u_spent = spent_flow.max_utilization_bps_1024(&policy.epoch_flow_cap);
        let u_stock = stock_peak.max_utilization_bps_1024(&policy.stock_cap);
        match (u_future, u_spent, u_stock) {
            (Some(uf), Some(us), Some(uk)) => {
                let u_max = uf.max(us).max(uk);
                Self {
                    epoch,
                    u_future_bps_1024: uf,
                    u_spent_bps_1024: us,
                    u_stock_bps_1024: uk,
                    u_max_bps_1024: u_max,
                    qualified_demand,
                    known_congestion,
                    observation_complete,
                    unavailable,
                }
            }
            _ => Self {
                epoch,
                u_future_bps_1024: 0,
                u_spent_bps_1024: 0,
                u_stock_bps_1024: 0,
                u_max_bps_1024: 0,
                qualified_demand,
                known_congestion,
                observation_complete: false,
                unavailable: true,
            },
        }
    }
}

/// Update `D_(b+2)` from `D_(b+1)` and epoch `b`'s observation (`RFC-0056 §12.2`).
///
/// Returns `(next_difficulty_d_b_plus_2, next_low_streak)`.
pub fn cad1_update_difficulty_v1(
    d_b_plus_1: u128,
    low_streak: u32,
    obs: &Cad1EpochObservationV1,
    policy: &Cad1AdmissionPolicyV1,
) -> Result<(u128, u32), Cad1ErrorV1> {
    if d_b_plus_1 == 0 || d_b_plus_1 > SWC1_MAX_DIFFICULTY_D_V1 {
        return Err(Swc1ErrorV1::InvalidDifficulty(d_b_plus_1).into());
    }
    if obs.unavailable || !obs.observation_complete {
        return Ok((d_b_plus_1, low_streak));
    }
    if obs.u_max_bps_1024 >= CAD1_U_HIGH_V1 || obs.known_congestion {
        let mul9_ceil8 = d_b_plus_1.saturating_mul(9).div_ceil(8);
        let stepped = (d_b_plus_1.saturating_add(1)).max(mul9_ceil8).min(policy.d_max);
        return Ok((stepped, 0));
    }
    if obs.u_max_bps_1024 <= CAD1_U_LOW_V1 && !obs.known_congestion {
        let next_streak = low_streak.saturating_add(1);
        if next_streak >= CAD1_W_DOWN_EPOCHS_V1 {
            let decayed = (d_b_plus_1.saturating_mul(8) / 9).max(policy.d_min);
            return Ok((decayed, 0));
        }
        return Ok((d_b_plus_1, next_streak));
    }
    Ok((d_b_plus_1, 0))
}

/// Target epoch state machine (`TargetState[e]`, `RFC-0056 §8`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1TargetEpochStateV1 {
    pub target_epoch: u64,
    pub difficulty_d: u128,
    pub frozen: bool,
    pub freeze_anchor: Option<Swc1TypedDigestV1>,
    pub seed_value: Option<Swc1TypedDigestV1>,
    pub seed_unavailable: bool,
    pub candidates: BTreeMap<[u8; 32], Cad1RegisteredCandidateV1>,
    pub ranked_eligible: Vec<Cad1RankKeyV1>,
    pub cursor: usize,
    pub dispositions: BTreeMap<[u8; 32], Cad1CandidateDispositionV1>,
    pub closed: bool,
}

impl Cad1TargetEpochStateV1 {
    pub fn new(target_epoch: u64, difficulty_d: u128) -> Self {
        Self {
            target_epoch,
            difficulty_d,
            frozen: false,
            freeze_anchor: None,
            seed_value: None,
            seed_unavailable: false,
            candidates: BTreeMap::new(),
            ranked_eligible: Vec::new(),
            cursor: 0,
            dispositions: BTreeMap::new(),
            closed: false,
        }
    }
}

/// Receipt emitted by an idempotent `AdmitBatch` / `AdmissionAdvance` (`RFC-0056 §11.4`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1BatchAdvanceReceiptV1 {
    pub batch_id: [u8; 32],
    pub target_epoch: u64,
    pub start_cursor: usize,
    pub end_cursor: usize,
    pub visited_dispositions: Vec<([u8; 32], Cad1CandidateDispositionV1)>,
}

/// Global pre-admission gate, service calendar, and `D_admit` controller state (`RFC-0055` & `RFC-0056`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1AdmissionEngineV1 {
    pub policy: Cad1AdmissionPolicyV1,
    pub epoch_difficulties: BTreeMap<u64, u128>,
    pub low_streak: u32,
    pub target_epochs: BTreeMap<u64, Cad1TargetEpochStateV1>,
    pub trial_slot_ledger: BTreeMap<[u8; 32], [u8; 32]>,
    pub capital_sequence_highwater: BTreeMap<[u8; 32], u64>,
    /// Global future flow reservation calendar indexed by absolute epoch `t`.
    pub reserved_epoch_flows: BTreeMap<u64, Cad1FlowVectorV1>,
    /// Actual spent flow per epoch `t`.
    pub spent_epoch_flows: BTreeMap<u64, Cad1FlowVectorV1>,
    /// Current live stock usage.
    pub live_stock: Cad1StockVectorV1,
    /// Peak stock observed per epoch `t`.
    pub epoch_peak_stock: BTreeMap<u64, Cad1StockVectorV1>,
    /// Executed admission batch receipts for idempotent replay.
    pub batch_receipts: BTreeMap<[u8; 32], Cad1BatchAdvanceReceiptV1>,
    /// Audit counter of heavy decode / planner calls (must remain 0 for `Ineligible` candidates).
    pub heavy_decode_invocations: u64,
}

impl Cad1AdmissionEngineV1 {
    pub fn new(policy: Cad1AdmissionPolicyV1) -> Self {
        let mut epoch_difficulties = BTreeMap::new();
        // Bootstrap rule (`RFC-0056 §12.3`): epochs 0 and 1 start at D_init.
        epoch_difficulties.insert(0, policy.d_init);
        epoch_difficulties.insert(1, policy.d_init);
        Self {
            policy,
            epoch_difficulties,
            low_streak: 0,
            target_epochs: BTreeMap::new(),
            trial_slot_ledger: BTreeMap::new(),
            capital_sequence_highwater: BTreeMap::new(),
            reserved_epoch_flows: BTreeMap::new(),
            spent_epoch_flows: BTreeMap::new(),
            live_stock: Cad1StockVectorV1::default(),
            epoch_peak_stock: BTreeMap::new(),
            batch_receipts: BTreeMap::new(),
            heavy_decode_invocations: 0,
        }
    }

    /// Return the scheduled admission difficulty `D_e` for target epoch `e`.
    pub fn difficulty_for_epoch(&self, target_epoch: u64) -> u128 {
        self.epoch_difficulties.get(&target_epoch).copied().unwrap_or(self.policy.d_init)
    }

    /// Common pre-admission compact gate (`RFC-0055 §8`, `RFC-0056 §6, §14`).
    ///
    /// Called at the very front of `Attempt/PAV3`, `FreePrompt`, `TFP1 Plain`, `TFO1/TFO2/TFO3`,
    /// `EvaluationClaim`, `MultiOriginalBatch`, and detailed `Quote RPC` BEFORE heavy trace/geometry/planner
    /// decoding. Continuations of already-accepted claims bypass `D_admit` immediately.
    pub fn check_compact_pre_admission_gate(
        &self,
        route_kind: Cad1IngressRouteKindV1,
        ticket_input: Option<&Cad1TicketInputV1>,
        target_epoch: u64,
    ) -> Result<bool, Cad1ErrorV1> {
        match route_kind {
            Cad1IngressRouteKindV1::ContinuationOfAcceptedClaim => Ok(true),
            Cad1IngressRouteKindV1::NewOriginalCandidate(_family) => {
                let Some(input) = ticket_input else {
                    return Ok(false);
                };
                let (ticket_be, _cid) = input.ticket_bytes_and_candidate_id()?;
                let d_e = self.difficulty_for_epoch(target_epoch);
                match input {
                    Cad1TicketInputV1::An1InputBoundTicket { .. } => {
                        let theta = An1TargetThreshold257V1::from_difficulty_d(d_e)?;
                        Ok(theta.admits_ticket_be(&ticket_be))
                    }
                    _ => Ok(swc1_ticket_meets_difficulty_v1(&ticket_be, d_e)?),
                }
            }
        }
    }

    /// Rank and filter `PALW-AN1` input-bound candidates under strict 257-bit target `theta(D_e)`
    /// and `TimingPlan` order (`RFC-0056 §17.1–§17.4`, `A56-01`..`A56-09`).
    pub fn rank_an1_candidates_by_ticket_and_slot_id(
        &self,
        target_epoch: u64,
        timing: &An1TimingPlanV1,
        candidates: &[Cad1TicketInputV1],
    ) -> Result<(Vec<Cad1RankKeyV1>, Vec<[u8; 32]>), Cad1ErrorV1> {
        timing.validate()?;
        let d_e = self.difficulty_for_epoch(target_epoch);
        let theta = An1TargetThreshold257V1::from_difficulty_d(d_e)?;
        let mut eligible = Vec::new();
        let mut rejected_slot_ids = Vec::new();
        for cand in candidates {
            let (ticket_be, slot_id) = cand.ticket_bytes_and_candidate_id()?;
            if theta.admits_ticket_be(&ticket_be) {
                eligible.push(Cad1RankKeyV1 { ticket_be, candidate_id: slot_id });
            } else {
                rejected_slot_ids.push(slot_id);
            }
        }
        eligible.sort();
        Ok((eligible, rejected_slot_ids))
    }

    /// Submit a preseal candidate for `target_epoch` during collection epoch `target_epoch - 2` (`RFC-0056 §8.1`).
    pub fn register_preseal_candidate(
        &mut self,
        current_epoch: u64,
        candidate: Cad1RegisteredCandidateV1,
    ) -> Result<[u8; 32], Cad1ErrorV1> {
        let target_epoch = candidate.preseal.target_epoch;
        if target_epoch != current_epoch.saturating_add(2) {
            return Err(Cad1ErrorV1::TargetEpochNotCollecting { target_epoch, current_epoch });
        }
        if candidate.locked_capital_sompi < self.policy.min_capital_lock_sompi {
            return Err(Cad1ErrorV1::CapitalLockInsufficient {
                locked_sompi: candidate.locked_capital_sompi,
                required_sompi: self.policy.min_capital_lock_sompi,
            });
        }
        if candidate.paid_preseal_fee_sompi < self.policy.min_preseal_fee_sompi {
            return Err(Cad1ErrorV1::PresealFeeInsufficient {
                fee_sompi: candidate.paid_preseal_fee_sompi,
                required_sompi: self.policy.min_preseal_fee_sompi,
            });
        }
        let prev_seq = self.capital_sequence_highwater.get(&candidate.preseal.capital_lock_ref).copied().unwrap_or(0);
        if candidate.preseal.capital_sequence <= prev_seq {
            return Err(Cad1ErrorV1::CapitalSequenceNotMonotone {
                capital_lock_ref: candidate.preseal.capital_lock_ref,
                sequence: candidate.preseal.capital_sequence,
                highwater: prev_seq,
            });
        }
        let expected_slot = candidate.preseal.compute_trial_slot_key()?;
        if candidate.preseal.trial_slot_key != expected_slot {
            return Err(Cad1ErrorV1::InvalidPolicyBounds("trial_slot_key does not match canonical preseal fields"));
        }
        let preseal_id = candidate.preseal.preseal_id()?;
        if let Some(existing) = self.trial_slot_ledger.get(&expected_slot) {
            return Err(Cad1ErrorV1::TrialSlotAlreadyUsed { trial_slot_key: expected_slot, existing_preseal_id: *existing });
        }
        let d_e = self.difficulty_for_epoch(target_epoch);
        let state = self.target_epochs.entry(target_epoch).or_insert_with(|| Cad1TargetEpochStateV1::new(target_epoch, d_e));
        if state.frozen {
            return Err(Cad1ErrorV1::TargetEpochAlreadyFrozen(target_epoch));
        }
        if (state.candidates.len() as u64) >= self.policy.stock_cap.preseal_count {
            return Err(Cad1ErrorV1::TargetEpochPresealCapExceeded { target_epoch, cap: self.policy.stock_cap.preseal_count });
        }
        // Check global control flow budget for preseal registration in current_epoch
        let cur_flow = self.spent_epoch_flows.get(&current_epoch).copied().unwrap_or_default();
        let next_flow = cur_flow
            .checked_add(&self.policy.per_preseal_control_cost)
            .ok_or(Cad1ErrorV1::TargetEpochPresealCapExceeded { target_epoch, cap: self.policy.stock_cap.preseal_count })?;
        if !next_flow.fits_within(&self.policy.epoch_flow_cap) {
            return Err(Cad1ErrorV1::TargetEpochPresealCapExceeded { target_epoch, cap: self.policy.stock_cap.preseal_count });
        }
        self.spent_epoch_flows.insert(current_epoch, next_flow);
        self.capital_sequence_highwater.insert(candidate.preseal.capital_lock_ref, candidate.preseal.capital_sequence);
        self.trial_slot_ledger.insert(expected_slot, preseal_id);
        state.candidates.insert(candidate.candidate_id, candidate);
        self.live_stock.preseal_count = self.live_stock.preseal_count.saturating_add(1);
        let peak = self.epoch_peak_stock.entry(current_epoch).or_default();
        if self.live_stock.preseal_count > peak.preseal_count {
            peak.preseal_count = self.live_stock.preseal_count;
        }
        Ok(preseal_id)
    }

    /// Freeze target epoch `e` at `Start(e - 1)` (`RFC-0056 §8.2`).
    pub fn freeze_target_epoch(&mut self, target_epoch: u64, freeze_anchor: Swc1TypedDigestV1) -> Result<(), Cad1ErrorV1> {
        freeze_anchor.validate()?;
        let d_e = self.difficulty_for_epoch(target_epoch);
        let state = self.target_epochs.entry(target_epoch).or_insert_with(|| Cad1TargetEpochStateV1::new(target_epoch, d_e));
        if state.frozen {
            return Err(Cad1ErrorV1::TargetEpochAlreadyFrozen(target_epoch));
        }
        state.frozen = true;
        state.freeze_anchor = Some(freeze_anchor);
        Ok(())
    }

    /// Record the post-freeze randomness seed for target epoch `e` in `[Start(e-1), Start(e) - G_ready_ms]` (`RFC-0056 §8.2`).
    pub fn record_target_epoch_seed(
        &mut self,
        target_epoch: u64,
        freeze_anchor: &Swc1TypedDigestV1,
        seed_value: Swc1TypedDigestV1,
        arrived_ms: u64,
    ) -> Result<usize, Cad1ErrorV1> {
        seed_value.validate()?;
        let state = self.target_epochs.get_mut(&target_epoch).ok_or(Cad1ErrorV1::TargetEpochNotFrozen(target_epoch))?;
        if !state.frozen {
            return Err(Cad1ErrorV1::TargetEpochNotFrozen(target_epoch));
        }
        if state.seed_value.is_some() {
            return Err(Cad1ErrorV1::TargetEpochSeedAlreadySet(target_epoch));
        }
        if state.freeze_anchor.as_ref() != Some(freeze_anchor) {
            return Err(Cad1ErrorV1::TargetEpochSeedAnchorMismatch);
        }
        let freeze_ms = target_epoch.saturating_sub(1).saturating_mul(self.policy.epoch_ms);
        let start_e_ms = target_epoch.saturating_mul(self.policy.epoch_ms);
        let ready_cut_ms = start_e_ms.saturating_sub(self.policy.g_ready_ms);
        if arrived_ms < freeze_ms || arrived_ms >= ready_cut_ms {
            return Err(Cad1ErrorV1::TargetEpochSeedWindowViolation { arrived_ms, freeze_ms, ready_cut_ms });
        }
        let threshold_be = swc1_difficulty_threshold_be_v1(state.difficulty_d)?;
        let mut eligible = Vec::new();
        for (cid, cand) in &state.candidates {
            let ticket_input = match (cand.swc1_opportunity_id, cand.swc1_work_key) {
                (Some(opp_id), Some(wk)) => {
                    let mut anchor32 = [0u8; 32];
                    anchor32.copy_from_slice(&freeze_anchor.bytes[..32]);
                    Cad1TicketInputV1::Swc1WorkTicket {
                        network_id: cand.preseal.network_id,
                        policy_id: cand.preseal.policy_id,
                        ticket_epoch: target_epoch,
                        original_freeze_id: anchor32,
                        r_ticket: seed_value.clone(),
                        opportunity_id: opp_id,
                        work_key: wk,
                        original_id: *cid,
                    }
                }
                _ => Cad1TicketInputV1::Cad1Preseal {
                    network_id: cand.preseal.network_id,
                    policy_id: cand.preseal.policy_id,
                    target_epoch,
                    seed_anchor: freeze_anchor.clone(),
                    seed_value: seed_value.clone(),
                    trial_slot_key: cand.preseal.trial_slot_key,
                    preseal_id: *cid,
                },
            };
            let (ticket_be, _) = ticket_input.ticket_bytes_and_candidate_id()?;
            if ticket_be <= threshold_be {
                eligible.push(Cad1RankKeyV1 { ticket_be, candidate_id: *cid });
            } else {
                // Ineligible candidates are filtered here without ever invoking heavy decode or planner!
                state.dispositions.insert(*cid, Cad1CandidateDispositionV1::Ineligible);
            }
        }
        eligible.sort();
        let count = eligible.len();
        state.seed_value = Some(seed_value);
        state.ranked_eligible = eligible;
        Ok(count)
    }

    /// Mark target epoch `e`'s seed as unavailable when no valid seed arrived before `Start(e) - G_ready_ms` (`RFC-0056 §8.3`).
    pub fn mark_target_epoch_seed_unavailable(&mut self, target_epoch: u64) -> Result<(), Cad1ErrorV1> {
        let state = self.target_epochs.get_mut(&target_epoch).ok_or(Cad1ErrorV1::TargetEpochNotFrozen(target_epoch))?;
        state.seed_unavailable = true;
        let ids: Vec<[u8; 32]> = state.candidates.keys().copied().collect();
        for cid in ids {
            state.dispositions.insert(cid, Cad1CandidateDispositionV1::SeedUnavailable);
        }
        Ok(())
    }

    /// Check whether `desc` fits an empty claim book vs the current active claim book (`RFC-0056 §11.1–§11.3`).
    fn check_resource_fit(&self, target_epoch: u64, desc: &Cad1ClaimResourceDescriptorV1, empty_book: bool) -> bool {
        // 1. Immediate flow in target_epoch
        let base_imm = if empty_book {
            Cad1FlowVectorV1::default()
        } else {
            self.reserved_epoch_flows.get(&target_epoch).copied().unwrap_or_default()
        };
        let Some(next_imm) = base_imm.checked_add(&desc.immediate_flow) else {
            return false;
        };
        if !next_imm.fits_within(&self.policy.epoch_flow_cap) {
            return false;
        }
        // 2. Future epoch flow reservations
        for (&delta, flow) in &desc.future_epoch_flows {
            if delta > self.policy.max_future_horizon_epochs {
                return false;
            }
            let abs_epoch = target_epoch.saturating_add(delta);
            let base_f = if empty_book {
                if delta == 0 { next_imm } else { Cad1FlowVectorV1::default() }
            } else if delta == 0 {
                next_imm
            } else {
                self.reserved_epoch_flows.get(&abs_epoch).copied().unwrap_or_default()
            };
            let Some(next_f) = base_f.checked_add(flow) else {
                return false;
            };
            if !next_f.fits_within(&self.policy.epoch_flow_cap) {
                return false;
            }
        }
        // 3. Stock footprint
        let base_stock = if empty_book { Cad1StockVectorV1::default() } else { self.live_stock };
        let Some(next_stock) = base_stock.checked_add(&desc.stock_footprint) else {
            return false;
        };
        next_stock.fits_within(&self.policy.stock_cap)
    }

    /// Advance the rank-ordered prefix cursor for `target_epoch` atomically (`AdmitBatch`, `RFC-0056 §11.4`).
    ///
    /// - Replays of an already-committed `batch_id` return the cached receipt without double-spending.
    /// - Visiting out-of-order or non-prefix candidates rolls back the entire batch.
    pub fn advance_admission_batch(
        &mut self,
        batch_id: [u8; 32],
        target_epoch: u64,
        expected_candidate_prefix: &[[u8; 32]],
        simulate_planner_exception_on: Option<[u8; 32]>,
    ) -> Result<Cad1BatchAdvanceReceiptV1, Cad1ErrorV1> {
        if let Some(existing_receipt) = self.batch_receipts.get(&batch_id) {
            return Ok(existing_receipt.clone());
        }
        let mut staged = self.clone();
        let start_e_ms = target_epoch.saturating_mul(staged.policy.epoch_ms);
        let ready_cut_ms = start_e_ms.saturating_sub(staged.policy.g_ready_ms);

        let (start_cursor, rank_slice) = {
            let state = staged.target_epochs.get(&target_epoch).ok_or(Cad1ErrorV1::TargetEpochNotFrozen(target_epoch))?;
            if state.seed_value.is_none() {
                return Err(Cad1ErrorV1::TargetEpochNotFrozen(target_epoch));
            }
            let start = state.cursor;
            let end = start.saturating_add(expected_candidate_prefix.len());
            if end > state.ranked_eligible.len() {
                return Err(Cad1ErrorV1::CandidateNotEligible(*expected_candidate_prefix.last().unwrap_or(&[0u8; 32])));
            }
            let slice = state.ranked_eligible[start..end].to_vec();
            for (idx, rk) in slice.iter().enumerate() {
                if rk.candidate_id != expected_candidate_prefix[idx] {
                    return Err(Cad1ErrorV1::PrefixOrderViolation {
                        expected_next: rk.candidate_id,
                        got: expected_candidate_prefix[idx],
                    });
                }
            }
            (start, slice)
        };

        let mut visited_dispositions = Vec::with_capacity(rank_slice.len());
        for rk in &rank_slice {
            let cid = rk.candidate_id;
            if simulate_planner_exception_on == Some(cid) {
                return Err(Cad1ErrorV1::PlannerSimulationFailed(cid));
            }
            staged.heavy_decode_invocations = staged.heavy_decode_invocations.saturating_add(1);
            let cand = staged.target_epochs[&target_epoch].candidates[&cid].clone();
            let disp = if !cand.qualification_valid {
                Cad1CandidateDispositionV1::InvalidQualification
            } else if !cand.shape_supported {
                Cad1CandidateDispositionV1::UnsupportedShape
            } else if !cand.material_ready_on_chain || cand.preseal.ready_at_ms > ready_cut_ms {
                Cad1CandidateDispositionV1::MissingMaterial
            } else if cand.available_escrow_sompi < cand.resource_descriptor.required_escrow_sompi {
                Cad1CandidateDispositionV1::InsufficientFunds
            } else if cand.preseal.hard_expiry_ms <= start_e_ms
                || cand.preseal.hard_expiry_ms.saturating_sub(cand.preseal.ready_at_ms) > cand.resource_descriptor.max_lifetime_ms
            {
                Cad1CandidateDispositionV1::IntrinsicDeadlineFailure
            } else if !staged.check_resource_fit(target_epoch, &cand.resource_descriptor, true) {
                Cad1CandidateDispositionV1::IntrinsicShapeFailure
            } else if !staged.check_resource_fit(target_epoch, &cand.resource_descriptor, false) {
                Cad1CandidateDispositionV1::NoCapacity
            } else {
                // Reserve flow and stock in staged engine
                let cur_imm = staged.reserved_epoch_flows.get(&target_epoch).copied().unwrap_or_default();
                let next_imm = cur_imm.saturating_add(&cand.resource_descriptor.immediate_flow);
                staged.reserved_epoch_flows.insert(target_epoch, next_imm);
                let cur_spent = staged.spent_epoch_flows.get(&target_epoch).copied().unwrap_or_default();
                staged.spent_epoch_flows.insert(target_epoch, cur_spent.saturating_add(&cand.resource_descriptor.immediate_flow));
                for (&delta, flow) in &cand.resource_descriptor.future_epoch_flows {
                    let abs_e = target_epoch.saturating_add(delta);
                    let cur_f = staged.reserved_epoch_flows.get(&abs_e).copied().unwrap_or_default();
                    staged.reserved_epoch_flows.insert(abs_e, cur_f.saturating_add(flow));
                }
                staged.live_stock =
                    staged.live_stock.checked_add(&cand.resource_descriptor.stock_footprint).expect("checked in check_resource_fit");
                let peak = staged.epoch_peak_stock.entry(target_epoch).or_default();
                peak.retained_bytes = peak.retained_bytes.max(staged.live_stock.retained_bytes);
                peak.live_cases = peak.live_cases.max(staged.live_stock.live_cases);
                peak.live_obligations = peak.live_obligations.max(staged.live_stock.live_obligations);
                peak.active_locks = peak.active_locks.max(staged.live_stock.active_locks);
                peak.preseal_count = peak.preseal_count.max(staged.live_stock.preseal_count);
                Cad1CandidateDispositionV1::Accepted
            };
            staged.target_epochs.get_mut(&target_epoch).expect("checked").dispositions.insert(cid, disp);
            visited_dispositions.push((cid, disp));
        }

        let end_cursor = start_cursor + rank_slice.len();
        staged.target_epochs.get_mut(&target_epoch).expect("checked").cursor = end_cursor;
        let receipt = Cad1BatchAdvanceReceiptV1 { batch_id, target_epoch, start_cursor, end_cursor, visited_dispositions };
        staged.batch_receipts.insert(batch_id, receipt.clone());
        *self = staged;
        Ok(receipt)
    }

    /// Close epoch `b`, finalize unvisited candidates as `ExpiredUnvisited`, and compute `D_(b+2)` (`RFC-0056 §12.2`).
    pub fn close_epoch_and_update_difficulty(&mut self, epoch_b: u64) -> Result<(Cad1EpochObservationV1, u128), Cad1ErrorV1> {
        let mut qualified_demand = 0u64;
        let mut known_congestion = false;
        let mut observation_complete = true;
        let mut unavailable = false;

        if let Some(state) = self.target_epochs.get_mut(&epoch_b) {
            if state.seed_unavailable {
                unavailable = true;
                observation_complete = false;
            } else {
                if state.cursor < state.ranked_eligible.len() {
                    observation_complete = false;
                    for rk in &state.ranked_eligible[state.cursor..] {
                        state.dispositions.insert(rk.candidate_id, Cad1CandidateDispositionV1::ExpiredUnvisited);
                    }
                    state.cursor = state.ranked_eligible.len();
                }
                for disp in state.dispositions.values() {
                    if disp.is_qualified_demand() {
                        qualified_demand = qualified_demand.saturating_add(1);
                    }
                    if disp.is_known_congestion() {
                        known_congestion = true;
                    }
                }
            }
            state.closed = true;
        }

        // Peak future reserved flow across [epoch_b, epoch_b + max_future_horizon_epochs]
        let mut future_peak = [0u64; 10];
        for delta in 0..=self.policy.max_future_horizon_epochs {
            if let Some(flow) = self.reserved_epoch_flows.get(&(epoch_b.saturating_add(delta))) {
                let arr = flow.as_array();
                for i in 0..10 {
                    future_peak[i] = future_peak[i].max(arr[i]);
                }
            }
        }
        let spent_flow = self.spent_epoch_flows.get(&epoch_b).copied().unwrap_or_default();
        let stock_peak = self.epoch_peak_stock.get(&epoch_b).copied().unwrap_or(self.live_stock);

        let obs = Cad1EpochObservationV1::compute(
            epoch_b,
            &Cad1FlowVectorV1::from_array(future_peak),
            &spent_flow,
            &stock_peak,
            &self.policy,
            qualified_demand,
            known_congestion,
            observation_complete,
            unavailable,
        );
        let d_b_plus_1 = self.difficulty_for_epoch(epoch_b.saturating_add(1));
        let (d_b_plus_2, next_streak) = cad1_update_difficulty_v1(d_b_plus_1, self.low_streak, &obs, &self.policy)?;
        self.low_streak = next_streak;
        self.epoch_difficulties.insert(epoch_b.saturating_add(2), d_b_plus_2);
        Ok((obs, d_b_plus_2))
    }
}

/// Route-level compact pre-admission gate context carried on `PalwTransitionExtrasV1` (`RFC-0055 §8`, `RFC-0056 §6, §14`).
///
/// Evaluated at the very front of `Attempt/PAV3`, `FreePromptPlain`, `Tfp1PlainOriginal`,
/// `TfoFundedOriginal` (`TFO1`/`TFO2`/`TFO3`), `EvaluationClaim`, `MultiOriginalBatchItem`,
/// and `QuoteRpcDetailed` BEFORE heavy trace, geometry, actor-graph, or planner decoding.
/// Continuations of already-accepted claims (`ContinuationOfAcceptedClaim`) always pass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cad1RouteGateContextV1 {
    pub engine: Cad1AdmissionEngineV1,
    pub target_epoch: u64,
    pub candidate_tickets: BTreeMap<crate::Hash64, Cad1TicketInputV1>,
}

impl Cad1RouteGateContextV1 {
    pub fn check_route_entry_v1(&self, route_kind: Cad1IngressRouteKindV1, candidate_id: Option<crate::Hash64>) -> Result<(), String> {
        match route_kind {
            Cad1IngressRouteKindV1::ContinuationOfAcceptedClaim => Ok(()),
            Cad1IngressRouteKindV1::NewOriginalCandidate(family) => {
                let ticket_input = candidate_id.and_then(|id| self.candidate_tickets.get(&id));
                let passed = self
                    .engine
                    .check_compact_pre_admission_gate(route_kind, ticket_input, self.target_epoch)
                    .map_err(|e| format!("CAD1/SWC1 pre-admission compact gate error ({family:?}): {e:?}"))?;
                if !passed {
                    return Err(format!(
                        "CAD1/SWC1 pre-admission compact gate rejected ineligible candidate on route {family:?} at target_epoch {}",
                        self.target_epoch
                    ));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_policy() -> Cad1AdmissionPolicyV1 {
        Cad1AdmissionPolicyV1 {
            policy_id: [0x22; 32],
            epoch_ms: CAD1_EPOCH_MS_V1,
            g_ready_ms: 20_000,
            d_init: 1,
            d_min: 1,
            d_max: SWC1_MAX_DIFFICULTY_D_V1,
            min_capital_lock_sompi: 1_000_000,
            min_preseal_fee_sompi: 10_000,
            per_preseal_control_cost: Cad1FlowVectorV1 { control_operations: 1, ..Default::default() },
            epoch_flow_cap: Cad1FlowVectorV1::from_array([100; 10]),
            stock_cap: Cad1StockVectorV1 {
                retained_bytes: 1_000,
                live_cases: 10,
                live_obligations: 20,
                active_locks: 20,
                preseal_count: 50,
            },
            max_future_horizon_epochs: 4,
        }
    }

    fn sample_candidate(seq: u64, target_epoch: u64, work_units: u64, ready_on_chain: bool) -> Cad1RegisteredCandidateV1 {
        let mut preseal = Cad1PresealFieldsV1 {
            version: 1,
            network_id: [0x11; 32],
            policy_id: [0x22; 32],
            target_epoch,
            route_family: Cad1IngressRouteFamilyV1::TfoFundedOriginal as u8,
            trial_slot_key: [0u8; 32],
            capital_lock_ref: [0x77; 32],
            capital_sequence: seq,
            preseal_fee_commitment: [0x88; 32],
            producer_principal_id: [0x99; 32],
            model_id: [0x33; 32],
            runtime_profile_id: [0x44; 32],
            job_input_commitment: [seq as u8; 32],
            trace_root: [0x01; 32],
            output_root: [0x02; 32],
            transcript_root: [0x03; 32],
            resource_descriptor_id: [0x04; 32],
            readiness_root: [0x05; 32],
            escrow_lock_id: [0x06; 32],
            ready_at_ms: target_epoch * CAD1_EPOCH_MS_V1 - 30_000,
            hard_expiry_ms: (target_epoch + 2) * CAD1_EPOCH_MS_V1,
        };
        preseal.trial_slot_key = preseal.compute_trial_slot_key().unwrap();
        let cid = preseal.preseal_id().unwrap();
        let mut future = BTreeMap::new();
        future.insert(1, Cad1FlowVectorV1 { deterministic_work: work_units, ..Default::default() });
        Cad1RegisteredCandidateV1 {
            candidate_id: cid,
            preseal,
            swc1_work_key: None,
            swc1_opportunity_id: None,
            locked_capital_sompi: 2_000_000,
            paid_preseal_fee_sompi: 20_000,
            available_escrow_sompi: 500_000,
            qualification_valid: true,
            shape_supported: true,
            material_ready_on_chain: ready_on_chain,
            resource_descriptor: Cad1ClaimResourceDescriptorV1 {
                descriptor_id: [0x04; 32],
                immediate_flow: Cad1FlowVectorV1 { deterministic_work: work_units, ..Default::default() },
                future_epoch_flows: future,
                stock_footprint: Cad1StockVectorV1 {
                    retained_bytes: 100,
                    live_cases: 1,
                    live_obligations: 1,
                    active_locks: 1,
                    preseal_count: 0,
                },
                required_escrow_sompi: 100_000,
                max_lifetime_ms: 4 * CAD1_EPOCH_MS_V1,
            },
        }
    }

    #[test]
    fn cad1_compact_gate_empty_vs_active_fit_and_difficulty_update() {
        let policy = sample_policy();
        let mut engine = Cad1AdmissionEngineV1::new(policy.clone());

        // Continuation of an already-accepted claim is never blocked by D_admit
        assert!(engine.check_compact_pre_admission_gate(Cad1IngressRouteKindV1::ContinuationOfAcceptedClaim, None, 3).unwrap());

        // Collect 3 candidates in epoch 1 for target_epoch = 3:
        // c1: 60 work units (fits empty book, fits active book)
        // c2: 60 work units (fits empty book, fails active book -> NoCapacity / known_congestion)
        // c3: missing material (fails readiness -> MissingMaterial, NOT NoCapacity)
        let c1 = sample_candidate(1, 3, 60, true);
        let c2 = sample_candidate(2, 3, 60, true);
        let c3 = sample_candidate(3, 3, 10, false);

        engine.register_preseal_candidate(1, c1.clone()).unwrap();
        engine.register_preseal_candidate(1, c2.clone()).unwrap();
        engine.register_preseal_candidate(1, c3.clone()).unwrap();

        let anchor = Swc1TypedDigestV1::new64([0xAA; 64]);
        let seed = Swc1TypedDigestV1::new64([0xBB; 64]);
        engine.freeze_target_epoch(3, anchor.clone()).unwrap();
        let eligible_count = engine.record_target_epoch_seed(3, &anchor, seed, 2 * CAD1_EPOCH_MS_V1 + 10_000).unwrap();
        assert_eq!(eligible_count, 3);

        let ordered_ids: Vec<[u8; 32]> = engine.target_epochs[&3].ranked_eligible.iter().map(|rk| rk.candidate_id).collect();

        // Out-of-order prefix must fail without mutating state
        let mut reversed = ordered_ids.clone();
        reversed.reverse();
        assert!(matches!(
            engine.advance_admission_batch([0x01; 32], 3, &reversed, None),
            Err(Cad1ErrorV1::PrefixOrderViolation { .. })
        ));

        // Planner exception rolls back entire batch
        assert!(matches!(
            engine.advance_admission_batch([0x02; 32], 3, &ordered_ids, Some(ordered_ids[2])),
            Err(Cad1ErrorV1::PlannerSimulationFailed(_))
        ));
        assert_eq!(engine.target_epochs[&3].cursor, 0);

        // Valid prefix advance succeeds and recordsAccepted, NoCapacity, and MissingMaterial
        let receipt = engine.advance_admission_batch([0x03; 32], 3, &ordered_ids, None).unwrap();
        assert_eq!(receipt.end_cursor, 3);
        let disps = &engine.target_epochs[&3].dispositions;
        assert_eq!(disps[&c3.candidate_id], Cad1CandidateDispositionV1::MissingMaterial);
        let c1_c2 = [disps[&c1.candidate_id], disps[&c2.candidate_id]];
        assert!(c1_c2.contains(&Cad1CandidateDispositionV1::Accepted));
        assert!(c1_c2.contains(&Cad1CandidateDispositionV1::NoCapacity));

        // Close epoch 3: known_congestion is true -> D_5 increases above 1!
        let (obs, d5) = engine.close_epoch_and_update_difficulty(3).unwrap();
        assert!(obs.known_congestion);
        assert_eq!(obs.qualified_demand, 2);
        assert!(d5 > 1);
    }

    #[test]
    fn cad1_an1_input_bound_ticket_target_257_and_timing_plan_a56_01_to_a56_09() {
        let policy = sample_policy();
        let mut engine = Cad1AdmissionEngineV1::new(policy);

        // Set epoch 4 difficulty to D = 2 -> theta(2) = 0x800000...00
        engine.epoch_difficulties.insert(4, 2);

        let c_a = Cad1TicketInputV1::An1InputBoundTicket {
            attempt_s: [0x11; 32],
            input_root: [0x22; 32],
            output_root: [0x33; 32],
            trace_root: [0x44; 32],
            slot_id: [0xAA; 32],
        };
        let c_b = Cad1TicketInputV1::An1InputBoundTicket {
            attempt_s: [0x55; 32],
            input_root: [0x66; 32],
            output_root: [0x77; 32],
            trace_root: [0x88; 32],
            slot_id: [0xBB; 32],
        };
        let valid_timing = An1TimingPlanV1::default_test_plan();
        let (eligible_at_d1, rejected_at_d1) =
            engine.rank_an1_candidates_by_ticket_and_slot_id(1, &valid_timing, &[c_a.clone(), c_b.clone()]).unwrap();
        assert_eq!(eligible_at_d1.len(), 2);
        assert!(rejected_at_d1.is_empty());
        assert!(eligible_at_d1[0] <= eligible_at_d1[1]);

        // Reversed timing plan must be rejected (A56-09)
        let bad_timing = An1TimingPlanV1 { close_reg: 20, beacon_confirm: 10, seal_original: 30, panel_select: 40, hard_deadline: 50 };
        assert!(engine.rank_an1_candidates_by_ticket_and_slot_id(1, &bad_timing, &[c_a.clone()]).is_err());

        // Under D = 2, check_compact_pre_admission_gate agrees with rank_an1_candidates_by_ticket_and_slot_id
        let (eligible_d2, rejected_d2) =
            engine.rank_an1_candidates_by_ticket_and_slot_id(4, &valid_timing, &[c_a.clone(), c_b.clone()]).unwrap();
        assert_eq!(eligible_d2.len() + rejected_d2.len(), 2);
        let gate_a = engine
            .check_compact_pre_admission_gate(
                Cad1IngressRouteKindV1::NewOriginalCandidate(Cad1IngressRouteFamilyV1::TfoFundedOriginal),
                Some(&c_a),
                4,
            )
            .unwrap();
        assert_eq!(gate_a, eligible_d2.iter().any(|rk| rk.candidate_id == [0xAA; 32]));
    }
}
