//! **The cumulative court scope, enforced by the ledger** (ADR-0177 D2, DA16b §7.3 / §7.9, C4R4 F-C4R4-17; lane G14-S, 2026-10-11) —
//! DA16b's predicate (`crate::scope`) and K2S's value modes (`crate::seg_scope`) as GATES on what may be demanded and served (DA),
//! opened and recomputed (court), and rewarded (the per-claim gate), instead of reports.
//!
//! Consumer-injected like `palw_verifier_pay_v1`'s terms: [`CourtScopePolicyV1`] is set by the consumer at every load past
//! `palw_provider_court_v1` (consensus-core's kernel-route fold) and is NOT part of the ledger's state or root. Below it every rule here
//! is the historical one (nothing is checked or written). Its one table, **28** (`court_scope`, claim → [`CourtScopeRowV1`]), is
//! written only past it and extends the root only once it holds a row.
//!
//! ```text
//! the scope of a class (the reward gate)       K2-TIR-v4/v5: every model-dependent value withheld (option (c)) or hiding (option (a)),
//!   class_court_scope_v1                        each with a bounded court at the class's context (seg_scope_report_v2) — always;
//!                                               v1/v2 single programs past the OPV fence: no model relation (the clear budget is
//!                                               unbounded), else COURT_SCOPE_INCOMPLETE — such a route serves activations in the clear
//! demands (DA)                                  ≤ 16 distinct units per (claim, requester bond)            COURT_SCOPE_UNITS
//!   cumulative_scope_allows_v1                  v1/v2 positions past the OPV fence: the model's clear budget  COURT_SCOPE_CLEAR_BUDGET
//!                                               (K_min − 1, 0 with an elementwise relation) over every claim
//!                                               of the model — past it the route has no commitment-only form
//! filings and court runs                        ≤ 4 dismissed filings per (claim, accuser) before the court   COURT_SCOPE_FILINGS
//!   court_scope_filing_allows_v1                runs; a segmented filing's position counts in its filer's 16 units
//! what a court may open                         a whole-value court only for a withheld value, a hiding-leaf court only for a
//!   (crate::element::verify_seg_fault_v1)       hiding one, every operand in its source's form (a hiding value never by v3
//!                                               leaves, never in the clear by a demand)
//! what DA serves                                a withheld value as its node opening, a hiding one as its leaf hashes (crate::seg_da)
//! ```
//!
//! **G14 (the producer and every seat colluding).** Every bound is per requester (no other bond, seat or front-runner can spend an
//! honest prosecutor's allowance) or per class (a property of the registered program, decided before any claim): one prosecution
//! needs at most 2 positions and 10 root probes (12 units) and one convicting filing. A refusal is free and before any court runs.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::Digest;
use crate::ledger::{ClaimBodyV1, KernelLedgerV1};
use crate::scope::{COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1, DisclosureV1, court_clear_position_budget_v1, court_model_exposure_v1};

/// **Dismissed filings one accuser may have on one claim** before its further filings on it are refused before the court runs (INTERIM,
/// as DA-8's per-session counts). A convicting filing ends the claim, so an honest prosecutor needs one; each dismissal also costs its
/// scaled fee.
pub const COURT_SCOPE_MAX_DISMISSED_PER_REQUESTER_V1: u32 = 4;

/// **The court scope's terms**, consumer-injected past `palw_provider_court_v1` (never part of the state or its root).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CourtScopePolicyV1 {
    pub activation_daa: u64,
}

/// **Table 28's row of one claim**: the distinct `(stage, position)` units each requester named (demands, joins and segmented filings)
/// and each accuser's dismissed filings — both ascending by bond.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CourtScopeRowV1 {
    pub requested: Vec<(Digest, Vec<(u8, u32)>)>,
    pub dismissed: Vec<(Digest, u32)>,
}

impl CourtScopeRowV1 {
    fn units_of(&self, requester: &Digest) -> &[(u8, u32)] {
        self.requested.binary_search_by(|(r, _)| r.cmp(requester)).map(|i| self.requested[i].1.as_slice()).unwrap_or(&[])
    }

    fn dismissed_of(&self, accuser: &Digest) -> u32 {
        self.dismissed.binary_search_by(|(r, _)| r.cmp(accuser)).map(|i| self.dismissed[i].1).unwrap_or(0)
    }
}

