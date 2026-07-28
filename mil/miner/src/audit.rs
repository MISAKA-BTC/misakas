//! PALW auditor role (ADR-0039 §10) — the batch **certificate** a single node cannot self-produce.
//!
//! A leaf chunk puts a provider's minted leaves on-chain, but a batch only becomes block-referenceable
//! after an AUDITOR QUORUM certifies it. In the INTENDED design the beacon deterministically selects a
//! slate of bonded auditors, each independently samples the batch's receipts, votes pass/reject, and
//! ML-DSA-87-signs its verdict; consensus caches the batch's certificate once the stake-weighted PASS
//! tally reaches quorum ([`PalwBatchCertificateV2::quorum_reached`]).
//!
//! **ENFORCED (ADR-0040 AUTHSET-01 / SEL-01 / SAMPLE-01, §5.17 — the atomic activation slice).**
//! Consensus now re-derives, at `verify_certificate_attestation`, BOTH the beacon-selected auditor
//! committee (the SEL-01 bond-weighted, credential-aggregated `select_auditor_committee` over the frozen
//! provider-bond view, minus the batch's own providers) and the beacon-selected on-chain leaf sample
//! (`audit_sample_root`, re-derived over the sampled leaves' `receipt_da_root`s), and REJECTS a
//! certificate whose declared `auditor_set_commitment` / `audit_sample_root` disagree or whose votes come
//! from outside the slate. So the beacon-slate clause above is now an ENFORCED fact, and this producer
//! MUST match it: [`select_audit_slate`] composes exactly `select_auditor_committee`, and the round's
//! `audit_sample_root` MUST be [`derive_audit_sample_root`] over the batch's leaves — a producer that
//! emits any other value has its certificate rejected on-chain (silently, since
//! `apply_palw_overlay_effect`'s result is discarded).
//! Also enforced (P1-3): live ML-DSA-87 + stake-weighted quorum, now over the PROVIDER bond's
//! `owner_public_key` + ECON-03 `amount_sompi`. This module is the auditor SIDE of that role — one
//! [`Auditor`] per independent key — plus the quorum assembly a certificate submitter runs. It is NOT a
//! single-operator shortcut: every vote is a real, separately keyed ML-DSA-87 signature over the
//! design-§10.1 binding, so a certificate produced here is indistinguishable from one produced by N
//! physically distinct auditors.
//!
//! Consensus requirements. The stateless
//! `validate_certificate` (transaction isolation) only length-checks each vote signature and requires
//! the canonical `bond_outpoint`-ascending vote order + a sane epoch range. Since ADR-0040 P1-3
//! (CERT-01) the cryptographic half is no longer hypothetical: `verify_certificate_attestation`
//! ML-DSA-87-verifies **every** vote under the bond's registered `validator_pubkey`, requires each
//! voting bond to be ACTIVE at the certifying block's DAA score, binds the declared `approving_stake`
//! to the recomputed PASS tally, and applies the stake-weighted quorum against the ENTIRE re-derived
//! selected slate (including selected auditors that withhold their votes). So this producer is not
//! rehearsing a future check — it is feeding a live one, and construction == validation is a hard
//! requirement: each vote is signed under [`PALW_AUDITOR_V2_MLDSA87_CONTEXT`] (the same `ctx` the verifier
//! passes; FIPS-204 binds `ctx` into the signature, so a mismatch here makes every certificate this
//! module emits unverifiable), and `approving_stake` is declared as exactly the tally the verifier will
//! re-derive. The tests prove both via `verify_mldsa87_with_context`.

use std::collections::HashSet;

#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
use kaspa_consensus_core::palw::PALW_RETIRED_AUDITOR_V1_MLDSA87_CONTEXT;
use kaspa_consensus_core::palw::{
    PALW_AUDITOR_V2_MLDSA87_CONTEXT, PALW_BATCH_CERTIFICATE_VERSION_V2, PALW_MAX_AUDITOR_VOTES_V2, PalwAuditorVoteV2,
    PalwBatchCertificateV2, PalwCredentialStake, PalwPublicLeafV1, ProviderBondView, palw_audit_sample_root,
    palw_deterministic_sample, select_weighted_auditor_committee,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::ValidatorKey;

/// The `0x33` subnetwork byte a batch-certificate PALW TX output carries (mirrors
/// `PalwTxKind::from_subnetwork_byte(0x33) == BatchCertificate`).
pub const BATCH_CERTIFICATE_SUBNETWORK_BYTE: u8 = 0x33;

/// Why a certificate could not be assembled.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AuditError {
    #[error("a certificate needs 1..={max} votes, got {got}")]
    VoteCount { got: usize, max: usize },
    #[error("two votes share a bond outpoint (votes must be over distinct auditors)")]
    DuplicateAuditor,
    #[error("a vote names a bond outside the selected credential slate")]
    VoteOutsideSlate,
    #[error("vote from {bond} does not bind the audit round's passed/rejected summary")]
    VoteSummaryMismatch { bond: TransactionOutpoint },
    #[error("degenerate certificate epochs: need audit_beacon_epoch <= certificate_epoch < activation_epoch < expiry_epoch")]
    EpochRange,
    #[error("passed_leaf_count must be > 0")]
    NoPassedLeaves,
    #[error("the PASS tally did not reach the {num}/{den} stake quorum")]
    QuorumNotReached { num: u16, den: u16 },
    #[error("borsh encoding failed")]
    Encode,
}

