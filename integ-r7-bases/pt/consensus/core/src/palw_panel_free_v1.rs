//! **RFC-0015: the Panel=0 fence — `palw_panel_free_v1`** (dormant, no height).
//!
//! RFC-0015's `OptimisticPublicVerification` replaces a fixed Panel's coverage with permissionless verifiers, objective fraud proofs, a
//! fixed public challenge window and a producer-centred reservation (`misaka-palw-kernel`: `OpvPolicyV1`, route tags 13 / 14,
//! `ClaimStateV1::Challengeable`). The consumer derives the kernel ledger's OPV activation height from this fence. RFC-0015 §13.3
//! forbids arming it until `ACTIVATION_ALLOWED` — G14 PASS on a real node for every rewarded profile, the new lifecycle / court / Final
//! implementation, the panel-free collateral review, supported monitoring and inclusion assumptions, an independent adversarial node
//! E2E with recovery and migration, and a separately coordinated schedule — and none of it exists.
//!
//! **Dormant**: `None` on every preset and in no testnet-12 flag-day list; hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())`, its activation alone visited by `for_each_fence`. Consensus reads it
//! (`palw_model_conformance_fold_v2`) and the node E2E suite runs it with test-armed params; arming it on a real network is
//! refused by [`Params::validate_palw_panel_free_v1`] (RFC-0015 amendment 2026-10-12: Panel=0 is not activated).
//!
//! **The activation gate is code** ([`crate::palw_panel_free_gate_v1`], 2026-10-15): the refusal lifts only for a fence value carrying
//! an authenticated `PalwPanelFreeGateRecordV1` — every §13.3 conjunct PASS with evidence and independent review, bound to the network,
//! height, terms and budget policies, the coupled fences armed at or below, and its digest pinned in the binary. No digest is pinned.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::constants::SOMPI_PER_KASPA;
use crate::palw_mode_v2::PalwModeV2Error;
use misaka_palw_kernel::opv::{CarrierCapsV1, OpvBudgetsV1, OpvEconomicsV1, OpvPolicyV1, OpvWindowV1};

/// **The interim effective-bits floor of OPV eligibility**: the user's ruled 128-bit effective false-accept target
/// (`misaka_palw_challenge::APPROVAL_MIN_TARGET_BITS_V1`).
pub const PALW_OPV_MIN_EFFECTIVE_BITS_V1: u16 = misaka_palw_challenge::APPROVAL_MIN_TARGET_BITS_V1;

/// **`Params::palw_panel_free_v1`'s value (RFC-0015): the fence and everything the network's OPV policy is** — the activation, the
/// policy's terms (a genesis constant of the kernel route, [`OpvPolicyV1`]) and the two network-side knobs of the DERIVED eligibility
/// (`docs/design/palw/verification/opv-beacon-bootstrap.md` §5): a deny-list and the effective-bits floor.
///
/// **OPV eligibility is never read from this value.** A class may use `OptimisticPublicVerification` only while
/// `opv_eligibility_v1` derives it from chain state (Active kernel, conformance passed, G14-complete, live public DA, bounded
/// resources, a verified challenge policy). The fence can only take eligibility AWAY: `denied_classes` (an incident brake the
/// network coordinates, auditable, no code change) and `min_effective_bits` (the effective false-accept floor a class's conformance
/// policy must reach; a complete check meets any floor). The manual `admitted_classes` list it replaces is gone.
///
/// Some-only everywhere: `None` on every preset, hashed into the params fingerprint and the schedule id only when `Some`, collapsed
/// whole from `Some(never())`, its ACTIVATION alone visited by `for_each_fence`. Arming it is refused by
/// [`Params::validate_palw_panel_free_v1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPanelFreeFenceV1 {
    pub activation: ForkActivation,
    /// (Mode-bound) class ids the network DENIES OPV whatever chain state derives, **strictly ascending** (canonical: hashed as given).
    /// A restriction only — never a source of eligibility. Initial value: empty.
    pub denied_classes: Vec<Hash64>,
    /// The effective false-accept floor (bits, `misaka-palw-challenge::effective_false_accept_bits_v1`) a class's conformance policy
    /// must reach for the class to be OPV-eligible. Interim: [`PALW_OPV_MIN_EFFECTIVE_BITS_V1`] (128, the user's ruled target), so no
    /// SAMPLED conformance under the interim 2-bit policy can make a class eligible — only a complete check can. A drill lowers it
    /// explicitly through the harness's `Config` seam.
    pub min_effective_bits: u16,
    /// **GAP-70** (the Lead, 2026-10-10): whether a SAMPLED conformance may satisfy E6. For the release only the complete check gates
    /// rewards, so this is `false` and [`Self::validate_value`] refuses `true` until a digest court exists. A drill sets it
    /// explicitly through the harness's `Config` seam (as it lowers `min_effective_bits`).
    pub sampled_conformance_gates_reward: bool,
    pub window: OpvWindowV1,
    pub budgets: OpvBudgetsV1,
    pub economics: OpvEconomicsV1,
    pub carrier: CarrierCapsV1,
    /// **RFC-0015 §13.3 as code** ([`crate::palw_panel_free_gate_v1`]): the authenticated activation gate record. `None` on every
    /// value this binary builds; arming the fence anywhere but an isolated calibration genesis is refused unless this record lists
    /// every §13.3 conjunct PASS, binds this network / height / terms / budget policies, finds the coupled fences armed at or below
    /// it, and its digest is pinned in the binary (`PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1`, empty). Hashed Some-only after the
    /// terms, so no existing identity moves.
    pub activation_gate: Option<crate::palw_panel_free_gate_v1::PalwPanelFreeGateRecordV1>,
}

