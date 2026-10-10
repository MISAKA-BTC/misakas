//! **Lane DA16: the provider court (RFC-0009 §4.2) on the kernel route** — the claim-material court, dormant behind
//! `Params::palw_provider_court_v1`, which no network can arm (and which needs the kernel route's own never-armable fence).
//!
//! ```text
//! 150 ProviderLease (a provider bond)     a bonded promise to serve every committed position of ONE kernel claim until serve_until —
//!                                         reserves `reserved` of its FREE collateral (V2's committed-collateral ledger and both
//!                                         withdrawal gates see it)
//! 151 ProviderChallenge (another operator) ONE claim position of ONE lease, on chain; a challenge bond; deadline now + 20 DAA
//! 152 ProviderAnswer (the provider)        the position, verified against the claim's commitments (the kernel's own classification)
//!                                         by the deadline ⇒ cleared (fee burned from the challenger's bond); a wrong answer neither
//!                                         clears nor defaults — the clock decides
//!     tick                                 deadline passed unanswered ⇒ THAT lease is charged: its reservation slashed, the challenger
//!                                         paid the PALW reporter share (49%, ADR-0032) before Final, the rest burned (all of it after
//!                                         Final); once per (subject, provider)
//! 153 DaTransfer (the claim's producer)    a kernel claim's DA responsibility moves to its leases (≥ 2 live, distinct operators, none the
//!                                         producer's, Σ reservations ≥ the claim's, serving through its liability bound) — irreversible
//!     kernel demand default on a transferred claim ⇒ `ProviderLiableDefault`: every live lease charged, the producer pays nothing
//!     no live, uncharged lease left on a transferred claim ⇒ the claim LAPSES: void (Unavailable, not producer-defaulted), never convicted
//! ```
//!
//! **ADR-0177 re-scope (2026-10-10).** The chain does not interfere with model acquisition: the `Artifact { v2_class, kernel_param_root }`
//! subject (availability leases of model bytes), the READY / LAPSED availability of an artifact pair, the tag-104 gate on it and the
//! `AVAILABILITY_REQUIRED` hold are WITHDRAWN. The variant still decodes (discriminant 0 stays reserved, so the bytes ride exactly as
//! before below the fence), and past the fence every object naming it is refused: no row, no reservation, no charge, at any height. A
//! court unit is claim-specific or nothing ([`crate::palw_court_scope_v1`]): a provider is never charged for not serving model bytes.
//!
//! **The challenger's share is the PALW reporter share (G14).** A producer's own Sybil challengers can void its claim by charging its
//! own leases and collect the share. With the share at ADR-0032's 49% — the share an accuser of a conviction gets (G14-R4's one 490‰
//! constant; here `PALW_RCORE_REPORTER_REWARD_BPS_V1`, the same 49%) — a self-lapse nets the coalition a loss of ≥ 51% of Σ leases ≥
//! 51% of the claim's reservation: exactly the floor a self-reported conviction leaves, so voiding is never the cheaper escape. (The
//! former 500‰ beside a 10% accuser share was a discount; the transfer's Σ leases ≥ the claim's reservation is what makes the floors
//! meet.) After Final the charge is burned whole — the kernel's post-Final rule, under which demanders are not paid either.
//!
//! **What never moves collateral**: a Panel's local timeout or a local fetch failure (not inputs at all), a lease alone (a promise with no
//! challenge), a wrong answer, a challenge of a position the claim does not commit or outside the lease (refused), anything about model
//! bytes. **One failure, one party**: a claim committed below the fence, or never transferred, keeps its producer's demand/default path
//! byte for byte; a provider is charged at most once per subject; a transferred claim's producer is never charged for its material (a
//! false computation still convicts it through the kernel's unchanged `FileProof`).
//!
//! Rows live in the kernel route's aux tables 43–45 (written through the route's one journaled writer, delta 160, tail `0xEC`, root
//! `kernel-route/v1`); RPC op 211 already pages them. Table 45's claim row also holds the court scope's per-requester tally.

use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeSet;

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::constants::SOMPI_PER_KASPA;
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_public_material_v1::PublicUnitV1;
use crate::palw_state_v2::PalwBondKeyV2;

/// The kernel route's aux tables of the court (the Lead's allocation; 36–40 onboarding, 41–42 G14-R4).
pub const PALW_PROVIDER_COURT_TABLE_LEASES_V1: u8 = 43;
pub const PALW_PROVIDER_COURT_TABLE_CHALLENGES_V1: u8 = 44;
pub const PALW_PROVIDER_COURT_TABLE_SUBJECTS_V1: u8 = 45;

