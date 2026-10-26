//! **Lane g14-fam-panelda: the PanelDa mandatory disclosure in the fold** (ADR-0206, accepted 2026-10-26; DORMANT behind
//! `palw_panel_da_disclosure_v1`). A child module of `palw_state_v2`: it reads the builder and writes `panel_da_disclosures` only
//! through its one journaled writer (delta 236).
//!
//! * **Admission** ([`admit_panel_da_on_commit_v1`]): a PanelDa TEXT commitment accepted at/after the fence writes its row and locks
//!   the disclosure deposit against the producer's bond (counted in `palw_bond_committed_v1`); refused whole if the bond's free
//!   collateral cannot hold it.
//! * **Disclosure** ([`fold_panel_da_disclosure_v1`], tag 233): anyone's ids that the claim's hash binds, inside `a..=a+1,040`, once.
//!   The ids become public state; the deposit is returned (no longer counted).
//! * **Final floor** ([`palw_panel_da_final_floor_v1`]): a governed claim reaches `Final` no earlier than `deadline + 1`, so no reward,
//!   weight or settlement credit moves before the disclosure is decided.
//! * **Final or default** ([`settle_panel_da_at_final_v1`]): at the Final, a disclosed claim proceeds; an undisclosed one is voided
//!   `PanelDaDisclosureDefault` and its deposit burned — never a conviction, never a computation-bond slash.
//! * The court clock (`palw_court_clock_v2`, a prerequisite of the fence) keeps every outsider door open to `a + 3,120` (notice) /
//!   `a + 10,000` (hard) from the ORIGINAL acceptance; the disclosure deadline sits inside the notice window by construction.

use super::*;
use crate::palw_panel_da_disclosure_v1::*;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::PanelDaDisclosureRefused(why.into())
}

impl PalwChainStateV2 {
    /// The PanelDa disclosure deposits still locked against `bond` (0 with no row: every shipped network).
    pub fn panel_da_deposit_locked_v1(&self, bond: &PalwBondKeyV2) -> u128 {
        self.panel_da_disclosures
            .values()
            .filter(|row| row.bond == *bond)
            .fold(0u128, |sum, row| sum.saturating_add(row.locked_deposit_v1() as u128))
    }

    /// A governed PanelDa claim's disclosure row (public read: the ids are public once disclosed).
    pub fn panel_da_disclosure_v1(&self, claim: &Hash64) -> Option<&PalwPanelDaDisclosureRowV1> {
        self.panel_da_disclosures.get(claim)
    }
}

/// **The Final floor a governed PanelDa claim adds** (read by `palw_claim_final_floor_v1`): `deadline + 1`; `None` with no row.
pub(super) fn palw_panel_da_final_floor_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> Option<u64> {
    state.panel_da_disclosures.get(claim_id).map(|row| row.deadline_daa.saturating_add(1))
}

impl TransitionBuilder<'_> {
    /// The one journaled writer of `panel_da_disclosures` (delta 236).
    pub(super) fn write_panel_da_disclosure_v1(
        &mut self,
        key: Hash64,
        new: Option<PalwPanelDaDisclosureRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.panel_da_disclosures, true, "native panel_da_disclosures writer")?;
        let old = match &new {
            Some(row) => self.state.panel_da_disclosures.insert(key, row.clone()),
            None => self.state.panel_da_disclosures.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::PanelDaDisclosure { key, old, new });
        }
        Ok(())
    }
}