/// One auditor's identity + verdict on a batch. `key` is the auditor's ML-DSA-87 validator key (its
/// bond's key); `bond` the DNS bond outpoint whose stake weights the vote; `pass` the sampled verdict;
/// `checked_leaf_bitmap_root` the commitment to WHICH leaves this auditor sampled (design §10.1).
pub struct Auditor {
    pub key: ValidatorKey,
    pub bond: TransactionOutpoint,
    pub pass: bool,
    pub checked_leaf_bitmap_root: Hash64,
}

/// The shared audit-round facts every vote in a certificate binds to (design §10.1) and the
/// certificate's own window/commitment fields. `audit_sample_root` MUST be [`derive_audit_sample_root`]
/// over the batch's on-chain leaves — consensus re-derives it and rejects any other value (SAMPLE-01,
/// §5.17.6); it is no longer a free off-protocol input. `leaf_root` / `manifest_hash` are copied from the
/// batch's manifest; `auditor_set_commitment` MUST be [`select_audit_slate`]'s commitment (AUTHSET-01).
///
/// **kaspa-pq ADR-0040 §5.15.9 step (iii) — `leaf_root` is a PASS-THROUGH, and that is the hazard.**
/// This module never computes a leaf root; [`assemble_certificate`] copies this field verbatim onto
/// `PalwBatchCertificateV2::leaf_root`, which consensus cross-binds as
/// `cert.leaf_root == manifest.leaf_root` (consensus/src/processes/palw.rs:387). So the auditor moved to
/// the §5.15.4 Merkle construction the moment `manifest_leaf_root` did — provided, and only provided,
/// that every caller fills this from the batch's ACTUAL manifest. A caller that puts a placeholder here
/// produces certificates that are refused with no error surfaced anywhere, because
/// `apply_palw_overlay_effect`'s result is discarded at virtual_processor/processor.rs:1800-1801. Fill
/// it from `manifest.leaf_root`; never from a literal.
#[derive(Clone, Debug)]
pub struct AuditRound {
    pub network_id: u32,
    pub batch_id: Hash64,
    pub manifest_hash: Hash64,
    pub leaf_root: Hash64,
    pub audit_beacon_epoch: u64,
    pub audit_sample_root: Hash64,
    pub passed_leaf_count: u32,
    pub rejected_leaf_bitmap_root: Hash64,
    pub certificate_epoch: u64,
    pub activation_epoch: u64,
    pub expiry_epoch: u64,
    pub auditor_set_commitment: Hash64,
}

/// The stake-quorum fraction a certificate must reach (testnet 2/3). `pass_stake · den >= total · num`.
#[derive(Clone, Copy, Debug)]
pub struct QuorumPolicy {
    pub num: u16,
    pub den: u16,
}

/// An assembled quorum certificate ready to become a PALW TX output tagged
/// [`BATCH_CERTIFICATE_SUBNETWORK_BYTE`].
#[derive(Clone, Debug)]
pub struct AuditCertificate {
    /// [`BATCH_CERTIFICATE_SUBNETWORK_BYTE`].
    pub subnetwork_byte: u8,
    /// `borsh(cert)` — the bytes the certificate PALW TX output carries.
    pub payload: Vec<u8>,
    pub cert: PalwBatchCertificateV2,
}

/// The beacon-selected auditor committee for `batch_id` under the prior-epoch beacon seed, plus the
/// canonical commitment over it — a DIRECT composition of the consensus verifier's own selector
/// [`select_weighted_auditor_committee`] (SEL-01 bond-weighted, credential-aggregated, non-replacement sampling
/// over the frozen provider-bond view), so the producer and `verify_certificate_attestation` compute the
/// identical `auditor_set_commitment` by construction. `excluded_credentials` / `excluded_operator_groups`
/// are the §5.17.4 exclusions the verifier derives from the batch's leaves (its own providers + operator
/// siblings); the caller passes the SAME sets.
///
/// AUTHSET-01 binding (ADR-0040 §5.17). `verify_certificate_attestation`
/// re-derives this committee and REJECTS a certificate whose declared `auditor_set_commitment` differs or
/// whose votes fall outside the slate — so a certificate whose auditors are an arbitrary bonded set no
/// longer verifies. Delegating (rather than re-implementing) the selection is what keeps the producer
/// from drifting: there is one selector, called by both sides.
pub fn select_audit_slate(
    prev_seed: &Hash64,
    batch_id: &Hash64,
    provider_bond_view: &ProviderBondView,
    pov_daa_score: u64,
    excluded_credentials: &HashSet<Hash64>,
    excluded_operator_groups: &HashSet<Hash64>,
    committee_size: usize,
) -> (Vec<PalwCredentialStake>, Hash64) {
    select_weighted_auditor_committee(
        prev_seed,
        batch_id,
        provider_bond_view,
        pov_daa_score,
        excluded_credentials,
        excluded_operator_groups,
        committee_size,
    )
}

/// The re-derived `audit_sample_root` a certificate MUST carry (SAMPLE-01, ADR-0040 §5.17.6) — a DIRECT
/// composition of the verifier's own primitives, so the producer emits exactly the value
/// `verify_certificate_attestation` re-derives and REJECTS any mismatch of. `leaves` are the batch's
/// on-chain public leaves in index order `[0, leaf_count)`; the beacon seed + `sample_size` select which
/// of them the round covers, and the root commits to exactly those leaves' `receipt_da_root`s.
///
/// A producer that supplies any other `audit_sample_root` — including the old free "off-protocol receipt
/// sample" value — has its certificate rejected on-chain. This is the §5.17.6 REDEFINITION: an on-chain
/// DA-commitment covering, strictly weaker than off-chain receipt-chunk possession (I-14) but the strongest
/// property consensus can re-derive.
pub fn derive_audit_sample_root(prev_seed: &Hash64, batch_id: &Hash64, leaves: &[PalwPublicLeafV1], sample_size: u32) -> Hash64 {
    let sampled = palw_deterministic_sample(prev_seed, batch_id, leaves.len() as u32, sample_size);
    let roots: Vec<Hash64> = sampled.iter().map(|&i| leaves[i as usize].receipt_da_root).collect();
    palw_audit_sample_root(&roots)
}