/// The ML-DSA-87 context of every court object's signature (not in a live network's committed set: the Some-only fence covers it).
pub const PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/provider-court/object/v1";
const PALW_PROVIDER_COURT_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/provider-court/object-message/v1";

/// The object tags (RFC-0009's reservation 150–153).
pub const PALW_PROVIDER_LEASE_TAG_V1: u8 = 150;
pub const PALW_PROVIDER_CHALLENGE_TAG_V1: u8 = 151;
pub const PALW_PROVIDER_ANSWER_TAG_V1: u8 = 152;
pub const PALW_DA_TRANSFER_TAG_V1: u8 = 153;

/// **INTERIM terms** — consensus constants of the (never-armed) fence, drill values, not security values (module doc of the design:
/// `docs/design/palw/da16-transport-and-provider-court.md` §2.6).
pub const PALW_PROVIDER_LEASE_MIN_RESERVATION_SOMPI_V1: u64 = 100 * SOMPI_PER_KASPA;
pub const PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1: u64 = 10 * SOMPI_PER_KASPA;
/// What a challenge the provider answered costs its challenger (burned from the bond): challenges are not free court work.
pub const PALW_PROVIDER_CHALLENGE_FEE_SOMPI_V1: u64 = SOMPI_PER_KASPA;
/// How long a provider has to answer (the kernel route's court deadline; inside the onboarding binding's 40-DAA window).
pub const PALW_PROVIDER_RESPONSE_WINDOW_DAA_V1: u64 = 20;
/// Distinct operators a subject needs live behind it: an artifact pair to be READY, a claim to be transferred.
pub const PALW_PROVIDER_MIN_PROVIDERS_V1: usize = 2;
/// Open challenges one bond may hold (its collateral bounds it too).
pub const PALW_PROVIDER_MAX_OPEN_CHALLENGES_V1: usize = 8;

/// **The court's reporter share** — what a challenger (of a charged lease) or the demanders (of a provider-liable default, of at most the
/// route's `default_penalty` of the charge) are paid before Final: ADR-0032's PALW reporter share (49%), the rest burned. The same rate
/// as the route's accuser and demander shares (G14-R4's `PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1` = 490‰; at its merge this reads that
/// constant), so the two reporter paths of a claim cannot drift.
pub fn palw_provider_reporter_share_v1(slashed: u64) -> u64 {
    (slashed as u128 * crate::palw_state_v2::PALW_RCORE_REPORTER_REWARD_BPS_V1 as u128 / 10_000) as u64
}

// ---- the fence ---------------------------------------------------------------------------------------------------------------

impl Params {
    /// Whether the provider court is in force at `daa_score` — never, in this binary.
    pub fn palw_provider_court_active_at(&self, daa_score: u64) -> bool {
        self.palw_provider_court_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The fence's refusal**: the court ships on the kernel route, which no network can arm, so any armed height is refused.
    pub fn validate_palw_provider_court_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_provider_court_v1 {
            Some(f) if f != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_provider_court_v1 cannot be armed: it is part of the kernel route's public-DA surface (RFC-0009 §4.2, RFC-0014 §16), which no network can arm",
            )),
            _ => Ok(()),
        }
    }
}

// ---- subjects, messages, rows -------------------------------------------------------------------------------------------------

/// **What a lease covers** (module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ProviderSubjectV1 {
    /// **WITHDRAWN (ADR-0177 D1)**: a V2 class's artifact under both roots. It decodes (so it rides below the fence exactly as before)
    /// and is refused past it at every height: model availability is not a consensus matter.
    Artifact { v2_class: Hash64, kernel_param_root: Hash64 } = 0,
    /// A kernel route claim's committed material (every position of every stage).
    KernelClaim { claim: Hash64 } = 1,
}

impl ProviderSubjectV1 {
    /// Whether this subject is the withdrawn artifact one.
    pub const fn is_withdrawn(&self) -> bool {
        matches!(self, Self::Artifact { .. })
    }

    pub fn key(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("a subject serializes")
    }
}