/// The named refusal of a unit past a requester's allowance.
fn units_refusal(requester_units: usize) -> String {
    format!(
        "COURT_SCOPE_UNITS: the requester has named {requester_units} distinct units of this claim, the most one requester may \
         ({COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1}); one prosecution needs at most 12"
    )
}

impl KernelLedgerV1 {
    /// Whether the court scope is in force at the ledger's clock (`palw_provider_court_v1`, consumer-injected).
    pub fn court_scope_in_force(&self) -> bool {
        self.court_scope_policy.is_some_and(|p| self.daa >= p.activation_daa)
    }

    /// The v1/v2 rules (the clear budget and the class gate) bind the new reward channel: past the court scope AND past
    /// `palw_panel_free_v1` (the OPV policy's activation, the G14-for-rewards gate).
    fn court_scope_clear_budget_in_force(&self) -> bool {
        self.court_scope_in_force() && self.optimistic_allowed()
    }

    /// **The court-scope gate of a single-program class** (the per-claim reward gate's scope condition): `Err` names why a claim of it
    /// is refused. A K2-TIR-v4/v5 class: always — its report ([`crate::seg_scope::seg_scope_report_v2`] at its context) must be
    /// complete. A v1/v2 class: past the court scope and the OPV fence — it must have no model relation (its responses serve every
    /// model-dependent value in the clear and the route has no commitment-only form, so any model relation is rebuilt by demands:
    /// F-C4R4-17). A pipeline or typed class is not judged here (consensus classifies its units, DA16b §7.3).
    pub fn class_court_scope_v1(&self, class: &Digest) -> Result<(), String> {
        let Some(row) = self.classes.get(class) else { return Ok(()) };
        if crate::descriptor::is_segmented_v1(&row.descriptor) {
            return match &row.court_scope {
                Some(report) => report.refusal().map_or(Ok(()), Err),
                None => Err("COURT_SCOPE_INCOMPLETE: the class has no court-scope report".into()),
            };
        }
        if !self.court_scope_clear_budget_in_force() {
            return Ok(());
        }
        // The prospective commitment-only route withholds every model-dependent value;
        // its executable court and admission work remain checked separately.
        if self.commitment_scope_from_daa.is_some_and(|at| self.daa >= at) {
            return Ok(());
        }
        let exposure = court_model_exposure_v1(&row.program, row.program.params.len());
        match court_clear_position_budget_v1(&exposure) {
            u64::MAX => Ok(()),
            budget => Err(format!(
                "COURT_SCOPE_INCOMPLETE: a K2-TIR-v1/v2 class serves every model-dependent value in the clear and has no commitment-only \
                 form; its model relations (elementwise {}, smallest linear K {:?}) allow {budget} clear position(s) over the model's \
                 lifetime — only a K2-TIR-v4/v5 class (withheld or hiding values) is reward-bearing past the court scope",
                exposure.elementwise_relations, exposure.min_linear_inner_dim
            )),
        }
    }

    /// Positions of `model` (an artifact root) served in the clear or owed in the clear by an open demand, over every v1/v2 claim.
    fn served_clear_for_model_v1(&self, model: &Digest) -> u64 {
        let of_model = |claim: &Digest| {
            self.claims.get(claim).is_some_and(|r| {
                !self.commitment_scope_applies_v1(r)
                    && matches!(r.body, ClaimBodyV1::Program { .. })
                    && self.classes.get(&r.class_binding_id).is_some_and(|c| c.param_commitments.root() == *model)
            })
        };
        let served = self.served.keys().filter(|(c, _, _)| of_model(c)).count();
        let open = self.demands.keys().filter(|k| !self.served.contains_key(*k) && of_model(&k.0)).count();
        (served + open) as u64
    }

