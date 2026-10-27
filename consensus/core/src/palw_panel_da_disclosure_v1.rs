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
//! This module holds the pure rules; the fold is `palw_panel_da_disclosure_fold_v1` (tag 233, delta 236, tail 0xE8). Arming stays
//! refused by `Params::validate_palw_panel_da_disclosure_v1` until a real-node E2E has run (see the lane report).
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
    AlreadyDisclosed,
    /// No governed PanelDa claim has this id (never admitted under the fence, or already retired).
    NoSuchRow,
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

/// **The object tag of a PanelDa disclosure** (`PalwConsensusObjectV2::PanelDaPromptDisclosedV1`). Below
/// `palw_panel_da_disclosure_v1` it rides unjudged (A-2: the kind→fence table owns it).
pub const PALW_PANEL_DA_DISCLOSURE_TAG_V1: u8 = 233;
/// Delta 236: the one journaled writer of `panel_da_disclosures`.
pub const PALW_PANEL_DA_DISCLOSURE_DELTA_V1: u8 = 236;
/// The carriage tail of `panel_da_disclosures`, written only when the table is non-empty.
pub const PALW_CARRIAGE_PANEL_DA_DISCLOSURES_TAIL_V1: u8 = 0xE8;
/// The row's layout version.
pub const PALW_PANEL_DA_DISCLOSURE_ROW_VERSION_V1: u8 = 1;

/// **One governed PanelDa claim's disclosure row** (`panel_da_disclosures`, keyed by claim; ADR-0206). Written at the claim's
/// admission past `palw_panel_da_disclosure_v1` (the deposit is then locked against `bond`), updated once by the disclosure
/// (`disclosed_daa`, the public ids — the deposit is returned), and removed at the claim's retirement or at its default (the deposit
/// burned). Rooted (`panel_da_disclosures/v1`), journaled (delta 236), carried (tail `0xE8`), each only when non-empty.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelDaDisclosureRowV1 {
    pub version: u8,
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    pub accepted_daa: u64,
    pub deadline_daa: u64,
    pub deposit_sompi: u64,
    pub prompt_token_ids_hash: crate::Hash64,
    pub prompt_tokens: u32,
    pub disclosed_daa: Option<u64>,
    /// The disclosed ids — PUBLIC once written (this is the disclosure). Empty while pending.
    pub prompt_token_ids: Vec<u32>,
}

impl PalwPanelDaDisclosureRowV1 {
    /// The deposit still locked against the bond (0 once disclosed).
    pub fn locked_deposit_v1(&self) -> u64 {
        if self.disclosed_daa.is_some() { 0 } else { self.deposit_sompi }
    }

    pub fn status_v1(&self, now: u64) -> PalwPanelDaDisclosureStatusV1 {
        palw_panel_da_disclosure_status_v1(self.accepted_daa, self.disclosed_daa, now)
    }

    /// **Admit a disclosure of `ids` at `at` against this row.** Count and hash must bind (under either prompt-ids form: the two
    /// forms hash under different domains, so a match under either names exactly the committed ids); in the window; once.
    pub fn admit_v1(&self, ids: &[u32], at: u64) -> Result<(), PalwPanelDaDisclosureRefusalV1> {
        use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1 as Form, prompt_token_ids_match_v1};
        if self.disclosed_daa.is_some() {
            return Err(PalwPanelDaDisclosureRefusalV1::AlreadyDisclosed);
        }
        if ids.len() as u64 > PALW_G14_PANEL_DA_MAX_DISCLOSED_IDS_V1 {
            return Err(PalwPanelDaDisclosureRefusalV1::TooManyIds { got: ids.len(), max: PALW_G14_PANEL_DA_MAX_DISCLOSED_IDS_V1 });
        }
        if at < self.accepted_daa {
            return Err(PalwPanelDaDisclosureRefusalV1::BeforeAcceptance);
        }
        if at > self.deadline_daa {
            return Err(PalwPanelDaDisclosureRefusalV1::AfterDeadline { deadline: self.deadline_daa });
        }
        if ids.len() != self.prompt_tokens as usize
            || !(prompt_token_ids_match_v1(Form::Flat, ids, &self.prompt_token_ids_hash)
                || prompt_token_ids_match_v1(Form::MerkleV1, ids, &self.prompt_token_ids_hash))
        {
            return Err(PalwPanelDaDisclosureRefusalV1::IdsDoNotBind);
        }
        Ok(())
    }
}

// ---- read-only user view (lane panelda-ux, op 227 view `panelDaDisclosure/v1`): no rule reads it --------------------------------------------------------------

/// The disclosure object a duty holder submits (tag 233). The fold decodes exactly this borsh value.
pub fn palw_panel_da_disclosure_object_v1(claim: crate::Hash64, ids: Vec<u32>) -> crate::palw_state_v2::PalwConsensusObjectV2 {
    crate::palw_state_v2::PalwConsensusObjectV2::PanelDaPromptDisclosedV1 { claim, prompt_token_ids: ids }
}

/// The deposit's state as a user reads it.
pub fn palw_panel_da_deposit_state_v1(row: Option<&PalwPanelDaDisclosureRowV1>, defaulted: bool) -> &'static str {
    match (row, defaulted) {
        (_, true) => "burned",
        (Some(r), false) if r.disclosed_daa.is_some() => "returned",
        (Some(_), false) => "locked",
        (None, false) => "none",
    }
}