/// **The message a court object's signer signs**: `H(domain; network ‖ kind ‖ signer ‖ len ‖ payload)`, `payload` the object's Borsh
/// without its signature.
pub fn palw_provider_court_message_v1(network_domain: Hash64, kind: u8, signer: &PalwBondKeyV2, payload: &[u8]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_PROVIDER_COURT_MESSAGE_DOMAIN_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(&[kind]);
    s.update(signer.0.transaction_id.as_byte_slice());
    s.update(&signer.0.index.to_le_bytes());
    s.update(&(payload.len() as u64).to_le_bytes());
    s.update(payload);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// One lease, keyed `(subject, provider)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProviderLeaseRowV1 {
    /// Held against the provider's collateral now (0 once charged or released).
    pub reserved: u64,
    pub filed_daa: u64,
    pub serve_until_daa: u64,
    pub charged: bool,
}

impl ProviderLeaseRowV1 {
    /// In force at `daa`: not charged, not released, inside its term.
    pub fn live_at(&self, daa: u64) -> bool {
        !self.charged && self.reserved > 0 && daa <= self.serve_until_daa
    }
}

/// One challenge, keyed `(subject, provider, unit)` — one per unit of a lease. The row lives until its deadline even once answered (a
/// tombstone): the signed challenge is valid only up to its `valid_until_daa ≤ filed + window ≤ deadline`, so a replay of it — by
/// anyone, at any later block — meets either this row or its own expiry, and never re-opens a closed challenge on its signer's account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProviderChallengeRowV1 {
    pub challenger: PalwBondKeyV2,
    /// The challenger's bond, held against its collateral until the challenge closes (0 once answered).
    pub bond: u64,
    pub filed_daa: u64,
    pub deadline_daa: u64,
    /// Answered by the provider (verified against the chain's root): cleared, kept as a tombstone until the deadline.
    pub answered: bool,
}

/// What the court remembers of a (kernel claim) subject, keyed by the subject.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProviderSubjectRowV1 {
    /// A kernel claim whose DA responsibility moved to its leases (tag 153), and when.
    pub transferred_daa: Option<u64>,
    /// The DAA at which a transferred claim was left with no live, uncharged lease after a charge (a lapse: void, never convicted).
    pub lapsed_daa: Option<u64>,
    /// Every provider charged for this subject (each at most once), ascending.
    pub charged: Vec<PalwBondKeyV2>,
    /// **The court scope's per-requester tally** (ADR-0177 D2, [`crate::palw_court_scope_v1`]): the distinct `(stage, position)` units
    /// each requester OPERATOR has demanded of this claim through the kernel's `FileDemand`, ascending by operator; at most
    /// `PALW_COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1` per operator.
    pub requested: Vec<(Hash64, Vec<(u8, u32)>)>,
}

pub fn palw_provider_lease_key_v1(subject: &ProviderSubjectV1, provider: &PalwBondKeyV2) -> Vec<u8> {
    borsh::to_vec(&(subject, provider)).expect("a lease key serializes")
}

pub fn palw_provider_challenge_key_v1(subject: &ProviderSubjectV1, provider: &PalwBondKeyV2, unit: &PublicUnitV1) -> Vec<u8> {
    borsh::to_vec(&(subject, provider, unit)).expect("a challenge key serializes")
}

/// **The latest DAA a kernel claim's liability can run to** — its liability horizon once Final, else the worst case from its commitment:
/// check window, challenge window (the OPV one when longer), court deadline, proof grace, liability. A lease that backs a transfer serves
/// at least this long.
pub fn palw_kernel_claim_horizon_bound_v1(
    row: &misaka_palw_kernel::ledger::ClaimRowV1,
    policy: &misaka_palw_kernel::ledger::LedgerPolicyV1,
    opv: Option<&misaka_palw_kernel::opv::OpvPolicyV1>,
) -> u64 {
    if let Some(until) = row.liability_until {
        return until;
    }
    let window = policy.challenge_window_daa.max(opv.map(|p| p.window_daa()).unwrap_or(0));
    row.committed_daa
        .saturating_add(policy.check_window_daa)
        .saturating_add(window)
        .saturating_add(policy.court_deadline_daa)
        .saturating_add(policy.proof_grace_daa)
        .saturating_add(policy.liability_daa)
}

// ---- reads (the route's aux rows) ---------------------------------------------------------------------------------------------

impl PalwKernelRouteStateV1 {
    /// Every aux row of `table` whose key starts with `prefix`, in key order.
    fn aux_prefixed(&self, table: u8, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.aux
            .range((table, prefix.to_vec())..(table + 1, Vec::new()))
            .take_while(|((t, key), _)| *t == table && key.starts_with(prefix))
            .map(|((_, key), row)| (key.clone(), row.clone()))
            .collect()
    }

