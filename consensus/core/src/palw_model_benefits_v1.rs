//! ADR-0095 — a position is a membership, not an income.
//!
//! A position buys no income and no vote (ADR-0091 settled that: the reward BUYS the pair and the
//! chain retires what it buys, so nothing is ever handed to a holder). What it buys instead is what
//! the line's owner has declared for its holders — the new version first, the private beta, the
//! front of the queue, experimental modes, a voice in what ships next.
//!
//! The reason this module is in consensus rather than in a gateway is the part the chain can
//! actually ENFORCE. A gateway's queue is nobody's business but the gateway's, but the exclusivity
//! WINDOW is arithmetic on two heights, and ADR-0095 §4.4 makes it a rule: while a line declares
//! `EARLY_VERSION` with a lead, a version must enter as a preview and may not be promoted until the
//! lead has passed. Everything here exists to give that rule one spelling — and to make a promise
//! that stops being kept (§4.6) lapse without anyone having to submit a complaint.

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;
use blake2b_simd::Params;

/// §4.7: how long a WEAKENING waits before it governs. Anything that takes a benefit away — a
/// removed grant, a raised threshold, a shortened expiry, a withdrawal — is stored as pending and
/// the old declaration keeps governing until this many DAA have passed. Without it every promise
/// here is theatre: declare, take the buyer's money at the curve, withdraw in the next block.
pub const PALW_MODEL_BENEFIT_NOTICE_DAA: u64 = 4_000;

/// §4.1: the most tiers a declaration may carry. Eight is a card a person can read.
pub const PALW_MODEL_BENEFIT_MAX_TIERS: usize = 8;

/// §4.1: the most bytes a tier's note may carry — it states the quota, the context length, the
/// name of the room. It is a label, never a rule.
pub const PALW_MODEL_BENEFIT_MAX_NOTE: usize = 64;

/// §4.2 — the grant set, closed, and the chain's rather than a line's.
///
/// **Nothing that pays is in here, and nothing that pays may be added without amending ADR-0095.**
/// A share, a rebate, a discount in MSK, a claim on the reserve: none of these is a service, and
/// the moment one becomes a bit a position stops being a membership and becomes an income. An
/// unknown bit is refused at the fold (N5) rather than stored and ignored, because a promise no
/// reader can render is not a promise.
pub mod grant {
    /// The artifact of a new version, `lead_daa` before the line MAY make it current (§4.4).
    pub const EARLY_VERSION: u32 = 1 << 0;
    /// Versions published as previews are served to holders and to nobody else.
    pub const PRIVATE_BETA: u32 = 1 << 1;
    /// The line's gateways serve holders' jobs ahead of others'.
    pub const PRIORITY_INFERENCE: u32 = 1 << 2;
    /// Modes the line runs but has not made default: longer context, thinking, tools, a new
    /// quantisation.
    pub const EXPERIMENTAL: u32 = 1 << 3;
    /// The line's own room: proposals, research previews, where the next version is argued about.
    pub const DEVELOPER_ACCESS: u32 = 1 << 4;
    /// Served capacity — a request allowance the gateway honours, stated in the tier's note. The
    /// UNITS are deliberately not in consensus (§8): a serving decision is not a consensus one.
    pub const INFERENCE_QUOTA: u32 = 1 << 5;
    /// Evaluations and proposals from holders carry the holder mark and the tier (§4.9).
    pub const HOLDER_VOICE: u32 = 1 << 6;
    /// The line answers holders' reports first.
    pub const SUPPORT: u32 = 1 << 7;

    /// Every bit this version of the protocol knows. N5 refuses anything outside it.
    pub const KNOWN: u32 =
        EARLY_VERSION | PRIVATE_BETA | PRIORITY_INFERENCE | EXPERIMENTAL | DEVELOPER_ACCESS | INFERENCE_QUOTA | HOLDER_VOICE | SUPPORT;