    /// **ADR-0177 D2 — the cumulative scope of a demand** (C4 F-C4R4-17), before it reserves anything: within the requester's
    /// allowance of distinct units of the claim, and — for a v1/v2 position past the OPV fence — inside its model's clear budget
    /// (`crate::scope::court_position_disclosure_v1`: past it the position would be owed `CommitmentOnly`, which that route cannot
    /// serve, so the demand is refused by name). A pure function of the ledger's rows: every node refuses the same demand.
    pub(crate) fn cumulative_scope_allows_v1(
        &self,
        requester: &Digest,
        claim: &Digest,
        stage: u8,
        position: u32,
    ) -> Result<(), String> {
        if !self.court_scope_in_force() {
            return Ok(());
        }
        let unit = (stage, position);
        let row = self.court_scope.get(claim);
        let units = row.map_or(&[][..], |r| r.units_of(requester));
        if !units.contains(&unit) && units.len() >= COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 {
            return Err(units_refusal(units.len()));
        }
        if !self.court_scope_clear_budget_in_force() || self.demands.contains_key(&(*claim, stage, position)) {
            // Joining an open demand discloses nothing it does not already owe.
            return Ok(());
        }
        let Some(claim_row) = self.claims.get(claim) else { return Ok(()) };
        if !matches!(claim_row.body, ClaimBodyV1::Program { .. }) {
            return Ok(());
        }
        // Unit and dismissal quotas above still apply. A commitment-only response owes
        // no model-dependent clear bytes and cannot spend another claim's clear allowance.
        if self.commitment_scope_applies_v1(claim_row) {
            return Ok(());
        }
        let Some(class) = self.classes.get(&claim_row.class_binding_id) else { return Ok(()) };
        let exposure = court_model_exposure_v1(&class.program, class.program.params.len());
        let served = self.served_clear_for_model_v1(&class.param_commitments.root());
        match crate::scope::court_position_disclosure_v1(served, &exposure) {
            DisclosureV1::Clear => Ok(()),
            DisclosureV1::CommitmentOnly => Err(format!(
                "COURT_SCOPE_CLEAR_BUDGET: the model's clear-disclosure budget ({} position(s); {served} already owed in the clear) is \
                 spent — past it a position is owed only as commitments, which a K2-TIR-v1/v2 response cannot be",
                court_clear_position_budget_v1(&exposure)
            )),
        }
    }

    /// **The court scope of a filing**, before the court runs: the accuser's dismissed filings on the claim are within
    /// [`COURT_SCOPE_MAX_DISMISSED_PER_REQUESTER_V1`], and a segmented filing's position is within its 16 units.
    pub(crate) fn court_scope_filing_allows_v1(
        &self,
        accuser: &Digest,
        claim: &Digest,
        unit: Option<(u8, u32)>,
    ) -> Result<(), String> {
        if !self.court_scope_in_force() {
            return Ok(());
        }
        let row = self.court_scope.get(claim);
        let dismissed = row.map_or(0, |r| r.dismissed_of(accuser));
        if dismissed >= COURT_SCOPE_MAX_DISMISSED_PER_REQUESTER_V1 {
            return Err(format!(
                "COURT_SCOPE_FILINGS: the accuser has {dismissed} dismissed filings on this claim, the most one accuser may \
                 ({COURT_SCOPE_MAX_DISMISSED_PER_REQUESTER_V1}); no court runs for it again"
            ));
        }
        if let Some(unit) = unit {
            let units = row.map_or(&[][..], |r| r.units_of(accuser));
            if !units.contains(&unit) && units.len() >= COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 {
                return Err(units_refusal(units.len()));
            }
        }
        Ok(())
    }

    /// Record a unit a requester named (an admitted demand, join or filing). Nothing below the court scope.
    pub(crate) fn court_scope_record_unit_v1(&mut self, requester: &Digest, claim: &Digest, unit: (u8, u32)) {
        if !self.court_scope_in_force() {
            return;
        }
        let row = self.court_scope.entry(*claim).or_default();
        // The allowance was checked before; a unit already named costs nothing again.
        let _ = crate::scope::court_scope_record_v1(&mut row.requested, *requester, unit);
    }

    /// Count a dismissed filing against its accuser on the claim. Nothing below the court scope.
    pub(crate) fn court_scope_record_dismissal_v1(&mut self, accuser: &Digest, claim: &Digest) {
        if !self.court_scope_in_force() {
            return;
        }
        let row = self.court_scope.entry(*claim).or_default();
        match row.dismissed.binary_search_by(|(r, _)| r.cmp(accuser)) {
            Ok(i) => row.dismissed[i].1 = row.dismissed[i].1.saturating_add(1),
            Err(i) => row.dismissed.insert(i, (*accuser, 1)),
        }
    }
}