    pub fn provider_lease_v1(&self, subject: &ProviderSubjectV1, provider: &PalwBondKeyV2) -> Option<ProviderLeaseRowV1> {
        self.aux_row(PALW_PROVIDER_COURT_TABLE_LEASES_V1, &palw_provider_lease_key_v1(subject, provider))
    }

    /// Every lease of `subject`, `(provider, row)`, in key order.
    pub fn provider_leases_of_v1(&self, subject: &ProviderSubjectV1) -> Vec<(PalwBondKeyV2, ProviderLeaseRowV1)> {
        self.aux_prefixed(PALW_PROVIDER_COURT_TABLE_LEASES_V1, &subject.key())
            .into_iter()
            .filter_map(|(key, row)| {
                let (_, provider) = borsh::from_slice::<(ProviderSubjectV1, PalwBondKeyV2)>(&key).ok()?;
                Some((provider, borsh::from_slice::<ProviderLeaseRowV1>(&row).ok()?))
            })
            .collect()
    }

    /// Every lease, `(subject, provider, row)`.
    pub fn provider_leases_v1(&self) -> Vec<(ProviderSubjectV1, PalwBondKeyV2, ProviderLeaseRowV1)> {
        self.aux_prefixed(PALW_PROVIDER_COURT_TABLE_LEASES_V1, &[])
            .into_iter()
            .filter_map(|(key, row)| {
                let (subject, provider) = borsh::from_slice::<(ProviderSubjectV1, PalwBondKeyV2)>(&key).ok()?;
                Some((subject, provider, borsh::from_slice::<ProviderLeaseRowV1>(&row).ok()?))
            })
            .collect()
    }

    pub fn provider_subject_v1(&self, subject: &ProviderSubjectV1) -> Option<ProviderSubjectRowV1> {
        self.aux_row(PALW_PROVIDER_COURT_TABLE_SUBJECTS_V1, &subject.key())
    }

    pub fn provider_challenge_v1(
        &self,
        subject: &ProviderSubjectV1,
        provider: &PalwBondKeyV2,
        unit: &PublicUnitV1,
    ) -> Option<ProviderChallengeRowV1> {
        self.aux_row(PALW_PROVIDER_COURT_TABLE_CHALLENGES_V1, &palw_provider_challenge_key_v1(subject, provider, unit))
    }

    /// Every open challenge, `(subject, provider, unit, row)`, in key order.
    pub fn provider_challenges_v1(&self) -> Vec<(ProviderSubjectV1, PalwBondKeyV2, PublicUnitV1, ProviderChallengeRowV1)> {
        self.aux_prefixed(PALW_PROVIDER_COURT_TABLE_CHALLENGES_V1, &[])
            .into_iter()
            .filter_map(|(key, row)| {
                let (subject, provider, unit) = borsh::from_slice::<(ProviderSubjectV1, PalwBondKeyV2, PublicUnitV1)>(&key).ok()?;
                Some((subject, provider, unit, borsh::from_slice::<ProviderChallengeRowV1>(&row).ok()?))
            })
            .collect()
    }

    /// **What the court holds against `bond`**: its uncharged leases' reservations and its open challenges' bonds — the term V2's
    /// committed-collateral ledger and both withdrawal gates add beside the kernel's and the onboarding's.
    pub fn provider_court_reserved_v1(&self, bond: &PalwBondKeyV2) -> u64 {
        let leases = self
            .provider_leases_v1()
            .into_iter()
            .filter(|(_, p, row)| p == bond && !row.charged)
            .fold(0u64, |acc, (_, _, row)| acc.saturating_add(row.reserved));
        let challenges = self
            .provider_challenges_v1()
            .into_iter()
            .filter(|(_, _, _, row)| row.challenger == *bond)
            .fold(0u64, |acc, (_, _, _, row)| acc.saturating_add(row.bond));
        leases.saturating_add(challenges)
    }