    /// The names a wallet renders, in bit order — one spelling for the whole network.
    pub const NAMES: [&str; 8] = [
        "EARLY_VERSION",
        "PRIVATE_BETA",
        "PRIORITY_INFERENCE",
        "EXPERIMENTAL",
        "DEVELOPER_ACCESS",
        "INFERENCE_QUOTA",
        "HOLDER_VOICE",
        "SUPPORT",
    ];

    /// Render a bitset as the names, for a card or a CLI line.
    pub fn names_of(grants: u32) -> Vec<&'static str> {
        (0..8).filter(|b| grants & (1 << b) != 0).map(|b| NAMES[b as usize]).collect()
    }
}

/// §4.1 — one tier of a line's declaration.
#[derive(Clone, Debug, PartialEq, Eq, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelBenefitTierV1 {
    /// The units a holder must hold to be in this tier — carrier lane plus EVM lane (§4.3).
    pub min_units: u64,
    /// The closed set of §4.2.
    pub grants: u32,
    /// §4.4: how long this tier's holders have the version before it may become current. Only
    /// meaningful with `EARLY_VERSION`; the fold enforces the LARGEST such lead in the declaration.
    pub lead_daa: u64,
    /// §4.5: how long the holder must have held WITHOUT SELLING to be in this tier.
    pub min_hold_daa: u64,
    /// A label: the quota, the context length, the name of the room. Never a rule.
    pub note: Vec<u8>,
}

/// §4.7 — a weakening that has been declared and is waiting out its notice.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelBenefitPendingV1 {
    pub tiers: Vec<PalwModelBenefitTierV1>,
    pub cadence_daa: u64,
    pub expires_daa: u64,
    /// The height at and after which `tiers` govern.
    pub effective_daa: u64,
}

/// §4.1 — the line's declaration, as the state holds it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelBenefitsV1 {
    /// The tiers governing now (subject to `pending` maturing).
    pub tiers: Vec<PalwModelBenefitTierV1>,
    /// §4.6: how often the line undertakes to publish a version. 0 = no undertaking.
    pub cadence_daa: u64,
    /// §4.6: the height at which the whole declaration stops granting. 0 = never.
    pub expires_daa: u64,
    /// When this declaration landed — shown on the card, and the anchor a reader dates it from.
    pub declared_daa: u64,
    /// The owner that signed it, so a reader can see the promise did not change hands silently.
    pub declared_by: Option<PalwBondKeyV2>,
    /// §4.7.
    pub pending: Option<PalwModelBenefitPendingV1>,
}

/// Why a declaration was refused (§6 N5, N6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwModelBenefitRejectV1 {
    /// More than `PALW_MODEL_BENEFIT_MAX_TIERS`.
    TooManyTiers,
    /// `min_units` did not strictly increase, or a tier asked for zero units.
    TiersNotIncreasing,
    /// A bit outside `grant::KNOWN` (N5).
    UnknownGrant,
    /// A note longer than `PALW_MODEL_BENEFIT_MAX_NOTE`.
    NoteTooLong,
    /// A lead was declared by a tier that does not grant `EARLY_VERSION`: it would be a number
    /// the fold enforces on behalf of a grant nobody was given.
    LeadWithoutEarlyAccess,
}

/// §4.1/§4.2/§4.6 — everything about a proposed declaration that is checkable without the state.
pub fn palw_model_benefits_validate_v1(
    tiers: &[PalwModelBenefitTierV1],
    expires_daa: u64,
    daa: u64,
) -> Result<(), PalwModelBenefitRejectV1> {
    if tiers.len() > PALW_MODEL_BENEFIT_MAX_TIERS {
        return Err(PalwModelBenefitRejectV1::TooManyTiers);
    }
    let mut last: Option<u64> = None;
    for t in tiers {
        if t.min_units == 0 {
            return Err(PalwModelBenefitRejectV1::TiersNotIncreasing);
        }
        if let Some(prev) = last {
            if t.min_units <= prev {
                return Err(PalwModelBenefitRejectV1::TiersNotIncreasing);
            }
        }
        last = Some(t.min_units);
        if t.grants & !grant::KNOWN != 0 {
            return Err(PalwModelBenefitRejectV1::UnknownGrant);
        }
        if t.note.len() > PALW_MODEL_BENEFIT_MAX_NOTE {
            return Err(PalwModelBenefitRejectV1::NoteTooLong);
        }
        if t.lead_daa > 0 && t.grants & grant::EARLY_VERSION == 0 {
            return Err(PalwModelBenefitRejectV1::LeadWithoutEarlyAccess);
        }
    }
    // An expiry already in the past would be a declaration that never governs — refuse it rather
    // than store a card that renders as LAPSED the moment it lands.
    if expires_daa > 0 && expires_daa <= daa {
        return Err(PalwModelBenefitRejectV1::TiersNotIncreasing);
    }
    Ok(())
}

