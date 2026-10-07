//! **RFC-0009 stage B — the provider challenge court** (`Params::palw_evidence_court_v1`, DORMANT on every preset).
//!
//! A storage receipt is a promise, not proof the chunk will be there when a Panel asks (RFC §4.2). This is the objective way to hold a
//! provider to it, as a pure state machine over events — the rules the chain would fold, written and tested before anything leans on them:
//!
//! ```text
//!   claim made under the court's fence ─▶ providers FILE receipts (manifest id, chunk count, retention) from their own bond
//!   anyone CHALLENGES one chunk of one provider's receipt, publicly, with a deadline (now + response window)
//!   the provider ANSWERS: the manifest (which must hash to the receipt's manifest id) and the chunk (which must hash to the manifest's entry)
//!   SWEEP at the deadline: no valid answer ⇒ that provider's collateral is the case's subject — ONCE per (claim, provider)
//!                          every provider that filed has defaulted ⇒ the CLAIM LAPSES (unpaid) — which is not the miner's fraud
//! ```
//!
//! **What never moves collateral.** A single Panel's local fetch failure or timeout ([`CourtEventV1::PanelLocalTimeout`] is a no-op by
//! construction), a provider's pre-signed receipt on its own, a WRONG answer (it neither clears nor defaults: the clock decides), a challenge
//! of a chunk the receipt never promised or after its retention ran out.
//!
//! **No double slash.** A claim is accountable to exactly one party for its material, chosen when it is made ([`evidence_responsibility_v1`]):
//! the producer (today's `DefaultAccused` / withholding path) or the providers (this court). A producer-withholding accusation against a claim
//! under this court is refused here, and a provider challenge against a claim not under it is refused too; and a provider is charged once per
//! claim however many of its chunks fail.
//!
//! **Not yet folded.** This module is not wired into `palw_state_v2`'s fold, its objects or its carriage — that is the next step, and the
//! fence has no reader in consensus today. The machine is `Clone`: a chain integration stores a delta (or a snapshot) per block and a reorg
//! restores the clone, which the tests exercise.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_evidence_v1::{EvidenceManifestV1, manifest_id_v1};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::PalwBondKeyV2;
use std::collections::{BTreeMap, BTreeSet};

pub const DOMAIN_CHALLENGE: &[u8] = b"misaka-palw/evidence/provider-challenge/v1";

/// How long a provider has to answer, in DAA. A companion value of the fence (hashed with its height). Chosen above twice the finality window
/// so a reorg across the deadline cannot flip a verdict without a finality violation.
pub const PALW_EVIDENCE_COURT_RESPONSE_WINDOW_DAA_V1: u64 = 600;

pub const fn palw_evidence_court_value_v1() -> [u64; 1] {
    [PALW_EVIDENCE_COURT_RESPONSE_WINDOW_DAA_V1]
}

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// A public challenge of one chunk a provider promised (the pure-rule view used by [`challenge_outcome_v1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderChallengeV1 {
    pub network_domain: Hash64,
    pub manifest_id: Hash64,
    pub chunk_index: u32,
    pub provider_id: Vec<u8>,
    pub challenged_at_daa: u64,
    /// The provider must answer by this DAA.
    pub deadline_daa: u64,
}