    /// **A kernel claim's stored row**, decoded alone (no ledger rebuild): what the court's arms check a claim subject against.
    pub fn kernel_claim_row_v1(&self, claim: &Hash64) -> Option<misaka_palw_kernel::ledger::ClaimRowV1> {
        let key = borsh::to_vec(&claim.as_bytes()).expect("a digest serializes");
        borsh::from_slice(self.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, key))?).ok()
    }

    /// The kernel claims whose DA responsibility moved to their leases (what the fold injects as `KernelLedgerV1::provider_liable`).
    pub fn provider_liable_claims_v1(&self) -> BTreeSet<Hash64> {
        self.aux_prefixed(PALW_PROVIDER_COURT_TABLE_SUBJECTS_V1, &[])
            .into_iter()
            .filter_map(|(key, row)| {
                let ProviderSubjectV1::KernelClaim { claim } = borsh::from_slice::<ProviderSubjectV1>(&key).ok()? else { return None };
                borsh::from_slice::<ProviderSubjectRowV1>(&row).ok()?.transferred_daa.map(|_| claim)
            })
            .collect()
    }

    /// Open (unanswered) challenges held by `challenger`.
    pub fn provider_open_challenges_by_v1(&self, challenger: &PalwBondKeyV2) -> usize {
        self.provider_challenges_v1().into_iter().filter(|(_, _, _, row)| row.challenger == *challenger && !row.answered).count()
    }

    /// **The court's rows of one subject, decoded** (the typed read of the aux rows RPC op 211 already pages; no new op): its subject
    /// row, every lease and every challenge (open or answered).
    pub fn provider_court_read_v1(&self, subject: &ProviderSubjectV1) -> ProviderCourtReadV1 {
        ProviderCourtReadV1 {
            subject: *subject,
            row: self.provider_subject_v1(subject),
            leases: self.provider_leases_of_v1(subject),
            challenges: self
                .provider_challenges_v1()
                .into_iter()
                .filter(|(s, _, _, _)| s == subject)
                .map(|(_, provider, unit, row)| (provider, unit, row))
                .collect(),
        }
    }
}

/// [`PalwKernelRouteStateV1::provider_court_read_v1`]'s answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderCourtReadV1 {
    pub subject: ProviderSubjectV1,
    pub row: Option<ProviderSubjectRowV1>,
    pub leases: Vec<(PalwBondKeyV2, ProviderLeaseRowV1)>,
    pub challenges: Vec<(PalwBondKeyV2, PublicUnitV1, ProviderChallengeRowV1)>,
}