impl PalwPanelFreeFenceV1 {
    /// A bare height with the INTERIM terms and nothing denied (what a fence probe builds from a height alone).
    pub fn at(activation: ForkActivation) -> Self {
        Self::interim_v1(activation, Vec::new())
    }

    /// **The INTERIM terms** (RFC-0015): relations the kernel validates, values chosen for the (never-armed) fence and written once
    /// here. The window is 50 DAA (40 base + 10 horizon); a fresh verifier's budgets fit it and the court deadline and proof grace;
    /// the reservation covers the claim's maximum gain (reward + work credit + a stated external bound) plus the default penalty and
    /// the gain over the assumed detection probability. A real activation would revisit every number and measure the budgets on real
    /// hardware (an external gate). `denied_classes` is sorted and de-duplicated; the floor is [`PALW_OPV_MIN_EFFECTIVE_BITS_V1`].
    pub fn interim_v1(activation: ForkActivation, mut denied_classes: Vec<Hash64>) -> Self {
        use misaka_palw_kernel::route::{MAX_COMMIT_CLAIM_BYTES_V1, MAX_FILE_PROOF_BYTES_V1, MAX_RESPOND_BYTES_V1};
        denied_classes.sort();
        denied_classes.dedup();
        let carrier = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 as u64;
        Self {
            activation,
            denied_classes,
            min_effective_bits: PALW_OPV_MIN_EFFECTIVE_BITS_V1,
            sampled_conformance_gates_reward: false,
            window: OpvWindowV1 { base_challenge_window_daa: 40, verification_horizon_daa: 10 },
            budgets: OpvBudgetsV1 {
                cold_material_daa: 10,
                check_daa: 10,
                localize_daa: 2,
                disclose_daa: 8,
                court_daa: 3,
                carrier_daa: 2,
                reorg_slack_daa: 2,
            },
            economics: OpvEconomicsV1 {
                reservation_per_claim: 1_000 * SOMPI_PER_KASPA,
                work_credit_per_claim: 5 * SOMPI_PER_KASPA,
                external_gain_bound: 10 * SOMPI_PER_KASPA,
                assumed_detection_permille: 500,
                max_live_claims_per_producer: 3,
                max_live_claims_total: 32,
                // C4 F-C4R3-05 (round 2): a new producer is shut out only once these too are bought (hard ceiling 48 ≤ the window's
                // reserved proof runs: 64 × 500‰ × 50, and 4 × 500‰ × 50 = 100 on the test node).
                fresh_producer_slots: 16,
                default_burn_permille: 100,
                // C4 F-C4R3-05: a refilled lane costs its occupiers (non-refundable, burned).
                admission_fee: SOMPI_PER_KASPA,
            },
            carrier: CarrierCapsV1 {
                filing_cap: carrier.min(MAX_FILE_PROOF_BYTES_V1 as u64),
                response_cap: carrier.min(MAX_RESPOND_BYTES_V1 as u64),
                commit_cap: carrier.min(MAX_COMMIT_CLAIM_BYTES_V1 as u64),
            },
            activation_gate: None,
        }
    }