/// Produce one auditor's signed vote for `round`. The signature is a real ML-DSA-87 signature over
/// [`PalwAuditorVoteV2::signing_hash`] under [`PALW_AUDITOR_V2_MLDSA87_CONTEXT`] — the exact digest +
/// context the audit-slice verifier checks. Binds the vote to the batch, the audit-beacon epoch, the
/// beacon-selected `audit_sample_root`, the auditor's bond, which leaves it checked, and the exact
/// certificate-level passed/rejected summary (SUMMARY-BIND V2).
pub fn sign_vote(round: &AuditRound, auditor: &Auditor) -> PalwAuditorVoteV2 {
    let mut vote = PalwAuditorVoteV2 {
        bond_outpoint: auditor.bond,
        vote: u8::from(auditor.pass),
        checked_leaf_bitmap_root: auditor.checked_leaf_bitmap_root,
        passed_leaf_count: round.passed_leaf_count,
        rejected_leaf_bitmap_root: round.rejected_leaf_bitmap_root,
        signature: Vec::new(),
    };
    let digest = vote.signing_hash(round.network_id, &round.batch_id, round.audit_beacon_epoch, &round.audit_sample_root);
    vote.signature = auditor.key.sign_with_context(&digest.as_bytes(), PALW_AUDITOR_V2_MLDSA87_CONTEXT).to_vec();
    vote
}

/// Canonical `bond_outpoint` order — the strict ascending order the stateless `validate_certificate`
/// requires (`transaction_id` bytes, then `index`; mirrors the consensus `cmp_outpoint`).
fn cmp_bond(a: &TransactionOutpoint, b: &TransactionOutpoint) -> std::cmp::Ordering {
    a.transaction_id.as_byte_slice().cmp(b.transaction_id.as_byte_slice()).then(a.index.cmp(&b.index))
}

/// Assemble a quorum certificate from already-signed `votes`. Sorts the votes into the canonical
/// `bond_outpoint`-ascending order, rejects duplicate auditors, checks the certificate epoch range and
/// `passed_leaf_count`, and verifies the stake-weighted PASS tally reaches `quorum` against
/// the full credential-aggregated `selected_slate` stake. The slate is explicit so missing votes cannot
/// silently shrink the denominator, and retains the same weights that drove selection. Returns the
/// borsh-encoded certificate ready to submit.
///
/// The result passes the stateless `validate_certificate` (1..=64 canonically-ordered votes, valid
/// signature lengths, sane epochs) AND [`PalwBatchCertificateV2::quorum_reached`] — the two gates a
/// real certificate must clear. Public Header-v4 additionally admits only an all-pass summary
/// (`passed_leaf_count == manifest.leaf_count`, zero rejected root) because V2 has no per-ticket
/// non-rejection witness; the operator-facing validator wrapper enforces that network-specific rule.
pub fn assemble_certificate(
    round: &AuditRound,
    mut votes: Vec<PalwAuditorVoteV2>,
    selected_slate: &[PalwCredentialStake],
    quorum: QuorumPolicy,
) -> Result<AuditCertificate, AuditError> {
    if votes.is_empty() || votes.len() > PALW_MAX_AUDITOR_VOTES_V2 {
        return Err(AuditError::VoteCount { got: votes.len(), max: PALW_MAX_AUDITOR_VOTES_V2 });
    }
    if round.passed_leaf_count == 0 {
        return Err(AuditError::NoPassedLeaves);
    }
    if let Some(vote) = votes.iter().find(|vote| {
        vote.passed_leaf_count != round.passed_leaf_count || vote.rejected_leaf_bitmap_root != round.rejected_leaf_bitmap_root
    }) {
        return Err(AuditError::VoteSummaryMismatch { bond: vote.bond_outpoint });
    }
    // The exact epoch chain the stateless validator requires.
    if !(round.audit_beacon_epoch <= round.certificate_epoch
        && round.certificate_epoch < round.activation_epoch
        && round.activation_epoch < round.expiry_epoch)
    {
        return Err(AuditError::EpochRange);
    }
    votes.sort_by(|x, y| cmp_bond(&x.bond_outpoint, &y.bond_outpoint));
    if votes.windows(2).any(|w| w[0].bond_outpoint == w[1].bond_outpoint) {
        return Err(AuditError::DuplicateAuditor);
    }
    let stake_of = |bond: &TransactionOutpoint| -> u128 {
        selected_slate.iter().find(|member| member.representative == *bond).map(|member| member.weight).unwrap_or(0)
    };
    if votes.iter().any(|vote| stake_of(&vote.bond_outpoint) == 0) {
        return Err(AuditError::VoteOutsideSlate);
    }
    // ADR-0040 §12′ — `approving_stake` is a COMMITMENT, not a free input: consensus
    // (`verify_certificate_attestation` step 4) recomputes the PASS tally from the active bond view and
    // rejects any certificate whose declared value disagrees. So the producer must declare exactly the
    // tally the verifier will derive — construction == validation. Computed here over the already
    // sorted/deduplicated votes, which is the same multiset the verifier walks.
    let approving_stake: u128 = votes.iter().filter(|v| v.vote == 1).map(|v| stake_of(&v.bond_outpoint)).sum();
    let cert = PalwBatchCertificateV2 {
        version: PALW_BATCH_CERTIFICATE_VERSION_V2,
        batch_id: round.batch_id,
        manifest_hash: round.manifest_hash,
        leaf_root: round.leaf_root,
        audit_beacon_epoch: round.audit_beacon_epoch,
        audit_sample_root: round.audit_sample_root,
        passed_leaf_count: round.passed_leaf_count,
        rejected_leaf_bitmap_root: round.rejected_leaf_bitmap_root,
        certificate_epoch: round.certificate_epoch,
        activation_epoch: round.activation_epoch,
        expiry_epoch: round.expiry_epoch,
        auditor_set_commitment: round.auditor_set_commitment,
        approving_stake,
        votes,
    };
    let total_slate_stake = selected_slate.iter().fold(0u128, |total, member| total.saturating_add(member.weight));
    if !cert.quorum_reached(total_slate_stake, quorum.num, quorum.den, stake_of) {
        return Err(AuditError::QuorumNotReached { num: quorum.num, den: quorum.den });
    }
    let payload = borsh::to_vec(&cert).map_err(|_| AuditError::Encode)?;
    Ok(AuditCertificate { subnetwork_byte: BATCH_CERTIFICATE_SUBNETWORK_BYTE, payload, cert })
}