/// **Where a subject's availability stands** at a DAA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderAvailabilityV1 {
    /// The live, uncharged leases' providers that serve through the horizon asked about.
    pub live: Vec<PalwBondKeyV2>,
    /// Distinct operators among them.
    pub operators: usize,
    /// Σ of their reservations.
    pub reserved: u64,
    /// ≥ [`PALW_PROVIDER_MIN_PROVIDERS_V1`] distinct operators.
    pub ready: bool,
    pub lapsed_daa: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};

    #[test]
    fn the_fence_is_dormant_everywhere_hashed_when_set_and_refused_when_armed() {
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_provider_court_v1, None);
            assert!(!p.palw_provider_court_active_at(u64::MAX));
            p.validate_palw_provider_court_v1().unwrap();
            assert!(p.palw_fences_v1().contains(&("palw_provider_court_v1", None)), "the exhaustive list names it");
        }
        let mut p = TESTNET_PARAMS;
        let (id, schedule) = (p.consensus_params_id(), p.consensus_schedule_id());
        p.palw_provider_court_v1 = Some(ForkActivation::never());
        p.validate_palw_provider_court_v1().unwrap();
        p.palw_provider_court_v1 = Some(ForkActivation::new(9_000));
        assert!(p.validate_palw_provider_court_v1().is_err() && p.validate_palw_v2().is_err());
        assert_ne!(p.consensus_params_id(), id, "an armed height is another network");
        assert_ne!(p.consensus_schedule_id(), schedule, "and another schedule");
        let mut q = TESTNET_PARAMS;
        q.palw_provider_court_v1 = Some(ForkActivation::new(9_001));
        assert_ne!(q.consensus_params_id(), p.consensus_params_id(), "the height is in the id");
    }

    #[test]
    fn the_tables_tags_and_keys_are_the_allocated_ones() {
        assert_eq!(
            (PALW_PROVIDER_COURT_TABLE_LEASES_V1, PALW_PROVIDER_COURT_TABLE_CHALLENGES_V1, PALW_PROVIDER_COURT_TABLE_SUBJECTS_V1),
            (43, 44, 45)
        );
        assert_eq!(
            (PALW_PROVIDER_LEASE_TAG_V1, PALW_PROVIDER_CHALLENGE_TAG_V1, PALW_PROVIDER_ANSWER_TAG_V1, PALW_DA_TRANSFER_TAG_V1),
            (150, 151, 152, 153)
        );
        let a = ProviderSubjectV1::Artifact { v2_class: Hash64::from_bytes([1; 64]), kernel_param_root: Hash64::from_bytes([2; 64]) };
        let c = ProviderSubjectV1::KernelClaim { claim: Hash64::from_bytes([3; 64]) };
        assert_eq!((a.key()[0], c.key()[0]), (0, 1));
        let p = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(7), 0));
        assert!(palw_provider_lease_key_v1(&a, &p).starts_with(&a.key()), "a subject's leases are one key range");
        let row = ProviderLeaseRowV1 { reserved: 5, filed_daa: 1, serve_until_daa: 10, charged: false };
        assert!(row.live_at(10) && !row.live_at(11) && !ProviderLeaseRowV1 { charged: true, ..row }.live_at(2));
        assert!(!ProviderLeaseRowV1 { reserved: 0, ..row }.live_at(2));
    }

    /// The four objects carry their tags as their first byte, round-trip, and are exactly what the acceptance walk drops by name below
    /// the fence (and nothing else is).
    #[test]
    fn the_four_objects_are_tags_150_to_153_and_the_only_provider_court_objects() {
        use crate::palw_public_material_v1::PublicUnitAnswerV1;
        use crate::palw_state_v2::{PalwConsensusObjectV2 as O, palw_object_is_onboarding_v1, palw_object_is_provider_court_v1};
        let h = Hash64::from_bytes([4; 64]);
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(9), 1));
        let subject = ProviderSubjectV1::Artifact { v2_class: h, kernel_param_root: h };
        let unit = PublicUnitV1::ArtifactLeaf { index: 3 };
        let objects = [
            (O::ProviderLeaseV1 { subject, reserved: 1, serve_until_daa: 2, provider: bond, signature: vec![1] }, 150),
            (O::ProviderChallengeV1 { subject, provider: bond, unit, valid_until_daa: 5, challenger: bond, signature: vec![1] }, 151),
            (
                O::ProviderAnswerV1 {
                    subject,
                    provider: bond,
                    unit: PublicUnitV1::ClaimPosition { stage: 0, position: 1 },
                    answer: Box::new(PublicUnitAnswerV1::ClaimPosition { bytes: vec![7, 7] }),
                    signature: vec![1],
                },
                152,
            ),
            (O::DaTransferV1 { claim: h, producer: bond, signature: vec![1] }, 153),
        ];
        for (object, tag) in objects {
            let bytes = borsh::to_vec(&object).unwrap();
            assert_eq!(bytes[0], tag, "{object:?}");
            assert_eq!(borsh::from_slice::<O>(&bytes).unwrap(), object, "round trip, tag {tag}");
            assert!(palw_object_is_provider_court_v1(&object) && !palw_object_is_onboarding_v1(&object));
        }
        let other = O::DaTransferV1 { claim: h, producer: bond, signature: vec![] };
        assert!(crate::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&other).is_err(), "an unsigned court object");
    }
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// What the provider court holds against `bond` (0 with no kernel route): V2's committed-collateral ledger and both withdrawal gates
    /// add it beside the kernel's and the onboarding's reservations.
    pub fn provider_court_reserved(&self, bond: &PalwBondKeyV2) -> u128 {
        self.kernel_route().map(|k| k.provider_court_reserved_v1(bond) as u128).unwrap_or(0)
    }

    /// **The availability of `subject` at `daa`**: its live, uncharged leases that serve through `need_until`, by distinct operators,
    /// leaving out `exclude_operator` (a claim's producer: its own leases never move its liability).
    pub fn provider_availability_v1(
        &self,
        subject: &ProviderSubjectV1,
        daa: u64,
        need_until: u64,
        exclude_operator: Option<Hash64>,
    ) -> ProviderAvailabilityV1 {
        let Some(route) = self.kernel_route() else {
            return ProviderAvailabilityV1 { live: Vec::new(), operators: 0, reserved: 0, ready: false, lapsed_daa: None };
        };
        let mut live = Vec::new();
        let mut operators = BTreeSet::new();
        let mut reserved = 0u64;
        for (provider, row) in route.provider_leases_of_v1(subject) {
            if !row.live_at(daa) || row.serve_until_daa < need_until {
                continue;
            }
            let Some(operator) = self.bond(&provider).map(|b| b.operator_id) else { continue };
            if Some(operator) == exclude_operator {
                continue;
            }
            operators.insert(operator);
            reserved = reserved.saturating_add(row.reserved);
            live.push(provider);
        }
        ProviderAvailabilityV1 {
            ready: operators.len() >= PALW_PROVIDER_MIN_PROVIDERS_V1,
            operators: operators.len(),
            live,
            reserved,
            lapsed_daa: route.provider_subject_v1(subject).and_then(|r| r.lapsed_daa),
        }
    }
}