/// **Admission of a governed PanelDa commitment** — after the claim row is written. A no-op below the fence, for a PublicDa job and
/// for a V5 (image) job (which stays `NonPublicEvidence` and is refused by the G14 route gate, not here).
pub(super) fn admit_panel_da_on_commit_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    claim: &PalwClaimStateV2,
    public_job: Option<&crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.panel_da_disclosure_governs_v1(claim.accepted_daa) {
        return Ok(());
    }
    let Some(job) = public_job.map(|c| &c.job) else {
        return Ok(());
    };
    if !palw_panel_da_disclosable_job_v1(job) {
        return Ok(());
    }
    let deadline = palw_panel_da_disclosure_deadline_v1(claim.accepted_daa).ok_or_else(|| refused("deadline overflow"))?;
    let deposit = crate::palw_g14_values_v1::PALW_G14_PANEL_DA_DISCLOSURE_DEPOSIT_SOMPI_V1;
    let bond = builder.state.bonds.get(&claim.bond).ok_or(PalwStateV2Error::MissingBond(claim.bond))?;
    let committed = palw_bond_committed_v1(&builder.state, &claim.bond, ctx.daa_score, None, builder.params.window_court());
    if (bond.collateral as u128).saturating_sub(committed) < deposit as u128 {
        return Err(refused("the producer bond's free collateral cannot lock the PanelDa disclosure deposit"));
    }
    builder.write_panel_da_disclosure_v1(
        claim_id,
        Some(PalwPanelDaDisclosureRowV1 {
            version: PALW_PANEL_DA_DISCLOSURE_ROW_VERSION_V1,
            bond: claim.bond,
            accepted_daa: claim.accepted_daa,
            deadline_daa: deadline,
            deposit_sompi: deposit,
            prompt_token_ids_hash: job.prompt_token_ids_hash,
            prompt_tokens: job.prompt_tokens,
            disclosed_daa: None,
            prompt_token_ids: Vec::new(),
        }),
    )
}

/// **Tag 233: fold one disclosure.** Below the fence the acceptance walk drops the kind unjudged (A-2) and this is the second lock.
pub(super) fn fold_panel_da_disclosure_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: &Hash64,
    prompt_token_ids: &[u32],
) -> Result<(), PalwStateV2Error> {
    if !builder.params.panel_da_disclosure_from_daa().is_some_and(|from| ctx.daa_score >= from) {
        return Err(refused("below palw_panel_da_disclosure_v1"));
    }
    let row = builder.state.panel_da_disclosures.get(claim).cloned().ok_or_else(|| refused(format!("{:?}", PalwPanelDaDisclosureRefusalV1::NoSuchRow)))?;
    row.admit_v1(prompt_token_ids, ctx.daa_score).map_err(|why| refused(format!("{why:?}")))?;
    let class_id = builder.state.claims.get(claim).map(|c| c.class_id).ok_or_else(|| refused("the disclosed claim is retired"))?;
    let mut disclosed = row;
    disclosed.disclosed_daa = Some(ctx.daa_score);
    disclosed.prompt_token_ids = prompt_token_ids.to_vec();
    let producer = disclosed.bond;
    builder.write_panel_da_disclosure_v1(*claim, Some(disclosed))?;
    // The public replay input every route reads (the claim's prompt row; the TIR frame's horizon where framed).
    super::palw_tir_public_fold_v1::keep_disclosed_prompt_ids_v1(builder, *claim, class_id, producer, prompt_token_ids, ctx.daa_score)
}

/// **At a governed claim's Final** (called first in `finalize_claim`): `Ok(true)` to proceed with the Final (disclosed, or no row);
/// `Ok(false)` when the claim was voided here as a disclosure default — the deposit burned, nothing else charged.
pub(super) fn settle_panel_da_at_final_v1(
    builder: &mut TransitionBuilder<'_>,
    id: Hash64,
    claim: &PalwClaimStateV2,
    final_daa: u64,
) -> Result<bool, PalwStateV2Error> {
    let Some(row) = builder.state.panel_da_disclosures.get(&id).cloned() else { return Ok(true) };
    match row.status_v1(final_daa) {
        PalwPanelDaDisclosureStatusV1::Disclosed { .. } => Ok(true),
        PalwPanelDaDisclosureStatusV1::Pending { .. } => {
            Err(refused("a governed PanelDa claim reached Final before its disclosure deadline (the Final floor forbids it)"))
        }
        PalwPanelDaDisclosureStatusV1::Defaulted { .. } => {
            // The deposit: unlocked from the row first (so the committed sum no longer counts it), then burned. `burn_registration_fee`
            // is the fold's non-conviction burn (it raises `slashed`, the release spend's burn record, which nothing reads as guilt).
            builder.write_panel_da_disclosure_v1(id, None)?;
            let collateral = builder.state.bonds.get(&row.bond).map(|b| b.collateral).unwrap_or(0);
            builder.burn_registration_fee(row.bond, row.deposit_sompi.min(collateral))?;
            builder.void_claim(id, claim, final_daa, PalwVoidReasonV2::PanelDaDisclosureDefault)?;
            Ok(false)
        }
    }
}
