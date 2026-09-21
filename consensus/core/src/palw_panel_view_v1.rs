//! **Panel observability: seat counts stay distinct, and a hold has a reason.**
//!
//! Operators kept collapsing four different numbers into one "operators = 7":
//!
//! * `bondedSeats` — bonds that declared the class
//! * `readySeats` — bonded seats whose possession proof still counts
//! * `selectedPanelSeats` — seats currently drawn onto an inflight claim
//! * `validReceiptSeats` — selected seats whose `Valid` the chain has credited
//!
//! Those are different facts. The RPC/CLI surface reads this module, so a client cannot mix them
//! by accident the way a log line could. A seat that is not ready also carries a machine-readable
//! [`PalwPanelHoldReasonV1`] (and a human sentence): `ready=false` alone is not enough to say
//! whether the artifact is missing, the proof expired, or the collateral is short.
//!
//! Local-only codes (`NO_ARTIFACT`, `NODE_IBD`, `NOT_SELECTED`, …) are produced by the node that
//! holds the weights. Chain reads never invent them: a missing proof on chain is
//! `READINESS_PROOF_MISSING`, not `NO_ARTIFACT`.

use crate::Hash64;
use crate::config::params::ForkActivation;
use crate::palw_fp_devnet_v3::{PALW_V2_PANEL_QUORUM, PALW_V2_PANEL_SEATS};
use crate::palw_model_registry_v1::{PalwModelLifecycleV1, PalwModelRegistryReadV1};
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwStateParamsV2, palw_bond_may_judge_class_v2,
    palw_bond_may_take_work_v2,
};
use crate::palw_verification_v2::{palw_segment_assignment_v2, palw_segment_count_v2};

/// Why a seat is not ready, or why this node cannot serve a class. Machine token first; the
/// sentence is for operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwPanelHoldReasonV1 {
    NoArtifact,
    ArtifactRootMismatch,
    ReplayBudgetInsufficient,
    NodeIbd,
    BondInactive,
    CollateralInsufficient,
    ReadinessProofMissing,
    ReadinessProofExpired,
    NotSelected,
    SegmentCheckpointMissing,
}

impl PalwPanelHoldReasonV1 {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NoArtifact => "NO_ARTIFACT",
            Self::ArtifactRootMismatch => "ARTIFACT_ROOT_MISMATCH",
            Self::ReplayBudgetInsufficient => "REPLAY_BUDGET_INSUFFICIENT",
            Self::NodeIbd => "NODE_IBD",
            Self::BondInactive => "BOND_INACTIVE",
            Self::CollateralInsufficient => "COLLATERAL_INSUFFICIENT",
            Self::ReadinessProofMissing => "READINESS_PROOF_MISSING",
            Self::ReadinessProofExpired => "READINESS_PROOF_EXPIRED",
            Self::NotSelected => "NOT_SELECTED",
            Self::SegmentCheckpointMissing => "SEGMENT_CHECKPOINT_MISSING",
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::NoArtifact => "this node does not hold a converted artifact for the class",
            Self::ArtifactRootMismatch => "the held artifact roots to a different value than the class registered",
            Self::ReplayBudgetInsufficient => "this host cannot spare the working set the class's replay needs",
            Self::NodeIbd => "the node is still in IBD, so a possession proof would name a span it cannot see",
            Self::BondInactive => "the bond is missing, inactive, or no longer taking work",
            Self::CollateralInsufficient => "free collateral is below the readiness floor or the readiness multiple",
            Self::ReadinessProofMissing => "the chain holds no standing possession proof for this seat and class",
            Self::ReadinessProofExpired => "the last possession proof is older than the readiness age",
            Self::NotSelected => "this seat is ready but the claim's panel did not draw it",
            Self::SegmentCheckpointMissing => "the assigned segment's SC01 checkpoint is not open, so a partial seat cannot resume",
        }
    }

    /// Map the registry's existing not-ready strings onto the enum. Unknown strings stay none so
    /// a new reason cannot be silently relabelled.
    pub fn from_registry_not_ready(reason: &str) -> Option<Self> {
        match reason {
            "" => None,
            "bond missing" | "bond inactive" => Some(Self::BondInactive),
            "below floor" | "collateral short" => Some(Self::CollateralInsufficient),
            "stale" => Some(Self::ReadinessProofExpired),
            _ => None,
        }
    }
}