/// §4.7 — is `next` a STRENGTHENING of `prev`? Strengthening lands at once; anything else waits out
/// the notice.
///
/// The comparison is per THRESHOLD rather than per index, because tiers are a ladder and an index
/// means nothing across two different ladders: for every tier in `next`, the holder at that
/// threshold must end up with at least what `prev` gave them, and no threshold `prev` served may
/// vanish. A withdrawal (`next` empty, `prev` not) is therefore a weakening, which is the case that
/// matters most.
pub fn palw_model_benefits_is_strengthening_v1(
    prev: &[PalwModelBenefitTierV1],
    prev_expires: u64,
    next: &[PalwModelBenefitTierV1],
    next_expires: u64,
) -> bool {
    // A shortened (or newly imposed) expiry takes a benefit away. 0 means "never", so it is the
    // strongest value and only a move away from 0 can weaken.
    let expiry_weakened = match (prev_expires, next_expires) {
        (0, 0) => false,
        (0, _) => true,
        (_, 0) => false,
        (p, n) => n < p,
    };
    if expiry_weakened {
        return false;
    }
    // Every holder `prev` served must be served at least as well by `next`.
    for pt in prev {
        let got = tier_for_units(next, pt.min_units, u64::MAX);
        let Some(nt) = got else { return false };
        if nt.grants & pt.grants != pt.grants {
            return false;
        }
        if nt.lead_daa < pt.lead_daa {
            return false;
        }
        // A raised tenure requirement takes the tier away from someone who has it today.
        if nt.min_hold_daa > pt.min_hold_daa {
            return false;
        }
    }
    true
}

/// The highest tier in `tiers` a holder of `units` with `tenure_daa` qualifies for (§4.3, §4.5).
/// Pass `u64::MAX` for `tenure_daa` to ignore the clock, which is what §4.7's comparison wants.
pub fn tier_for_units(tiers: &[PalwModelBenefitTierV1], units: u64, tenure_daa: u64) -> Option<&PalwModelBenefitTierV1> {
    tiers.iter().filter(|t| units >= t.min_units && tenure_daa >= t.min_hold_daa).next_back()
}

/// §4.6 — the tiers GOVERNING at `daa`, after any pending weakening has matured and after the
/// lapse rules.
///
/// This is a read-time function of the row and the line's last publication, never a state write
/// (N12): a line that stops shipping stops granting on the block that crosses the line, and two
/// nodes at the same height agree without anyone submitting anything.
pub fn palw_model_benefits_in_effect_v1(row: &PalwModelBenefitsV1, last_version_daa: u64, daa: u64) -> &[PalwModelBenefitTierV1] {
    let (tiers, cadence, expires) = match &row.pending {
        Some(p) if daa >= p.effective_daa => (&p.tiers, p.cadence_daa, p.expires_daa),
        _ => (&row.tiers, row.cadence_daa, row.expires_daa),
    };
    if expires > 0 && daa >= expires {
        return &[];
    }
    if cadence > 0 && daa > last_version_daa.saturating_add(cadence) {
        return &[];
    }
    tiers
}