impl ProviderChallengeV1 {
    pub fn id(&self) -> Hash64 {
        let mut s = keyed(DOMAIN_CHALLENGE);
        s.update(self.network_domain.as_bytes().as_slice());
        s.update(self.manifest_id.as_bytes().as_slice());
        s.update(&self.chunk_index.to_le_bytes());
        s.update(&(self.provider_id.len() as u64).to_le_bytes());
        s.update(&self.provider_id);
        s.update(&self.challenged_at_daa.to_le_bytes());
        s.update(&self.deadline_daa.to_le_bytes());
        finish(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChallengeOutcome {
    /// An opening that hashes to the manifest's entry was published in time: the provider is cleared, the challenger pays.
    Answered,
    /// The deadline passed with no valid opening: the provider's collateral is the case's subject — and only the provider's.
    Defaulted,
    Pending,
}

/// The pure rule: a response counts only if its bytes verify against the manifest entry for the challenged chunk, and only if it lands by
/// the deadline. A WRONG response does not clear the provider and does not by itself default it; the clock does that.
pub fn challenge_outcome_v1(
    manifest: &EvidenceManifestV1,
    challenge: &ProviderChallengeV1,
    response: Option<(&[u8], u64)>,
    now_daa: u64,
) -> ChallengeOutcome {
    if let Some((bytes, landed_daa)) = response {
        if landed_daa <= challenge.deadline_daa && manifest.verify_chunk(challenge.chunk_index, bytes).is_ok() {
            return ChallengeOutcome::Answered;
        }
    }
    if now_daa > challenge.deadline_daa { ChallengeOutcome::Defaulted } else { ChallengeOutcome::Pending }
}

/// Who answers for a claim's material. **Exactly one party**, chosen by the claim's version/fence, so one failure can never be slashed twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Responsible {
    Producer,
    Providers,
}

pub fn evidence_responsibility_v1(claim_under_provider_court: bool) -> Responsible {
    if claim_under_provider_court { Responsible::Providers } else { Responsible::Producer }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderReceiptRowV1 {
    pub manifest_id: Hash64,
    pub chunk_count: u32,
    pub retain_until_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderChallengeRowV1 {
    pub claim: Hash64,
    pub provider: PalwBondKeyV2,
    pub manifest_id: Hash64,
    pub chunk_index: u32,
    pub challenger: PalwBondKeyV2,
    pub deadline_daa: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderCourtStateV1 {
    under_court: BTreeSet<Hash64>,
    receipts: BTreeMap<(Hash64, PalwBondKeyV2), ProviderReceiptRowV1>,
    challenges: BTreeMap<Hash64, ProviderChallengeRowV1>,
    settled: BTreeSet<Hash64>,
    provider_charged: BTreeSet<(Hash64, PalwBondKeyV2)>,
    lapsed: BTreeSet<Hash64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CourtEventV1 {
    /// The claim was made under this court's fence (a property of the claim's version, recorded when it is accepted).
    ClaimUnderCourt { claim: Hash64 },
    FileReceipt { claim: Hash64, provider: PalwBondKeyV2, manifest_id: Hash64, chunk_count: u32, retain_until_daa: u64, now: u64 },
    Challenge { claim: Hash64, provider: PalwBondKeyV2, chunk_index: u32, challenger: PalwBondKeyV2, now: u64 },
    Answer { challenge: Hash64, manifest: EvidenceManifestV1, chunk: Vec<u8>, now: u64 },
    /// One Panel could not fetch (timeout, refusal, a local failure). **Never** evidence: a no-op by construction.
    PanelLocalTimeout { claim: Hash64 },
    /// An accusation that the claim's PRODUCER withheld material (today's path). Refused for a claim under this court: one failure, one party.
    ProducerWithholdingAccusation { claim: Hash64 },
    Sweep { now: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapseReason {
    /// Every provider that filed a receipt for the claim has defaulted on a challenge: nobody can serve it. NOT the miner's fraud.
    NoProviderAnswers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CourtOutcomeV1 {
    ProviderCleared { challenge: Hash64 },
    /// The provider's collateral is the case's subject — once per `(claim, provider)`.
    ProviderCharged { claim: Hash64, provider: PalwBondKeyV2 },
    /// The claim lapses unpaid. `miner_fraud` is always `false`: a withheld or lost chunk is not a false computation, and the miner's
    /// fraud slash is the court's separate (and untouched) path.
    ClaimLapsed { claim: Hash64, reason: LapseReason, miner_fraud: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CourtErrorV1 {
    #[error("claim {0} was not made under the provider court")]
    NotUnderCourt(Hash64),
    #[error("claim {0} is accountable to the provider court: a producer-withholding accusation would charge one failure twice")]
    ProducerNotAccountable(Hash64),
    #[error("a receipt retention in the past or an empty chunk set is not a promise")]
    EmptyPromise,
    #[error("provider already filed a receipt for claim {0}")]
    ReceiptExists(Hash64),
    #[error("no receipt from that provider for claim {0}")]
    NoReceipt(Hash64),
    #[error("the receipt's retention ended at {retain_until}; challenged at {now}")]
    RetentionOver { retain_until: u64, now: u64 },
    #[error("chunk {index} is not one the receipt promised ({count} chunks)")]
    ChunkNotPromised { index: u32, count: u32 },
    #[error("a provider cannot challenge itself")]
    SelfChallenge,
    #[error("a challenge for this chunk is already open")]
    AlreadyOpen,
    #[error("the provider was already charged for claim {0}")]
    AlreadyCharged(Hash64),
    #[error("claim {0} has lapsed")]
    ClaimLapsed(Hash64),
    #[error("no open challenge {0}")]
    NoOpenChallenge(Hash64),
    #[error("the answer's manifest is not the one the receipt promised")]
    WrongManifest,
    #[error("the answer's chunk does not verify against the manifest (this neither clears nor defaults the provider)")]
    BadOpening,
    #[error("the answer landed after the deadline {deadline} (at {now})")]
    TooLate { deadline: u64, now: u64 },
}

impl ProviderCourtStateV1 {
    pub fn is_under_court(&self, claim: &Hash64) -> bool {
        self.under_court.contains(claim)
    }
    pub fn has_lapsed(&self, claim: &Hash64) -> bool {
        self.lapsed.contains(claim)
    }
    pub fn open_challenges(&self) -> usize {
        self.challenges.len()
    }
    pub fn is_charged(&self, claim: &Hash64, provider: &PalwBondKeyV2) -> bool {
        self.provider_charged.contains(&(*claim, *provider))
    }
    /// The producer's withholding path applies to a claim exactly when it is NOT under this court (one failure, one party).
    pub fn producer_withholding_applies(&self, claim: &Hash64) -> bool {
        evidence_responsibility_v1(self.under_court.contains(claim)) == Responsible::Producer
    }

    /// Apply one event. A refused event changes nothing.
    pub fn apply(&mut self, window_daa: u64, event: CourtEventV1) -> Result<Vec<CourtOutcomeV1>, CourtErrorV1> {
        match event {
            CourtEventV1::ClaimUnderCourt { claim } => {
                self.under_court.insert(claim);
                Ok(vec![])
            }
            CourtEventV1::PanelLocalTimeout { .. } => Ok(vec![]),
            CourtEventV1::ProducerWithholdingAccusation { claim } => {
                if self.under_court.contains(&claim) { Err(CourtErrorV1::ProducerNotAccountable(claim)) } else { Ok(vec![]) }
            }
            CourtEventV1::FileReceipt { claim, provider, manifest_id, chunk_count, retain_until_daa, now } => {
                if !self.under_court.contains(&claim) {
                    return Err(CourtErrorV1::NotUnderCourt(claim));
                }
                if chunk_count == 0 || retain_until_daa < now {
                    return Err(CourtErrorV1::EmptyPromise);
                }
                if self.receipts.contains_key(&(claim, provider)) {
                    return Err(CourtErrorV1::ReceiptExists(claim));
                }
                self.receipts.insert((claim, provider), ProviderReceiptRowV1 { manifest_id, chunk_count, retain_until_daa });
                Ok(vec![])
            }
            CourtEventV1::Challenge { claim, provider, chunk_index, challenger, now } => {
                if !self.under_court.contains(&claim) {
                    return Err(CourtErrorV1::NotUnderCourt(claim));
                }
                if self.lapsed.contains(&claim) {
                    return Err(CourtErrorV1::ClaimLapsed(claim));
                }
                let row = self.receipts.get(&(claim, provider)).ok_or(CourtErrorV1::NoReceipt(claim))?.clone();
                if now > row.retain_until_daa {
                    return Err(CourtErrorV1::RetentionOver { retain_until: row.retain_until_daa, now });
                }
                if chunk_index >= row.chunk_count {
                    return Err(CourtErrorV1::ChunkNotPromised { index: chunk_index, count: row.chunk_count });
                }
                if challenger == provider {
                    return Err(CourtErrorV1::SelfChallenge);
                }
                if self.provider_charged.contains(&(claim, provider)) {
                    return Err(CourtErrorV1::AlreadyCharged(claim));
                }
                if self.challenges.values().any(|c| c.claim == claim && c.provider == provider && c.chunk_index == chunk_index) {
                    return Err(CourtErrorV1::AlreadyOpen);
                }
                let deadline = now.saturating_add(window_daa);
                let id = ProviderChallengeV1 {
                    network_domain: claim,
                    manifest_id: row.manifest_id,
                    chunk_index,
                    provider_id: borsh::to_vec(&provider.0).expect("borsh-serializable"),
                    challenged_at_daa: now,
                    deadline_daa: deadline,
                }
                .id();
                if self.settled.contains(&id) || self.challenges.contains_key(&id) {
                    return Err(CourtErrorV1::AlreadyOpen);
                }
                self.challenges.insert(
                    id,
                    ProviderChallengeRowV1 { claim, provider, manifest_id: row.manifest_id, chunk_index, challenger, deadline_daa: deadline },
                );
                Ok(vec![])
            }
            CourtEventV1::Answer { challenge, manifest, chunk, now } => {
                let row = self.challenges.get(&challenge).ok_or(CourtErrorV1::NoOpenChallenge(challenge))?.clone();
                if now > row.deadline_daa {
                    return Err(CourtErrorV1::TooLate { deadline: row.deadline_daa, now });
                }
                if manifest_id_v1(&manifest) != row.manifest_id {
                    return Err(CourtErrorV1::WrongManifest);
                }
                if manifest.verify_chunk(row.chunk_index, &chunk).is_err() {
                    return Err(CourtErrorV1::BadOpening);
                }
                self.challenges.remove(&challenge);
                self.settled.insert(challenge);
                Ok(vec![CourtOutcomeV1::ProviderCleared { challenge }])
            }
            CourtEventV1::Sweep { now } => {
                let due: Vec<Hash64> = self.challenges.iter().filter(|(_, c)| now > c.deadline_daa).map(|(id, _)| *id).collect();
                let mut out = Vec::new();
                for id in due {
                    let row = self.challenges.remove(&id).expect("listed above");
                    self.settled.insert(id);
                    // Once per (claim, provider), however many of its chunks failed.
                    if self.provider_charged.insert((row.claim, row.provider)) {
                        out.push(CourtOutcomeV1::ProviderCharged { claim: row.claim, provider: row.provider });
                    }
                    // Common failure: every provider that filed has defaulted ⇒ the claim lapses — a distinct outcome from a miner's fraud.
                    let filed: Vec<&PalwBondKeyV2> = self.receipts.keys().filter(|(c, _)| *c == row.claim).map(|(_, p)| p).collect();
                    if !filed.is_empty()
                        && filed.iter().all(|p| self.provider_charged.contains(&(row.claim, **p)))
                        && self.lapsed.insert(row.claim)
                    {
                        out.push(CourtOutcomeV1::ClaimLapsed { claim: row.claim, reason: LapseReason::NoProviderAnswers, miner_fraud: false });
                    }
                }
                Ok(out)
            }
        }
    }
}

impl Params {
    /// `palw_evidence_court_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_evidence_court_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_evidence_court_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **The fence's refusals**, asked by [`Params::validate_palw_v2`]: off a `ConsensusV2` network, and without `palw_da_court` at or below it
    /// (the court is the data-availability court's provider-side twin: a claim is accountable to one or the other).
    pub fn validate_palw_evidence_court_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_evidence_court_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score()) else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_evidence_court_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        if !self.palw_da_court.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_evidence_court_v1 needs palw_da_court at or below it: a claim is accountable to the producer's DA court or to the providers', never both",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(h(n), 0))
    }
    const W: u64 = 600;
    const CLAIM: u8 = 0xC1;

    fn chunks() -> Vec<Vec<u8>> {
        vec![vec![1u8; 100], vec![2u8; 50], vec![3u8; 7]]
    }
    fn manifest() -> EvidenceManifestV1 {
        EvidenceManifestV1::build(h(9), &TransactionOutpoint::new(h(0xB0), 1), &[5u8; 32], h(1), h(2), h(3), 3, 10_000, &chunks())
    }

    /// A court with the claim under it and two providers holding receipts for the same manifest.
    fn court() -> ProviderCourtStateV1 {
        let mut s = ProviderCourtStateV1::default();
        let id = manifest_id_v1(&manifest());
        s.apply(W, CourtEventV1::ClaimUnderCourt { claim: h(CLAIM) }).unwrap();
        for p in [10u8, 11] {
            s.apply(
                W,
                CourtEventV1::FileReceipt { claim: h(CLAIM), provider: bond(p), manifest_id: id, chunk_count: 3, retain_until_daa: 5_000, now: 100 },
            )
            .unwrap();
        }
        s
    }
    fn challenge(s: &mut ProviderCourtStateV1, provider: u8, chunk: u32, now: u64) -> Result<(), CourtErrorV1> {
        s.apply(W, CourtEventV1::Challenge { claim: h(CLAIM), provider: bond(provider), chunk_index: chunk, challenger: bond(99), now }).map(|_| ())
    }
    fn open_id(s: &ProviderCourtStateV1, provider: u8, chunk: u32) -> Hash64 {
        *s.challenges.iter().find(|(_, c)| c.provider == bond(provider) && c.chunk_index == chunk).map(|(id, _)| id).unwrap()
    }

    #[test]
    fn a_receipt_needs_a_claim_under_the_court_a_real_promise_and_is_filed_once() {
        let mut s = ProviderCourtStateV1::default();
        let id = manifest_id_v1(&manifest());
        let file = |s: &mut ProviderCourtStateV1, chunk_count, retain, now| {
            s.apply(W, CourtEventV1::FileReceipt { claim: h(CLAIM), provider: bond(10), manifest_id: id, chunk_count, retain_until_daa: retain, now })
        };
        assert_eq!(file(&mut s, 3, 5_000, 100), Err(CourtErrorV1::NotUnderCourt(h(CLAIM))));
        s.apply(W, CourtEventV1::ClaimUnderCourt { claim: h(CLAIM) }).unwrap();
        assert_eq!(file(&mut s, 0, 5_000, 100), Err(CourtErrorV1::EmptyPromise));
        assert_eq!(file(&mut s, 3, 50, 100), Err(CourtErrorV1::EmptyPromise));
        file(&mut s, 3, 5_000, 100).unwrap();
        assert_eq!(file(&mut s, 3, 5_000, 100), Err(CourtErrorV1::ReceiptExists(h(CLAIM))));
    }

    #[test]
    fn a_challenge_needs_a_promise_in_force_a_promised_chunk_and_a_challenger_who_is_not_the_provider() {
        let mut s = court();
        assert_eq!(challenge(&mut s, 77, 0, 200), Err(CourtErrorV1::NoReceipt(h(CLAIM))));
        assert_eq!(challenge(&mut s, 10, 3, 200), Err(CourtErrorV1::ChunkNotPromised { index: 3, count: 3 }));
        assert_eq!(challenge(&mut s, 10, 0, 5_001), Err(CourtErrorV1::RetentionOver { retain_until: 5_000, now: 5_001 }));
        assert_eq!(
            s.apply(W, CourtEventV1::Challenge { claim: h(CLAIM), provider: bond(10), chunk_index: 0, challenger: bond(10), now: 200 }),
            Err(CourtErrorV1::SelfChallenge)
        );
        challenge(&mut s, 10, 0, 200).unwrap();
        assert_eq!(challenge(&mut s, 10, 0, 201), Err(CourtErrorV1::AlreadyOpen));
        let mut other = ProviderCourtStateV1::default();
        assert_eq!(
            other.apply(W, CourtEventV1::Challenge { claim: h(0xEE), provider: bond(10), chunk_index: 0, challenger: bond(99), now: 1 }),
            Err(CourtErrorV1::NotUnderCourt(h(0xEE)))
        );
    }

    #[test]
    fn a_valid_opening_by_the_deadline_clears_the_provider_and_a_wrong_one_neither_clears_nor_defaults() {
        let mut s = court();
        challenge(&mut s, 10, 1, 200).unwrap();
        let id = open_id(&s, 10, 1);
        let c = chunks();
        // A wrong chunk, a wrong manifest and a late answer change nothing.
        assert_eq!(s.apply(W, CourtEventV1::Answer { challenge: id, manifest: manifest(), chunk: c[0].clone(), now: 300 }), Err(CourtErrorV1::BadOpening));
        let mut foreign = manifest();
        foreign.retention_until_daa += 1;
        assert_eq!(s.apply(W, CourtEventV1::Answer { challenge: id, manifest: foreign, chunk: c[1].clone(), now: 300 }), Err(CourtErrorV1::WrongManifest));
        assert_eq!(
            s.apply(W, CourtEventV1::Answer { challenge: id, manifest: manifest(), chunk: c[1].clone(), now: 200 + W + 1 }),
            Err(CourtErrorV1::TooLate { deadline: 200 + W, now: 200 + W + 1 })
        );
        assert_eq!(s.open_challenges(), 1, "nothing moved");
        // The right one, on the deadline itself, clears.
        let out = s.apply(W, CourtEventV1::Answer { challenge: id, manifest: manifest(), chunk: c[1].clone(), now: 200 + W }).unwrap();
        assert_eq!(out, vec![CourtOutcomeV1::ProviderCleared { challenge: id }]);
        assert_eq!(s.open_challenges(), 0);
        assert!(s.apply(W, CourtEventV1::Sweep { now: 100_000 }).unwrap().is_empty(), "a cleared challenge is never swept into a default");
        assert!(!s.is_charged(&h(CLAIM), &bond(10)));
    }

    #[test]
    fn no_answer_by_the_deadline_charges_that_provider_once_however_many_of_its_chunks_fail() {
        let mut s = court();
        challenge(&mut s, 10, 0, 200).unwrap();
        challenge(&mut s, 10, 1, 205).unwrap();
        assert!(s.apply(W, CourtEventV1::Sweep { now: 200 + W }).unwrap().is_empty(), "the deadline itself is inside");
        let out = s.apply(W, CourtEventV1::Sweep { now: 205 + W + 1 }).unwrap();
        assert_eq!(out, vec![CourtOutcomeV1::ProviderCharged { claim: h(CLAIM), provider: bond(10) }], "two failed chunks, ONE charge; provider 11 untouched");
        assert!(s.is_charged(&h(CLAIM), &bond(10)) && !s.is_charged(&h(CLAIM), &bond(11)));
        assert!(!s.has_lapsed(&h(CLAIM)), "one provider still stands behind the claim's material");
        // Nothing further can be charged to a provider already charged for the claim.
        assert_eq!(challenge(&mut s, 10, 2, 900), Err(CourtErrorV1::AlreadyCharged(h(CLAIM))));
        assert!(s.apply(W, CourtEventV1::Sweep { now: 1_000_000 }).unwrap().is_empty(), "sweeping again does nothing");
    }

    #[test]
    fn when_every_provider_that_filed_has_defaulted_the_claim_lapses_and_that_is_not_the_miners_fraud() {
        let mut s = court();
        challenge(&mut s, 10, 0, 200).unwrap();
        challenge(&mut s, 11, 0, 200).unwrap();
        let out = s.apply(W, CourtEventV1::Sweep { now: 200 + W + 1 }).unwrap();
        assert_eq!(out.len(), 3);
        assert!(out.contains(&CourtOutcomeV1::ProviderCharged { claim: h(CLAIM), provider: bond(10) }));
        assert!(out.contains(&CourtOutcomeV1::ProviderCharged { claim: h(CLAIM), provider: bond(11) }));
        assert!(
            out.contains(&CourtOutcomeV1::ClaimLapsed { claim: h(CLAIM), reason: LapseReason::NoProviderAnswers, miner_fraud: false }),
            "common-mode failure: the claim lapses unpaid, and the outcome says it is not a fraud"
        );
        assert!(s.has_lapsed(&h(CLAIM)));
        assert_eq!(challenge(&mut s, 10, 1, 2_000), Err(CourtErrorV1::ClaimLapsed(h(CLAIM))));
        // Lapsing happens once.
        assert!(s.apply(W, CourtEventV1::Sweep { now: 9_999_999 }).unwrap().is_empty());
    }

    #[test]
    fn a_single_panels_timeout_is_never_evidence_and_moves_nothing() {
        let mut s = court();
        let before = s.clone();
        for _ in 0..50 {
            assert_eq!(s.apply(W, CourtEventV1::PanelLocalTimeout { claim: h(CLAIM) }), Ok(vec![]));
        }
        assert_eq!(s, before, "any number of local fetch failures leaves the court exactly as it was");
        assert!(s.apply(W, CourtEventV1::Sweep { now: 1_000_000 }).unwrap().is_empty(), "and with no challenge there is nothing to default");
    }

    #[test]
    fn one_failure_is_one_partys_never_both() {
        let mut s = court();
        // Under the court: the producer's withholding path is closed for this claim…
        assert!(!s.producer_withholding_applies(&h(CLAIM)));
        assert_eq!(
            s.apply(W, CourtEventV1::ProducerWithholdingAccusation { claim: h(CLAIM) }),
            Err(CourtErrorV1::ProducerNotAccountable(h(CLAIM)))
        );
        // …and for a claim NOT under it the producer path is the live one and this court takes no part.
        assert!(s.producer_withholding_applies(&h(0xD0)));
        assert_eq!(s.apply(W, CourtEventV1::ProducerWithholdingAccusation { claim: h(0xD0) }), Ok(vec![]));
        assert_eq!(evidence_responsibility_v1(false), Responsible::Producer);
        assert_eq!(evidence_responsibility_v1(true), Responsible::Providers);
    }

    #[test]
    fn a_reorg_restores_the_court_exactly_and_two_branches_do_not_share_a_verdict() {
        let base = court();
        let mut a = base.clone();
        challenge(&mut a, 10, 0, 200).unwrap();
        let out_a = a.apply(W, CourtEventV1::Sweep { now: 200 + W + 1 }).unwrap();
        assert!(!out_a.is_empty());
        // The other branch never saw the challenge: its court is the base, untouched, and nothing is charged there.
        let mut b = base.clone();
        assert!(b.apply(W, CourtEventV1::Sweep { now: 200 + W + 1 }).unwrap().is_empty());
        assert_eq!(b, base);
        assert!(!b.is_charged(&h(CLAIM), &bond(10)));
    }

    #[test]
    fn the_pure_outcome_rule_is_the_clocks() {
        let m = manifest();
        let c = chunks();
        let ch = ProviderChallengeV1 {
            network_domain: h(9),
            manifest_id: manifest_id_v1(&m),
            chunk_index: 1,
            provider_id: b"prov-1".to_vec(),
            challenged_at_daa: 100,
            deadline_daa: 200,
        };
        assert_ne!(ch.id(), ProviderChallengeV1 { chunk_index: 2, ..ch.clone() }.id());
        assert_eq!(challenge_outcome_v1(&m, &ch, None, 150), ChallengeOutcome::Pending);
        assert_eq!(challenge_outcome_v1(&m, &ch, Some((&c[1], 199)), 150), ChallengeOutcome::Answered);
        assert_eq!(challenge_outcome_v1(&m, &ch, Some((&c[1], 200)), 300), ChallengeOutcome::Answered);
        assert_eq!(challenge_outcome_v1(&m, &ch, Some((&c[1], 201)), 300), ChallengeOutcome::Defaulted);
        assert_eq!(challenge_outcome_v1(&m, &ch, Some((&c[0], 150)), 150), ChallengeOutcome::Pending);
        assert_eq!(challenge_outcome_v1(&m, &ch, Some((&c[0], 150)), 250), ChallengeOutcome::Defaulted);
    }
}