/// **op 227 view `panelDaDisclosure/v1` document, version 1**: `None` when the chain holds neither a disclosure row nor a claim.
/// Never prints a prompt id (only their count once disclosed).
pub fn palw_panel_da_disclosure_view_json_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim: &crate::Hash64,
    clock: &crate::palw_user_views_v1::PalwUserViewClockV1,
) -> Option<serde_json::Value> {
    use crate::palw_state_v2::{PalwClaimPhaseV2, PalwVoidReasonV2};
    let row = state.panel_da_disclosure_v1(claim);
    let record = state.claim(claim);
    if row.is_none() && record.is_none() {
        return None;
    }
    let void_reason = record.and_then(|c| match &c.phase {
        PalwClaimPhaseV2::Voided { reason, voided_daa } => Some((*reason, *voided_daa)),
        _ => None,
    });
    let defaulted = matches!(void_reason, Some((PalwVoidReasonV2::PanelDaDisclosureDefault, _)));
    let now = clock.next_daa;
    Some(serde_json::json!({
        "version": 1,
        "claim": claim.to_string(),
        "governed": row.is_some() || defaulted,
        "acceptedDaa": row.map(|r| r.accepted_daa).or(record.map(|c| c.accepted_daa)),
        "deadlineDaa": row.map(|r| r.deadline_daa),
        "nowDaa": now,
        "remainingDaa": row.filter(|r| r.disclosed_daa.is_none()).map(|r| r.deadline_daa.saturating_sub(now.saturating_sub(1))),
        "disclosed": row.is_some_and(|r| r.disclosed_daa.is_some()),
        "disclosedDaa": row.and_then(|r| r.disclosed_daa),
        "disclosedIds": row.filter(|r| r.disclosed_daa.is_some()).map(|r| r.prompt_token_ids.len()),
        "promptTokens": row.map(|r| r.prompt_tokens),
        "promptTokenIdsHash": row.map(|r| r.prompt_token_ids_hash.to_string()),
        "producerBond": row.map(|r| format!("{}:{}", r.bond.0.transaction_id, r.bond.0.index)),
        "depositSompi": row.map(|r| r.deposit_sompi).unwrap_or(if defaulted { PALW_G14_PANEL_DA_DISCLOSURE_DEPOSIT_SOMPI_V1 } else { 0 }),
        "deposit": palw_panel_da_deposit_state_v1(row, defaulted),
        "claimPhase": record.map(|c| format!("{:?}", c.phase).split([' ', '{', '(']).next().unwrap_or_default().to_string()),
        "voidReason": void_reason.map(|(r, _)| format!("{r:?}")),
        "voidedDaa": void_reason.map(|(_, d)| d),
    }))
}

#[cfg(test)]
mod view_tests {
    use super::*;

    fn row(disclosed: Option<u64>) -> PalwPanelDaDisclosureRowV1 {
        PalwPanelDaDisclosureRowV1 {
            version: 1,
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_u64_word(1),
                index: 0,
            }),
            accepted_daa: 100,
            deadline_daa: 1_140,
            deposit_sompi: 7,
            prompt_token_ids_hash: crate::Hash64::default(),
            prompt_tokens: 3,
            disclosed_daa: disclosed,
            prompt_token_ids: disclosed.map(|_| vec![1, 2, 3]).unwrap_or_default(),
        }
    }

    #[test]
    fn the_built_object_is_tag_233_and_decodes_as_the_fold_reads_it() {
        let claim = crate::Hash64::from_u64_word(9);
        let object = palw_panel_da_disclosure_object_v1(claim, vec![5, 6]);
        let bytes = borsh::to_vec(&object).unwrap();
        assert_eq!(bytes[0], PALW_PANEL_DA_DISCLOSURE_TAG_V1);
        let back: crate::palw_state_v2::PalwConsensusObjectV2 = borsh::from_slice(&bytes).unwrap();
        assert!(matches!(back, crate::palw_state_v2::PalwConsensusObjectV2::PanelDaPromptDisclosedV1 { claim: c, ref prompt_token_ids }
            if c == claim && *prompt_token_ids == vec![5, 6]));
        crate::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&back).expect("it may ride a carrier");
    }

    #[test]
    fn the_deposit_state_reads_locked_returned_burned() {
        assert_eq!(palw_panel_da_deposit_state_v1(Some(&row(None)), false), "locked");
        assert_eq!(palw_panel_da_deposit_state_v1(Some(&row(Some(200))), false), "returned");
        assert_eq!(palw_panel_da_deposit_state_v1(None, true), "burned");
        assert_eq!(palw_panel_da_deposit_state_v1(None, false), "none");
    }

    #[test]
    fn the_view_names_the_deadline_and_never_a_prompt_id() {
        let mut state = crate::palw_state_v2::PalwChainStateV2::genesis();
        let claim = crate::Hash64::from_u64_word(9);
        let clock = crate::palw_user_views_v1::PalwUserViewClockV1 { next_daa: 141, raw_depth: None, objective_offence: false };
        assert!(palw_panel_da_disclosure_view_json_v1(&state, &claim, &clock).is_none(), "unknown claim");
        state.panel_da_disclosures_insert_for_tests_v1(claim, row(None));
        let doc = palw_panel_da_disclosure_view_json_v1(&state, &claim, &clock).unwrap();
        assert_eq!(doc["version"], 1);
        assert_eq!(doc["deadlineDaa"], 1_140);
        assert_eq!(doc["remainingDaa"], 1_000);
        assert_eq!(doc["deposit"], "locked");
        assert_eq!(doc["disclosed"], false);
        state.panel_da_disclosures_insert_for_tests_v1(claim, row(Some(150)));
        let doc = palw_panel_da_disclosure_view_json_v1(&state, &claim, &clock).unwrap();
        assert_eq!((doc["deposit"].as_str(), doc["disclosedIds"].as_u64()), (Some("returned"), Some(3)));
        assert!(!doc.to_string().contains("[1,2,3]"), "no prompt id is printed");
    }
}

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