/// §4.6 — why a declaration is granting nothing, for the card. `None` means it is in effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwModelBenefitLapseV1 {
    /// Past `expires_daa`.
    Expired { at_daa: u64 },
    /// Nothing has been published for longer than `cadence_daa` (§4.6): the line stopped shipping,
    /// so it stops advertising a membership it is no longer servicing.
    CadenceMissed { due_daa: u64 },
}

/// The lapse reason at `daa`, if any.
pub fn palw_model_benefits_lapse_v1(row: &PalwModelBenefitsV1, last_version_daa: u64, daa: u64) -> Option<PalwModelBenefitLapseV1> {
    let (cadence, expires) = match &row.pending {
        Some(p) if daa >= p.effective_daa => (p.cadence_daa, p.expires_daa),
        _ => (row.cadence_daa, row.expires_daa),
    };
    if expires > 0 && daa >= expires {
        return Some(PalwModelBenefitLapseV1::Expired { at_daa: expires });
    }
    let due = last_version_daa.saturating_add(cadence);
    if cadence > 0 && daa > due {
        return Some(PalwModelBenefitLapseV1::CadenceMissed { due_daa: due });
    }
    None
}

/// §4.4 — the lead the fold enforces: the LARGEST `lead_daa` among tiers granting `EARLY_VERSION`
/// in the declaration in effect. Zero means the version paths are unconstrained.
///
/// The largest rather than the smallest, because the window has to hold for the tier that was
/// promised the longest one; a holder at the top tier who was promised 72 hours does not get 6
/// because a lower tier exists.
pub fn palw_model_benefits_enforced_lead_v1(tiers: &[PalwModelBenefitTierV1]) -> u64 {
    tiers.iter().filter(|t| t.grants & grant::EARLY_VERSION != 0).map(|t| t.lead_daa).max().unwrap_or(0)
}

fn begin(domain: &[u8], network_domain: Hash64, line_id: &Hash64) -> blake2b_simd::State {
    let mut s = Params::new().hash_length(64).to_state();
    s.update(&(domain.len() as u32).to_le_bytes());
    s.update(domain);
    s.update(network_domain.as_byte_slice());
    s.update(line_id.as_byte_slice());
    s
}

fn finish(s: blake2b_simd::State) -> Hash64 {
    Hash64::from_slice(s.finalize().as_bytes())
}

/// §4.1 — the message `ModelLineBenefitsDeclared` is signed over, by the line's OWNER.
pub fn palw_model_benefits_message_v1(
    network_domain: Hash64,
    line_id: &Hash64,
    tiers: &[PalwModelBenefitTierV1],
    cadence_daa: u64,
    expires_daa: u64,
) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-benefits/declared/v1", network_domain, line_id);
    s.update(&(tiers.len() as u32).to_le_bytes());
    for t in tiers {
        s.update(&t.min_units.to_le_bytes());
        s.update(&t.grants.to_le_bytes());
        s.update(&t.lead_daa.to_le_bytes());
        s.update(&t.min_hold_daa.to_le_bytes());
        s.update(&(t.note.len() as u32).to_le_bytes());
        s.update(&t.note);
    }
    s.update(&cadence_daa.to_le_bytes());
    s.update(&expires_daa.to_le_bytes());
    finish(s)
}

