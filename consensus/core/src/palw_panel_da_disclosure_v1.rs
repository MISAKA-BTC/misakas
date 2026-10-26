//! **PanelDa mandatory disclosure (lane g14-fam-panelda; ADR-0206, PROPOSED — arming is the user's decision; DORMANT).**
//!
//! `PanelDa` (privacy mode 2, [`crate::palw_panel_da_v1`]) keeps a prompt's ids off the commitment transaction; only drawn seats see
//! them, and only a dispute publishes them. That leaves G14 open: when the producer and every owner (Panel) collude, nobody disputes,
//! and a Panel outsider never sees the material. ADR-0206 closes it with a **deadline-bound mandatory disclosure**: past
//! `Params::palw_panel_da_disclosure_v1` (keyed on the claim's original acceptance `a`)
//!
//! 1. the job's `prompt_token_ids_hash` binds the ids at commit (identity only — a hash commitment, never a proof of computation,
//!    PRINCIPLES §8);
//! 2. during service the ids stay with the Panel (private, never confidential — ADR-0077 SA-5);
//! 3. by `a + PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1` someone (normally the producer; anyone holding ids that match the hash may) must
//!    publish the ids as public material; the outsider then still has `NOTICE_WINDOW_DAA − DISCLOSE_BY` of notice window, and the
//!    claim rides exactly the public FP routes (selection / constraint / prefix / tokenizer — ③〜⑥) a PublicDa claim rides;
//! 4. no reward, weight or settlement credit is released before the disclosure is folded;
//! 5. a claim undisclosed at the deadline **defaults**: void, no reward, the disclosure deposit forfeited — and nothing more. An
//!    unavailable prompt is never a computation conviction and never slashes the computation bond.
//!
//! This module holds the pure rules. The disclosure carrier (transaction object), its fold row and the deposit lock are NOT wired
//! yet; `Params::validate_palw_panel_da_disclosure_v1` therefore refuses arming on any network (see the lane report).
//! Image prompts (FP Job V5) get no route here: they stay `NonPublicEvidence`.

use crate::palw_court_policy_v1::NOTICE_WINDOW_DAA;
use crate::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PalwFpJobTailV1, PalwFreePromptJobV3};
use crate::palw_g14_values_v1::{
    PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1, PALW_G14_PANEL_DA_DISCLOSURE_DEPOSIT_SOMPI_V1, PALW_G14_PANEL_DA_MAX_DISCLOSED_IDS_V1,
};
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;

/// The disclosure sentence a gateway shows before a first PanelDa use on a network where this fence governs, verbatim. It replaces
/// [`crate::palw_panel_da_v1::PALW_FP_PANEL_DA_DISCLOSURE_V1`] there: the prompt WILL become public, on a deadline, disputed or not.
pub const PALW_FP_PANEL_DA_MANDATORY_DISCLOSURE_V1: &str = "Private only for a while: the verifying seat sees this prompt during the \
     work, and the prompt is published on chain within about 1,040 DAA of acceptance whether or not anyone disputes it. This is not \
     confidentiality. If it is not published in time the claim is void and its disclosure deposit is forfeited.";

/// Is `job` a PanelDa text job that this disclosure rule can route (not V5 / image input)?
pub fn palw_panel_da_disclosable_job_v1(job: &PalwFreePromptJobV3) -> bool {
    job.privacy_mode == PALW_FP_PRIVACY_PANEL_DA && !job.is_v5() && !matches!(job.tail, Some(PalwFpJobTailV1::V5(_)))
}

/// The disclosure deadline (inclusive) of a claim originally accepted at `accepted_daa`.
pub fn palw_panel_da_disclosure_deadline_v1(accepted_daa: u64) -> Option<u64> {
    accepted_daa.checked_add(PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1)
}

/// The window arithmetic arming must hold: the outsider keeps at least its minimum notice window after the deadline.
pub fn palw_panel_da_disclosure_window_sound_v1(disclose_by: u64, outsider_min: u64) -> bool {
    disclose_by.checked_add(outsider_min).is_some_and(|t| t <= NOTICE_WINDOW_DAA)
}

/// Where a governed PanelDa claim's disclosure stands at `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwPanelDaDisclosureStatusV1 {
    /// Not yet disclosed, deadline not passed: no reward / weight / settlement credit yet (held).
    Pending { deadline: u64 },
    /// Disclosed at `at` (≤ deadline): the claim is a public-material claim from here; the deposit is returned.
    Disclosed { at: u64 },
    /// The deadline passed with no timely disclosure: default (see [`PalwPanelDaDefaultOutcomeV1`]).
    Defaulted { deadline: u64 },
}

/// The status of a claim accepted at `accepted_daa`, disclosed at `disclosed_at` (if any), seen at `now`. A disclosure after the
/// deadline never revives a defaulted claim (the outsider's window is what the deadline protects).
pub fn palw_panel_da_disclosure_status_v1(accepted_daa: u64, disclosed_at: Option<u64>, now: u64) -> PalwPanelDaDisclosureStatusV1 {
    let deadline = palw_panel_da_disclosure_deadline_v1(accepted_daa).unwrap_or(u64::MAX);
    match disclosed_at {
        Some(at) if at >= accepted_daa && at <= deadline && at <= now => PalwPanelDaDisclosureStatusV1::Disclosed { at },
        _ if now > deadline => PalwPanelDaDisclosureStatusV1::Defaulted { deadline },
        _ => PalwPanelDaDisclosureStatusV1::Pending { deadline },
    }
}

/// Why a disclosure is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwPanelDaDisclosureRefusalV1 {
    NotPanelDaText,
    TooManyIds { got: usize, max: u64 },
    /// The ids do not bind to the job's `prompt_token_ids_hash` (or the count differs).
    IdsDoNotBind,
    BeforeAcceptance,
    AfterDeadline { deadline: u64 },
}