/// Run a full audit round over an independent set of `auditors`: each signs its vote for `round`, the
/// votes are assembled into a quorum certificate, and the stake-weighted PASS tally is checked against
/// the total stake of the eligible slate. `selected_slate` includes EVERY selected credential with its
/// aggregate stake, including selected auditors that did not submit a vote; the total is summed over
/// this full slate, never over `auditors`. This is the certificate-submitter's driver;
/// the per-auditor [`sign_vote`] runs on each auditor's own machine in the network.
pub fn run_audit_round(
    round: &AuditRound,
    auditors: &[Auditor],
    selected_slate: &[PalwCredentialStake],
    quorum: QuorumPolicy,
) -> Result<AuditCertificate, AuditError> {
    let votes: Vec<PalwAuditorVoteV2> = auditors.iter().map(|a| sign_vote(round, a)).collect();
    assemble_certificate(round, votes, selected_slate, quorum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registration::tests::CROSS_CRATE_GOLDEN_LEAF_ROOT;
    use crate::registration::{BatchPolicy, build_batch_manifest};
    use kaspa_consensus_core::palw::{
        PalwBatchManifestV1, PalwProviderBondRecord, PalwPublicLeafV1, auditor_set_commitment, validate_palw_overlay_payload,
    };
    use kaspa_txscript::verify_mldsa87_with_context;

    const NET: u32 = 0x9107;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn op(n: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(h(n), n as u32)
    }

    /// A provider-bond view over the given bond bytes, each a DISTINCT credential + operator group, all
    /// Active at pov 0 with equal weight — so a committee sized ≥ the pool selects every one and the
    /// representative outpoints (hence the commitment) are exactly the input bonds.
    fn provider_view(bonds: &[u8]) -> ProviderBondView {
        ProviderBondView::from_records(bonds.iter().map(|&b| {
            let outpoint = op(b);
            (
                outpoint,
                PalwProviderBondRecord {
                    version: 1,
                    bond_outpoint: outpoint,
                    owner_pubkey_hash: h(b ^ 0xC0),
                    owner_public_key: vec![],
                    operator_group_id: h(b ^ 0xE0),
                    runtime_classes: vec![],
                    capacity_by_shape: vec![],
                    reward_key_root: h(b),
                    amount_sompi: 100,
                    activation_daa_score: 0,
                    created_daa_score: 0,
                    unbond_delay_epochs: 0,
                    unbond_request_daa_score: None,
                    slashed_at_daa_score: None,
                },
            )
        }))
    }

    /// A distinctly-seeded auditor keyed by its seed byte, bonded at `op(seed)`, voting `pass`.
    fn auditor(seed: u8, pass: bool) -> Auditor {
        Auditor { key: ValidatorKey::from_seed([seed; 32]), bond: op(seed), pass, checked_leaf_bitmap_root: h(seed ^ 0x5A) }
    }

    fn slate_from_stakes(stakes: &HashMap<TransactionOutpoint, u128>) -> Vec<PalwCredentialStake> {
        let mut slate: Vec<_> = stakes
            .iter()
            .map(|(bond, weight)| PalwCredentialStake { credential: bond.transaction_id, weight: *weight, representative: *bond })
            .collect();
        slate.sort_by(|a, b| cmp_bond(&a.representative, &b.representative));
        slate
    }

    /// The manifest every certificate field below is DERIVED from — built by the REAL miner producer
    /// ([`crate::registration::build_batch_manifest`]) over the shared cross-crate golden leaf fixture,
    /// not hand-assembled here.
    ///
    /// kaspa-pq ADR-0040 §5.15.10 flags the literals this replaces: the fixture used to write
    /// `manifest_hash: h(0x11)` / `leaf_root: h(0x22)`, so it could not detect a change in the shape of
    /// `leaf_root` and would have kept passing while the same producer path was rejected on-chain with
    /// `CertificateManifestMismatch` / `CertificateLeafRootMismatch`. Deriving instead of substituting
    /// is the §5.15.9 rule for rebuilt fixtures — and calling the producer is a stronger derivation than
    /// re-implementing what the producer does, because it cannot drift from it.
    ///
    /// The policy reproduces the fixture's original ACTIVATION/EXPIRY epochs exactly (activation 7,
    /// expiry 13), so nothing about the audit round's timing semantics changed with this rebuild.
    ///
    /// kaspa-pq **ADR-0040 §5.14.3 item 7** — the registration epoch itself had to move, from `5` to
    /// `FIXTURE_REGISTRATION_EPOCH`. `golden_leaf` is the CROSS-CRATE GOLDEN fixture: its
    /// `registered_epoch` sits inside the pinned leaf-hash vector, so moving the LEAF to meet the policy
    /// would be a re-genesis (see `manifest_leaf_root_is_pinned_to_the_consensus_cross_crate_golden_
    /// vector`). The policy is the side that is free to move, and the lead absorbs the difference:
    /// `registration 3 + lead 3 + audit 1 ⇒ activation 7`, `+ active window 6 ⇒ expiry 13`. The assert
    /// below is what holds that arithmetic in place.
    fn certified_manifest() -> PalwBatchManifestV1 {
        let leaves: Vec<PalwPublicLeafV1> = (0..3u32).map(crate::registration::tests::golden_leaf).collect();
        let policy = BatchPolicy {
            registration_epoch: crate::registration::tests::FIXTURE_REGISTRATION_EPOCH,
            registration_lead_epochs: 3,
            audit_window_epochs: 1,
            active_window_epochs: 6,
            min_leaf_bond_sompi: 0,
            max_batch_leaves: kaspa_consensus_core::palw::PALW_MAX_BATCH_LEAVES_V1 as u32,
        };
        let (_batch_id, (_byte, payload)) =
            build_batch_manifest(&leaves, h(1), h(2), h(3), h(4), 0, &policy).expect("the golden fixture is a valid batch");
        let m: PalwBatchManifestV1 = borsh::from_slice(&payload).expect("manifest decodes");
        assert_eq!((m.activation_not_before_epoch, m.expiry_epoch), (7, 13), "the policy must reproduce the fixture's epochs");
        m
    }

    fn round(auditor_set_commitment: Hash64) -> AuditRound {
        let m = certified_manifest();
        AuditRound {
            network_id: NET,
            batch_id: m.batch_id,
            manifest_hash: m.content_id(),
            leaf_root: m.leaf_root,
            audit_beacon_epoch: 5,
            audit_sample_root: h(0x33),
            // The golden fixture is a 3-leaf batch (3 is not a power of two — that is why it is the
            // golden), and an all-pass round passes every leaf.
            passed_leaf_count: 3,
            rejected_leaf_bitmap_root: h(0x44),
            certificate_epoch: 6,
            activation_epoch: 7,
            expiry_epoch: 13,
            auditor_set_commitment,
        }
    }

    /// The centerpiece: three independently-keyed auditors sign real ML-DSA-87 votes; the assembled
    /// certificate passes the exact stateless validator the mempool/body runs, EACH vote verifies under
    /// the auditor's own pubkey + the audit-vote context (the audit-slice check), and the stake-weighted
    /// quorum is reached.
    #[test]
    fn independent_auditors_form_a_certificate_that_validates_verifies_and_reaches_quorum() {
        let auditors = [auditor(0x11, true), auditor(0x22, true), auditor(0x33, true)];
        // Bind the certificate to this exact slate, re-derived by the SEL-01 weighted selector (the same
        // one the consensus verifier uses) over a provider-bond view of the SAME batch's auditors.
        let manifest = certified_manifest();
        let view = provider_view(&[0x11, 0x22, 0x33]);
        let empty: HashSet<Hash64> = HashSet::new();
        let (slate, set_commit) = select_audit_slate(&h(0x99), &manifest.batch_id, &view, 0, &empty, &empty, 3);
        let r = AuditRound { auditor_set_commitment: set_commit, ..round(set_commit) };

        let ac = run_audit_round(&r, &auditors, &slate, QuorumPolicy { num: 2, den: 3 }).expect("quorum certificate");
        assert_eq!(ac.subnetwork_byte, BATCH_CERTIFICATE_SUBNETWORK_BYTE);
        assert_eq!(ac.cert.version, PALW_BATCH_CERTIFICATE_VERSION_V2);

        // (1) The stateless certificate validator the mempool/body runs accepts it.
        assert_eq!(validate_palw_overlay_payload(ac.subnetwork_byte, &ac.payload), Ok(()));

        // (2) Every vote is a genuine ML-DSA-87 signature over the §10.1 binding, verifiable under the
        //     auditor's own pubkey + the audit-vote context — the exact call the audit slice makes.
        assert_eq!(ac.cert.votes.len(), 3);
        for vote in &ac.cert.votes {
            assert_eq!(vote.passed_leaf_count, r.passed_leaf_count);
            assert_eq!(vote.rejected_leaf_bitmap_root, r.rejected_leaf_bitmap_root);
            // votes are re-sorted in the cert; match each back to its auditor by bond.
            let signer = auditors.iter().find(|x| x.bond == vote.bond_outpoint).expect("vote maps to an auditor");
            let digest = vote.signing_hash(NET, &r.batch_id, r.audit_beacon_epoch, &r.audit_sample_root);
            assert_eq!(
                verify_mldsa87_with_context(
                    signer.key.public_key(),
                    &digest.as_bytes(),
                    &vote.signature,
                    PALW_AUDITOR_V2_MLDSA87_CONTEXT
                ),
                Ok(true),
                "vote from {:?} must verify",
                vote.bond_outpoint
            );
            assert_ne!(
                verify_mldsa87_with_context(
                    signer.key.public_key(),
                    &digest.as_bytes(),
                    &vote.signature,
                    PALW_RETIRED_AUDITOR_V1_MLDSA87_CONTEXT
                ),
                Ok(true),
                "a V2 vote signature must not verify under the retired V1 context"
            );
        }

        // (3) The certificate independently reaches the 2/3 stake quorum.
        assert!(ac.cert.quorum_reached(300, 2, 3, |bond| {
            slate.iter().find(|member| member.representative == *bond).map(|member| member.weight).unwrap_or(0)
        }));

        // (4) kaspa-pq ADR-0040 §5.15.12 (CROSS-CRATE GOLDEN, auditor half) — the certificate carries
        //     exactly the three values consensus cross-binds to the manifest at
        //     consensus/src/processes/palw.rs:384-389. This is the SECOND silent-death path: a drift here
        //     makes every certificate rejected with no error surfaced anywhere, because
        //     `apply_palw_overlay_effect`'s result is discarded (`let _ =`,
        //     virtual_processor/processor.rs:1800-1801).
        //
        //     `manifest` here is built by the REAL miner producer (`build_batch_manifest`), so this is a
        //     genuine producer-to-producer tie: the auditor's certificate is checked against what the
        //     miner actually registers, not against a re-implementation of it.
        assert_eq!(ac.cert.batch_id, manifest.batch_id, "CertBatchMismatch on-chain otherwise");
        assert_eq!(ac.cert.manifest_hash, manifest.content_id(), "CertificateManifestMismatch on-chain otherwise");
        assert_eq!(ac.cert.leaf_root, manifest.leaf_root, "CertificateLeafRootMismatch on-chain otherwise");
        //     ...and that shared value is the pinned cross-crate golden root. Without this line the
        //     three assertions above are satisfied by ANY consistent pair — including a pair where the
        //     miner has silently reverted to the retired FLAT reduction and the auditor has faithfully
        //     copied the wrong root. Consistency is not correctness; the constant is.
        assert_eq!(
            ac.cert.leaf_root.to_string(),
            CROSS_CRATE_GOLDEN_LEAF_ROOT,
            "the auditor is certifying a root that is not the pinned ADR-0040 §5.15.4 Merkle root — \
             consensus will refuse every certificate for this batch, silently"
        );
        assert!(manifest.batch_id_is_content_derived(), "leaf_root sits inside content_id, so batch_id moves with it");
        // The borsh payload round-trips to the same certificate.
        let decoded: PalwBatchCertificateV2 = borsh::from_slice(&ac.payload).unwrap();
        assert_eq!(decoded, ac.cert);
    }

    /// Votes fed in any order are assembled into the strict `bond_outpoint`-ascending order the
    /// stateless validator requires.
    #[test]
    fn votes_are_canonically_ordered_regardless_of_input() {
        let auditors = [auditor(0x33, true), auditor(0x11, true), auditor(0x22, true)];
        let stakes: HashMap<_, _> = auditors.iter().map(|a| (a.bond, 100u128)).collect();
        let set_commit = auditor_set_commitment(&auditors.iter().map(|a| a.bond).collect::<Vec<_>>());
        let ac = run_audit_round(&round(set_commit), &auditors, &slate_from_stakes(&stakes), QuorumPolicy { num: 2, den: 3 }).unwrap();
        // Strictly ascending by (transaction_id bytes, index).
        assert!(ac.cert.votes.windows(2).all(|w| cmp_bond(&w[0].bond_outpoint, &w[1].bond_outpoint) == std::cmp::Ordering::Less));
        assert_eq!(validate_palw_overlay_payload(ac.subnetwork_byte, &ac.payload), Ok(()));
    }

    #[test]
    fn certificate_assembly_rejects_votes_bound_to_another_summary() {
        let auditors = [auditor(0x11, true), auditor(0x22, true), auditor(0x33, true)];
        let stakes: HashMap<_, _> = auditors.iter().map(|a| (a.bond, 100u128)).collect();
        let r = round(auditor_set_commitment(&auditors.iter().map(|a| a.bond).collect::<Vec<_>>()));
        let mut votes: Vec<_> = auditors.iter().map(|auditor| sign_vote(&r, auditor)).collect();
        votes[0].passed_leaf_count -= 1;
        let err = assemble_certificate(&r, votes, &slate_from_stakes(&stakes), QuorumPolicy { num: 2, den: 3 }).unwrap_err();
        assert_eq!(err, AuditError::VoteSummaryMismatch { bond: auditors[0].bond });
    }

    /// Below quorum: a lone submitted PASS vote among a three-auditor selected slate (only 100 of 300
    /// stake) fails the 2/3 quorum. The two omitted votes remain in the denominator.
    #[test]
    fn omitted_selected_votes_do_not_shrink_the_producer_quorum_denominator() {
        // One passing auditor submits; two other selected auditors withhold.
        let passing = auditor(0x11, true);
        let stakes: HashMap<_, _> = [(op(0x11), 100u128), (op(0x22), 100), (op(0x33), 100)].into_iter().collect();
        let r = round(auditor_set_commitment(&[op(0x11), op(0x22), op(0x33)]));
        // The producer uses all `stakes` entries as the selected-slate denominator:
        // 100 PASS · 3 < 300 selected · 2.
        let err = run_audit_round(&r, std::slice::from_ref(&passing), &slate_from_stakes(&stakes), QuorumPolicy { num: 2, den: 3 })
            .unwrap_err();
        assert_eq!(err, AuditError::QuorumNotReached { num: 2, den: 3 });
    }

    /// A reject vote (`vote = 0`) contributes no PASS stake; a slate that only rejects cannot certify.
    #[test]
    fn reject_votes_do_not_count_toward_quorum() {
        let rejecting = [auditor(0x11, false), auditor(0x22, false)];
        let stakes: HashMap<_, _> = rejecting.iter().map(|a| (a.bond, 100u128)).collect();
        let r = round(auditor_set_commitment(&rejecting.iter().map(|a| a.bond).collect::<Vec<_>>()));
        let err = run_audit_round(&r, &rejecting, &slate_from_stakes(&stakes), QuorumPolicy { num: 2, den: 3 }).unwrap_err();
        assert_eq!(err, AuditError::QuorumNotReached { num: 2, den: 3 });
    }

    /// A degenerate epoch chain is rejected before any certificate is emitted.
    #[test]
    fn degenerate_epoch_range_is_rejected() {
        let a = auditor(0x11, true);
        let stakes: HashMap<_, _> = [(a.bond, 100u128)].into_iter().collect();
        let bad = AuditRound { activation_epoch: 6, certificate_epoch: 6, ..round(h(0)) }; // certificate_epoch !< activation_epoch
        let err =
            run_audit_round(&bad, std::slice::from_ref(&a), &slate_from_stakes(&stakes), QuorumPolicy { num: 1, den: 1 }).unwrap_err();
        assert_eq!(err, AuditError::EpochRange);
    }

    /// Auditor selection is deterministic and its commitment is exactly `auditor_set_commitment` over the
    /// selected slate — and it composes the consensus verifier's `select_weighted_auditor_committee`, so the
    /// producer and verifier cannot drift.
    #[test]
    fn slate_selection_is_deterministic_and_commits_to_the_selected_set() {
        let view = provider_view(&[1, 2, 3, 4, 5]);
        let empty: HashSet<Hash64> = HashSet::new();
        let (slate_a, commit_a) = select_audit_slate(&h(0x77), &h(0x42), &view, 0, &empty, &empty, 3);
        let (slate_b, commit_b) = select_audit_slate(&h(0x77), &h(0x42), &view, 0, &empty, &empty, 3);
        assert_eq!(slate_a, slate_b, "deterministic");
        assert_eq!(commit_a, commit_b);
        assert_eq!(slate_a.len(), 3, "committee_size draws exactly 3 of the 5 candidates");
        let bonds: Vec<_> = slate_a.iter().map(|member| member.representative).collect();
        assert_eq!(commit_a, auditor_set_commitment(&bonds), "the commitment is over exactly the selected slate");
        // A different beacon seed yields a different slate/commitment.
        let (_slate_c, commit_c) = select_audit_slate(&h(0x88), &h(0x42), &view, 0, &empty, &empty, 3);
        assert_ne!(commit_a, commit_c);
    }

    /// SAMPLE-01 (ADR-0040 §5.17.6) — [`derive_audit_sample_root`] is the producer half of the on-chain
    /// sample-root redefinition: deterministic, seed-sensitive, and equal to the consensus primitive
    /// `palw_audit_sample_root` over the beacon-selected leaves' `receipt_da_root`s (the value the
    /// verifier re-derives and rejects any mismatch of).
    #[test]
    fn derive_audit_sample_root_matches_the_consensus_primitive() {
        let mut leaves: Vec<PalwPublicLeafV1> = (0..4u32).map(crate::registration::tests::golden_leaf).collect();
        for (i, leaf) in leaves.iter_mut().enumerate() {
            leaf.receipt_da_root = h(0xD0 + i as u8);
        }
        let seed = h(0x99);
        let batch = h(0x42);
        // Deterministic and seed-sensitive.
        assert_eq!(derive_audit_sample_root(&seed, &batch, &leaves, 2), derive_audit_sample_root(&seed, &batch, &leaves, 2));
        assert_ne!(derive_audit_sample_root(&seed, &batch, &leaves, 2), derive_audit_sample_root(&h(0x9a), &batch, &leaves, 2));
        // Equal to the verifier's re-derivation over the SAME sampled indices — the producer/verifier tie.
        let sampled = palw_deterministic_sample(&seed, &batch, leaves.len() as u32, 2);
        let roots: Vec<Hash64> = sampled.iter().map(|&i| leaves[i as usize].receipt_da_root).collect();
        assert_eq!(derive_audit_sample_root(&seed, &batch, &leaves, 2), palw_audit_sample_root(&roots));
    }

    // =====================================================================================================
    // ADR-0040 §5.17 — the CROSS-CRATE GOLDEN, PRODUCER side (Phase 4).
    //
    // These two constants are BYTE-FOR-BYTE the ones the consensus verifier's crate pins in
    // `kaspa-consensus-core` (`palw.rs::tests::cross_crate_golden_auditor_set_and_sample_root`). The
    // consensus `verify_certificate_attestation` re-derives a certificate's `auditor_set_commitment` and
    // `audit_sample_root`; this producer must emit exactly those values or every certificate it builds is
    // rejected on-chain, SILENTLY (the `apply_palw_overlay_effect` error is discarded). Pinning the SAME
    // literals on both sides makes producer/verifier drift — including a silent change to the shared
    // consensus-core selector/sampler that would otherwise move both sides together — break the build
    // LOUDLY in BOTH crates, exactly like `CROSS_CRATE_GOLDEN_LEAF_ROOT` does for `leaf_root`.
    //
    // The fixture is the same shared fixture: five distinct-credential provider bonds (committee 3 of 5)
    // and four leaves with distinct `receipt_da_root`s (sample 2 of 4), built from the SAME explicit
    // literals so the two crates construct a byte-identical view and DA-root set.
    // =====================================================================================================

    /// If either of these fails, the consensus-core golden
    /// (`cross_crate_golden_auditor_set_and_sample_root`) must be inspected too: the producer and verifier
    /// have diverged, or the shared selector/sampler moved.
    const CROSS_CRATE_GOLDEN_AUDITOR_SET_COMMITMENT: &str = "f6b70c92baebadc4849b4f0ce44b1d166989f340b8d6d95cfbd30e51236161eb\
         372c28580814c3c202fc406fd8e901bddfd8703950f0a3bd28179fd89095980d";
    const CROSS_CRATE_GOLDEN_AUDIT_SAMPLE_ROOT: &str = "6abe582463e5bbb8e654ae3e0bab5aad4a2d0dfd8cec49daf3a497b1e71dec8a\
         49605cedc7d3349f5c01bc60255df01c4f39628a4f28d1987130dd11dff2e852";

    const GOLDEN_SEED: u8 = 0xC0;
    const GOLDEN_BATCH: u8 = 0x42;
    const GOLDEN_POV: u64 = 1_000;
    const GOLDEN_COMMITTEE_SIZE: usize = 3;
    const GOLDEN_SAMPLE_SIZE: u32 = 2;

    /// `(credential, operator_group, amount_sompi)` — the SAME rows the consensus-core golden's
    /// `cross_crate_golden_provider_records` builds, with bond `TransactionOutpoint::new(h(cred), 0)`.
    const GOLDEN_ROWS: [(u8, u8, u64); 5] =
        [(0x71, 0x81, 500_000), (0x72, 0x82, 400_000), (0x73, 0x83, 300_000), (0x74, 0x84, 200_000), (0x75, 0x85, 100_000)];

    /// The shared golden provider-bond view, producer side. Every field is set to the same literal the
    /// consensus-core fixture uses, so `select_audit_slate` here and `select_auditor_committee` there see a
    /// byte-identical view.
    fn golden_provider_view() -> ProviderBondView {
        ProviderBondView::from_records(GOLDEN_ROWS.into_iter().map(|(cred, grp, amount)| {
            let bond = TransactionOutpoint::new(h(cred), 0);
            (
                bond,
                PalwProviderBondRecord {
                    version: 1,
                    bond_outpoint: bond,
                    owner_pubkey_hash: h(cred),
                    owner_public_key: vec![],
                    operator_group_id: h(grp),
                    runtime_classes: vec![],
                    capacity_by_shape: vec![],
                    reward_key_root: Hash64::default(),
                    amount_sompi: amount,
                    activation_daa_score: 0,
                    created_daa_score: 0,
                    unbond_delay_epochs: 10,
                    unbond_request_daa_score: None,
                    slashed_at_daa_score: None,
                },
            )
        }))
    }

    /// The shared golden batch's four leaves, producer side: distinct `receipt_da_root`s h(0xD0..0xD3),
    /// the SAME four roots the consensus-core `cross_crate_golden_receipt_da_roots` lists.
    fn golden_leaves() -> Vec<PalwPublicLeafV1> {
        (0..4u32)
            .map(|i| {
                let mut leaf = crate::registration::tests::golden_leaf(i);
                leaf.receipt_da_root = h(0xD0 + i as u8);
                leaf
            })
            .collect()
    }

    /// Producer-side cross-crate vector. The miner-built certificate's `auditor_set_commitment`
    /// and `audit_sample_root` equal the consensus-re-derived values pinned above — proving the producer
    /// and `verify_certificate_attestation` agree for the shared fixture, so any drift breaks the build.
    #[test]
    fn cross_crate_golden_certificate_matches_consensus_rederivation() {
        let empty: HashSet<Hash64> = HashSet::new();
        let seed = h(GOLDEN_SEED);
        let batch = h(GOLDEN_BATCH);

        // (1) The producer's committee selector emits the pinned consensus commitment.
        let view = golden_provider_view();
        let (slate, commitment) = select_audit_slate(&seed, &batch, &view, GOLDEN_POV, &empty, &empty, GOLDEN_COMMITTEE_SIZE);
        assert_eq!(
            commitment.to_string(),
            CROSS_CRATE_GOLDEN_AUDITOR_SET_COMMITMENT,
            "select_audit_slate must equal the consensus re-derivation of auditor_set_commitment"
        );

        // (2) The producer's sample-root helper emits the pinned consensus sample root.
        let leaves = golden_leaves();
        let sample_root = derive_audit_sample_root(&seed, &batch, &leaves, GOLDEN_SAMPLE_SIZE);
        assert_eq!(
            sample_root.to_string(),
            CROSS_CRATE_GOLDEN_AUDIT_SAMPLE_ROOT,
            "derive_audit_sample_root must equal the consensus re-derivation of audit_sample_root"
        );

        // (3) ...and BOTH values flow onto the actual certificate fields the verifier reads. Build a real
        //     quorum certificate over the SELECTED slate carrying the producer values, and assert the
        //     certificate carries exactly the pinned hex — the certificate, not just the helper output.
        let auditors: Vec<Auditor> = slate
            .iter()
            .map(|member| {
                let bond = member.representative;
                // The slate bond's txid is h(cred) = [cred; 64]; key the auditor deterministically off it.
                let cred = bond.transaction_id.as_byte_slice()[0];
                Auditor { key: ValidatorKey::from_seed([cred; 32]), bond, pass: true, checked_leaf_bitmap_root: h(cred ^ 0x5A) }
            })
            .collect();
        let r = AuditRound { audit_sample_root: sample_root, ..round(commitment) };
        let ac = run_audit_round(&r, &auditors, &slate, QuorumPolicy { num: 2, den: 3 }).expect("slate reaches quorum");
        assert_eq!(ac.cert.auditor_set_commitment.to_string(), CROSS_CRATE_GOLDEN_AUDITOR_SET_COMMITMENT);
        assert_eq!(ac.cert.audit_sample_root.to_string(), CROSS_CRATE_GOLDEN_AUDIT_SAMPLE_ROOT);
    }
}