/// §4.8 — the message a holder signs to prove membership to a gateway. Nothing is submitted and
/// nothing is spent: the gateway verifies this signature and then reads the tier at `daa` itself.
pub fn palw_model_benefit_challenge_v1(network_domain: Hash64, line_id: &Hash64, holder: &Hash64, nonce: &[u8], daa: u64) -> Hash64 {
    let mut s = begin(b"misaka-palw/model-benefits/challenge/v1", network_domain, line_id);
    s.update(holder.as_byte_slice());
    s.update(&(nonce.len() as u32).to_le_bytes());
    s.update(nonce);
    s.update(&daa.to_le_bytes());
    finish(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(min_units: u64, grants: u32, lead_daa: u64, min_hold_daa: u64) -> PalwModelBenefitTierV1 {
        PalwModelBenefitTierV1 { min_units, grants, lead_daa, min_hold_daa, note: Vec::new() }
    }

    fn row(tiers: Vec<PalwModelBenefitTierV1>, cadence_daa: u64, expires_daa: u64) -> PalwModelBenefitsV1 {
        PalwModelBenefitsV1 { tiers, cadence_daa, expires_daa, declared_daa: 10, declared_by: None, pending: None }
    }

    /// N6, N5: the shape rules a declaration must satisfy before it is stored.
    #[test]
    fn a_declaration_is_a_ladder_over_a_closed_set() {
        let ok = vec![tier(1, grant::EARLY_VERSION, 100, 0), tier(1_000, grant::EARLY_VERSION | grant::SUPPORT, 500, 0)];
        assert_eq!(palw_model_benefits_validate_v1(&ok, 0, 5), Ok(()));

        // Not increasing.
        let flat = vec![tier(10, grant::SUPPORT, 0, 0), tier(10, grant::SUPPORT, 0, 0)];
        assert_eq!(palw_model_benefits_validate_v1(&flat, 0, 5), Err(PalwModelBenefitRejectV1::TiersNotIncreasing));

        // Zero units would make every address a member, including addresses that hold nothing.
        let zero = vec![tier(0, grant::SUPPORT, 0, 0)];
        assert_eq!(palw_model_benefits_validate_v1(&zero, 0, 5), Err(PalwModelBenefitRejectV1::TiersNotIncreasing));

        // N5: an unknown bit is refused, not stored and ignored.
        let unknown = vec![tier(1, 1 << 20, 0, 0)];
        assert_eq!(palw_model_benefits_validate_v1(&unknown, 0, 5), Err(PalwModelBenefitRejectV1::UnknownGrant));

        // A lead is a number the FOLD enforces; a lead without the grant it belongs to would have
        // the chain refusing promotions on behalf of a benefit nobody was given.
        let orphan_lead = vec![tier(1, grant::SUPPORT, 900, 0)];
        assert_eq!(palw_model_benefits_validate_v1(&orphan_lead, 0, 5), Err(PalwModelBenefitRejectV1::LeadWithoutEarlyAccess));

        let many: Vec<_> = (1..=9).map(|i| tier(i as u64, grant::SUPPORT, 0, 0)).collect();
        assert_eq!(palw_model_benefits_validate_v1(&many, 0, 5), Err(PalwModelBenefitRejectV1::TooManyTiers));
    }

    /// N2, N5: the tier is the balance and the clock.
    #[test]
    fn the_tier_is_the_balance_and_the_clock() {
        let tiers = vec![
            tier(1, grant::SUPPORT, 0, 0),
            tier(100, grant::PRIORITY_INFERENCE, 0, 500),
            tier(1_000, grant::DEVELOPER_ACCESS, 0, 0),
        ];
        assert!(tier_for_units(&tiers, 0, 10_000).is_none(), "below the first tier is not a member");
        assert_eq!(tier_for_units(&tiers, 1, 10_000).unwrap().min_units, 1);
        assert_eq!(tier_for_units(&tiers, 100, 10_000).unwrap().min_units, 100);
        // The clock: 100 units but only 10 DAA of tenure falls back to the tier below, which asks
        // for none — a holder is not thrown out of the ladder, only out of the tier they have not
        // earned yet.
        assert_eq!(tier_for_units(&tiers, 100, 10).unwrap().min_units, 1);
        // And the tier above 100 asks for no tenure at all, so a fresh whale still gets it.
        assert_eq!(tier_for_units(&tiers, 1_000, 0).unwrap().min_units, 1_000);
    }

    /// N9: giving lands at once, taking away waits.
    #[test]
    fn taking_a_benefit_away_is_not_the_same_as_giving_one() {
        let prev = vec![tier(1, grant::EARLY_VERSION | grant::SUPPORT, 600, 0)];

        // Adding a grant, a tier and a longer lead: strengthening.
        let stronger = vec![
            tier(1, grant::EARLY_VERSION | grant::SUPPORT | grant::PRIVATE_BETA, 900, 0),
            tier(500, grant::EARLY_VERSION | grant::SUPPORT | grant::DEVELOPER_ACCESS, 900, 0),
        ];
        assert!(palw_model_benefits_is_strengthening_v1(&prev, 0, &stronger, 0));

        // Removing a grant.
        let dropped = vec![tier(1, grant::EARLY_VERSION, 600, 0)];
        assert!(!palw_model_benefits_is_strengthening_v1(&prev, 0, &dropped, 0));

        // A9: shortening the lead is a weakening, so it cannot be used as an escape from §4.4.
        let shorter = vec![tier(1, grant::EARLY_VERSION | grant::SUPPORT, 10, 0)];
        assert!(!palw_model_benefits_is_strengthening_v1(&prev, 0, &shorter, 0));

        // Raising the threshold takes the tier from whoever sits between the two.
        let raised = vec![tier(50, grant::EARLY_VERSION | grant::SUPPORT, 600, 0)];
        assert!(!palw_model_benefits_is_strengthening_v1(&prev, 0, &raised, 0));

        // Raising the tenure requirement takes the tier from someone who has it today.
        let slower = vec![tier(1, grant::EARLY_VERSION | grant::SUPPORT, 600, 5_000)];
        assert!(!palw_model_benefits_is_strengthening_v1(&prev, 0, &slower, 0));

        // A1: withdrawal is the weakening that matters most.
        assert!(!palw_model_benefits_is_strengthening_v1(&prev, 0, &[], 0));

        // Imposing an expiry where there was none.
        assert!(!palw_model_benefits_is_strengthening_v1(&prev, 0, &prev, 9_000));
        // Extending one is fine, and so is removing it.
        assert!(palw_model_benefits_is_strengthening_v1(&prev, 9_000, &prev, 20_000));
        assert!(palw_model_benefits_is_strengthening_v1(&prev, 9_000, &prev, 0));
    }

    /// N9: the pending weakening does not govern until its height.
    #[test]
    fn the_old_declaration_governs_until_the_notice_runs_out() {
        let mut r = row(vec![tier(1, grant::EARLY_VERSION, 600, 0)], 0, 0);
        r.pending = Some(PalwModelBenefitPendingV1 { tiers: Vec::new(), cadence_daa: 0, expires_daa: 0, effective_daa: 4_100 });
        assert_eq!(palw_model_benefits_in_effect_v1(&r, 0, 4_099).len(), 1, "the old promise still stands");
        assert_eq!(palw_model_benefits_enforced_lead_v1(palw_model_benefits_in_effect_v1(&r, 0, 4_099)), 600);
        assert!(palw_model_benefits_in_effect_v1(&r, 0, 4_100).is_empty(), "and on the height itself it is gone");
    }

    /// N10, A4: a line that stops shipping stops granting — and stops constraining.
    #[test]
    fn a_promise_that_stops_being_kept_lapses_by_itself() {
        let r = row(vec![tier(1, grant::EARLY_VERSION, 600, 0)], 1_000, 0);
        // Shipped at 500, so the cadence is due at 1500.
        assert_eq!(palw_model_benefits_in_effect_v1(&r, 500, 1_500).len(), 1);
        assert!(palw_model_benefits_lapse_v1(&r, 500, 1_500).is_none());
        assert!(palw_model_benefits_in_effect_v1(&r, 500, 1_501).is_empty());
        assert_eq!(palw_model_benefits_lapse_v1(&r, 500, 1_501), Some(PalwModelBenefitLapseV1::CadenceMissed { due_daa: 1_500 }));
        // N10's second half: a lapsed promise constrains nobody, so the fold stops refusing.
        assert_eq!(palw_model_benefits_enforced_lead_v1(palw_model_benefits_in_effect_v1(&r, 500, 1_501)), 0);

        let e = row(vec![tier(1, grant::EARLY_VERSION, 600, 0)], 0, 2_000);
        assert_eq!(palw_model_benefits_in_effect_v1(&e, 0, 1_999).len(), 1);
        assert!(palw_model_benefits_in_effect_v1(&e, 0, 2_000).is_empty());
        assert_eq!(palw_model_benefits_lapse_v1(&e, 0, 2_000), Some(PalwModelBenefitLapseV1::Expired { at_daa: 2_000 }));
    }

    /// §4.4: the enforced lead is the LARGEST, and only `EARLY_VERSION` tiers contribute one.
    #[test]
    fn the_enforced_lead_is_the_longest_promise_in_the_ladder() {
        let tiers =
            vec![tier(1, grant::EARLY_VERSION, 100, 0), tier(100, grant::EARLY_VERSION, 600, 0), tier(1_000, grant::SUPPORT, 0, 0)];
        assert_eq!(palw_model_benefits_enforced_lead_v1(&tiers), 600);
        assert_eq!(palw_model_benefits_enforced_lead_v1(&[tier(1, grant::SUPPORT, 0, 0)]), 0);
        assert_eq!(palw_model_benefits_enforced_lead_v1(&[]), 0);
    }

    /// The signed messages must move with every field, or a signature covers less than it appears
    /// to and a tier can be edited under it.
    #[test]
    fn the_declaration_message_covers_every_field() {
        let nd = Hash64::from_slice(&[7u8; 64]);
        let line = Hash64::from_slice(&[9u8; 64]);
        let base = vec![tier(1, grant::EARLY_VERSION, 600, 0)];
        let m = |t: &[PalwModelBenefitTierV1], c: u64, e: u64| palw_model_benefits_message_v1(nd, &line, t, c, e);
        let m0 = m(&base, 1_000, 2_000);
        assert_ne!(m0, m(&[tier(2, grant::EARLY_VERSION, 600, 0)], 1_000, 2_000), "min_units");
        assert_ne!(m0, m(&[tier(1, grant::EARLY_VERSION | grant::SUPPORT, 600, 0)], 1_000, 2_000), "grants");
        assert_ne!(m0, m(&[tier(1, grant::EARLY_VERSION, 601, 0)], 1_000, 2_000), "lead");
        assert_ne!(m0, m(&[tier(1, grant::EARLY_VERSION, 600, 1)], 1_000, 2_000), "tenure");
        assert_ne!(m0, m(&base, 1_001, 2_000), "cadence");
        assert_ne!(m0, m(&base, 1_000, 2_001), "expiry");
        let mut noted = base.clone();
        noted[0].note = b"x".to_vec();
        assert_ne!(m0, m(&noted, 1_000, 2_000), "note");
        // And the line it is for.
        assert_ne!(m0, palw_model_benefits_message_v1(nd, &Hash64::from_slice(&[8u8; 64]), &base, 1_000, 2_000));
    }

    /// §4.8: a challenge that did not move with the height would let a gateway be shown a proof
    /// made when the holder still held.
    #[test]
    fn the_challenge_names_the_height_it_is_good_for() {
        let nd = Hash64::from_slice(&[1u8; 64]);
        let line = Hash64::from_slice(&[2u8; 64]);
        let who = Hash64::from_slice(&[3u8; 64]);
        let a = palw_model_benefit_challenge_v1(nd, &line, &who, b"n", 100);
        assert_ne!(a, palw_model_benefit_challenge_v1(nd, &line, &who, b"n", 101));
        assert_ne!(a, palw_model_benefit_challenge_v1(nd, &line, &who, b"m", 100));
        assert_ne!(a, palw_model_benefit_challenge_v1(nd, &line, &Hash64::from_slice(&[4u8; 64]), b"n", 100));
    }

    #[test]
    fn the_grant_names_are_one_spelling_for_the_whole_network() {
        assert_eq!(grant::names_of(grant::EARLY_VERSION | grant::SUPPORT), vec!["EARLY_VERSION", "SUPPORT"]);
        assert_eq!(grant::names_of(0), Vec::<&str>::new());
        assert_eq!(grant::names_of(grant::KNOWN).len(), 8);
    }
}