    /// The kernel ledger's OPV policy: the terms, activating at the fence's height (`None`: it never activates).
    pub fn opv_policy(&self) -> OpvPolicyV1 {
        OpvPolicyV1 {
            activation_daa: (self.activation != ForkActivation::never()).then(|| self.activation.daa_score()),
            window: self.window,
            budgets: self.budgets,
            economics: self.economics,
            carrier: self.carrier,
        }
    }

    /// The value's own refusals: the deny-list canonical, and the terms satisfying every relation the kernel validates against the
    /// route's interim ledger policy (the relations do not read the network's digests).
    pub fn validate_value(&self) -> Result<(), String> {
        if self.denied_classes.windows(2).any(|w| w[0] >= w[1]) {
            return Err("the denied class list must be strictly ascending (canonical, unique)".to_string());
        }
        if self.sampled_conformance_gates_reward {
            return Err(
                "a sampled conformance gates no reward until a digest court exists (GAP-70): only the complete check does".to_string()
            );
        }
        let ledger = crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
        self.opv_policy().validate(&ledger)
    }

    /// The value as the identity hashers write it: the denied ids, the floor, GAP-70's switch and the terms (Borsh), after the
    /// activation. (A domain tag separates this layout from the earlier ones, so no value of any can collide with another.)
    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(b"palw_panel_free_v1/derived-eligibility/v3");
        h.write((self.denied_classes.len() as u64).to_le_bytes());
        for class in &self.denied_classes {
            h.write(class.as_byte_slice());
        }
        h.write(self.min_effective_bits.to_le_bytes());
        h.write([self.sampled_conformance_gates_reward as u8]);
        h.write(borsh::to_vec(&(self.window, self.budgets, self.economics, self.carrier)).expect("the terms serialize"));
        // RFC-0015 §13.3: the activation gate record, Some-only (no value any shipped or existing calibration network spells moves).
        if let Some(gate) = &self.activation_gate {
            h.write(b"palw_panel_free_v1/activation-gate-record/v1");
            h.write(gate.digest().as_byte_slice());
        }
    }
}

impl Params {
    /// Whether the Panel-free mode is in force at `daa_score` — never, in this binary.
    pub fn palw_panel_free_active_at(&self, daa_score: u64) -> bool {
        self.palw_panel_free_v1.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa_score))
    }

    /// **The fence's refusal** (RFC-0015 §13.3 `ACTIVATION_ALLOWED`, an AND contract): an armed height is refused unless the fence's
    /// value carries an activation gate record that [`crate::palw_panel_free_gate_v1::PalwPanelFreeGateRecordV1::verify_v1`] accepts
    /// against this binary's pins — which are empty, so every armed height is refused here (2026-10-12 amendment, N5: Panel=0 is not
    /// activated). An isolated calibration genesis keeps its own validation (it never joins a real network).
    pub fn validate_palw_panel_free_v1(&self) -> Result<(), PalwModeV2Error> {
        if self.has_isolated_native_calibration_v34() {
            return self.validate_isolated_native_calibration_v34().map_err(|_| {
                PalwModeV2Error::Invalid("native calibration34 owning isolated genesis or complete original policies are invalid")
            });
        }
        // A fence that is spelled `Some(never())` is collapsed whole (its value is never read); any other value is checked first, so a
        // malformed policy is named as such and not hidden behind the arming refusal.
        if let Some(f) = self.palw_panel_free_v1.as_ref().filter(|f| f.activation != ForkActivation::never())
            && f.validate_value().is_err()
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_panel_free_v1's value is invalid: the denied class list must be strictly ascending, a sampled conformance may not \
                 gate rewards (GAP-70), and the OPV terms must satisfy every relation OpvPolicyV1::validate states \
                 (PalwPanelFreeFenceV1::validate_value names the one that fails)",
            ));
        }
        match &self.palw_panel_free_v1 {
            Some(f) if f.activation != ForkActivation::never() => match &f.activation_gate {
                None => Err(PalwModeV2Error::Invalid(
                    "palw_panel_free_v1 cannot be armed: RFC-0015 §13.3 ACTIVATION_ALLOWED is an AND contract and the fence carries no \
                     authenticated activation gate record (PalwPanelFreeGateRecordV1: every conjunct PASS with evidence and review, bound \
                     to this network, height, terms and budget policies, its digest pinned in the binary); Panel=0 is deferred \
                     (2026-10-12 amendment, N5)",
                )),
                Some(gate) => gate
                    .verify_v1(self, f, crate::palw_panel_free_gate_v1::PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1)
                    .map_err(|refusal| PalwModeV2Error::Invalid(refusal.reason())),
            },
            _ => Ok(()),
        }
    }
}