/// S1/S2/S3 fence facts at one DAA — schedule and whether each is in force.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwVerificationScheduleV1 {
    pub v2: Option<ForkActivation>,
    pub s3: Option<ForkActivation>,
    pub s2: Option<ForkActivation>,
}

impl PalwVerificationScheduleV1 {
    pub fn at(self, daa: u64) -> PalwVerificationModeV1 {
        let v2 = self.v2.is_some_and(|f| f.is_active(daa));
        let s3 = self.s3.is_some_and(|f| f.is_active(daa));
        let s2 = self.s2.is_some_and(|f| f.is_active(daa));
        PalwVerificationModeV1 {
            name: if s2 {
                "s1+s3+s2"
            } else if s3 {
                "s1+s3"
            } else if v2 {
                "s1"
            } else {
                "v1"
            },
            s1_active: v2,
            s1_scheduled_daa: self.v2.map(|f| f.daa_score()).unwrap_or(0),
            s3_active: s3,
            s3_scheduled_daa: self.s3.map(|f| f.daa_score()).unwrap_or(0),
            s2_active: s2,
            s2_scheduled_daa: self.s2.map(|f| f.daa_score()).unwrap_or(0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwVerificationModeV1 {
    pub name: &'static str,
    pub s1_active: bool,
    pub s1_scheduled_daa: u64,
    pub s3_active: bool,
    pub s3_scheduled_daa: u64,
    pub s2_active: bool,
    pub s2_scheduled_daa: u64,
}

impl PalwVerificationModeV1 {
    /// Geometry a claim's panel uses at this mode. V1: every seat replays whole. S1: one full seat
    /// and `K = seats − 1` disjoint partials.
    pub fn panel_geometry(self) -> PalwPanelGeometryV1 {
        if self.s1_active {
            PalwPanelGeometryV1 {
                panel_size: PALW_V2_PANEL_SEATS,
                receipt_quorum: PALW_V2_PANEL_QUORUM,
                full_seats_per_panel: 1,
                partial_seats_per_panel: PALW_V2_PANEL_SEATS.saturating_sub(1),
                segment_count: palw_segment_count_v2(PALW_V2_PANEL_SEATS),
            }
        } else {
            PalwPanelGeometryV1 {
                panel_size: PALW_V2_PANEL_SEATS,
                receipt_quorum: PALW_V2_PANEL_QUORUM,
                full_seats_per_panel: PALW_V2_PANEL_SEATS,
                partial_seats_per_panel: 0,
                segment_count: 0,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelGeometryV1 {
    pub panel_size: u16,
    pub receipt_quorum: u16,
    pub full_seats_per_panel: u16,
    pub partial_seats_per_panel: u16,
    pub segment_count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClassPanelViewV1 {
    pub class_id: Hash64,
    pub model_name: String,
    pub registry_state: String,
    pub bonded_seats: u32,
    pub ready_seats: u32,
    pub required_ready_seats: u32,
    pub selected_panel_seats: u32,
    pub valid_receipt_seats: u32,
    pub geometry: PalwPanelGeometryV1,
    pub inflight_claims: u32,
    pub active_assignments: u32,
    pub admission_permille: u64,
    pub verification: PalwVerificationModeV1,
    pub missing: Vec<(PalwPanelHoldReasonV1, u32)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPanelSeatViewV1 {
    pub seat_id: PalwBondKeyV2,
    pub class_id: Hash64,
    pub ready: bool,
    pub eligible: bool,
    pub readiness_version: u8,
    pub readiness_proved_daa: u64,
    pub readiness_expires_daa: u64,
    pub collateral_available: u128,
    pub collateral_locked: u128,
    pub assigned: u32,
    pub hold: Option<PalwPanelHoldReasonV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPanelAssignmentSeatV1 {
    pub seat_id: PalwBondKeyV2,
    pub seat_index: u16,
    pub full_seat: bool,
    pub segment_index: Option<u16>,
    pub mask: u32,
    pub receipt_status: &'static str,
    pub credited_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPanelAssignmentViewV1 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub licensed_state: &'static str,
    pub deadline_daa: u64,
    pub coverage_mask: u32,
    pub full_seat: PalwBondKeyV2,
    pub valid_receipt_seats: u32,
    pub selected_panel_seats: u32,
    pub seats: Vec<PalwPanelAssignmentSeatV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwPanelNetworkViewV1 {
    pub available: bool,
    pub tip_daa: u64,
    pub classes: Vec<PalwClassPanelViewV1>,
    pub seats: Vec<PalwPanelSeatViewV1>,
    pub assignments: Vec<PalwPanelAssignmentViewV1>,
}

pub fn palw_lifecycle_state_name_v1(state: &PalwModelLifecycleV1) -> &'static str {
    match state {
        PalwModelLifecycleV1::Candidate => "Candidate",
        PalwModelLifecycleV1::Registered => "Registered",
        PalwModelLifecycleV1::Prefetching => "Prefetching",
        PalwModelLifecycleV1::Probation { .. } => "Probation",
        PalwModelLifecycleV1::ActiveLimited { .. } => "ActiveLimited",
        PalwModelLifecycleV1::Active => "Active",
        PalwModelLifecycleV1::Held => "Held",
    }
}

pub fn palw_claim_licensed_state_v1(phase: &PalwClaimPhaseV2) -> &'static str {
    match phase {
        PalwClaimPhaseV2::Provisional => "provisional",
        PalwClaimPhaseV2::PanelBound { .. } => "panelBound",
        PalwClaimPhaseV2::ReceiptLicensed { .. } => "receiptLicensed",
        PalwClaimPhaseV2::Final { .. } => "final",
        PalwClaimPhaseV2::Voided { .. } => "voided",
        PalwClaimPhaseV2::DefaultDisputed { .. } => "defaultDisputed",
    }
}

pub fn palw_bond_seat_id_v1(bond: PalwBondKeyV2) -> String {
    format!("{}:{}", bond.0.transaction_id, bond.0.index)
}

/// Known class aliases an operator would type (`QWEN36`, `BASE0`). Empty when the id is not one
/// of the shipped named classes.
pub fn palw_class_model_name_v1(class_id: Hash64) -> String {
    if class_id == crate::palw_qwen36_profile::qwen36_class_id_v3() {
        "QWEN36".to_string()
    } else {
        String::new()
    }
}

/// Resolve `QWEN36` / 128-hex into a class id. Caller still has to check the chain holds it.
pub fn palw_parse_class_alias_v1(raw: &str) -> Result<Hash64, String> {
    let t = raw.trim();
    if t.eq_ignore_ascii_case("QWEN36") || t.eq_ignore_ascii_case("qwen36") {
        return Ok(crate::palw_qwen36_profile::qwen36_class_id_v3());
    }
    t.parse::<Hash64>().map_err(|_| format!("'{raw}' is neither QWEN36 nor a 128-hex class id"))
}

pub fn palw_panel_missing_reason_counts_v1(seats: &[PalwPanelSeatViewV1]) -> Vec<(PalwPanelHoldReasonV1, u32)> {
    let mut counts = std::collections::BTreeMap::new();
    for seat in seats.iter().filter(|s| !s.ready) {
        if let Some(reason) = seat.hold {
            *counts.entry(reason).or_insert(0u32) += 1;
        }
    }
    counts.into_iter().collect()
}

const PALW_PANEL_ASSIGNMENTS_CAP_V1: usize = 512;

pub fn palw_panel_network_view_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    tip_daa: u64,
    registry: &PalwModelRegistryReadV1,
    schedule: PalwVerificationScheduleV1,
) -> PalwPanelNetworkViewV1 {
    let verification = schedule.at(tip_daa);
    let geometry = verification.panel_geometry();
    let floor = params.min_collateral_sompi();
    let max_age_daa = registry
        .globals
        .map(|g| (g.readiness_probe_max_age_spans as u64).saturating_mul(registry.span_daa.max(1)))
        .unwrap_or(0);

    let mut assigned_by_class: std::collections::BTreeMap<(PalwBondKeyV2, Hash64), u32> = std::collections::BTreeMap::new();
    let mut selected_by_class: std::collections::BTreeMap<Hash64, std::collections::BTreeSet<PalwBondKeyV2>> =
        std::collections::BTreeMap::new();
    let mut valid_by_class: std::collections::BTreeMap<Hash64, u32> = std::collections::BTreeMap::new();
    let mut active_by_class: std::collections::BTreeMap<Hash64, u32> = std::collections::BTreeMap::new();
    let mut assignments = Vec::new();

    for (claim_id, claim) in state.claims_iter() {
        let terminal = matches!(claim.phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. });
        let Some(panel) = state.panel(claim_id) else { continue };
        if !terminal {
            *active_by_class.entry(claim.class_id).or_insert(0) += 1;
        }
        let assignment = palw_segment_assignment_v2(panel.anchor, *claim_id, panel.seats.len() as u16);
        let duty = state.panel_duty_row_of(claim_id);
        let mut coverage = 0u32;
        let mut valid = 0u32;
        let mut seats_out = Vec::with_capacity(panel.seats.len());
        for (idx, seat) in panel.seats.iter().enumerate() {
            if !terminal {
                *assigned_by_class.entry((seat.bond, claim.class_id)).or_insert(0) += 1;
                selected_by_class.entry(claim.class_id).or_default().insert(seat.bond);
            }
            let credited = duty.and_then(|d| d.seats.get(&seat.bond).copied()).unwrap_or(0);
            let mask = assignment.mask_of(idx as u16);
            if credited > 0 {
                valid += 1;
                coverage |= mask.0;
            }
            let full = assignment.full_seat == idx as u16;
            seats_out.push(PalwPanelAssignmentSeatV1 {
                seat_id: seat.bond,
                seat_index: idx as u16,
                full_seat: full,
                segment_index: if full { None } else { (0..32u16).find(|&i| mask.covers(i)) },
                mask: mask.0,
                receipt_status: if credited > 0 {
                    "valid"
                } else if terminal {
                    "none"
                } else {
                    "pending"
                },
                credited_daa: credited,
            });
        }
        if !terminal {
            *valid_by_class.entry(claim.class_id).or_insert(0) += valid;
        }
        if assignments.len() < PALW_PANEL_ASSIGNMENTS_CAP_V1 {
            let deadline = match claim.phase {
                PalwClaimPhaseV2::PanelBound { bound_daa } => {
                    bound_daa.saturating_add(params.receipt_window_for_claim_v1(state, &claim.class_id, bound_daa))
                }
                _ => panel.bound_daa.saturating_add(params.receipt_window_for_claim_v1(state, &claim.class_id, panel.bound_daa)),
            };
            let full_bond = panel.seats.get(assignment.full_seat as usize).map(|s| s.bond).unwrap_or(PalwBondKeyV2(
                crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([0u8; 64]), index: 0 },
            ));
            assignments.push(PalwPanelAssignmentViewV1 {
                claim_id: *claim_id,
                class_id: claim.class_id,
                licensed_state: palw_claim_licensed_state_v1(&claim.phase),
                deadline_daa: deadline,
                coverage_mask: coverage,
                full_seat: full_bond,
                valid_receipt_seats: valid,
                selected_panel_seats: panel.seats.len() as u32,
                seats: seats_out,
            });
        }
    }

    let mut seats = Vec::new();
    for class in &registry.classes {
        if class.is_base_class {
            continue;
        }
        for (bond_key, bond) in state.bonds_iter() {
            if !palw_bond_may_judge_class_v2(bond, &class.class_id) {
                continue;
            }
            let row = registry.readiness.iter().find(|r| r.bond == *bond_key && r.class_id == class.class_id);
            let (ready, hold, version, proved, expires) = match row {
                Some(r) if r.fresh && r.not_ready_reason.is_empty() => {
                    (true, None, r.row.proof_version, r.row.proved_daa, r.row.proved_daa.saturating_add(max_age_daa))
                }
                Some(r) => {
                    let hold = PalwPanelHoldReasonV1::from_registry_not_ready(&r.not_ready_reason)
                        .unwrap_or(PalwPanelHoldReasonV1::ReadinessProofExpired);
                    (false, Some(hold), r.row.proof_version, r.row.proved_daa, r.row.proved_daa.saturating_add(max_age_daa))
                }
                None => (false, Some(PalwPanelHoldReasonV1::ReadinessProofMissing), 0, 0, 0),
            };
            let held = state.reserved_exposure(bond_key).saturating_add(state.registration_exposure(bond_key));
            let free = (bond.collateral as u128).saturating_sub(bond.slashed as u128).saturating_sub(held);
            let eligible = ready
                && matches!(bond.status, PalwBondStatusV2::Active)
                && palw_bond_may_take_work_v2(bond, floor);
            seats.push(PalwPanelSeatViewV1 {
                seat_id: *bond_key,
                class_id: class.class_id,
                ready,
                eligible,
                readiness_version: version,
                readiness_proved_daa: proved,
                readiness_expires_daa: expires,
                collateral_available: free,
                collateral_locked: held,
                assigned: assigned_by_class.get(&(*bond_key, class.class_id)).copied().unwrap_or(0),
                hold: if ready { None } else { hold },
            });
        }
    }

    let mut classes = Vec::new();
    for class in &registry.classes {
        if class.is_base_class {
            continue;
        }
        let class_seats: Vec<&PalwPanelSeatViewV1> = seats.iter().filter(|s| s.class_id == class.class_id).collect();
        let bonded = class_seats.len() as u32;
        let missing = palw_panel_missing_reason_counts_v1(
            &class_seats.iter().copied().cloned().collect::<Vec<_>>(),
        );
        let row = class.row.as_ref();
        classes.push(PalwClassPanelViewV1 {
            class_id: class.class_id,
            model_name: palw_class_model_name_v1(class.class_id),
            registry_state: row.map(|r| palw_lifecycle_state_name_v1(&r.state).to_string()).unwrap_or_else(|| "Legacy".to_string()),
            bonded_seats: bonded,
            ready_seats: class.ready_seats_now,
            required_ready_seats: row.map(|r| r.profile.required_ready_seats).unwrap_or(0),
            selected_panel_seats: selected_by_class.get(&class.class_id).map(|s| s.len() as u32).unwrap_or(0),
            valid_receipt_seats: valid_by_class.get(&class.class_id).copied().unwrap_or(0),
            geometry,
            inflight_claims: class.inflight_now,
            active_assignments: active_by_class.get(&class.class_id).copied().unwrap_or(0),
            admission_permille: row.map(|r| r.admission_milli.min(u64::from(u32::MAX))).unwrap_or(0),
            verification,
            missing,
        });
    }

    PalwPanelNetworkViewV1 { available: true, tip_daa, classes, seats, assignments }
}

/// Compact previous-tick facts so a virtual change can emit only the four notify kinds that moved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwPanelNotifySnapshotV1 {
    pub classes: std::collections::BTreeMap<Hash64, (String, u32, u32, u32)>,
    pub assignment_ids: std::collections::BTreeSet<Hash64>,
    pub receipts: std::collections::BTreeMap<Hash64, (Hash64, u32, u32, u32)>,
    pub eligibility: std::collections::BTreeMap<(PalwBondKeyV2, Hash64), (bool, bool, Option<PalwPanelHoldReasonV1>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClassReadinessNotifyV1 {
    pub class_id: Hash64,
    pub model_name: String,
    pub registry_state: String,
    pub previous_registry_state: String,
    pub ready_seats: u32,
    pub previous_ready_seats: u32,
    pub required_ready_seats: u32,
    pub bonded_seats: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPanelReceiptNotifyV1 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub coverage_mask: u32,
    pub previous_coverage_mask: u32,
    pub valid_receipt_seats: u32,
    pub previous_valid_receipt_seats: u32,
    pub selected_panel_seats: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPanelEligibilityNotifyV1 {
    pub seat_id: PalwBondKeyV2,
    pub class_id: Hash64,
    pub eligible: bool,
    pub ready: bool,
    pub hold: Option<PalwPanelHoldReasonV1>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwPanelNotifyDiffV1 {
    pub readiness: Vec<PalwClassReadinessNotifyV1>,
    pub assignments: Vec<PalwPanelAssignmentViewV1>,
    pub receipts: Vec<PalwPanelReceiptNotifyV1>,
    pub eligibility: Vec<PalwPanelEligibilityNotifyV1>,
}

pub fn palw_panel_notify_snapshot_v1(view: &PalwPanelNetworkViewV1) -> PalwPanelNotifySnapshotV1 {
    PalwPanelNotifySnapshotV1 {
        classes: view
            .classes
            .iter()
            .map(|c| (c.class_id, (c.registry_state.clone(), c.ready_seats, c.required_ready_seats, c.bonded_seats)))
            .collect(),
        assignment_ids: view.assignments.iter().map(|a| a.claim_id).collect(),
        receipts: view
            .assignments
            .iter()
            .map(|a| (a.claim_id, (a.class_id, a.coverage_mask, a.valid_receipt_seats, a.selected_panel_seats)))
            .collect(),
        eligibility: view
            .seats
            .iter()
            .map(|s| ((s.seat_id, s.class_id), (s.eligible, s.ready, s.hold)))
            .collect(),
    }
}

/// First observation is a baseline (no flood). Later ticks emit only what moved.
pub fn palw_panel_notify_diff_v1(
    prev: Option<&PalwPanelNotifySnapshotV1>,
    view: &PalwPanelNetworkViewV1,
) -> (PalwPanelNotifySnapshotV1, PalwPanelNotifyDiffV1) {
    let next = palw_panel_notify_snapshot_v1(view);
    let Some(prev) = prev else {
        return (next, PalwPanelNotifyDiffV1::default());
    };
    let mut diff = PalwPanelNotifyDiffV1::default();
    for class in &view.classes {
        let now = (class.registry_state.clone(), class.ready_seats, class.required_ready_seats, class.bonded_seats);
        match prev.classes.get(&class.class_id) {
            Some(old) if *old == now => {}
            Some(old) => diff.readiness.push(PalwClassReadinessNotifyV1 {
                class_id: class.class_id,
                model_name: class.model_name.clone(),
                registry_state: class.registry_state.clone(),
                previous_registry_state: old.0.clone(),
                ready_seats: class.ready_seats,
                previous_ready_seats: old.1,
                required_ready_seats: class.required_ready_seats,
                bonded_seats: class.bonded_seats,
            }),
            None => diff.readiness.push(PalwClassReadinessNotifyV1 {
                class_id: class.class_id,
                model_name: class.model_name.clone(),
                registry_state: class.registry_state.clone(),
                previous_registry_state: String::new(),
                ready_seats: class.ready_seats,
                previous_ready_seats: 0,
                required_ready_seats: class.required_ready_seats,
                bonded_seats: class.bonded_seats,
            }),
        }
    }
    for assignment in &view.assignments {
        if !prev.assignment_ids.contains(&assignment.claim_id) {
            diff.assignments.push(assignment.clone());
        }
        if let Some(&(class_id, coverage, valid, selected)) = prev.receipts.get(&assignment.claim_id) {
            if coverage != assignment.coverage_mask || valid != assignment.valid_receipt_seats {
                diff.receipts.push(PalwPanelReceiptNotifyV1 {
                    claim_id: assignment.claim_id,
                    class_id,
                    coverage_mask: assignment.coverage_mask,
                    previous_coverage_mask: coverage,
                    valid_receipt_seats: assignment.valid_receipt_seats,
                    previous_valid_receipt_seats: valid,
                    selected_panel_seats: assignment.selected_panel_seats.max(selected),
                });
            }
        }
    }
    for seat in &view.seats {
        let now = (seat.eligible, seat.ready, seat.hold);
        match prev.eligibility.get(&(seat.seat_id, seat.class_id)) {
            Some(old) if *old == now => {}
            _ => diff.eligibility.push(PalwPanelEligibilityNotifyV1 {
                seat_id: seat.seat_id,
                class_id: seat.class_id,
                eligible: seat.eligible,
                ready: seat.ready,
                hold: seat.hold,
            }),
        }
    }
    (next, diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_strings_map_onto_reason_codes() {
        assert_eq!(PalwPanelHoldReasonV1::from_registry_not_ready("bond inactive"), Some(PalwPanelHoldReasonV1::BondInactive));
        assert_eq!(PalwPanelHoldReasonV1::from_registry_not_ready("stale"), Some(PalwPanelHoldReasonV1::ReadinessProofExpired));
        assert_eq!(
            PalwPanelHoldReasonV1::from_registry_not_ready("collateral short"),
            Some(PalwPanelHoldReasonV1::CollateralInsufficient)
        );
        assert_eq!(PalwPanelHoldReasonV1::from_registry_not_ready(""), None);
        assert_eq!(PalwPanelHoldReasonV1::NoArtifact.code(), "NO_ARTIFACT");
        assert!(!PalwPanelHoldReasonV1::NoArtifact.message().is_empty());
    }

    #[test]
    fn verification_mode_names_the_armed_bundle() {
        let v2 = ForkActivation::new(7200);
        let s3 = ForkActivation::new(8600);
        let s2 = ForkActivation::new(8700);
        let s = PalwVerificationScheduleV1 { v2: Some(v2), s3: Some(s3), s2: Some(s2) };
        assert_eq!(s.at(7100).name, "v1");
        assert_eq!(s.at(7200).name, "s1");
        assert_eq!(s.at(8600).name, "s1+s3");
        assert_eq!(s.at(8700).name, "s1+s3+s2");
        let g = s.at(7200).panel_geometry();
        assert_eq!(g.panel_size, 5);
        assert_eq!(g.receipt_quorum, 3);
        assert_eq!(g.full_seats_per_panel, 1);
        assert_eq!(g.partial_seats_per_panel, 4);
        assert_eq!(g.segment_count, 4);
        let v1 = s.at(0).panel_geometry();
        assert_eq!(v1.full_seats_per_panel, 5);
        assert_eq!(v1.partial_seats_per_panel, 0);
    }

    #[test]
    fn missing_counts_group_by_reason_and_do_not_count_ready_seats() {
        let class = Hash64::from_bytes([1u8; 64]);
        let seat = |ready, hold| PalwPanelSeatViewV1 {
            seat_id: PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_bytes([2u8; 64]),
                index: 0,
            }),
            class_id: class,
            ready,
            eligible: ready,
            readiness_version: 2,
            readiness_proved_daa: 0,
            readiness_expires_daa: 0,
            collateral_available: 0,
            collateral_locked: 0,
            assigned: 0,
            hold,
        };
        let seats = vec![
            seat(true, None),
            seat(false, Some(PalwPanelHoldReasonV1::NoArtifact)),
            seat(false, Some(PalwPanelHoldReasonV1::NoArtifact)),
            seat(false, Some(PalwPanelHoldReasonV1::CollateralInsufficient)),
            seat(false, Some(PalwPanelHoldReasonV1::ReadinessProofExpired)),
        ];
        let counts = palw_panel_missing_reason_counts_v1(&seats);
        assert_eq!(counts, vec![
            (PalwPanelHoldReasonV1::NoArtifact, 2),
            (PalwPanelHoldReasonV1::CollateralInsufficient, 1),
            (PalwPanelHoldReasonV1::ReadinessProofExpired, 1),
        ]);
    }

    #[test]
    fn qwen36_alias_is_the_shipped_class_id() {
        let id = palw_parse_class_alias_v1("QWEN36").expect("alias");
        assert_eq!(id, crate::palw_qwen36_profile::qwen36_class_id_v3());
        assert_eq!(palw_class_model_name_v1(id), "QWEN36");
        assert!(palw_parse_class_alias_v1("not-a-class").is_err());
    }

    #[test]
    fn seat_counts_are_four_different_fields() {
        // A single number cannot stand for operators, ready seats, a selected panel, and receipts.
        let c = PalwClassPanelViewV1 {
            class_id: Hash64::from_bytes([9u8; 64]),
            model_name: "QWEN36".into(),
            registry_state: "Probation".into(),
            bonded_seats: 10,
            ready_seats: 8,
            required_ready_seats: 7,
            selected_panel_seats: 5,
            valid_receipt_seats: 3,
            geometry: PalwVerificationScheduleV1 { v2: Some(ForkActivation::always()), s3: None, s2: None }
                .at(u64::MAX)
                .panel_geometry(),
            inflight_claims: 41,
            active_assignments: 41,
            admission_permille: 50,
            verification: PalwVerificationScheduleV1 { v2: Some(ForkActivation::always()), s3: None, s2: None }.at(u64::MAX),
            missing: vec![],
        };
        let ns = [c.bonded_seats, c.ready_seats, c.selected_panel_seats, c.valid_receipt_seats];
        for i in 0..ns.len() {
            for j in (i + 1)..ns.len() {
                assert_ne!(ns[i], ns[j], "seat counts must not collapse into one number");
            }
        }
        assert!(c.ready_seats >= c.required_ready_seats);
        assert_eq!(c.geometry.panel_size, c.selected_panel_seats as u16);
        assert_eq!(c.geometry.receipt_quorum, c.valid_receipt_seats as u16);
    }

    #[test]
    fn notify_diff_is_silent_on_the_first_tick_then_names_what_moved() {
        let class = Hash64::from_bytes([3u8; 64]);
        let seat_id = PalwBondKeyV2(crate::tx::TransactionOutpoint {
            transaction_id: crate::tx::TransactionId::from_bytes([4u8; 64]),
            index: 2,
        });
        let class_row = |ready, state: &str| PalwClassPanelViewV1 {
            class_id: class,
            model_name: "QWEN36".into(),
            registry_state: state.into(),
            bonded_seats: 7,
            ready_seats: ready,
            required_ready_seats: 7,
            selected_panel_seats: 5,
            valid_receipt_seats: 0,
            geometry: PalwVerificationScheduleV1 { v2: Some(ForkActivation::always()), s3: None, s2: None }
                .at(u64::MAX)
                .panel_geometry(),
            inflight_claims: 41,
            active_assignments: 41,
            admission_permille: 50,
            verification: PalwVerificationScheduleV1 { v2: Some(ForkActivation::always()), s3: None, s2: None }.at(u64::MAX),
            missing: vec![],
        };
        let seat_row = |ready, hold| PalwPanelSeatViewV1 {
            seat_id,
            class_id: class,
            ready,
            eligible: ready,
            readiness_version: 2,
            readiness_proved_daa: 7810,
            readiness_expires_daa: 7900,
            collateral_available: 1,
            collateral_locked: 0,
            assigned: 0,
            hold,
        };
        let first = PalwPanelNetworkViewV1 {
            available: true,
            tip_daa: 7800,
            classes: vec![class_row(6, "Prefetching")],
            seats: vec![seat_row(false, Some(PalwPanelHoldReasonV1::ReadinessProofExpired))],
            assignments: vec![],
        };
        let (snap, diff) = palw_panel_notify_diff_v1(None, &first);
        assert!(diff.readiness.is_empty());
        assert!(diff.eligibility.is_empty());
        let second = PalwPanelNetworkViewV1 {
            available: true,
            tip_daa: 7812,
            classes: vec![class_row(7, "Probation")],
            seats: vec![seat_row(true, None)],
            assignments: vec![],
        };
        let (_, moved) = palw_panel_notify_diff_v1(Some(&snap), &second);
        assert_eq!(moved.readiness.len(), 1);
        assert_eq!(moved.readiness[0].previous_ready_seats, 6);
        assert_eq!(moved.readiness[0].ready_seats, 7);
        assert_eq!(moved.readiness[0].previous_registry_state, "Prefetching");
        assert_eq!(moved.readiness[0].registry_state, "Probation");
        assert_eq!(moved.eligibility.len(), 1);
        assert!(moved.eligibility[0].eligible);
        assert!(moved.eligibility[0].hold.is_none());
    }
}