/// **Admit one disclosure** of `ids` for `job` (in the class's prompt-ids `form`) (claim accepted at `accepted_daa`), landing at `at`. Anyone may file it: the hash
/// commitment fixes WHICH ids, so the filer's identity does not matter. Never logs an id.
pub fn palw_panel_da_disclosure_admit_v1(
    job: &PalwFreePromptJobV3,
    ids: &[u32],
    form: PalwPromptIdsFormV1,
    accepted_daa: u64,
    at: u64,
) -> Result<(), PalwPanelDaDisclosureRefusalV1> {
    if !palw_panel_da_disclosable_job_v1(job) {
        return Err(PalwPanelDaDisclosureRefusalV1::NotPanelDaText);
    }
    if ids.len() as u64 > PALW_G14_PANEL_DA_MAX_DISCLOSED_IDS_V1 {
        return Err(PalwPanelDaDisclosureRefusalV1::TooManyIds { got: ids.len(), max: PALW_G14_PANEL_DA_MAX_DISCLOSED_IDS_V1 });
    }
    if at < accepted_daa {
        return Err(PalwPanelDaDisclosureRefusalV1::BeforeAcceptance);
    }
    let deadline = palw_panel_da_disclosure_deadline_v1(accepted_daa).unwrap_or(u64::MAX);
    if at > deadline {
        return Err(PalwPanelDaDisclosureRefusalV1::AfterDeadline { deadline });
    }
    crate::palw_freeprompt_v3::palw_fp_prompt_ids_admit_v1(job, ids, form)
        .map_err(|_| PalwPanelDaDisclosureRefusalV1::IdsDoNotBind)
}

/// **The settlement hold**: `Some(why)` while a governed claim may not release reward, weight or settlement credit.
pub fn palw_panel_da_settlement_hold_v1(status: PalwPanelDaDisclosureStatusV1) -> Option<&'static str> {
    match status {
        PalwPanelDaDisclosureStatusV1::Disclosed { .. } => None,
        PalwPanelDaDisclosureStatusV1::Pending { .. } => {
            Some("palw_panel_da_disclosure_v1: the prompt is not yet publicly disclosed; reward and credit are held")
        }
        PalwPanelDaDisclosureStatusV1::Defaulted { .. } => {
            Some("palw_panel_da_disclosure_v1: disclosure default; the claim is void and earns nothing")
        }
    }
}

/// **What a default costs** — and, as importantly, what it does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelDaDefaultOutcomeV1 {
    /// The claim is void: no reward, weight, consensus credit or settlement.
    pub claim_void: bool,
    /// The disclosure deposit forfeited (burned).
    pub deposit_forfeit_sompi: u64,
    /// Never: an unavailable prompt is not a computation error.
    pub computation_conviction: bool,
    /// Never: the computation bond is not slashed for a default.
    pub computation_bond_slash_sompi: u64,
}

/// The outcome of [`PalwPanelDaDisclosureStatusV1::Defaulted`].
pub const PALW_PANEL_DA_DEFAULT_OUTCOME_V1: PalwPanelDaDefaultOutcomeV1 = PalwPanelDaDefaultOutcomeV1 {
    claim_void: true,
    deposit_forfeit_sompi: PALW_G14_PANEL_DA_DISCLOSURE_DEPOSIT_SOMPI_V1,
    computation_conviction: false,
    computation_bond_slash_sompi: 0,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_g14_values_v1::PALW_G14_PANEL_DA_OUTSIDER_MIN_WINDOW_DAA_V1;

    #[test]
    fn values_leave_the_outsider_its_window() {
        assert!(palw_panel_da_disclosure_window_sound_v1(PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1, PALW_G14_PANEL_DA_OUTSIDER_MIN_WINDOW_DAA_V1));
        assert!(!palw_panel_da_disclosure_window_sound_v1(NOTICE_WINDOW_DAA, 1));
        assert_eq!(PALW_G14_PANEL_DA_DISCLOSURE_DEPOSIT_SOMPI_V1, 13_107_200);
    }

    #[test]
    fn status_pending_disclosed_defaulted_and_no_late_revival() {
        let a = 5_000;
        let d = a + PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1;
        assert_eq!(palw_panel_da_disclosure_status_v1(a, None, a + 1), PalwPanelDaDisclosureStatusV1::Pending { deadline: d });
        assert_eq!(palw_panel_da_disclosure_status_v1(a, Some(d), d), PalwPanelDaDisclosureStatusV1::Disclosed { at: d });
        assert_eq!(palw_panel_da_disclosure_status_v1(a, None, d), PalwPanelDaDisclosureStatusV1::Pending { deadline: d });
        assert_eq!(palw_panel_da_disclosure_status_v1(a, None, d + 1), PalwPanelDaDisclosureStatusV1::Defaulted { deadline: d });
        assert_eq!(palw_panel_da_disclosure_status_v1(a, Some(d + 1), d + 5), PalwPanelDaDisclosureStatusV1::Defaulted { deadline: d });
        assert!(palw_panel_da_settlement_hold_v1(PalwPanelDaDisclosureStatusV1::Pending { deadline: d }).is_some());
        assert!(palw_panel_da_settlement_hold_v1(PalwPanelDaDisclosureStatusV1::Disclosed { at: d }).is_none());
    }

    #[test]
    fn a_default_is_never_a_conviction() {
        let o = PALW_PANEL_DA_DEFAULT_OUTCOME_V1;
        assert!(o.claim_void && !o.computation_conviction);
        assert_eq!(o.computation_bond_slash_sompi, 0);
        assert!(o.deposit_forfeit_sompi > 0);
    }
}
