//! kaspa-pq ADR-0039 §9.3/§9.5/§18 — PALW overlay-payload processing: parse a PALW subnetwork
//! (`0x30`–`0x37`) transaction's payload and apply the resulting batch-state transition to the
//! [`PalwStore`]. Pure parse + a store-application step, so the transition logic is unit-testable.
//!
//! The caller gates this on the PALW activation fence. Mainnet, testnet-10, simnet and devnet use
//! `u64::MAX`; the three PALW presets use `0` and write [`PalwStore`] rows from genesis. These
//! transitions are carried by ordinary transactions (subnetworks `0x30`–`0x37`) and are independent
//! of the algo-4 header-admission lever.

use std::sync::Arc;

use borsh::BorshDeserialize;
use kaspa_consensus_core::palw::{
    PALW_BATCH_CERTIFICATE_VERSION_V2, PalwBatchCertificateV2, PalwBatchManifestV1, PalwBeaconCommitV1, PalwBeaconRevealV1,
    PalwLeafChunkV1, PalwProviderBondPayloadV1, PalwPublicLeafV1, PalwTicketBinding, ProviderBondView, is_provider_bond_active_at,
    palw_audit_sample_root, palw_certificate_included_within_audit_window, palw_deterministic_sample, palw_leaf_merkle_depth,
    palw_verify_leaf_membership, select_weighted_auditor_committee,
};
/// ADR-0040 P1-6 — re-exported so the isolation validator can name the authorization subnetwork without
/// reaching across crates for it.
pub use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION;
use kaspa_consensus_core::subnets::{
    SUBNETWORK_ID_PALW_BATCH_CERT, SUBNETWORK_ID_PALW_BATCH_MANIFEST, SUBNETWORK_ID_PALW_BEACON_COMMIT,
    SUBNETWORK_ID_PALW_BEACON_REVEAL, SUBNETWORK_ID_PALW_LEAF_CHUNK, SUBNETWORK_ID_PALW_PROVIDER_BOND,
};
use kaspa_hashes::Hash64;

use crate::model::services::reachability::MTReachabilityService;
use crate::model::stores::headers::{DbHeadersStore, HeaderStoreReader};
use kaspa_database::prelude::StoreErrorPredicates;

use crate::model::stores::palw::{PalwStore, PalwStoreReader};
use crate::model::stores::palw_beacon::DbPalwBeaconStore;
use crate::model::stores::reachability::DbReachabilityStore;

/// A parsed PALW overlay transaction. Covers the batch lifecycle (`0x30`–`0x33`) and the DNS beacon
/// commit/reveal (`0x35`/`0x36`); the slashing (`0x34`) and provider-unbond (`0x37`) kinds are their own
/// later slices and still fall through to `UnhandledSubnet`.
#[derive(Clone, Debug)]
pub enum PalwOverlayEffect {
    ProviderBond(PalwProviderBondPayloadV1),
    Manifest(PalwBatchManifestV1),
    LeafChunk(PalwLeafChunkV1),
    Certificate(PalwBatchCertificateV2),
    BeaconCommit(PalwBeaconCommitV1),
    BeaconReveal(PalwBeaconRevealV1),
    /// ADR-0040 P1-6 — per-block ticket authorization. Parsed so the overlay walkers can SKIP it
    /// without treating it as a malformed payload; it has no overlay-state effect, because it is
    /// consumed by body-validation clause 7 on the block that carries it, not by the acceptance walk.
    BlockAuthorization,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwOverlayError {
    /// The subnetwork's first byte is not a batch-lifecycle PALW kind this processor handles.
    UnhandledSubnet(u8),
    /// The payload did not borsh-decode as its declared type.
    MalformedPayload,
    /// ADR-0040 §5.15.4 (ACCEPT-BIND/M2) — a leaf chunk reached the acceptance arm declaring a version
    /// other than `PALW_LEAF_CHUNK_VERSION_V2`. v1 chunks carry no membership proofs, so admitting one
    /// would restore exactly the unbound-leaf hole this gate closes.
    LeafChunkUnsupportedVersion(u16),
    /// The batch-state machine rejects this event from the batch's current status (§9.5).
    InvalidTransition,
    /// A manifest's `batch_id` is not its own content id (§9.2) — an attacker-chosen key that must not
    /// be allowed to pollute the content-addressed blob store.
    NonContentAddressedBatchId,
    /// ADR-0040 P1-1 (BIND-01): a leaf chunk referenced a `batch_id` with no admitted manifest. A leaf
    /// must never be the effect that first materialises a batch key.
    UnknownBatch,
    /// ADR-0040 P1-1 (BIND-01): `leaf_index >= manifest.leaf_count`, so the leaf could never be part of
    /// the batch's `leaf_root`.
    LeafIndexOutOfRange { leaf_index: u32, leaf_count: u32 },
    /// ADR-0040 P1-1 (BIND-01): a leaf inside the chunk claims a different `batch_id` than the chunk.
    LeafBatchIdMismatch,
    /// kaspa-pq ADR-0040 §5.14.3 item 7 (P1-10 prerequisite): the leaf's `registered_epoch` is not the
    /// manifest's `registration_epoch`.
    ///
    /// Before this check the leaf's registration epoch was constrained ONLY relationally
    /// (`registered_epoch < activation_epoch < expiry_epoch`, `validate_public_leaf`), while the
    /// manifest's `registration_epoch` is pinned to the batch's real accept epoch by
    /// `PalwBatchManifestV1::admission_valid` (via `PalwBatchViewV1::apply_manifest`). The two numbers
    /// were never compared, so a batch author could publish an admissible manifest at the true epoch and
    /// still stamp its leaves with an arbitrary earlier `registered_epoch` — which is the value
    /// `palw_work_reward_class` feeds to `palw_premium_at_window` at the REWARD coordinate.
    ///
    /// This check is only sound BECAUSE of §5.15 (ACCEPT-BIND/M2). Both numbers are now committed to the
    /// same `batch_id`: `registered_epoch` sits inside `leaf_hash` → `leaf_root` → `content_id()` ==
    /// `batch_id`, and `registration_epoch` sits directly inside `content_id()`. So the pair is fixed at
    /// batch-construction time and neither side can be swapped afterwards. Pre-M2 the same comparison
    /// would have been decorative — the whole leaf could be replaced at `(batch_id, leaf_index)`.
    LeafRegistrationEpochMismatch { leaf_index: u32, leaf_registered_epoch: u64, manifest_registration_epoch: u64 },
    /// ADR-0040 P1-1 (LEAF-01): an attempt to replace an already-written leaf with different content.
    /// Leaves are write-once because coinbase reward scripts are read from them after acceptance.
    LeafImmutabilityViolation,
    /// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2, gate G3 clause 1): the chunk carries fewer membership
    /// proofs than leaves, so some leaf has no proof at all.
    ///
    /// `validate_leaf_chunk` already requires `proofs.len() == leaves.len()`, but `parse_palw_overlay`
    /// is a bare Borsh decode and this arm must never depend on a check performed by a different pass —
    /// least of all by indexing a caller-supplied `Vec` and panicking inside consensus.
    LeafProofCountMismatch { leaves: usize, proofs: usize },
    /// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2, gate G3 clause 1): a proof's length is not exactly
    /// `palw_leaf_merkle_depth(manifest.leaf_count)`.
    ///
    /// This is the CONTEXT-BEARING half of the split the context-free `validate_leaf_chunk`
    /// deliberately leaves open (it can only assert the static `<= 8` bound, having no manifest). The
    /// exact bound is what makes the proof for a given `(leaf, index, root)` UNIQUE; a mere upper bound
    /// leaves the variable-length-path forgeries open. Kept a SEPARATE variant from
    /// [`PalwOverlayError::LeafMembershipProofInvalid`] so a rejection is attributable, and so a test
    /// can state that a mis-length proof is refused BEFORE any hashing happens.
    LeafMembershipProofLengthInvalid { leaf_index: u32, got: usize, expected: u32 },
    /// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2, gate G3 clause 1): the leaf's membership proof does not
    /// fold to `manifest.leaf_root` at the leaf's own `leaf_index` — i.e. this leaf is not a member of
    /// the batch it is being written into.
    ///
    /// This is the CHUNK-INDEX SQUAT closure. `batch_id` is public, so anyone can copy one; before this
    /// gate existed, a squatter could win the write-once race under an honest `batch_id` with ITS OWN
    /// leaves (own `provider_{a,b}_reward_script`, own `ticket_authority_pk_hash`) and take the 77 %
    /// worker base, because consensus never re-derived `leaf_root` from what it stored. Now those fields
    /// live inside `leaf_hash`, `leaf_hash` must open `manifest.leaf_root`, `leaf_root` is inside
    /// `content_id()`, and `batch_id == content_id()` is enforced on both arms — so writing someone
    /// else's `(batch_id, leaf_index)` costs a BLAKE2b-512 second preimage.
    LeafMembershipProofInvalid { leaf_index: u32 },
    /// ADR-0045 D3-b: the chunk carries fewer PCPB witnesses than leaves. Same never-index-a-
    /// caller-supplied-`Vec` rationale as [`PalwOverlayError::LeafProofCountMismatch`].
    LeafPcpbWitnessCountMismatch { leaves: usize, witnesses: usize },
    /// ADR-0045 D3-b clause 11: the freshness window `issued ≤ anchor ∧ anchor+Δ ≤ registered ∧
    /// registered−issued ≤ w` fails for this leaf. All three epochs are content-sealed, so this is a
    /// permanent property of the leaf, not a timing accident.
    LeafChallengeStale { leaf_index: u32, issued_epoch: u64, anchor_epoch: u64, registered_epoch: u64 },
    /// ADR-0045 D3-b clauses 11/12: a PCPB context read failed CLOSED — the epoch's beacon seed,
    /// provider snapshot, or the A-commit anchor row is not resolvable in the retained window.
    /// Substituting a live value here is precisely the grindable state D3-b forbids, so the leaf is
    /// refused instead.
    LeafPcpbContextUnresolvable { leaf_index: u32, epoch: u64, what: &'static str },
    /// ADR-0045 D3-b clause 11: re-deriving the job challenge under `R_{issued_epoch}` from the
    /// witness's preimage triple does not reproduce `receipt_v3_job_challenge` — the declared issue
    /// epoch (and with it the whole freshness argument) is not real.
    LeafChallengeMismatch { leaf_index: u32 },
    /// ADR-0045 D3-b clause 12: the self arm's anchor is not on-chain at-or-before the leaf's
    /// declared `a_commit_epoch` (`registry row ≤ declared` is the grind-killing bound: the draw
    /// beacon `R_{declared+Δ}` must provably post-date the anchor's registration).
    LeafACommitUnanchored { leaf_index: u32, declared_epoch: u64, registry_epoch: Option<u64> },
    /// ADR-0045 D3-b clause 12: the dispatch evidence does not re-derive — wrong snapshot roots
    /// (clause 0), a provider the draw did not select, a seat not matching the leaf's declared
    /// bonds, a receipt not embedding `a_commit`, or a signature that does not verify.
    LeafDispatchEvidenceInvalid { leaf_index: u32 },
    /// ADR-0040 P1-4 (BIND-05): the certificate's `manifest_hash` is not the content id of the manifest
    /// for the batch it names — i.e. it certifies a different manifest than the one on chain.
    CertificateManifestMismatch,
    /// SUMMARY-BIND V2: legacy or unknown certificate wire revisions are never applied to state.
    CertificateUnsupportedVersion(u16),
    /// ADR-0040 P1-4 (BIND-05): the certificate's `leaf_root` disagrees with the batch manifest's, so it
    /// attests to a leaf set that is not this batch's.
    CertificateLeafRootMismatch,
    /// A certificate may not repackage valid votes around an activation window different from the
    /// immutable manifest it certifies.
    CertificateActivationEpochMismatch,
    /// A certificate may not extend or shorten the immutable manifest expiry window.
    CertificateExpiryEpochMismatch,
    /// The certificate must be included in the exact epoch it declares. Votes do not sign this field,
    /// so accepting any other value would allow post-signature envelope repackaging.
    CertificateInclusionEpochMismatch,
    /// The beacon snapshot a certificate audits must belong to the batch's immutable audit interval:
    /// at or after registration, and strictly before activation. Without this cross-bind, a handcrafted
    /// certificate can select an older, pre-registration provider set/seed even though the public audit
    /// facts API and validator tooling refuse that round.
    CertificateAuditEpochOutsideManifestWindow,
    /// ADR-0040 P1-3 (CERT-01): a vote's `bond_outpoint` does not resolve to a bond that is ACTIVE at the
    /// certifying block's DAA score — an unbonded (hence unslashable) "auditor".
    CertificateVoteBondNotActive,
    /// ADR-0040 P1-3 (CERT-01): a vote's ML-DSA-87 signature does not verify under its bond's registered
    /// validator key over the vote's `signing_hash`. Previously only the signature LENGTH was checked.
    CertificateVoteSignatureInvalid,
    /// ADR-0040 P1-3 (CERT-01): the stake-weighted PASS tally did not reach the quorum threshold.
    CertificateQuorumNotReached,
    /// ADR-0040 P1-3 (CERT-01): two votes share a `bond_outpoint`, which would let one bond's stake be
    /// counted more than once toward quorum.
    CertificateDuplicateVoteBond,
    /// ADR-0040 §12′: the certificate's declared `approving_stake` disagrees with the tally recomputed
    /// from the active bond view. Rejected because the supersession comparator reads the declared value.
    CertificateApprovingStakeMismatch,
    /// SUMMARY-BIND V2: the declared pass count exceeds the on-chain batch cardinality.
    CertificatePassedLeafCountOutOfRange,
    /// SUMMARY-BIND V2: an embedded auditor vote binds a different pass/reject summary than the
    /// enclosing certificate. Every vote must attest to the one certificate-wide summary.
    CertificateVoteSummaryMismatch,
    /// kaspa-pq **ADR-0040 §5.17.3 (AUTHSET-01 / SAMPLE-01)** — the audit-epoch beacon seed
    /// `R_{audit_beacon_epoch − 1}` could not be resolved from the block's selected-parent chain (pruned
    /// history, pre-activation / zero-seed boundary, or `audit_beacon_epoch == 0`). Both the auditor-set
    /// and the sample-root re-derivations depend on this seed, so an unresolvable seed FAILS CLOSED. Sound
    /// only together with [`Self::CertificateOutsideAuditInclusionWindow`], which keeps an honest
    /// certificate's audit epoch inside the unpruned window so this branch is never hit for it.
    CertificateAuditEpochSeedUnresolved,
    /// kaspa-pq **ADR-0040 §5.17.3** — the certificate is included more than `N` epochs after its own
    /// `audit_beacon_epoch` (or before it). `N` is [`palw_audit_epoch_inclusion_window_epochs`], the widest
    /// legal batch lifecycle span, so an honest certificate is never rejected here — but a stale one whose
    /// audit epoch may have been pruned is, which is what makes the seed fail-closed above sound.
    CertificateOutsideAuditInclusionWindow,
    /// kaspa-pq **ADR-0040 §5.17.4 (AUTHSET-01)** — the certificate's declared `auditor_set_commitment`
    /// does not equal the commitment re-derived over the beacon-selected auditor committee (the weighted,
    /// credential-aggregated sample over the provider-bond view at the audit snapshot, minus the batch's
    /// own providers). The declared auditor set is not the one the beacon selected.
    CertificateAuditorSetMismatch,
    /// kaspa-pq **ADR-0040 §5.17.4 (AUTHSET-01)** — a vote's `bond_outpoint` is not a member of the
    /// re-derived auditor slate: a vote from OUTSIDE the beacon-selected committee.
    CertificateVoteOutsideCommittee,
    /// kaspa-pq **ADR-0040 §5.17.6 (SAMPLE-01)** — the certificate's declared `audit_sample_root` does not
    /// equal the value re-derived from the beacon-selected on-chain leaves' `receipt_da_root`s. The
    /// certificate does not commit to the leaves the beacon selected.
    CertificateAuditSampleRootMismatch,
    /// kaspa-pq **ADR-0040 §5.17** — a leaf in `[0, manifest.leaf_count)` the AUTHSET-01 / SAMPLE-01
    /// re-derivations need is not on chain. A certificate cannot be attested against a batch whose leaves
    /// are not fully present, so this FAILS CLOSED.
    CertificateLeafAbsent,
    /// A backing-store read/write failed.
    StoreError,
}

/// Context required to verify a batch certificate against the beacon-selected committee.
///
/// ## Provider-bond registry
///
/// [`select_auditor_committee`] draws the auditor committee from the provider-bond view,
/// and the §5.17.4 exclusions (`operator_group_id`, the batch's own `provider_{a,b}_bond`) are
/// provider-bond concepts with no DNS-stake-bond analogue. Votes resolve against `ProviderBondView` and
/// are weighted by the ECON-03 credential aggregate. The representative outpoint identifies the vote
/// key without reducing selected weight to one split bond.
pub struct PalwCertificateAttestationCtx<'a> {
    /// Domain-separates signatures across networks (same value the beacon path uses).
    pub network_id: u32,
    /// The **selection snapshot** DAA score — derived from the certificate's own `audit_beacon_epoch`,
    /// NOT from the certifying block's DAA (ADR-0040 §12′ / §5.17.2).
    ///
    /// Eligibility must freeze at selection, exactly as B assignment does. Evaluating at inclusion time
    /// would let an attacker holding a certificate choose to include it just after an honest auditor's
    /// bond lapses — invalidating that vote, and thereby either killing the honest certificate or
    /// handing the supersession comparison to a censored one. `audit_beacon_epoch` is committed in the
    /// certificate and covered by every vote's `signing_hash`, so it cannot be re-aimed afterwards.
    pub pov_daa_score: u64,
    /// Selected-parent PALW provider-bond view: the fork-local auditor candidate pool AND the source of
    /// each voter's ML-DSA-87 key + ECON-03-verified stake. Walked in lockstep to the certifying block's
    /// selected parent, so `apply`/`revert` being exact inverses makes it order-independent (§5.17.2).
    pub provider_bond_view: &'a ProviderBondView,
    /// The audit-epoch beacon seed `R_{audit_beacon_epoch − 1}` resolved by the buried selected-parent
    /// walk ([`resolve_palw_audit_epoch_seed`], §5.17.3). `None` when unresolvable (pruned /
    /// pre-activation / `audit_beacon_epoch == 0`), in which case verification FAILS CLOSED. Order-
    /// independence is the resolver's property (it reads only the deterministic selected-parent chain).
    pub prev_seed: Option<Hash64>,
    /// The PALW epoch of the block that CARRIED the certificate transaction
    /// (`carrier_block_daa / epoch_len`), for the §5.17.3 bounded-window rule that keeps the seed
    /// fail-closed sound. This is deliberately not the later chain block that may accept the carrier
    /// from its mergeset; otherwise certificate validity would vary with merge delay.
    pub inclusion_epoch: u64,
    /// `N` — the maximum epochs a certificate may lag its `audit_beacon_epoch` and still be included
    /// ([`palw_audit_epoch_inclusion_window_epochs`], the widest legal batch lifecycle span).
    pub inclusion_window_epochs: u64,
    /// AUTHSET-01 committee cardinality (`Params::palw_audit_committee_size`).
    pub committee_size: usize,
    /// SAMPLE-01 leaf sample size (`Params::palw_audit_sample_size`).
    pub sample_size: u32,
    /// Stake-weighted quorum threshold, `num/den` (testnet 2/3).
    pub quorum_num: u16,
    pub quorum_den: u16,
}

/// kaspa-pq **ADR-0040 §5.17 (AUTHSET-01 / SAMPLE-01 / SEL-01)** — verify that a certificate is genuinely
/// ATTESTED by the beacon-selected auditor committee, over the beacon-selected on-chain leaves.
///
/// ## What this closes (three findings, one defect class — a value the certificate declares that
/// consensus never re-derives, CERT-TRUST)
///
/// The P1-3 predecessor verified vote signatures and a stake-weighted quorum over the bonds that
/// happened to VOTE — but it re-derived neither WHO was supposed to audit (`auditor_set_commitment` had
/// zero readers, AUTHSET-01) nor WHAT they were supposed to sample (`audit_sample_root` was a
/// producer-declared field, SAMPLE-01), and the "auditor set" was drawn from an unweighted per-outpoint
/// score a bond-split could stuff (SEL-01). All three are re-derived here now, at the one order-
/// independent coordinate that has the frozen snapshot.
///
/// `leaves` are the batch's on-chain public leaves in index order (`[0, manifest.leaf_count)`), resolved
/// by the caller from the leaf store; they carry both the `provider_{a,b}_bond`s (AUTHSET-01 exclusions)
/// and the `receipt_da_root`s (SAMPLE-01 sample).
///
/// ## Verification order
///
/// 0. The audit epoch belongs to the manifest and remains resolvable. It must be in
///    `[manifest.registration_epoch, manifest.activation_not_before_epoch)`, its seed must resolve, and
///    the certificate must be within the inclusion window. Both re-derivations key off
///    `prev_seed = R_{audit_beacon_epoch − 1}`. An unresolvable seed (pruned / pre-activation /
///    `audit_beacon_epoch == 0`) fails closed; the §5.17.3 bounded-window rule keeps a valid
///    certificate's audit epoch inside the unpruned window so it is never stranded.
/// 1. AUTHSET-01: re-derive the beacon-selected committee with the
///    SEL-01 weighted, credential-aggregated sampler over the provider-bond view at the frozen audit
///    snapshot, excluding the batch's own providers (their credentials + operator groups), and REJECT if
///    `cert.auditor_set_commitment` disagrees. Every vote's `bond_outpoint` must be IN that slate.
/// 2. SAMPLE-01: re-derive the sample root from the beacon-selected leaves' DA roots as
///    `palw_audit_sample_root` over the `receipt_da_root`s of `palw_deterministic_sample(prev_seed,
///    batch_id, leaf_count, sample_size)`, and REJECT if `cert.audit_sample_root` disagrees. Votes are
///    verified over the RE-DERIVED root, not the declared one — a vote signed over an arbitrary sample no
///    longer verifies (this is the §5.17.6 REDEFINITION: an enforceable on-chain DA-commitment covering,
///    strictly weaker than I-14's off-chain possession but re-derivable by every node).
/// 3. **Every vote's ML-DSA-87 signature verifies** under its provider bond's registered
///    `owner_public_key`, over [`PalwAuditorVoteV2::signing_hash`] with the re-derived root. Reject /
///    abstain votes are verified too; an omitted vote contributes no PASS stake but cannot shrink the
///    denominator.
/// 4. **Declared `approving_stake` equals the recomputed PASS tally**, and **stake-weighted quorum** is
///    reached against the ECON-03-verified credential-aggregated amount of the ENTIRE re-derived selected
///    slate. Withholding still counts against quorum, and splitting one credential across bonds changes
///    neither selection nor quorum weight.
///
/// ## ORDER INDEPENDENCE (the disqualifier — proved per input)
///
/// Every re-derivation input is read at the block's frozen selected-parent snapshot and is identical on
/// every node that reaches the block by any reorg path: `prev_seed` from the buried selected-parent walk
/// (resolver's proof); the provider-bond view walked to the selected parent (its `apply`/`revert` are
/// exact inverses); `pov_daa_score` from the certificate's own committed `audit_beacon_epoch`; the on-
/// chain `leaves` content-addressed under the batch; `committee_size` / `sample_size` / quorum from
/// `Params`. No read touches the epoch-keyed accum store or any tip-relative / mutable state.
///
/// Reached only from the `Certificate` arm of `apply_palw_overlay_effect`,
/// which the virtual processor invokes only at/above `palw_activation_daa_score` (`u64::MAX` on the four
/// non-PALW presets).
pub fn verify_certificate_attestation(
    cert: &PalwBatchCertificateV2,
    manifest: &PalwBatchManifestV1,
    ctx: &PalwCertificateAttestationCtx<'_>,
    leaves: &[Arc<PalwPublicLeafV1>],
) -> Result<(), PalwOverlayError> {
    use kaspa_consensus_core::palw::{PALW_AUDITOR_V2_MLDSA87_CONTEXT, PALW_BATCH_CERTIFICATE_VERSION_V2};
    use kaspa_txscript::verify_mldsa87_with_context;
    use std::collections::HashSet;

    if cert.version != PALW_BATCH_CERTIFICATE_VERSION_V2 {
        return Err(PalwOverlayError::CertificateUnsupportedVersion(cert.version));
    }
    if cert.activation_epoch != manifest.activation_not_before_epoch {
        return Err(PalwOverlayError::CertificateActivationEpochMismatch);
    }
    if cert.expiry_epoch != manifest.expiry_epoch {
        return Err(PalwOverlayError::CertificateExpiryEpochMismatch);
    }
    // The audit snapshot must be one at which this batch already existed but was not yet active.
    // `audit_beacon_epoch` is signed by every vote, but signatures only authenticate the DECLARED
    // coordinate; they do not prove that the coordinate belongs to this manifest. Keep this check in
    // the authoritative verifier (not only the audit-facts producer/tooling), before resolving the
    // historical seed/provider snapshot that the untrusted field selects.
    if cert.audit_beacon_epoch < manifest.registration_epoch || cert.audit_beacon_epoch >= manifest.activation_not_before_epoch {
        return Err(PalwOverlayError::CertificateAuditEpochOutsideManifestWindow);
    }
    if cert.certificate_epoch != ctx.inclusion_epoch {
        return Err(PalwOverlayError::CertificateInclusionEpochMismatch);
    }
    if cert.passed_leaf_count == 0 || cert.passed_leaf_count > leaves.len() as u32 {
        return Err(PalwOverlayError::CertificatePassedLeafCountOutOfRange);
    }

    // (0a) The audit-epoch seed both re-derivations depend on. FAIL CLOSED if unresolvable.
    let prev_seed = ctx.prev_seed.ok_or(PalwOverlayError::CertificateAuditEpochSeedUnresolved)?;

    // (0b) Bounded inclusion window (§5.17.3): a certificate stale enough that its audit epoch may have
    // been pruned is rejected, which is what makes the fail-closed seed above sound. `N` is the widest
    // legal batch lifecycle span, so an honest certificate is never rejected here.
    if !palw_certificate_included_within_audit_window(cert.audit_beacon_epoch, ctx.inclusion_epoch, ctx.inclusion_window_epochs) {
        return Err(PalwOverlayError::CertificateOutsideAuditInclusionWindow);
    }

    // (1) AUTHSET-01 — re-derive the beacon-selected auditor committee.
    //
    // The candidate pool is the provider-bond view at the frozen audit snapshot MINUS the batch's own
    // providers: an auditor may not audit a batch it produced. Each leaf names two provider bonds; their
    // credentials (`owner_pubkey_hash`) and operator groups are excluded so neither a producer nor an
    // operator-group sibling can be drawn as its own auditor. Bonds a leaf names that do not resolve in
    // the view are simply not in the candidate pool, so they need no separate exclusion.
    let mut excluded_credentials: HashSet<Hash64> = HashSet::new();
    let mut excluded_operator_groups: HashSet<Hash64> = HashSet::new();
    for leaf in leaves {
        for bond_outpoint in [&leaf.provider_a_bond, &leaf.provider_b_bond] {
            if let Some(record) = ctx.provider_bond_view.get(bond_outpoint) {
                excluded_credentials.insert(record.owner_pubkey_hash);
                excluded_operator_groups.insert(record.operator_group_id);
            }
        }
    }
    let (slate, rederived_auditor_commitment) = select_weighted_auditor_committee(
        &prev_seed,
        &cert.batch_id,
        ctx.provider_bond_view,
        ctx.pov_daa_score,
        &excluded_credentials,
        &excluded_operator_groups,
        ctx.committee_size,
    );
    if cert.auditor_set_commitment != rederived_auditor_commitment {
        return Err(PalwOverlayError::CertificateAuditorSetMismatch);
    }
    let slate_set: HashSet<kaspa_consensus_core::tx::TransactionOutpoint> = slate.iter().map(|member| member.representative).collect();
    let stake_of = |o: &kaspa_consensus_core::tx::TransactionOutpoint| -> u128 {
        slate.iter().find(|member| member.representative == *o).map(|member| member.weight).unwrap_or(0)
    };
    // The denominator is the full beacon-selected slate, not merely the subset that submitted votes.
    // Otherwise one selected auditor can withhold and a lone PASS becomes 1/1, defeating the 2/3 rule.
    let total_slate_stake = slate.iter().fold(0u128, |total, member| total.saturating_add(member.weight));

    // (2) SAMPLE-01 — re-derive `audit_sample_root` over the beacon-selected on-chain leaves' DA roots.
    // `leaf_count` is the on-chain leaf set's cardinality; indices are into `leaves`. A sampled index
    // with no corresponding stored leaf fails closed (the caller resolves `[0, leaf_count)`, so this is
    // defence in depth against a short slice).
    let leaf_count = leaves.len() as u32;
    let sampled_indices = palw_deterministic_sample(&prev_seed, &cert.batch_id, leaf_count, ctx.sample_size);
    let mut sampled_da_roots: Vec<Hash64> = Vec::with_capacity(sampled_indices.len());
    for idx in &sampled_indices {
        let leaf = leaves.get(*idx as usize).ok_or(PalwOverlayError::CertificateAuditSampleRootMismatch)?;
        sampled_da_roots.push(leaf.receipt_da_root);
    }
    let rederived_sample_root = palw_audit_sample_root(&sampled_da_roots);
    if cert.audit_sample_root != rederived_sample_root {
        return Err(PalwOverlayError::CertificateAuditSampleRootMismatch);
    }

    // Votes now sign over the RE-DERIVED sample root — a vote signed over an attacker-chosen sample fails
    // signature verification below (SAMPLE-01, §5.17.6 step 4).
    let digest = |v: &kaspa_consensus_core::palw::PalwAuditorVoteV2| {
        v.signing_hash(ctx.network_id, &cert.batch_id, cert.audit_beacon_epoch, &rederived_sample_root)
    };

    let mut seen: Vec<&kaspa_consensus_core::tx::TransactionOutpoint> = Vec::with_capacity(cert.votes.len());
    let mut pass_stake: u128 = 0;

    for vote in &cert.votes {
        // (3a) one bond, one vote.
        if seen.contains(&&vote.bond_outpoint) {
            return Err(PalwOverlayError::CertificateDuplicateVoteBond);
        }
        seen.push(&vote.bond_outpoint);

        // (3b) the vote must come from INSIDE the beacon-selected committee (AUTHSET-01).
        if !slate_set.contains(&vote.bond_outpoint) {
            return Err(PalwOverlayError::CertificateVoteOutsideCommittee);
        }

        // SUMMARY-BIND V2 — the enclosing certificate is only an envelope. Its summary must be the
        // exact summary every independently signed vote carries; assembly cannot rewrite either field.
        if vote.passed_leaf_count != cert.passed_leaf_count || vote.rejected_leaf_bitmap_root != cert.rejected_leaf_bitmap_root {
            return Err(PalwOverlayError::CertificateVoteSummaryMismatch);
        }

        // (3c) the provider bond must resolve ACTIVE at the audit snapshot — the source of both the
        // voter's ML-DSA-87 key and its ECON-03-verified stake weight. A slate member is active by
        // construction (the sampler filters on `is_provider_bond_active_at`); the explicit check keeps
        // this function sound on its own and yields the voter's record.
        let record = ctx.provider_bond_view.get(&vote.bond_outpoint).filter(|r| is_provider_bond_active_at(r, ctx.pov_daa_score));
        let Some(record) = record else {
            return Err(PalwOverlayError::CertificateVoteBondNotActive);
        };

        // (3d) real signature, over the vote's own signing hash (bound to the re-derived root), under the
        // provider bond's registered owner key.
        let d = digest(vote);
        if !matches!(
            verify_mldsa87_with_context(
                &record.owner_public_key,
                d.as_bytes().as_slice(),
                &vote.signature,
                PALW_AUDITOR_V2_MLDSA87_CONTEXT
            ),
            Ok(true)
        ) {
            return Err(PalwOverlayError::CertificateVoteSignatureInvalid);
        }

        if vote.vote == 1 {
            pass_stake = pass_stake.saturating_add(stake_of(&vote.bond_outpoint));
        }
    }

    // (4) The DECLARED approving stake must equal the recomputed tally — the field is a commitment, not a
    // trusted input; keeping the equality means a future reader of `approving_stake` cannot reintroduce a
    // hole by trusting the declaration.
    if cert.approving_stake != pass_stake {
        return Err(PalwOverlayError::CertificateApprovingStakeMismatch);
    }

    // (5) stake-weighted quorum. The guards inside `quorum_reached` (ADR-0040 P0-5) make a zero total or a
    // zero threshold fail closed rather than vacuously pass.
    if !cert.quorum_reached(total_slate_stake, ctx.quorum_num, ctx.quorum_den, stake_of) {
        return Err(PalwOverlayError::CertificateQuorumNotReached);
    }
    Ok(())
}

/// ADR-MA §17.3 — verify a Compute Set activation certificate's embedded validator attestation.
///
/// The validator-quorum analogue of [`verify_certificate_attestation`], but over the DNS stake
/// bond set instead of a beacon-selected auditor committee: the §17.3 quorum is «既存の
/// ML-DSA-87 validator quorum machinery», so eligibility, keys and stake all come from the SAME
/// frozen selected-parent [`ActiveBondView`] the beacon/attestation paths read (never this
/// block's own bond mutations, never tip-relative state — reorg-path-independent by the same
/// argument).
///
/// Declared-commitment posture throughout: `validator_set_commitment`, `total_selected_stake`,
/// `approving_stake` and `votes_root` are all recomputed here and REJECTED on mismatch — a
/// certificate author must predict the active bond set exactly; if governance moved a bond
/// between authoring and folding, the certificate is re-issued (fail-closed, never silently
/// re-aimed). Every error is [`ComputeSetRegistryError::CertificateAttestationInvalid`], an
/// ADMISSION rejection: the fold warns and skips, the set stays un-certified, block validity is
/// untouched (the §21.2 overlay-fold posture).
pub fn verify_compute_set_activation_certificate(
    cert: &kaspa_consensus_core::palw_compute_set::PalwComputeSetActivationCertificateV1,
    bond_view: &kaspa_consensus_core::dns_finality::ActiveBondView,
    network_id: u32,
    pov_daa_score: u64,
    quorum_num: u16,
    quorum_den: u16,
) -> Result<(), kaspa_consensus_core::palw_compute_set::ComputeSetRegistryError> {
    use kaspa_consensus_core::dns_finality::is_bond_active_at;
    use kaspa_consensus_core::palw_compute_set::{
        ComputeSetRegistryError, MAX_ACTIVATION_CERT_VOTES, PALW_COMPUTE_SET_ACTIVATION_MLDSA87_CONTEXT,
        palw_activation_validator_set_commitment, palw_activation_votes_root,
    };
    use kaspa_txscript::verify_mldsa87_with_context;
    let reject = |reason: &'static str| ComputeSetRegistryError::CertificateAttestationInvalid(cert.compute_set_id, reason);

    if cert.votes.is_empty() {
        return Err(reject("no embedded votes"));
    }
    if cert.votes.len() > MAX_ACTIVATION_CERT_VOTES {
        return Err(reject("vote list exceeds the anti-explosion ceiling"));
    }
    // Canonical vote list + the declared votes_root commitment (§17.3 «this record carries what
    // was certified» — the root pins WHO voted WHAT).
    let votes_root = palw_activation_votes_root(&cert.votes).ok_or(reject("vote list is unsorted or has duplicates"))?;
    if votes_root != cert.votes_root {
        return Err(reject("declared votes_root does not match the embedded votes"));
    }

    // The frozen active bond set at the verification point-of-view: bond-granular, ascending by
    // outpoint — the exact fingerprint the author committed.
    let mut active_bonds: Vec<_> = bond_view.records().into_iter().filter(|bond| is_bond_active_at(bond, pov_daa_score)).collect();
    active_bonds.sort_by(|a, b| {
        (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
    });
    let commitment = palw_activation_validator_set_commitment(&active_bonds).ok_or(reject("active bond set is not canonical"))?;
    if commitment != cert.validator_set_commitment {
        return Err(reject("declared validator_set_commitment does not match the active bond set"));
    }
    // The quorum denominator is the ENTIRE active set, not the subset that voted — otherwise one
    // withheld vote turns a lone PASS into 1/1 (the batch-cert denominator rule).
    let total_stake = active_bonds.iter().fold(0u128, |total, bond| total.saturating_add(bond.amount as u128));
    if total_stake != cert.total_selected_stake {
        return Err(reject("declared total_selected_stake does not match the active bond set"));
    }

    let mut pass_stake = 0u128;
    for vote in &cert.votes {
        if vote.vote > 1 {
            return Err(reject("vote value is not pass/reject"));
        }
        let bond =
            bond_view.active_bond_at(&vote.bond_outpoint, pov_daa_score).ok_or(reject("vote from an unknown or inactive bond"))?;
        let digest = vote.signing_hash(network_id, cert);
        if !matches!(
            verify_mldsa87_with_context(
                &bond.validator_pubkey,
                digest.as_byte_slice(),
                &vote.signature,
                PALW_COMPUTE_SET_ACTIVATION_MLDSA87_CONTEXT
            ),
            Ok(true)
        ) {
            return Err(reject("vote signature invalid"));
        }
        if vote.vote == 1 {
            pass_stake = pass_stake.saturating_add(bond.amount as u128);
        }
    }
    if pass_stake != cert.approving_stake {
        return Err(reject("declared approving_stake does not match the recomputed tally"));
    }

    // Stake-weighted quorum with the ADR-0040 P0-5 vacuity guards: a zero total, zero threshold
    // numerator or zero denominator fails closed rather than vacuously passing.
    if quorum_den == 0 || quorum_num == 0 || total_stake == 0 {
        return Err(reject("degenerate quorum parameters or empty validator set"));
    }
    if pass_stake.saturating_mul(quorum_den as u128) < total_stake.saturating_mul(quorum_num as u128) {
        return Err(reject("stake-weighted quorum not reached"));
    }
    Ok(())
}

/// §17.4 — verifies the ML-DSA-87 validator-stake quorum authorising a governed registry mutation
/// (policy revision / allocation plan / emergency halt) against the FROZEN selected-parent DNS bond
/// view.
///
/// This is [`verify_compute_set_activation_certificate`] over the governance domain, and it exists
/// for the same reason: before it, the three mutable actions reached `apply_policy` / `apply_plan`
/// / `apply_halt` on nothing but "the transaction was accepted", so any user could retune
/// `compute_work_scale`, move every Compute Set's block share, or halt an arbitrary set. Every
/// declared quantity here is recomputed, never trusted.
///
/// The emergency halt deliberately shares the ordinary quorum rather than a lower "fast" threshold:
/// a faster stop is a legitimate future refinement, but a weaker one is not a prerequisite for
/// closing the unauthenticated path, and the same threshold is strictly the safer default.
pub fn verify_palw_governance_envelope(
    envelope: &kaspa_consensus_core::palw_compute_set::PalwGovernanceEnvelopeV1,
    bond_view: &kaspa_consensus_core::dns_finality::ActiveBondView,
    network_id: u32,
    pov_daa_score: u64,
    quorum_num: u16,
    quorum_den: u16,
) -> Result<(), kaspa_consensus_core::palw_compute_set::ComputeSetRegistryError> {
    use kaspa_consensus_core::dns_finality::is_bond_active_at;
    use kaspa_consensus_core::palw_compute_set::{
        ComputeSetRegistryError, MAX_GOVERNANCE_ENVELOPE_VOTES, PALW_GOVERNANCE_MLDSA87_CONTEXT,
        palw_activation_validator_set_commitment, palw_governance_votes_root,
    };
    use kaspa_txscript::verify_mldsa87_with_context;
    let reject = |reason: &'static str| ComputeSetRegistryError::GovernanceAttestationInvalid(reason);

    if envelope.votes.is_empty() {
        return Err(reject("no embedded votes"));
    }
    if envelope.votes.len() > MAX_GOVERNANCE_ENVELOPE_VOTES {
        return Err(reject("vote list exceeds the anti-explosion ceiling"));
    }
    let votes_root = palw_governance_votes_root(&envelope.votes).ok_or(reject("vote list is unsorted or has duplicates"))?;
    if votes_root != envelope.votes_root {
        return Err(reject("declared votes_root does not match the embedded votes"));
    }

    // The frozen active bond set at the verification point-of-view: bond-granular, ascending by
    // outpoint — the exact fingerprint the author committed.
    let mut active_bonds: Vec<_> = bond_view.records().into_iter().filter(|bond| is_bond_active_at(bond, pov_daa_score)).collect();
    active_bonds.sort_by(|a, b| {
        (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
    });
    let commitment = palw_activation_validator_set_commitment(&active_bonds).ok_or(reject("active bond set is not canonical"))?;
    if commitment != envelope.validator_set_commitment {
        return Err(reject("declared validator_set_commitment does not match the active bond set"));
    }
    // Denominator is the ENTIRE active set, not the subset that voted — otherwise one withheld
    // vote turns a lone approval into 1/1.
    let total_stake = active_bonds.iter().fold(0u128, |total, bond| total.saturating_add(bond.amount as u128));
    if total_stake != envelope.total_selected_stake {
        return Err(reject("declared total_selected_stake does not match the active bond set"));
    }

    let mut approve_stake = 0u128;
    for vote in &envelope.votes {
        if vote.vote > 1 {
            return Err(reject("vote value is not approve/reject"));
        }
        let bond =
            bond_view.active_bond_at(&vote.bond_outpoint, pov_daa_score).ok_or(reject("vote from an unknown or inactive bond"))?;
        let digest = vote.signing_hash(network_id, envelope);
        if !matches!(
            verify_mldsa87_with_context(
                &bond.validator_pubkey,
                digest.as_byte_slice(),
                &vote.signature,
                PALW_GOVERNANCE_MLDSA87_CONTEXT
            ),
            Ok(true)
        ) {
            return Err(reject("vote signature invalid"));
        }
        if vote.vote == 1 {
            approve_stake = approve_stake.saturating_add(bond.amount as u128);
        }
    }
    if approve_stake != envelope.approving_stake {
        return Err(reject("declared approving_stake does not match the recomputed tally"));
    }

    // Same ADR-0040 P0-5 vacuity guards as the activation path: a zero total, zero numerator or
    // zero denominator fails closed rather than vacuously passing.
    if quorum_den == 0 || quorum_num == 0 || total_stake == 0 {
        return Err(reject("degenerate quorum parameters or empty validator set"));
    }
    if approve_stake.saturating_mul(quorum_den as u128) < total_stake.saturating_mul(quorum_num as u128) {
        return Err(reject("stake-weighted quorum not reached"));
    }
    Ok(())
}

/// Validates provider-unbond owner authorization under ADR-0040 ECON-03.
///
/// # Release path
///
/// The ECON-03 value lock plus the leg-4 spend gate make a provider's output-0 unspendable for the
/// life of its bond. The unbond request supplies the corresponding release path.
///
/// # Authorization
///
/// `Unbonding` removes a provider from the active set. If any party could publish a `0x37` naming
/// another provider's bond, so the request must authenticate the owner.
///
/// # Verification order
///
/// 1. The bond resolves in the point-of-view view. A request naming nothing is unauthorized, so a
///    block can carry no state transition against a phantom bond.
/// 2. The bond is `Pending` or `Active` at `pov_daa_score` — not already `Unbonding`, and not
///    `Slashed` (a forfeit bond has no exit).
///
///    Every request in the block is judged against the same point of view, so this
///    rejects a request against a bond that was already unbonding BEFORE this block, not a second
///    request inside it. Two `0x37` transactions naming one bond in a single block therefore both
///    pass, and that is harmless rather than overlooked: both would stamp the same
///    `accepted_daa_score`, so the registry producer deterministically keeps the first effect for
///    that outpoint. Apply/revert remain a strict one-to-one inverse.
///    `econ03_duplicate_exits_in_one_block_are_canonicalized` pins it.
/// 3. The requesting key owns the bond: `validator_id_from_pubkey(owner_public_key)` equals
///    the record's `owner_pubkey_hash`, which acceptance derived from the bond payload.
/// 4. The ML-DSA-87 signature verifies under that key over
///    [`PalwProviderUnbondRequestV1::signing_hash`], under the dedicated
///    [`PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT`]. The digest binds `network_id` and `bond_outpoint`, so an
///    authorization is replayable onto neither another network nor another bond; the context (registered
///    in `signature_domains::SIGNATURE_DOMAINS` and held distinct by its table tests) stops a signature
///    made for any other PALW or DNS object being presented here.
///
/// Step 3 checks the key against the stored record rather than the request's own key field.
///
/// # Effect of the verdict
///
/// None here. This function is a pure predicate. `true` lets acceptance keep the carrying transaction,
/// after which [`palw_provider_bond_mutations_from_accepted_txs`] emits `Unbond(outpoint,
/// accepted_daa)` and stamps `unbond_request_daa_score`. Status stays DERIVED — release becomes
/// possible only at [`provider_bond_release_daa_score`], computed from the CLAMPED delay, never the
/// declared one.
///
/// # WIRED — and where
///
/// Consulted by `ProviderUnbondAuthFilter` (virtual_processor/utxo_validation.rs), the acceptance-time
/// SKIP threaded through `calculate_utxo_state` → `validate_transaction_in_utxo_context`, against the
/// SELECTED-PARENT [`ProviderBondView`] walked by `calculate_utxo_state_relatively`.
///
/// `false` means the carrying transaction is NOT ACCEPTED — it is skipped exactly like any other
/// invalid mergeset transaction, and **the carrying block stays valid**. That shape is deliberate and
/// is the second time this codebase has had to choose it: an earlier revision rejected the whole block
/// (`RuleError::PalwProviderUnbondUnauthorized`, now removed) over the full mergeset acceptance data,
/// which is a consensus denial of service — a miner does not choose the contents of the merge-blue
/// blocks it merges, so one unauthorized `0x37` published by an attacker invalidated every honest
/// block that merged it. The DNS bond spend-gate reached the same conclusion for the same reason; see
/// the `bond_gate_view` note in `calculate_utxo_state`. Do not reject the block; do not apply the
/// effect.
///
/// Because an unauthorized request never enters the acceptance data, it never reaches the registry
/// writer (`stage_palw_provider_bond_mutations`), which applies the canonical first `Unbond` mutation
/// for each outpoint among accepted `0x37` transactions and deliberately re-checks nothing. Bypass this predicate and any party can push
/// a stranger's bond into `Unbonding`, which — through the ECON-03 collateral-resolution rule in
/// `palw_work_reward_class` — strips that provider of its 77 % base. The ML-DSA-87 signature is what
/// makes that griefing impossible.
///
/// **Fenced with the lane**: the filter is only built at/above `palw_activation_daa_score`, `u64::MAX`
/// on mainnet / testnet-10 / simnet / devnet. On `testnet-palw-110` / `devnet-palw-111` (fence 0) it
/// runs on every block.
pub fn palw_provider_unbond_request_authorized(
    req: &kaspa_consensus_core::palw::PalwProviderUnbondRequestV1,
    bond_view: &kaspa_consensus_core::palw::ProviderBondView,
    network_id: u32,
    pov_daa_score: u64,
) -> bool {
    use kaspa_consensus_core::dns_finality::validator_id_from_pubkey;
    use kaspa_consensus_core::palw::{PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT, PalwProviderBondStatus, effective_provider_bond_status};
    use kaspa_txscript::verify_mldsa87_with_context;

    // (1) the bond must exist in this point of view.
    let Some(record) = bond_view.get(&req.bond_outpoint) else {
        return false;
    };
    // (2) it must still be locked-but-not-yet-exiting.
    if !matches!(
        effective_provider_bond_status(record, pov_daa_score),
        PalwProviderBondStatus::Pending | PalwProviderBondStatus::Active
    ) {
        return false;
    }
    // (3) the signing key must be THIS bond's owner, per the record rather than the request.
    if validator_id_from_pubkey(&req.owner_public_key) != record.owner_pubkey_hash {
        return false;
    }
    // (4) a real signature over the network- and bond-bound digest, under the dedicated context.
    let digest = req.signing_hash(network_id);
    matches!(
        verify_mldsa87_with_context(
            &req.owner_public_key,
            digest.as_bytes().as_slice(),
            &req.signature,
            PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT
        ),
        Ok(true)
    )
}

/// ADR-0039 §9.2/§9.3/§11.2 — parse a PALW overlay tx payload by its subnetwork's first byte. Handles
/// the batch lifecycle (`0x30`–`0x33`) and the beacon commit/reveal (`0x35`/`0x36`); pure (borsh decode),
/// touches no store.
pub fn parse_palw_overlay(subnet_first_byte: u8, payload: &[u8]) -> Result<PalwOverlayEffect, PalwOverlayError> {
    let malformed = |_| PalwOverlayError::MalformedPayload;
    // Resolve each handled subnetwork id to its first byte; the slashing (0x34) / unbond (0x37) kinds
    // fall through to `UnhandledSubnet` here (their own later slices).
    let bond = SUBNETWORK_ID_PALW_PROVIDER_BOND.palw_tx_kind().unwrap();
    let manifest = SUBNETWORK_ID_PALW_BATCH_MANIFEST.palw_tx_kind().unwrap();
    let leaf_chunk = SUBNETWORK_ID_PALW_LEAF_CHUNK.palw_tx_kind().unwrap();
    let cert = SUBNETWORK_ID_PALW_BATCH_CERT.palw_tx_kind().unwrap();
    let beacon_commit = SUBNETWORK_ID_PALW_BEACON_COMMIT.palw_tx_kind().unwrap();
    let beacon_reveal = SUBNETWORK_ID_PALW_BEACON_REVEAL.palw_tx_kind().unwrap();
    match subnet_first_byte {
        b if b == bond => PalwProviderBondPayloadV1::try_from_slice(payload).map(PalwOverlayEffect::ProviderBond).map_err(malformed),
        b if b == manifest => PalwBatchManifestV1::try_from_slice(payload).map(PalwOverlayEffect::Manifest).map_err(malformed),
        b if b == leaf_chunk => PalwLeafChunkV1::try_from_slice(payload).map(PalwOverlayEffect::LeafChunk).map_err(malformed),
        b if b == cert => PalwBatchCertificateV2::try_from_slice(payload).map(PalwOverlayEffect::Certificate).map_err(malformed),
        b if b == beacon_commit => PalwBeaconCommitV1::try_from_slice(payload).map(PalwOverlayEffect::BeaconCommit).map_err(malformed),
        b if b == beacon_reveal => PalwBeaconRevealV1::try_from_slice(payload).map(PalwOverlayEffect::BeaconReveal).map_err(malformed),
        b if b == SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION.palw_tx_kind().unwrap() => Ok(PalwOverlayEffect::BlockAuthorization),
        other => Err(PalwOverlayError::UnhandledSubnet(other)),
    }
}

/// ADR-0045 D3-b — the resolution context the LeafChunk arm's clauses 11/12 verify against: the
/// PCPB store pair (per-epoch provider snapshots + A-commit registry), the network id the job
/// challenge binds, and the three windows. Bundled so a caller cannot supply half a context — and
/// callers that HAVE no context (`None`) do not skip the clauses: the arm rejects every leaf chunk
/// outright, because verification that cannot run is a refusal, never a waiver (fail-closed).
pub struct PalwPcpbAcceptanceCtx<'a> {
    pub pcpb_store: &'a crate::model::stores::palw_pcpb::DbPalwPcpbStore,
    pub network_id: u32,
    pub freshness_window_epochs: u64,
    pub snapshot_lag_epochs: u64,
    pub post_commit_delta_epochs: u64,
}

/// The production [`kaspa_consensus_core::palw::PalwMlDsaVerifier`] binding — the same
/// txscript-backed ML-DSA-87 verify every other PALW signature check in this crate uses, so the
/// clause-12 self arm inherits the identical portable/cross-CPU determinism argument.
pub struct TxscriptPalwMlDsaVerifier;

impl kaspa_consensus_core::palw::PalwMlDsaVerifier for TxscriptPalwMlDsaVerifier {
    fn verify(&self, verification_key: &[u8], message: &[u8], context: &[u8], signature: &[u8]) -> bool {
        matches!(kaspa_txscript::verify_mldsa87_with_context(verification_key, message, signature, context), Ok(true))
    }
}

/// ADR-0039 §9.5 / §11.2 — apply a parsed overlay effect. **Batch-lifecycle effects persist only the
/// immutable, CONTENT-ADDRESSED blob** (manifest / leaves / certificate) into the [`PalwStore`]; they do
/// **not** write a mutable `batch_status`. The fork-dependent lifecycle (Registering → … → Active /
/// Revoked) lives in the block-keyed overlay VIEW (`commit_palw_overlay_view`), which `check_palw_ticket`
/// resolves against (C5). The old global `set_batch_status` here was the sink-search-loser fork-unsafe
/// write the C4 panel flagged (a UTXO-valid candidate later rejected by sink selection would overwrite the
/// canonical status); with the view as the authoritative lifecycle it is retired. What remains is
/// write-once, content-addressed, fork-safe (same content ⇒ same key), and admission-guarded: a manifest
/// whose `batch_id` is not its own content id is rejected so the store cannot be polluted under an
/// attacker-chosen key. Beacon commit/reveal effects accumulate into the epoch's [`DbPalwBeaconStore`].
/// Deterministic; the caller has already gated on the PALW fence.
pub fn apply_palw_overlay_effect(
    effect: PalwOverlayEffect,
    store: &dyn PalwStore,
    beacon: &DbPalwBeaconStore,
    attest: Option<&PalwCertificateAttestationCtx<'_>>,
    pcpb: Option<&PalwPcpbAcceptanceCtx<'_>>,
) -> Result<(), PalwOverlayError> {
    match effect {
        PalwOverlayEffect::BeaconCommit(c) => {
            // §11.2: record the commitment for its epoch (idempotent per bond). No batch-state effect.
            beacon.record_commit(c.epoch, c.bond_outpoint, c.commitment).map_err(|_| PalwOverlayError::StoreError)
        }
        PalwOverlayEffect::BeaconReveal(r) => {
            // §11.2: a reveal counts only if a prior commit for this (epoch, bond) exists AND the reveal
            // validly opens it. Otherwise it is inert (dropped) — a reveal with no/wrong commit is not a
            // seed input. `commitment_of` reads the same-epoch commit (submitted in an earlier block).
            if let Some(commitment) = beacon.commitment_of(r.epoch, &r.bond_outpoint).map_err(|_| PalwOverlayError::StoreError)?
                && r.matches_commit(&commitment)
            {
                beacon.record_valid_reveal(r.epoch, r.bond_outpoint, r.entropy_digest()).map_err(|_| PalwOverlayError::StoreError)?;
            }
            Ok(())
        }
        // ADR-0040 P1-6: no overlay-state effect — clause 7 already consumed it on its own block.
        PalwOverlayEffect::BlockAuthorization => Ok(()),
        PalwOverlayEffect::ProviderBond(_bond) => {
            // ADR-0040 ECON-03 (THE WIRE): NOT discarded any more, but also not applied HERE. The
            // provider-bond registry (prefix 241) is written on the acceptance-derived path —
            // `palw_provider_bond_mutations_from_accepted_txs` → `stage_palw_provider_bond_mutations`
            // (virtual_processor/processor.rs) — which is the same path the DNS stake bond takes, and
            // is the only one that can be REVERTED on a reorg. This function is the content-addressed
            // blob applier: it has no chain path, no point of view, and no revert, so a registry
            // mutation made here could not be undone when the carrying block leaves the selected
            // chain. Hence: no batch-state effect at this coordinate, by construction.
            Ok(())
        }
        PalwOverlayEffect::Manifest(m) => {
            // Content-address guard (§9.2): the manifest's key must be its own content id, else it is an
            // attacker-chosen key that could pollute the blob store / collide across forks. (The full
            // admission window/bounds check lives in the authoritative view builder, `apply_manifest`.)
            if !m.batch_id_is_content_derived() {
                return Err(PalwOverlayError::NonContentAddressedBatchId);
            }
            store.insert_manifest(m.batch_id, Arc::new(m)).map_err(|_| PalwOverlayError::StoreError)
        }
        PalwOverlayEffect::LeafChunk(c) => {
            // ADR-0040 P1-1 contextual admission for leaf blobs.
            // The manifest is the batch's only content-addressed anchor (`batch_id == content_id()`,
            // enforced in the Manifest arm), so requiring it here is what ties a leaf to a real batch.
            let manifest = store.batch_manifest(c.batch_id).map_err(|error| {
                if error.is_key_not_found() { PalwOverlayError::UnknownBatch } else { PalwOverlayError::StoreError }
            })?;
            // Defence in depth: the Manifest arm cannot admit a non-content-derived id, but a leaf chunk
            // must never be the thing that first materialises a batch key.
            if !manifest.batch_id_is_content_derived() {
                return Err(PalwOverlayError::NonContentAddressedBatchId);
            }
            // ADR-0040 §5.15.4 — the chunk VERSION is re-checked here, not only in the context-free
            // validator. The two checks are not redundant: `validate_leaf_chunk` runs on the
            // transaction-isolation path, while this arm is reachable from the acceptance path, and an
            // adversarial review drove a `version: 1` chunk carrying an otherwise-valid membership proof
            // straight into this function and had the leaf STORED. A v1 chunk has no `proofs` field by
            // construction, so accepting one here is precisely the lenient parse §5.15.4 forbids.
            if c.version != kaspa_consensus_core::palw::PALW_LEAF_CHUNK_VERSION_V3 {
                return Err(PalwOverlayError::LeafChunkUnsupportedVersion(c.version));
            }
            for (position, leaf) in c.leaves.iter().enumerate() {
                // Index bound: the manifest fixes `leaf_count`, so an out-of-range index is a leaf that
                // can never be part of `leaf_root` — i.e. pure blob-store pollution.
                if leaf.leaf_index >= manifest.leaf_count {
                    return Err(PalwOverlayError::LeafIndexOutOfRange {
                        leaf_index: leaf.leaf_index,
                        leaf_count: manifest.leaf_count,
                    });
                }
                // Cross-check the leaf's own `batch_id` against the chunk's, so a chunk cannot smuggle a
                // leaf that claims membership in a different batch.
                if leaf.batch_id != c.batch_id {
                    return Err(PalwOverlayError::LeafBatchIdMismatch);
                }

                // ---- kaspa-pq ADR-0040 §5.14.3 item 7 (P1-10 prerequisite) — PIN THE LEAF'S EPOCH ----
                //
                // `validate_public_leaf` constrains `registered_epoch` only RELATIONALLY (it must be less
                // than `activation_epoch`). Nothing tied it to the batch. The manifest side is pinned:
                // `admission_valid` refuses `registration_epoch != accept_epoch`, and a batch must pass
                // through `apply_manifest` into the fork-relative view before `check_palw_ticket`'s
                // `view.resolvable_batch` will let any header mine against it. So the manifest's number is
                // the batch's real acceptance epoch; the leaf's was free.
                //
                // That freedom is not cosmetic — `palw_work_reward_class` reads `leaf.registered_epoch`
                // and feeds it to `palw_premium_at_window`, i.e. it selects which π-controller window
                // prices the leaf. The controller returns the neutral constant today, so nothing is
                // mispriced YET; this closes the degree of freedom BEFORE the sampler lands rather than
                // after, because once the premium varies the leaf is immutable and the choice is already
                // committed.
                //
                // Chained with the manifest-side pin this gives §5.14.3 item 7 in full:
                //   mineable ⇒ resolvable in view(SP) ⇒ admission_valid ⇒ registration_epoch == accept
                //   epoch, and (here) leaf.registered_epoch == registration_epoch.
                // The acceptance arm alone does NOT reach "the real epoch" — it binds leaf to manifest,
                // and the view's admission gate binds manifest to the carrier epoch. A batch that never
                // enters the view may still be stored with any declared epoch; it simply cannot mint.
                if leaf.registered_epoch != manifest.registration_epoch {
                    return Err(PalwOverlayError::LeafRegistrationEpochMismatch {
                        leaf_index: leaf.leaf_index,
                        leaf_registered_epoch: leaf.registered_epoch,
                        manifest_registration_epoch: manifest.registration_epoch,
                    });
                }

                // ---- kaspa-pq ADR-0040 §5.15.4(3) (ACCEPT-BIND/M2) — THE MEMBERSHIP GATE ----
                //
                // Everything above checks what the leaf DECLARES about itself; none of it binds the
                // leaf's CONTENT to the batch. `leaf.batch_id` is a field the submitter wrote, and
                // `batch_id` is public — so before this gate an observer could copy an honest
                // `batch_id`, author leaves naming its own reward scripts and ticket authority, and win
                // the write-once race at `(batch_id, leaf_index)`. The honest auditors' certificate then
                // covered the squatter's leaves, because nothing ever re-derived `leaf_root` from what
                // was stored (`palw_leaf_root` had ZERO consensus callers, and the "§9.3 completeness
                // gate" its doc named did not exist — §5.15.2/§5.15.3).
                //
                // WHY IT MUST BE HERE, BEFORE `insert_leaf`, and not at certificate admission: a gate
                // that re-derived the root from the store at certificate time would close THEFT but
                // leave DENIAL wide open and, worse, permanent — the poisoned leaves are already
                // stored, the honest leaf is then refused as differing content, the batch is
                // permanently uncertifiable, and because `batch_id == content_id()` re-registering the
                // same manifest reuses the very same poisoned keys. Firing before the write is what
                // makes DENIAL fall out of `insert_leaf`'s same-content idempotence instead: a chunk
                // that passes this gate is byte-identical to the honest chunk, so a front-run degrades
                // to the attacker paying the fee to publish the victim's own data, and the honest
                // transaction still succeeds.
                let proof = c
                    .proofs
                    .get(position)
                    .ok_or(PalwOverlayError::LeafProofCountMismatch { leaves: c.leaves.len(), proofs: c.proofs.len() })?;
                // The EXACT length — both too short and too long are rejected — and BEFORE any hashing.
                // `palw_verify_leaf_membership` re-checks this internally; the explicit check here is
                // what gives the failure its own attributable variant and pins the ordering.
                let expected_proof_len = palw_leaf_merkle_depth(manifest.leaf_count);
                if proof.siblings.len() as u32 != expected_proof_len {
                    return Err(PalwOverlayError::LeafMembershipProofLengthInvalid {
                        leaf_index: leaf.leaf_index,
                        got: proof.siblings.len(),
                        expected: expected_proof_len,
                    });
                }
                // The tree is built over the `batch_id`-ZEROED projection of each leaf (`leaf_root` is
                // itself inside `content_id()`, which is the definition of `batch_id` — a leaf hash that
                // included `batch_id` would make the manifest's identity self-referential). Producers
                // project identically in `mil::miner::registration::ordered_batch_leaf_hashes`.
                //
                // NOTE (§5.15.12, FIXED-POINT): this is deliberately NOT the same digest as
                // `resolve_palw_binding`'s, which keeps `batch_id` populated on purpose for the
                // eligibility draw. The tree therefore holds two intentionally different hashes of the
                // same leaf. Do not "de-duplicate" them.
                let mut projected = leaf.clone();
                projected.batch_id = Hash64::default();
                // Direction bits come from `leaf.leaf_index`, never from the payload, and the index is
                // bound INSIDE the level-0 node — so a leaf that is a legitimate member at index i
                // cannot be replayed as a member at any j != i.
                if !palw_verify_leaf_membership(
                    &projected.leaf_hash(),
                    leaf.leaf_index,
                    manifest.leaf_count,
                    proof,
                    &manifest.leaf_root,
                ) {
                    return Err(PalwOverlayError::LeafMembershipProofInvalid { leaf_index: leaf.leaf_index });
                }

                // ---- ADR-0045 D3-b — clauses 11/12, ALL enforced here, BEFORE `insert_leaf` ----
                //
                // This is the "全 clause 同時有効" decision made real: no leaf reaches the store
                // without (11) a freshness window over content-sealed epochs AND a challenge that
                // actually re-derives under `R_{issued}`, and (12) dispatch evidence that re-runs
                // against the independently store-resolved snapshot + post-commit beacon, with the
                // drawn providers bound to the leaf's declared seats. Every unresolvable context
                // read is a rejection (fail-closed) — substituting live values is the grindable
                // middle state D3-b forbids. A caller with no PCPB context cannot skip any of it:
                // the whole chunk is refused.
                let Some(ctx) = pcpb else {
                    return Err(PalwOverlayError::LeafPcpbContextUnresolvable {
                        leaf_index: leaf.leaf_index,
                        epoch: leaf.receipt_v3_issued_epoch,
                        what: "acceptance ran without a PCPB context",
                    });
                };
                let witness = c.witnesses.get(position).ok_or(PalwOverlayError::LeafPcpbWitnessCountMismatch {
                    leaves: c.leaves.len(),
                    witnesses: c.witnesses.len(),
                })?;
                let issued = leaf.receipt_v3_issued_epoch;
                let anchor = if leaf.dispatch_kind == kaspa_consensus_core::palw::PALW_DISPATCH_KIND_SELF_SERIAL {
                    leaf.a_commit_epoch
                } else {
                    issued
                };
                // Clause 11, pure half: issued ≤ anchor ∧ anchor+Δ ≤ registered ∧ registered−issued ≤ w.
                if !kaspa_consensus_core::palw::palw_challenge_fresh(
                    issued,
                    anchor,
                    leaf.registered_epoch,
                    ctx.freshness_window_epochs,
                    ctx.post_commit_delta_epochs,
                ) {
                    return Err(PalwOverlayError::LeafChallengeStale {
                        leaf_index: leaf.leaf_index,
                        issued_epoch: issued,
                        anchor_epoch: anchor,
                        registered_epoch: leaf.registered_epoch,
                    });
                }
                // Clause 11, heavy half: the challenge must re-derive under R_{issued} from the
                // witness's preimage triple — otherwise `issued` is a free declaration and the
                // freshness window above binds nothing (cached-activation replay, audit H-10).
                let issued_seed = beacon
                    .beacon_seed_at(issued)
                    .map_err(|_| PalwOverlayError::StoreError)?
                    .ok_or(PalwOverlayError::LeafPcpbContextUnresolvable {
                        leaf_index: leaf.leaf_index,
                        epoch: issued,
                        what: "issued-epoch beacon seed",
                    })?;
                let expected_challenge = kaspa_consensus_core::palw::palw_job_challenge(
                    ctx.network_id,
                    issued,
                    &issued_seed,
                    &witness.scheduler_job_id,
                    &witness.requester_credential,
                    &witness.request_commitment,
                    leaf.shape_id,
                );
                if expected_challenge != leaf.receipt_v3_job_challenge {
                    return Err(PalwOverlayError::LeafChallengeMismatch { leaf_index: leaf.leaf_index });
                }
                // Clause 12: resolve the anchor's epoch context (snapshot at anchor−k, draw beacon
                // at anchor+Δ), check the self arm's on-chain ordering anchor, and re-run the
                // dispatch evidence with the leaf's declared seats bound in.
                let snapshot_epoch =
                    anchor.checked_sub(ctx.snapshot_lag_epochs).ok_or(PalwOverlayError::LeafPcpbContextUnresolvable {
                        leaf_index: leaf.leaf_index,
                        epoch: anchor,
                        what: "anchor predates the snapshot lag",
                    })?;
                let resolved = ctx
                    .pcpb_store
                    .snapshot_at(snapshot_epoch)
                    .map_err(|_| PalwOverlayError::StoreError)?
                    .ok_or(PalwOverlayError::LeafPcpbContextUnresolvable {
                        leaf_index: leaf.leaf_index,
                        epoch: snapshot_epoch,
                        what: "provider snapshot",
                    })?;
                let draw_epoch =
                    anchor.checked_add(ctx.post_commit_delta_epochs).ok_or(PalwOverlayError::LeafPcpbContextUnresolvable {
                        leaf_index: leaf.leaf_index,
                        epoch: anchor,
                        what: "anchor+Δ overflows",
                    })?;
                let post_commit_beacon = beacon
                    .beacon_seed_at(draw_epoch)
                    .map_err(|_| PalwOverlayError::StoreError)?
                    .ok_or(PalwOverlayError::LeafPcpbContextUnresolvable {
                        leaf_index: leaf.leaf_index,
                        epoch: draw_epoch,
                        what: "post-commit beacon seed",
                    })?;
                if leaf.dispatch_kind == kaspa_consensus_core::palw::PALW_DISPATCH_KIND_SELF_SERIAL {
                    let registry_epoch =
                        ctx.pcpb_store.acommit_epoch(&leaf.a_commit).map_err(|_| PalwOverlayError::StoreError)?;
                    // `row ≤ declared`: the anchor was on-chain at-or-before the epoch whose `+Δ`
                    // beacon draws B, so that beacon provably post-dates the commitment. (The leaf
                    // cannot claim EARLIER than the row — that direction would let a late anchor
                    // borrow an already-known beacon.)
                    if !registry_epoch.is_some_and(|row| row <= leaf.a_commit_epoch) {
                        return Err(PalwOverlayError::LeafACommitUnanchored {
                            leaf_index: leaf.leaf_index,
                            declared_epoch: leaf.a_commit_epoch,
                            registry_epoch,
                        });
                    }
                }
                let facts = kaspa_consensus_core::palw::PalwDispatchLeafFacts {
                    snapshot_root: leaf.provider_snapshot_root,
                    assignment_root: leaf.assignment_proof_root,
                    a_commit: leaf.a_commit,
                    dispatch_kind: leaf.dispatch_kind,
                    provider_a_id: kaspa_consensus_core::palw::palw_provider_id(&leaf.provider_a_bond),
                    provider_b_id: kaspa_consensus_core::palw::palw_provider_id(&leaf.provider_b_bond),
                };
                if !kaspa_consensus_core::palw::palw_dispatch_evidence_valid(
                    &witness.dispatch,
                    &resolved,
                    &post_commit_beacon,
                    &facts,
                    &TxscriptPalwMlDsaVerifier,
                ) {
                    return Err(PalwOverlayError::LeafDispatchEvidenceInvalid { leaf_index: leaf.leaf_index });
                }
                // Global job-nullifier accounting belongs at the reward/virtual coordinate. Applying a
                // fork-relative rule while writing the content-addressed blob store would make admission
                // depend on processing coordinate and order.
                //
                // Blob persistence is therefore permissive, and duplicate-work rejection is an
                // Activation-class gate that blocks mainnet activation — not a body-validity rule.

                // Preflight write-once state for the WHOLE chunk before writing any leaf. A previous
                // implementation inserted each leaf at the end of this validation loop, so a malformed
                // later leaf returned a semantic error after earlier leaves had already escaped into the
                // direct-write blob store. Full preflight keeps every semantic rejection side-effect
                // free; only infrastructure failures can interrupt the write pass below.
                match store.leaf(c.batch_id, leaf.leaf_index) {
                    Ok(existing) if existing.leaf_hash() != leaf.leaf_hash() => {
                        return Err(PalwOverlayError::LeafImmutabilityViolation);
                    }
                    Ok(_) => {}
                    Err(error) if error.is_key_not_found() => {}
                    Err(_) => return Err(PalwOverlayError::StoreError),
                }
            }
            // Every contextual check and every existing-slot comparison succeeded. From this point on,
            // any failed insert is an infrastructure/consistency failure and must drive the caller's
            // process-wide fail-stop; it is never downgraded to an inert payload rejection after a
            // partial write.
            for leaf in &c.leaves {
                store.insert_leaf(c.batch_id, leaf.leaf_index, Arc::new(leaf.clone())).map_err(|_| PalwOverlayError::StoreError)?;
            }
            Ok(())
        }
        PalwOverlayEffect::Certificate(cert) => {
            if cert.version != kaspa_consensus_core::palw::PALW_BATCH_CERTIFICATE_VERSION_V2 {
                return Err(PalwOverlayError::CertificateUnsupportedVersion(cert.version));
            }
            // kaspa-pq **ADR-0040 P1-4 (BIND-02 / BIND-05)** — bind the certificate to the batch it claims
            // to certify BEFORE persisting it.
            //
            // Previously this arm persisted the blob unconditionally ("keyed by its own hash, so it is
            // self-content-addressed"). Self-addressing makes the KEY honest; it says nothing about the
            // CONTENTS. Downstream, `is_block_eligible_at` only requires `cert_hash.is_some()`, and
            // `resolve_palw_binding` reads only the certificate's epoch window — so an unbound certificate
            // blob in the store could satisfy an algo-4 header for a batch it never certified.
            //
            // The three fields below are the ones the design says identify the certified batch, and all
            // three were decoded-but-never-read. Checking them here means a certificate that names the
            // wrong batch, the wrong manifest, or the wrong leaf set can never reach the view at all.
            //
            // NOT closed here (needs the provider/auditor bond state, ADR-0040 P2-5/P2-7): ML-DSA
            // verification of each vote, auditor-selection membership, stake-weighted quorum, and
            // `audit_sample_root` re-derivation. A vote carries only a `bond_outpoint`, so resolving it to
            // a signing key requires the bond store that does not exist yet. Until then a certificate is
            // *correctly bound* but *not yet attested* — which is exactly why `palw_algo4_accept` stays
            // false (ADR-0040 §7.1.1: CERT-01 is gate G4, an `Activation`-class gate).
            let manifest = store.batch_manifest(cert.batch_id).map_err(|error| {
                if error.is_key_not_found() { PalwOverlayError::UnknownBatch } else { PalwOverlayError::StoreError }
            })?;
            if cert.manifest_hash != manifest.content_id() {
                return Err(PalwOverlayError::CertificateManifestMismatch);
            }
            if cert.leaf_root != manifest.leaf_root {
                return Err(PalwOverlayError::CertificateLeafRootMismatch);
            }
            if cert.activation_epoch != manifest.activation_not_before_epoch {
                return Err(PalwOverlayError::CertificateActivationEpochMismatch);
            }
            if cert.expiry_epoch != manifest.expiry_epoch {
                return Err(PalwOverlayError::CertificateExpiryEpochMismatch);
            }
            // kaspa-pq ADR-0040 §5.17 (AUTHSET-01 / SAMPLE-01) — the ATTESTATION half. Both re-derivations
            // read the batch's on-chain leaves (`provider_{a,b}_bond` for the auditor-set exclusions,
            // `receipt_da_root` for the sample), so resolve them here in index order `[0, leaf_count)`
            // from the leaf store the arm already keys into. A missing leaf FAILS CLOSED: the certificate
            // cannot be attested against a batch whose leaves are not fully on chain.
            if let Some(ctx) = attest {
                if cert.certificate_epoch != ctx.inclusion_epoch {
                    return Err(PalwOverlayError::CertificateInclusionEpochMismatch);
                }
                let mut leaves: Vec<Arc<PalwPublicLeafV1>> = Vec::with_capacity(manifest.leaf_count as usize);
                for leaf_index in 0..manifest.leaf_count {
                    let leaf = store.leaf(cert.batch_id, leaf_index).map_err(|error| {
                        if error.is_key_not_found() { PalwOverlayError::CertificateLeafAbsent } else { PalwOverlayError::StoreError }
                    })?;
                    leaves.push(leaf);
                }
                verify_certificate_attestation(&cert, &manifest, ctx, &leaves)?;
            }
            store.insert_certificate(cert.hash(), Arc::new(cert)).map_err(|_| PalwOverlayError::StoreError)
        }
    }
}

/// The store-resolved facts an algo-4 (PALW) header binds to (ADR-0039 §14.2 / §18.1): the pure
/// [`PalwTicketBinding`] fed to [`kaspa_consensus_core::palw::verify_palw_ticket_store_facts`], the
/// certificate's active window (so the caller computes `cert_active` at the block's epoch), and the
/// resolved `leaf_hash` (the eligibility-draw preimage input the beacon slice will consume).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwResolvedBinding {
    pub binding: PalwTicketBinding,
    pub cert_activation_epoch: u64,
    pub cert_expiry_epoch: u64,
    pub leaf_hash: Hash64,
    /// ADR-0040 P1-6 (AUTH-03): the leaf's declared ticket authority. Projected here because it had
    /// ZERO production readers — the field named an authority nothing checked. Clause 7 checks it.
    pub ticket_authority_pk_hash: Hash64,
    /// ADR-0045 D3-b clause 13 — the leaf's PCPB commitments, projected for the mint-time
    /// presence/equality re-check. Read-only projection; the authoritative verification of these
    /// values happened at acceptance (clauses 11/12).
    pub pcpb: PalwResolvedPcpb,
}

/// The clause-13 projection of a resolved leaf's PCPB fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwResolvedPcpb {
    pub dispatch_kind: u8,
    pub a_commit: Hash64,
    pub a_commit_epoch: u64,
    pub issued_epoch: u64,
    pub provider_snapshot_root: Hash64,
    pub assignment_proof_root: Hash64,
}

/// Why an algo-4 header's overlay binding could not be resolved from the stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBindingError {
    /// No leaf at `(palw_batch_id, palw_leaf_index)` — the ticket references a leaf not on-chain.
    LeafAbsent,
    /// No certificate at `palw_epoch_certificate_hash` — the batch has no on-chain certification.
    CertAbsent,
    /// A certificate blob exists but is not the summary-bound V2 wire revision.
    CertUnsupportedVersion(u16),
    /// kaspa-pq **ADR-0040 CERT-BATCH** — a certificate WAS found at `palw_epoch_certificate_hash`, but
    /// it certifies a DIFFERENT batch than the header's `palw_batch_id`.
    ///
    /// Distinct from [`Self::CertAbsent`] on purpose: "the blob is missing" and "the blob is present but
    /// belongs to someone else" are different operator-visible failures, and collapsing them would hide
    /// a substitution attempt behind a benign-looking propagation gap.
    CertBatchMismatch,
}

/// ADR-0039 §18.1 — resolve the leaf + certificate an algo-4 header names into the pure verify inputs.
/// This is the concrete `verify_palw_ticket ↔ PalwStore` bridge: a header carries `(batch_id, leaf_index,
/// epoch_certificate_hash, target_daa_interval)`; this reads the corresponding [`PalwPublicLeafV1`] and
/// [`PalwBatchCertificateV2`] and packs them into a [`PalwResolvedBinding`]. Store absence fails closed
/// (`LeafAbsent` / `CertAbsent`). Pure w.r.t. the store snapshot — the caller then applies
/// [`kaspa_consensus_core::palw::verify_palw_ticket_store_facts`] (and, once the beacon / lane-DAA /
/// checkpoint / compute-cap state is live, the full [`kaspa_consensus_core::palw::verify_palw_ticket`]).
pub fn resolve_palw_binding(
    batch_id: Hash64,
    leaf_index: u32,
    epoch_certificate_hash: Hash64,
    target_daa_interval: u64,
    store: &dyn PalwStoreReader,
) -> Result<PalwResolvedBinding, PalwBindingError> {
    let leaf = store.leaf(batch_id, leaf_index).map_err(|_| PalwBindingError::LeafAbsent)?;
    let cert = store.certificate(epoch_certificate_hash).map_err(|_| PalwBindingError::CertAbsent)?;
    if cert.version != PALW_BATCH_CERTIFICATE_VERSION_V2 {
        return Err(PalwBindingError::CertUnsupportedVersion(cert.version));
    }
    // kaspa-pq **ADR-0040 CERT-BATCH** — the certificate is resolved BY HASH ALONE, so without this the
    // resolver would happily project ANY stored certificate's window onto ANY batch. `cert.batch_id` is
    // the certified subject and is covered by `PalwBatchCertificateV2::hash` (hence by the store key), so
    // comparing it here is a total, cheap cross-bind. Kept in the RESOLVER rather than only at the call
    // site so every present and future caller inherits it.
    //
    // Scope note (honest): this closes CROSS-BATCH substitution. It does NOT pin WHICH of a batch's own
    // certificates a header may name — several attested certificates for one batch legitimately coexist
    // in the content-addressed store, and the fork-relative view deliberately no longer records a
    // canonical winner (see `PalwBatchViewV1::apply_certificate`, ADR-0040 CERT-TRUST): making the view's
    // first-arrival `cert_hash` binding would hand any unattested overlay tx a censorship lever, which is
    // the very failure CERT-TRUST removes. Every same-batch alternative is itself quorum-attested and
    // manifest/leaf-root-bound, and the field is not free to an OBSERVER either — the clause-7
    // authorization commits to the whole header preimage, `palw_epoch_certificate_hash` included.
    if cert.batch_id != batch_id {
        return Err(PalwBindingError::CertBatchMismatch);
    }
    Ok(PalwResolvedBinding {
        binding: PalwTicketBinding {
            ticket_nullifier_commitment: leaf.ticket_nullifier_commitment,
            proof_type: leaf.proof_type,
            leaf_activation_epoch: leaf.activation_epoch,
            leaf_expiry_epoch: leaf.expiry_epoch,
            target_daa_interval,
        },
        cert_activation_epoch: cert.activation_epoch,
        cert_expiry_epoch: cert.expiry_epoch,
        // ---- kaspa-pq ADR-0040 §5.15.12 (FIXED-POINT) — READ THIS BEFORE "DE-DUPLICATING" ----
        //
        // This is the `batch_id`-POPULATED `leaf_hash()`, and that is DELIBERATE. It feeds the clause-9
        // eligibility draw (`eligibility_hash`), where binding the draw to the batch the ticket names is
        // the whole point.
        //
        // The ACCEPTANCE arm (`apply_palw_overlay_effect`, LeafChunk) hashes the SAME leaf with
        // `batch_id` ZEROED, because the Merkle `leaf_root` it opens sits inside `content_id()`, which IS
        // `batch_id` — a populated hash there would be an unsolvable fixed point. So the tree carries two
        // intentionally different hashes of one leaf.
        //
        // Collapsing them breaks something in either direction: switching the acceptance arm to this
        // digest makes every honest chunk unopenable (silently — the arm's error is discarded by the
        // production caller), and switching this line to the projected digest unbinds the eligibility
        // draw from the batch. `producer_built_batch_round_trips_through_the_real_acceptance_arm` asserts
        // BOTH directions, so a de-duplication fails loudly rather than shipping.
        leaf_hash: leaf.leaf_hash(),
        ticket_authority_pk_hash: leaf.ticket_authority_pk_hash,
        pcpb: PalwResolvedPcpb {
            dispatch_kind: leaf.dispatch_kind,
            a_commit: leaf.a_commit,
            a_commit_epoch: leaf.a_commit_epoch,
            issued_epoch: leaf.receipt_v3_issued_epoch,
            provider_snapshot_root: leaf.provider_snapshot_root,
            assignment_proof_root: leaf.assignment_proof_root,
        },
    })
}

/// ADR-0039 §12.3 — the `R_E → eligibility_digest` bridge: resolve the beacon seed active for a block
/// (the seed carried by its `selected_parent`, past-relative + reorg-safe) and compute the header's
/// one-shot draw digest via [`kaspa_consensus_core::palw::eligibility_hash`]. Every other input is on the
/// header, in config (`network_id`), or resolvable from the leaf store (`leaf_hash` via
/// [`resolve_palw_binding`]). Returns `None` when the beacon has not yet produced a seed in this block's
/// history.
///
/// This computation helper is exercised by conformance tests but is not called by
/// `check_palw_ticket`. Eligibility must be enforced together with the lane difficulty and chain
/// commitment checks: `palw_eligibility_win` compares against the header's `bits`, so applying it
/// without validating `bits` would not provide a meaningful gate.
pub fn resolve_palw_eligibility(
    beacon: &DbPalwBeaconStore,
    selected_parent: kaspa_consensus_core::BlockHash,
    network_id: u32,
    header_chain_commit: &Hash64,
    header_target_interval: u64,
    header_batch_id: &Hash64,
    header_leaf_index: u32,
    leaf_hash: &Hash64,
    header_ticket_nullifier: &Hash64,
) -> Result<Option<Hash64>, kaspa_database::prelude::StoreError> {
    let Some(state) = beacon.beacon_state(selected_parent)? else { return Ok(None) };
    Ok(Some(kaspa_consensus_core::palw::eligibility_hash(
        network_id,
        &state.seed,
        header_chain_commit,
        header_target_interval,
        header_batch_id,
        header_leaf_index,
        leaf_hash,
        header_ticket_nullifier,
    )))
}

/// ADR-0039 §12.1 — the clause-6 bridge: resolve a header's `expected_chain_commit` from the beacon
/// record carried at its **selected parent** (design-panel resolution: the exact same single record
/// read clause 9 uses via [`resolve_palw_eligibility`] — cert and seed share one provenance, so
/// clause 6 adds zero fork-degrees-of-freedom over clause 9, c==v is structural, and a boundary-
/// crossing header simply binds the previous epoch's frozen facts). The certificate digest is derived
/// on demand from the record's anchor-pure facts ([`PalwBeaconStateV1::dns_certificate_hash`]).
///
/// Returns `None` when no carried record or DNS-confirmed anchor exists; the zero bootstrap anchor
/// certifies nothing. Like [`resolve_palw_eligibility`], this helper is covered by conformance tests
/// but is not called by `check_palw_ticket`.
pub fn resolve_palw_chain_commit(
    beacon: &DbPalwBeaconStore,
    selected_parent: kaspa_consensus_core::BlockHash,
    network_id: u32,
    target_interval: u64,
) -> Result<Option<Hash64>, kaspa_database::prelude::StoreError> {
    let Some(state) = beacon.beacon_state(selected_parent)? else { return Ok(None) };
    let Some(certificate) = state.dns_certificate_hash() else { return Ok(None) };
    Ok(Some(kaspa_consensus_core::palw::chain_commit(&state.dns_anchor, &certificate, target_interval, network_id)))
}

/// ADR-0039 §16.3 — the clause-7 HOLD bridge: resolve a lane's carried "last bits" from the block-keyed
/// lane-bits store at a block's **selected parent** (past-relative). A `None` row (genesis / a
/// pre-activation parent) falls back to the lane's `genesis_bits` — so the first PALW blocks HOLD the
/// genesis lane difficulty rather than reading the selected parent's `header.bits` (which, at a
/// mixed-lane boundary, is the OTHER lane's difficulty — the structural blocker). This is the retarget
/// HOLD source; the full lane window build + `lane_retarget_bits` Adjust path is the pipeline wiring.
/// Tested seam — NOT spliced into the enforced difficulty check until the C7 pipeline wiring + C5 flip.
pub fn resolve_palw_lane_hold_bits(
    lane_bits_store: &crate::model::stores::palw_lane_bits::DbPalwLaneBitsStore,
    selected_parent: kaspa_consensus_core::BlockHash,
    lane: kaspa_consensus_core::pow_layer0::WorkLane,
    lane_params: &kaspa_consensus_core::palw::LaneDifficultyParams,
) -> Result<u32, kaspa_database::prelude::StoreError> {
    Ok(match lane_bits_store.lane_bits(selected_parent)? {
        Some(carried) => carried.lane_bits(lane),
        None => lane_params.genesis_bits(lane),
    })
}

/// ADR-0039 §12.1 / C6 SLICE 0 — resolve the **finality-buried DNS anchor** for a block from its
/// selected parent, as a PURE FUNCTION OF THE PAST over `(headers_store, reachability, dns_params)`
/// alone — no virtual/UTXO/bond state. This is the body-stage-callable extraction of the virtual
/// processor's `canonical_anchor_by_blue_score`, using the **window-INDEPENDENT** variant (no
/// `stake_score_window` break) so the resolved anchor is tip-independent for a buried epoch and the
/// miner's template + the validator resolve the identical anchor across a pruning-window advance
/// (construction==validation, C6 panel SLICE 0). The anchor is buried by `attestation_lag_blue_score`;
/// the re-genesis band gate (`palw_checkpoint_params_consistent`, C6 SLICE 5) additionally requires the
/// lag to exceed the reorg horizon (so the anchor's selected-chain identity is settled) and stay below
/// the pruning depth (so its header survives on pruned nodes). Returns `None` before any lag-ready epoch
/// exists in this block's history.
pub fn resolve_palw_lagged_anchor(
    headers: &DbHeadersStore,
    reachability: &MTReachabilityService<DbReachabilityStore>,
    dns_params: &kaspa_consensus_core::dns_finality::DnsParams,
    selected_parent: kaspa_consensus_core::BlockHash,
) -> Option<kaspa_consensus_core::dns_finality::CanonicalLaggedEpochAnchor> {
    use kaspa_consensus_core::dns_finality::{
        anchor_cutoff_blue_score, canonical_lagged_epoch_anchor, ready_epoch_from_tip_blue_score,
    };
    let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
    let lag = dns_params.attestation_lag_blue_score;
    let backoff = dns_params.attestation_anchor_backoff_blue_score;
    let sp_blue = headers.get_blue_score(selected_parent).ok()?;
    let dns_epoch = ready_epoch_from_tip_blue_score(sp_blue, epoch_len, lag)?;
    let cutoff = anchor_cutoff_blue_score(dns_epoch, epoch_len, backoff);
    if cutoff > sp_blue {
        return None;
    }
    // Walk the selected-parent chain down until the PREVIOUS epoch's cutoff is buried (decidable
    // duplicate-anchor check), with NO window cap (the acceptance path must resolve the identical
    // canonical anchor even after a pruning-window advance). Position is read from blue_score.
    let needed = anchor_cutoff_blue_score(dns_epoch.saturating_sub(1), epoch_len, backoff);
    let mut ancestors: Vec<(kaspa_consensus_core::BlockHash, u64, u64)> = Vec::new();
    for hash in std::iter::once(selected_parent).chain(reachability.default_backward_chain_iterator(selected_parent)) {
        let compact = headers.get_compact_header_data(hash).ok()?;
        ancestors.push((hash, compact.blue_score, compact.daa_score));
        if compact.blue_score <= needed {
            break;
        }
    }
    canonical_lagged_epoch_anchor(dns_epoch, epoch_len, backoff, &ancestors)
}

/// ADR-0039 §11.3 (K5, clause-10 sampler) — collect one `(palw_epoch, palw_beacon_seed)` sample per
/// PALW DAA epoch (keyed `daa_score / palw_epoch_length_daa` — NOT per DNS anchor: consecutive DNS
/// anchors inside one PALW epoch legitimately share a seed and must not read as a carry), walking the
/// selected-parent chain DOWN from the finality-buried clause-6 anchor (inclusive). Every sampled
/// header sits at or below the anchor, so its `palw_beacon_seed` is trustworthy by the same burial
/// argument as clause 6 (it was S2-authenticated as a chain block, then buried past the reorg horizon).
/// A pure function of the past over `(headers, reachability)` — no virtual/beacon-store read (the C5
/// hazard this K5 wiring exists to avoid).
///
/// FAIL-OPEN stops (return what was collected so far): a pre-v3 / pre-activation / default-zero-seed
/// header (the activation boundary carries no derivable seed — a zero seed must break the run, never
/// extend it), any header-read miss (pruned history), or `max_epochs` distinct epochs collected.
/// Returned ASCENDING by epoch, ready for [`palw_seed_carry_run`] / [`palw_lagged_activation_open`]
/// (both of which are themselves fail-open on `< 2` samples).
pub fn resolve_palw_buried_epoch_seeds(
    headers: &DbHeadersStore,
    reachability: &MTReachabilityService<DbReachabilityStore>,
    anchor_hash: kaspa_consensus_core::BlockHash,
    palw_activation_daa_score: u64,
    palw_epoch_length_daa: u64,
    max_epochs: u64,
) -> Vec<(u64, kaspa_hashes::Hash64)> {
    use kaspa_consensus_core::constants::PALW_HEADER_VERSION;
    let epoch_len = palw_epoch_length_daa.max(1);
    let mut samples: Vec<(u64, kaspa_hashes::Hash64)> = Vec::new();
    for hash in std::iter::once(anchor_hash).chain(reachability.default_backward_chain_iterator(anchor_hash)) {
        let Ok(header) = headers.get_header(hash) else { break }; // pruned history ⇒ fail-open stop
        if header.version < PALW_HEADER_VERSION
            || header.daa_score < palw_activation_daa_score
            || header.palw_beacon_seed == kaspa_hashes::Hash64::default()
        {
            break; // activation boundary / underivable seed ⇒ fail-open stop
        }
        let epoch = header.daa_score / epoch_len;
        // Walking DOWN, the first header seen for an epoch is the NEWEST buried header of that epoch
        // (every header within one epoch carries the same seed — the derivation advances only at epoch
        // boundaries — so any representative is equivalent; we key on first-seen).
        if samples.last().is_none_or(|&(e, _)| e != epoch) {
            if samples.len() as u64 >= max_epochs {
                break;
            }
            samples.push((epoch, header.palw_beacon_seed));
        }
    }
    samples.reverse(); // collected newest→oldest; return ascending
    samples
}

/// kaspa-pq **ADR-0040 §5.17.3 (§CERT-REDERIVE — the shared missing primitive)** — resolve the
/// audit-epoch beacon seed `R_{audit_beacon_epoch − 1}` at the certificate-verification coordinate
/// ([`verify_certificate_attestation`]), as an ORDER-INDEPENDENT pure function of `(headers,
/// reachability)` over the block-being-validated's SELECTED-PARENT chain.
///
/// All three CERT-REDERIVE findings (AUTHSET-01 / SAMPLE-01 / SEL-01) need this seed: the weighted
/// auditor sampler is keyed by it, and the `audit_sample_root` redefinition samples on-chain leaves by
/// it. It is built once, here, so every consumer reads the identical value.
///
/// # What it returns
///
/// The `palw_beacon_seed` carried by the NEWEST buried header whose PALW epoch equals
/// `audit_beacon_epoch − 1`, found by walking DOWN the selected-parent chain from `selected_parent` via
/// [`MTReachabilityService::default_backward_chain_iterator`] — the SAME buried-walk pattern as
/// [`resolve_palw_buried_epoch_seeds`] / [`resolve_palw_lagged_anchor`]. Every header within one PALW
/// epoch carries the same seed (the derivation advances only at epoch boundaries), so the first header
/// seen for the target epoch while descending is an equivalent representative.
///
/// # The seed source is the block-keyed header field, NOT the epoch-keyed accum store
///
/// The ONLY order-independent seed source is `header.palw_beacon_seed` (Header v3, already carried over
/// P2P/RPC). The epoch-keyed `accum` store is DELIBERATELY NOT read: a side branch processed first
/// contaminates `R_E` there, and reading it would make the resolved seed depend on block-arrival order —
/// a consensus split. This mirrors the explicit prohibition in `resolve_palw_buried_epoch_seeds` and in
/// the C5 hazard note it exists to avoid.
///
/// # FAIL-CLOSED (returns `None`), by design
///
/// - `audit_beacon_epoch == 0` — there is no previous-epoch seed to resolve.
/// - a header read misses (pruned history) before the target epoch is reached — the audit epoch has
///   fallen off this node's history. Soundness of treating this as a REJECT rests on the §5.17.3 bounded
///   inclusion rule ([`kaspa_consensus_core::palw::palw_certificate_included_within_audit_window`]),
///   which keeps a valid certificate's audit epoch within an unpruned window so this branch is never hit
///   for an honest certificate.
/// - a pre-v3 / pre-activation / default-zero-seed header is reached — the activation boundary carries no
///   derivable seed, so the audit epoch's predecessor is underivable.
/// - the walk descends BELOW the target epoch without a hit (the audit epoch is in the block's FUTURE, or
///   a daa gap) — the seed is not in this block's past.
///
/// Note the polarity flip from [`resolve_palw_buried_epoch_seeds`], which fail-OPENS (returns what it
/// collected) on the same boundary conditions because it is a best-effort *sampler* feeding fail-open
/// consumers. Here an unresolved seed must REJECT the certificate, so every stop returns `None`.
///
/// # ORDER-INDEPENDENCE PROOF
///
/// The walk starts at the fixed `selected_parent` of the block being validated and follows
/// `default_backward_chain_iterator`, which yields the deterministic selected-parent chain — a total
/// order fixed by `(headers, reachability)` alone, independent of which sequence of block arrivals or
/// reorgs brought the validating node to this block. Every header read sits at/below the selected parent
/// (strictly in the block's past). `header.palw_beacon_seed`, `header.daa_score`, `header.version` are
/// immutable header fields; `palw_epoch_length_daa` and `palw_activation_daa_score` come from `Params`,
/// identical on every node. No read touches the epoch-keyed accum store or any virtual/mutable/tip-
/// relative state. Therefore two nodes that reach the same block by ANY path compute a byte-identical
/// `R_{audit_beacon_epoch − 1}`. ∎
///
/// `verify_certificate_attestation` calls this to resolve the
/// audit-epoch seed at the frozen snapshot; a `None` return (pruned / pre-activation / zero-seed) is a
/// fail-CLOSED certificate rejection, made sound by the bounded-inclusion rule
/// `palw_certificate_included_within_audit_window`.
pub fn resolve_palw_audit_epoch_seed(
    headers: &DbHeadersStore,
    reachability: &MTReachabilityService<DbReachabilityStore>,
    selected_parent: kaspa_consensus_core::BlockHash,
    palw_activation_daa_score: u64,
    palw_epoch_length_daa: u64,
    audit_beacon_epoch: u64,
) -> Option<kaspa_hashes::Hash64> {
    // Feed the selected-parent chain NEWEST→OLDEST as (version, daa_score, palw_beacon_seed) facts into
    // the pure selector [`kaspa_consensus_core::palw::palw_audit_epoch_seed_select`]. `map_while(.ok())`
    // TRUNCATES at the first header-read miss (pruned history); the selector runs off the end of a
    // truncated sequence and returns `None` — the same fail-CLOSED-on-pruned behaviour as an explicit
    // per-header read check, and the same pruned-history stop `resolve_palw_buried_epoch_seeds` uses.
    let facts = std::iter::once(selected_parent)
        .chain(reachability.default_backward_chain_iterator(selected_parent))
        .map_while(|hash| headers.get_header(hash).ok())
        .map(|h| (h.version, h.daa_score, h.palw_beacon_seed));
    kaspa_consensus_core::palw::palw_audit_epoch_seed_select(
        audit_beacon_epoch,
        palw_activation_daa_score,
        palw_epoch_length_daa,
        facts,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw::{
        PalwAuditorVoteV2, PalwLeafMembershipProofV1, PalwPublicLeafV1, palw_leaf_merkle_proof, palw_leaf_merkle_root,
    };
    use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptVec, TransactionOutpoint};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{CachePolicy, ConnBuilder, StoreError};
    use kaspa_hashes::Hash64;

    use crate::model::stores::palw::{DbPalwStore, PalwStore, PalwStoreReader};

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    /// D3-b: the REAL per-leaf witnesses for a producer-built batch — the same values the shared
    /// fixture stamped into each leaf, so the producer's chunk opens under the real acceptance arm.
    /// (A shape-only filler would make the round trip prove nothing about clauses 11/12.)
    fn e2e_pcpb_witnesses(
        leaves: &[PalwPublicLeafV1],
    ) -> std::collections::BTreeMap<u32, kaspa_consensus_core::palw::PalwLeafPcpbWitnessV1> {
        let fixture = pcpb_fixture();
        leaves.iter().map(|l| (l.leaf_index, fixture.witness(l.leaf_index))).collect()
    }

    #[allow(dead_code)]
    fn dummy_pcpb_witnesses(
        leaves: &[PalwPublicLeafV1],
    ) -> std::collections::BTreeMap<u32, kaspa_consensus_core::palw::PalwLeafPcpbWitnessV1> {
        use kaspa_consensus_core::palw::{
            BeaconAssignedProof, PalwAssignmentInterval, PalwDispatchEvidence, PalwLeafPcpbWitnessV1, PalwMerkleMembership,
            PalwProviderSnapshotEntry, SlotAssignmentWitness,
        };
        let entry = |b: u8| PalwProviderSnapshotEntry {
            provider_id: h(b),
            ml_dsa_pk_hash: h(b.wrapping_add(1)),
            bond_sompi: 10,
            reward_script_commitment: h(b.wrapping_add(2)),
        };
        let membership = |b: u8| PalwMerkleMembership { index: 0, leaf: h(b), siblings: Vec::new() };
        let slot = |b: u8, lo: u128| SlotAssignmentWitness {
            entry: entry(b),
            snapshot_membership: membership(b),
            interval: PalwAssignmentInterval { provider_id: h(b), bond_sompi: 10, cumulative_lo: lo },
            assignment_membership: membership(b),
        };
        leaves
            .iter()
            .map(|l| {
                (
                    l.leaf_index,
                    PalwLeafPcpbWitnessV1 {
                        scheduler_job_id: h(0xe1),
                        requester_credential: h(0xe2),
                        request_commitment: h(0xe3),
                        dispatch: PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof {
                            slot_a: slot(0xe4, 0),
                            slot_b: slot(0xe8, 10),
                        }),
                    },
                )
            })
            .collect()
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum InjectedReadFault {
        Manifest,
        Leaf,
    }

    /// Delegates writes and ordinary reads to a real store while injecting one non-missing read error.
    /// This keeps the regression below on the production `PalwStore` surface instead of testing a
    /// duplicated error-classification helper.
    struct ReadFaultStore<'a> {
        inner: &'a DbPalwStore,
        fault: InjectedReadFault,
    }

    impl PalwStoreReader for ReadFaultStore<'_> {
        fn leaf(&self, batch_id: Hash64, leaf_index: u32) -> Result<Arc<PalwPublicLeafV1>, StoreError> {
            if self.fault == InjectedReadFault::Leaf {
                Err(StoreError::DataInconsistency("injected PALW leaf read failure".into()))
            } else {
                self.inner.leaf(batch_id, leaf_index)
            }
        }

        fn batch_manifest(&self, batch_id: Hash64) -> Result<Arc<PalwBatchManifestV1>, StoreError> {
            if self.fault == InjectedReadFault::Manifest {
                Err(StoreError::DataInconsistency("injected PALW manifest read failure".into()))
            } else {
                self.inner.batch_manifest(batch_id)
            }
        }

        fn certificate(&self, cert_hash: Hash64) -> Result<Arc<PalwBatchCertificateV2>, StoreError> {
            self.inner.certificate(cert_hash)
        }

        fn batch_status(&self, batch_id: Hash64) -> Result<kaspa_consensus_core::palw::PalwBatchStatus, StoreError> {
            self.inner.batch_status(batch_id)
        }

        fn has_leaf(&self, batch_id: Hash64, leaf_index: u32) -> Result<bool, StoreError> {
            self.inner.has_leaf(batch_id, leaf_index)
        }
    }

    impl PalwStore for ReadFaultStore<'_> {
        fn insert_leaf(&self, batch_id: Hash64, leaf_index: u32, leaf: Arc<PalwPublicLeafV1>) -> Result<(), StoreError> {
            self.inner.insert_leaf(batch_id, leaf_index, leaf)
        }

        fn insert_manifest(&self, batch_id: Hash64, manifest: Arc<PalwBatchManifestV1>) -> Result<(), StoreError> {
            self.inner.insert_manifest(batch_id, manifest)
        }

        fn insert_certificate(&self, cert_hash: Hash64, cert: Arc<PalwBatchCertificateV2>) -> Result<(), StoreError> {
            self.inner.insert_certificate(cert_hash, cert)
        }

        fn set_batch_status(&self, batch_id: Hash64, status: kaspa_consensus_core::palw::PalwBatchStatus) -> Result<(), StoreError> {
            self.inner.set_batch_status(batch_id, status)
        }
    }

    /// Prevents reintroduction of a job-nullifier registry at the body/mergeset coordinate.
    ///
    /// `job_nullifier` remains a valid field of `PalwPublicLeafV1` and
    /// `ReplicaExecutionReceiptV1`, but body-state storage and accessors for a first-claim-wins registry
    /// are prohibited here.
    #[test]
    fn no_job_nullifier_registry_at_the_body_coordinate() {
        for (name, src) in [
            ("consensus/core/src/palw.rs", include_str!("../../core/src/palw.rs")),
            ("consensus/src/pipeline/body_processor/processor.rs", include_str!("../pipeline/body_processor/processor.rs")),
            ("consensus/src/processes/palw.rs", include_str!("palw.rs")),
        ] {
            for (line_no, line) in src.lines().enumerate() {
                // Doc comments and the ADR-referencing prose are where the withdrawal is EXPLAINED, so
                // they are allowed to name it; code is not.
                let code = line.trim_start();
                if code.starts_with("//") || code.starts_with("///") {
                    continue;
                }
                // Assembled at runtime so this test's own source does not match itself.
                let jn = ["job", "_nullifier"].concat();
                for banned in [format!("{jn}s"), format!("claim_{jn}"), format!("{jn}_spent")] {
                    let banned = banned.as_str();
                    assert!(
                        !code.contains(banned),
                        "{name}:{}: `{banned}` is back. The body/mergeset coordinate cannot authenticate a \
                         job nullifier (no ActiveBondView, no signature verification), so it must not \
                         operate a first-claim-wins registry keyed on one, at any size. See ADR-0040.",
                        line_no + 1
                    );
                }
            }
        }
    }

    /// The fixture batch is two leaves, at indices 0 and 1.
    const FIXTURE_LEAF_COUNT: u32 = 2;

    /// kaspa-pq ADR-0040 §5.14.3 item 7 — the ONE registration epoch the fixture batch is built at.
    /// `leaf_raw`'s `registered_epoch` and `manifest()`'s `registration_epoch` both read it, because the
    /// acceptance arm now requires them to be equal. Two constants here would let the fixture drift back
    /// into the state the rule forbids without any test noticing.
    /// D3-b (design memo §10.2): this must be at least `k + Δ`, because clause 12 resolves the
    /// provider snapshot at `anchor − k` and the draw beacon at `anchor + Δ`. `1` underflowed the
    /// former — the same genesis-boundary condition an honest producer obeys (no batch registers
    /// before epoch `k + Δ`).
    const FIXTURE_REGISTRATION_EPOCH: u64 = 4;

    /// kaspa-pq ADR-0040 §5.15 — the fixture batch's ORDERED, `batch_id`-ZEROED leaf hashes: exactly the
    /// sequence [`palw_leaf_merkle_root`] reduces to `manifest.leaf_root` and [`palw_leaf_merkle_proof`]
    /// opens.
    ///
    /// The projection is why there is no fixed point to solve here: `leaf_root` feeds `content_id()`
    /// which IS `batch_id`, so the tree must not depend on `batch_id` — and it does not.
    fn fixture_leaf_hashes() -> Vec<Hash64> {
        (0..FIXTURE_LEAF_COUNT)
            .map(|i| {
                let mut projected = leaf_raw(i);
                projected.batch_id = Hash64::default();
                projected.leaf_hash()
            })
            .collect()
    }

    /// A v2 leaf chunk carrying DERIVED membership proofs for `leaves`.
    ///
    /// §5.15.9's audit note: a previous "end-to-end" test passed only because it handed consensus
    /// literals where consensus requires derived values. Fixtures here therefore go through the same
    /// `palw_leaf_merkle_proof` a producer uses — a proof is never pasted.
    fn chunk_with_proofs(batch_id: Hash64, leaves: Vec<PalwPublicLeafV1>) -> PalwLeafChunkV1 {
        let hashes = fixture_leaf_hashes();
        let proofs =
            leaves.iter().map(|l| palw_leaf_merkle_proof(&hashes, l.leaf_index).expect("fixture index is in range")).collect();
        let fixture = pcpb_fixture();
        let witnesses = leaves.iter().map(|l| fixture.witness(l.leaf_index)).collect();
        PalwLeafChunkV1 {
            version: kaspa_consensus_core::palw::PALW_LEAF_CHUNK_VERSION_V3,
            batch_id,
            chunk_index: 0,
            leaves,
            proofs,
            witnesses,
        }
    }

    fn manifest() -> PalwBatchManifestV1 {
        manifest_at_epoch(FIXTURE_REGISTRATION_EPOCH)
    }

    /// `registration_epoch` is the only knob varied, so a second batch differs in `content_id()` (hence
    /// `batch_id`) while keeping the SAME leaf set — and therefore the same `leaf_root` and the same
    /// membership proofs.
    ///
    /// kaspa-pq ADR-0040 §5.14.3 item 7: `leaf_root` is deliberately still built over
    /// `fixture_leaf_hashes()`, i.e. over leaves stamped `FIXTURE_REGISTRATION_EPOCH`. So any argument
    /// other than `FIXTURE_REGISTRATION_EPOCH` yields a manifest whose membership proofs VERIFY while its
    /// `registration_epoch` disagrees with the leaves — precisely the fixture the epoch-pin test needs,
    /// and the reason that test cannot be satisfied by a broken proof.
    fn manifest_at_epoch(registration_epoch: u64) -> PalwBatchManifestV1 {
        let mut m = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(1),
            registration_epoch,
            model_profile_id: h(2),
            runtime_class_id: h(3),
            leaf_count: FIXTURE_LEAF_COUNT,
            chunk_count: 1,
            // kaspa-pq ADR-0040 §5.15 — DERIVED, not a literal. This used to be `h(4)`, which is exactly
            // the structural blind spot §5.15.10 warns about: a literal root cannot move when the
            // construction moves, so a fixture carrying one silently stops modelling anything the
            // acceptance gate checks.
            leaf_root: palw_leaf_merkle_root(&fixture_leaf_hashes()),
            descriptor_root: h(5),
            total_leaf_bond_sompi: 0,
            audit_policy_id: h(6),
            activation_not_before_epoch: 7,
            expiry_epoch: 13,
        };
        // Content-address it so `apply_palw_overlay_effect`'s manifest arm accepts it (§9.2 guard).
        m.batch_id = m.content_id();
        m
    }

    fn leaf(idx: u32) -> PalwPublicLeafV1 {
        leaf_in(h(1), idx)
    }

    /// ADR-0040 P1-1: a leaf's own `batch_id` must equal the chunk's, so fixtures must build the leaf for
    /// the batch they are inserted under rather than a hardcoded one.
    fn leaf_in(batch_id: Hash64, idx: u32) -> PalwPublicLeafV1 {
        PalwPublicLeafV1 { batch_id, ..leaf_raw(idx) }
    }

    fn leaf_raw(idx: u32) -> PalwPublicLeafV1 {
        let spk = ScriptPublicKey::new(0, ScriptVec::from_slice(&[1]));
        let mut leaf = PalwPublicLeafV1 {
            version: 1,
            batch_id: h(1),
            leaf_index: idx,
            job_nullifier: h(2),
            // I-13: the leaf stores the commitment of the raw nullifier `h(3)`.
            ticket_nullifier_commitment: kaspa_consensus_core::palw::ticket_nullifier_commitment(&h(3)),
            model_profile_id: h(4),
            runtime_class_id: h(5),
            shape_id: 3,
            quantum_count: 2,
            proof_type: 1,
            provider_a_bond: TransactionOutpoint::new(h(6), 0),
            provider_b_bond: TransactionOutpoint::new(h(7), 0),
            provider_a_reward_script: spk.clone(),
            provider_b_reward_script: spk,
            ticket_authority_pk_hash: h(8),
            private_match_commitment: h(9),
            receipt_da_object_version: 1,
            receipt_da_root: h(10),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            // kaspa-pq ADR-0040 §5.14.3 item 7 — MUST equal the fixture manifest's `registration_epoch`,
            // which is why both read the same constant. This was `5` against a manifest registered at `1`;
            // the acceptance arm never compared them, so the fixture itself modelled the hole.
            registered_epoch: FIXTURE_REGISTRATION_EPOCH,
            activation_epoch: 7,
            expiry_epoch: 13,
            leaf_bond_sompi: 0,
            // Overwritten by the PCPB stamp below; listed so the literal stays exhaustive.
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: Hash64::default(),
            assignment_proof_root: Hash64::default(),
            dispatch_kind: 0,
        };
        // D3-b: every stored leaf passed clauses 11/12, so the fixture leaf is stamped into the same
        // PCPB world the shared fixture stages — drawn provider seats, committed roots, an anchor
        // epoch schedule inside the windows, and a challenge that re-derives. Stamping HERE (rather
        // than per test) keeps `fixture_leaf_hashes` / `manifest()` / every chunk consistent, since
        // all of them reduce this one builder.
        pcpb_fixture().stamp(&mut leaf);
        leaf
    }

    /// The one PCPB fixture every leaf in this module is stamped into (deterministic, so the leaf
    /// hashes — and therefore `leaf_root` and `batch_id` — stay stable across calls).
    fn pcpb_fixture() -> pcpb_test_support::PcpbFixture {
        pcpb_test_support::fixture(FIXTURE_REGISTRATION_EPOCH)
    }

    /// The payload of each kind round-trips borsh and parses to the right effect; a wrong subnet byte or
    /// garbage payload errors instead of panicking.
    #[test]
    fn parse_palw_overlay_kinds() {
        let m = manifest();
        let bytes = borsh::to_vec(&m).unwrap();
        assert!(matches!(parse_palw_overlay(0x31, &bytes), Ok(PalwOverlayEffect::Manifest(_))));
        let chunk = chunk_with_proofs(h(1), vec![leaf(0), leaf(1)]);
        assert!(matches!(parse_palw_overlay(0x32, &borsh::to_vec(&chunk).unwrap()), Ok(PalwOverlayEffect::LeafChunk(_))));
        // unhandled subnet byte + malformed payload.
        assert_eq!(parse_palw_overlay(0x34, &bytes).unwrap_err(), PalwOverlayError::UnhandledSubnet(0x34));
        assert_eq!(parse_palw_overlay(0x31, &[0xff, 0x00]).unwrap_err(), PalwOverlayError::MalformedPayload);
    }

    /// §9.5 (post-C5): the overlay-effect apply persists only the CONTENT-ADDRESSED blobs (manifest /
    /// leaves / certificate) — NO mutable global batch_status (that lifecycle is the block-keyed view's
    /// job). A manifest whose batch_id is not its own content id is rejected; a well-formed one is
    /// idempotently persisted (write-once content address).
    #[test]
    fn apply_overlay_persists_content_only() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id; // content-derived

        // content-addressed manifest ⇒ persisted; NO batch_status row is written.
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m.clone()), &store, &beacon, None, None).unwrap();
        assert_eq!(store.batch_manifest(bid).unwrap().leaf_count, 2);
        assert!(store.batch_status(bid).is_err(), "no mutable batch_status is written on the global store");
        // re-applying the same content is idempotent (write-once content address).
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m.clone()), &store, &beacon, None, None).unwrap();

        // a forged batch_id (not the content id) is rejected — the store cannot be polluted.
        let forged = PalwBatchManifestV1 { batch_id: h(0xff), ..m };
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::Manifest(forged), &store, &beacon, None, None),
            Err(PalwOverlayError::NonContentAddressedBatchId)
        );

        // leaf chunk ⇒ leaves persisted under (batch_id, leaf_index).
        let chunk = chunk_with_proofs(bid, vec![leaf_in(bid, 0), leaf_in(bid, 1)]);
        apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), &store, &beacon, None, Some(&pcpb_ctx)).unwrap();
        assert!(store.has_leaf(bid, 0).unwrap() && store.has_leaf(bid, 1).unwrap());

        // certificate ⇒ persisted by its own hash (self-content-addressed); no batch_status effect.
        // ADR-0040 P1-4: it must also BIND to the batch it names — `manifest_hash` is the manifest's
        // content id and `leaf_root` is the manifest's, so the fixture carries the real values (it used
        // to carry unrelated hashes, which the binding guard now correctly rejects).
        let cert = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: bid,
            manifest_hash: bid, // == manifest.content_id() for a content-addressed manifest
            // ADR-0040 §5.15: DERIVED. The certificate arm cross-binds `cert.leaf_root ==
            // manifest.leaf_root`, so a literal here would have to be re-pasted every time the Merkle
            // construction moves — the "substitution instead of derivation" defect §5.15.9 calls out.
            leaf_root: manifest().leaf_root,
            audit_beacon_epoch: 5,
            audit_sample_root: h(4),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(5),
            certificate_epoch: 6,
            activation_epoch: 7,
            expiry_epoch: 13,
            auditor_set_commitment: h(7),
            approving_stake: 0,
            votes: vec![PalwAuditorVoteV2 {
                bond_outpoint: TransactionOutpoint::new(h(8), 0),
                vote: 1,
                checked_leaf_bitmap_root: h(6),
                passed_leaf_count: 2,
                rejected_leaf_bitmap_root: h(5),
                signature: vec![],
            }],
        };
        let cert_hash = cert.hash();
        apply_palw_overlay_effect(PalwOverlayEffect::Certificate(cert), &store, &beacon, None, None).unwrap();
        assert_eq!(store.certificate(cert_hash).unwrap().passed_leaf_count, 2);
    }

    /// kaspa-pq **ADR-0040 P1-1 / gate G3** — BIND-01 + LEAF-01 closure.
    ///
    /// Two properties, both load-bearing for the lane's proof-of-work:
    ///
    /// * **A leaf cannot be injected into a batch it does not belong to.** algo-4 headers are exempt from
    ///   the Layer-0 hash floor, so the lane's entire PoW is the clause-9 draw over
    ///   `eligibility_hash(.., leaf_hash, nullifier)`. If an attacker could author and inject leaves, both
    ///   miner-variable inputs to that draw would be attacker-chosen — i.e. the draw becomes an offline
    ///   grind. The manifest is the only content-addressed anchor, so membership is checked against it.
    /// * **A written leaf is immutable.** `palw_work_reward_class` re-reads the CURRENT leaf at coinbase
    ///   time for `provider_{a,b}_reward_script`, so a post-acceptance overwrite re-routes the 77 % worker
    ///   base. Identical content stays idempotent so reorg replay is still legal.
    #[test]
    fn leaf_chunk_admission_binds_to_manifest_and_is_write_once() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id; // content-derived
        let leaf_count = m.leaf_count;
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();

        // NOTE (ADR-0040 §5.15.12): every adversarial fixture below now carries a REAL, derived
        // membership proof, so each still fails for its ORIGINAL reason rather than being caught by the
        // new gate on its way in. That is the whole point of rebuilding them instead of letting the
        // M2 gate quietly absorb the older assertions' coverage.

        // ---- (1) a chunk for a batch with NO admitted manifest is rejected outright ----
        let unknown = h(0xde);
        let orphan = chunk_with_proofs(unknown, vec![leaf_in(unknown, 0)]);
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(orphan), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::UnknownBatch),
            "a leaf must never be the effect that first materialises a batch key"
        );
        assert!(!store.has_leaf(unknown, 0).unwrap(), "nothing may be persisted for an unknown batch");

        // ---- (2) a leaf claiming a different batch than its chunk is rejected ----
        // The proof is valid (the Merkle projection zeroes `batch_id`, so this leaf's CONTENT is a
        // genuine member), which is what makes this assertion still about the batch-id cross-check.
        let smuggled = chunk_with_proofs(bid, vec![leaf_in(h(0xaa), 0)]);
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(smuggled), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafBatchIdMismatch)
        );

        // ---- (3) an index outside the manifest's leaf_count can never be part of leaf_root ----
        // No proof can exist for an out-of-range index, so this fixture carries a length-correct
        // placeholder; the index bound fires first, which is the ordering being asserted.
        let oob = PalwLeafChunkV1 {
            version: kaspa_consensus_core::palw::PALW_LEAF_CHUNK_VERSION_V3,
            batch_id: bid,
            chunk_index: 0,
            leaves: vec![leaf_in(bid, leaf_count)],
            proofs: vec![PalwLeafMembershipProofV1 { siblings: vec![h(0)] }],
            // Arity-correct placeholder: the index bound fires long before clause 11 reads this.
            witnesses: vec![pcpb_fixture().witness(leaf_count)],
        };
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(oob), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafIndexOutOfRange { leaf_index: leaf_count, leaf_count })
        );

        // ---- (4) the honest chunk lands ----
        let good = chunk_with_proofs(bid, vec![leaf_in(bid, 0), leaf_in(bid, 1)]);
        apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(good.clone()), &store, &beacon, None, Some(&pcpb_ctx)).unwrap();
        let sealed = store.leaf(bid, 0).unwrap().leaf_hash();

        // ---- (5) re-applying IDENTICAL content is idempotent (reorg replay must stay legal) ----
        apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(good), &store, &beacon, None, Some(&pcpb_ctx)).unwrap();
        assert_eq!(store.leaf(bid, 0).unwrap().leaf_hash(), sealed);

        // ---- (6) a thief's leaf is now refused by the MEMBERSHIP GATE, before the store is touched ----
        //
        // ADR-0040 §5.15: this used to be caught by `insert_leaf`'s write-once check — i.e. only because
        // the honest leaf happened to already occupy the slot, which is precisely the race the squatter
        // wins by going first. The gate now refuses it on CONTENT, so the ordering no longer matters.
        // The write-once assertion has NOT been deleted: it moved to
        // `leaf_write_once_still_fires_after_the_membership_gate`, which reaches `insert_leaf` with a
        // valid proof.
        let mut thief = leaf_in(bid, 0);
        thief.provider_a_reward_script = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0xbe, 0xef]));
        assert_ne!(thief.leaf_hash(), sealed, "the fixture must actually differ, else the test proves nothing");
        let overwrite = chunk_with_proofs(bid, vec![thief]);
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(overwrite), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafMembershipProofInvalid { leaf_index: 0 })
        );
        assert_eq!(store.leaf(bid, 0).unwrap().leaf_hash(), sealed, "the originally admitted leaf must survive");
    }

    /// The blob store is direct-write, so semantic validation must finish for the entire chunk before
    /// the first insert. In particular, a valid prefix must not escape when a later membership proof
    /// fails; otherwise transaction arrival order changes which leaf slots descendants can resolve.
    #[test]
    fn leaf_chunk_semantic_preflight_prevents_partial_prefix_writes() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id;
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();

        let mut chunk = chunk_with_proofs(bid, vec![leaf_in(bid, 0), leaf_in(bid, 1)]);
        // Leaf 0 and its proof remain valid. Corrupt only leaf 1's one-level proof so the failure is
        // reached after the first leaf has completed every semantic and write-once preflight check.
        chunk.proofs[1].siblings[0] = h(0xee);
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafMembershipProofInvalid { leaf_index: 1 })
        );
        assert!(!store.has_leaf(bid, 0).unwrap(), "a valid prefix must not be written before the whole chunk passes");
        assert!(!store.has_leaf(bid, 1).unwrap(), "the rejected leaf must not be written");
    }

    /// Only `KeyNotFound` has a semantic meaning at these reads. Corruption/deserialization/database
    /// failures must remain `StoreError` so the virtual commit caller can process-wide fail-stop rather
    /// than misclassifying an infrastructure fault as an unknown batch or an empty leaf slot.
    #[test]
    fn non_missing_manifest_and_leaf_read_failures_remain_store_errors() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id;
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();
        let chunk = chunk_with_proofs(bid, vec![leaf_in(bid, 0), leaf_in(bid, 1)]);

        let manifest_fault = ReadFaultStore { inner: &store, fault: InjectedReadFault::Manifest };
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk.clone()), &manifest_fault, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::StoreError),
            "a non-missing manifest read failure must not become UnknownBatch"
        );

        let leaf_fault = ReadFaultStore { inner: &store, fault: InjectedReadFault::Leaf };
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), &leaf_fault, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::StoreError),
            "a non-missing leaf read failure must not look like an empty write-once slot"
        );
        assert!(!store.has_leaf(bid, 0).unwrap());
        assert!(!store.has_leaf(bid, 1).unwrap());
    }

    /// Rejects a chunk-index substitution before the leaf reaches storage.
    ///
    /// The fixture submits a substituted leaf before the valid leaf. Membership verification must
    /// reject the substituted payout scripts and ticket authority against the manifest's `leaf_root`.
    ///
    /// The assertion is made on the STORE, not merely on the return value: semantic rejections are inert
    /// in the production caller (only `StoreError` process-wide fail-stops), so "returned an error" is
    /// not by itself evidence that nothing was written.
    #[test]
    fn chunk_index_squat_is_rejected_before_the_leaf_is_stored() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id;
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();

        // The squatter copies the public batch_id and substitutes its own payout + ticket authority.
        let mut squat = leaf_in(bid, 0);
        let squat_a = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51, 0x51]));
        let squat_b = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x52, 0x52]));
        squat.provider_a_reward_script = squat_a.clone();
        squat.provider_b_reward_script = squat_b.clone();
        squat.ticket_authority_pk_hash = h(0xaa);
        // It builds a well-formed chunk: right batch, right index, right proof LENGTH. The one thing it
        // cannot produce is a proof that opens the honest `leaf_root` to its own content.
        let chunk = chunk_with_proofs(bid, vec![squat]);
        assert_eq!(
            chunk.proofs[0].len() as u32,
            palw_leaf_merkle_depth(FIXTURE_LEAF_COUNT),
            "the fixture must fail on CONTENT, not length"
        );

        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafMembershipProofInvalid { leaf_index: 0 })
        );
        assert!(
            !store.has_leaf(bid, 0).unwrap(),
            "the squatter's leaf must not be in the store — the slot stays free for the honest one"
        );

        // The valid provider leaf remains insertable after the rejected substitution.
        let honest = chunk_with_proofs(bid, vec![leaf_in(bid, 0), leaf_in(bid, 1)]);
        apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(honest.clone()), &store, &beacon, None, Some(&pcpb_ctx)).unwrap();

        // ---- paired with the REWARD PATH (§5.15.12) ----
        //
        // `palw_work_reward_class` (virtual_processor/utxo_validation.rs) builds
        // `WorkRewardClass::ReplicaPalw` by re-reading `palw_store.leaf(header.palw_batch_id,
        // header.palw_leaf_index)` at coinbase time and cloning `provider_{a,b}_reward_script` off it.
        // So the reward-relevant statement of "the squat failed" is exactly this: THAT read, at THAT key,
        // still yields the honest provider pair — all three fields the squatter substituted.
        //
        // Scope, stated rather than implied: this asserts the store half of the chain. The store →
        // coinbase-output half is asserted by the algo-4 reward-rail E2Es in
        // virtual_processor/tests.rs. No single test spans both, because that harness seeds its leaf
        // directly (its `registered_epoch == activation_epoch == 0` leaf cannot pass
        // `validate_public_leaf`, so it can never traverse the acceptance arm).
        let stored = store.leaf(bid, 0).unwrap();
        let honest_leaf = leaf_in(bid, 0);
        assert_eq!(stored.provider_a_reward_script, honest_leaf.provider_a_reward_script, "the 77% worker base must stay with A");
        assert_eq!(stored.provider_b_reward_script, honest_leaf.provider_b_reward_script, "…and with B");
        assert_ne!(stored.provider_a_reward_script, squat_a, "the squatter's payout must not be what the coinbase reads");
        assert_ne!(stored.provider_b_reward_script, squat_b, "the squatter's payout must not be what the coinbase reads");
        assert_eq!(stored.ticket_authority_pk_hash, honest_leaf.ticket_authority_pk_hash, "nor its ticket authority");

        // IDEMPOTENT REPLAY (§5.15.12): the honest chunk re-sent — reorg replay, or an attacker paying
        // to publish the victim's own bytes — still succeeds. This is what makes the DENIAL half of the
        // closure true rather than merely argued.
        apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(honest), &store, &beacon, None, Some(&pcpb_ctx)).unwrap();
        assert!(store.has_leaf(bid, 0).unwrap() && store.has_leaf(bid, 1).unwrap());
    }

    /// A leaf whose every field is a function of `index` — no miner, no mock runtime, nothing that is
    /// free to move for unrelated reasons. `batch_id` starts zeroed; `restamp_leaves` sets the real one.
    ///
    /// `provider_{a,b}_reward_script` must be the exact 69-byte P2PKH ML-DSA-87 template, because
    /// `validate_public_leaf` (ADR-0040 P0-4 / ECON-01) requires a coinbase-representable script and the
    /// round trip below runs the REAL `validate_palw_overlay_payload` on the producer's bytes.
    fn e2e_leaf(index: u32) -> PalwPublicLeafV1 {
        // Distinct per index, in a way that cannot collide across a 65-leaf batch (a `[b; 64]`-style
        // fixture wraps at 256 and would silently produce duplicate nullifier commitments).
        let hx = |tag: u8, seed: u32| {
            let mut b = [0u8; 64];
            b[0] = tag;
            b[1..5].copy_from_slice(&seed.to_le_bytes());
            Hash64::from_bytes(b)
        };
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0xa0; 64]);
        let mut leaf = PalwPublicLeafV1 {
            version: 1,
            batch_id: Hash64::default(),
            leaf_index: index,
            job_nullifier: hx(0x10, index),
            ticket_nullifier_commitment: kaspa_consensus_core::palw::ticket_nullifier_commitment(&hx(0x11, index)),
            model_profile_id: h(2),
            runtime_class_id: h(3),
            shape_id: 1,
            quantum_count: 1,
            proof_type: 1,
            // `validate_public_leaf` rejects `provider_a_bond == provider_b_bond`.
            provider_a_bond: TransactionOutpoint::new(hx(0x12, index), 0),
            provider_b_bond: TransactionOutpoint::new(hx(0x13, index), 1),
            provider_a_reward_script: spk.clone(),
            provider_b_reward_script: spk,
            ticket_authority_pk_hash: h(8),
            private_match_commitment: Hash64::default(),
            receipt_da_object_version: 1,
            receipt_da_root: h(10),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: FIXTURE_REGISTRATION_EPOCH,
            activation_epoch: 7,
            expiry_epoch: 1000,
            leaf_bond_sompi: 0,
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: Hash64::default(),
            assignment_proof_root: Hash64::default(),
            dispatch_kind: 0,
        };
        // D3-b: the producer round trip runs the REAL acceptance arm, so the producer's leaves must
        // live in the same PCPB world the test stages — including the switch to receipt DA Object V2
        // that clause 11 makes mandatory. Note the per-index provider bonds above are REPLACED by the
        // drawn pair: the external draw is per-epoch, so every leaf anchored at one epoch names the
        // same two providers (design memo §1, "抽選 seed が job を含まない").
        pcpb_fixture().stamp(&mut leaf);
        leaf
    }

    /// kaspa-pq **ADR-0040 §5.15.12 — THE E2E ROUND TRIP.** The one named test the ACCEPT-BIND slice
    /// did not land.
    ///
    /// A batch assembled by the REAL miner producers (`misaka_palw_miner::registration::
    /// build_batch_manifest` + `build_leaf_chunk`) is driven through the REAL context-free validator
    /// (`validate_palw_overlay_payload`), the REAL parser (`parse_palw_overlay`) and the REAL acceptance
    /// arm (`apply_palw_overlay_effect`), and every leaf must end up in the store.
    ///
    /// Cross-crate integration call. `misaka-palw-miner` is a
    /// dev-dependency of this crate (acyclic — its closure does not contain `kaspa-consensus`), so both
    /// implementations execute here. The miner-side test
    /// `every_emitted_proof_opens_the_manifest_leaf_root_under_the_consensus_verifier` calls the verify
    /// FUNCTION; this one calls the acceptance ARM, which is where the length bound, the index bound,
    /// the batch-id cross-check, the version check and `insert_leaf` also live. Neither subsumes the
    /// other: a drift that only the arm's ordering exposes would pass over there.
    ///
    /// §5.15.12 requires two properties of the fixture, both asserted below rather than left to a
    /// comment:
    ///  * **multi-chunk** (`leaf_count > PALW_MAX_LEAVES_PER_CHUNK`), so the second chunk's proofs —
    ///    whose sibling paths share no prefix with the first chunk's — are exercised through the arm;
    ///  * **non-power-of-two `leaf_count`**, so the uniform `H_EMPTY` padding is what the honest proofs
    ///    fold through end to end. 65 satisfies both at the cheapest depth (7) that can.
    #[test]
    fn producer_built_batch_round_trips_through_the_real_acceptance_arm() {
        use kaspa_consensus_core::palw::{PALW_MAX_LEAVES_PER_CHUNK, validate_palw_overlay_payload};
        use misaka_palw_miner::registration::{BatchPolicy, build_batch_manifest, build_leaf_chunk, restamp_leaves};

        const LEAF_COUNT: u32 = 65;
        // Stated as assertions on the CONSTANT, so shrinking the fixture to "make the test faster"
        // cannot quietly retire the two cases §5.15.12 names.
        assert!(LEAF_COUNT as usize > PALW_MAX_LEAVES_PER_CHUNK, "the fixture must be multi-chunk");
        assert!(!LEAF_COUNT.is_power_of_two(), "the fixture must exercise the uniform H_EMPTY padding");

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(256));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);

        let policy = BatchPolicy {
            registration_epoch: FIXTURE_REGISTRATION_EPOCH,
            registration_lead_epochs: 2,
            audit_window_epochs: 1,
            active_window_epochs: 100,
            min_leaf_bond_sompi: 0,
            max_batch_leaves: kaspa_consensus_core::palw::PALW_MAX_BATCH_LEAVES_V1 as u32,
        };
        let minted: Vec<PalwPublicLeafV1> = (0..LEAF_COUNT).map(e2e_leaf).collect();

        // ---- (1) the producer's MANIFEST, through validate → parse → apply ----
        let (batch_id, (mbyte, mpayload)) =
            build_batch_manifest(&minted, h(2), h(3), h(4), h(5), 0, &policy).expect("the fixture is a valid batch");
        assert_eq!(validate_palw_overlay_payload(mbyte, &mpayload), Ok(()), "the producer's manifest must pass isolation");
        let manifest = match parse_palw_overlay(mbyte, &mpayload).expect("manifest parses") {
            PalwOverlayEffect::Manifest(m) => m,
            other => panic!("expected a Manifest effect, got {other:?}"),
        };
        // FIXED-POINT, positive half (§5.15.12): the producer's manifest is content-addressed UNDER the
        // Merkle `leaf_root` — i.e. `leaf_root → content_id() → batch_id` closes, which it only can
        // because the tree is built over the `batch_id`-ZEROED projection.
        assert!(manifest.batch_id_is_content_derived(), "leaf_root sits inside content_id, so batch_id must move with it");
        assert_eq!(manifest.leaf_count, LEAF_COUNT);
        assert_eq!(
            manifest.chunk_count,
            65u32.div_ceil(kaspa_consensus_core::palw::PALW_MAX_LEAVES_PER_CHUNK as u32) as u16,
            "chunk_count follows the (D3-b lowered) per-chunk cap"
        );
        assert_eq!(palw_leaf_merkle_depth(manifest.leaf_count), 7, "65 leaves pads to 128 — depth 7");
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(manifest.clone()), &store, &beacon, None, None).unwrap();

        // ---- (2) every CHUNK, through validate → parse → apply ----
        let restamped = restamp_leaves(batch_id, &minted);
        let mut chunk_payloads = Vec::new();
        for chunk_index in 0..manifest.chunk_count {
            let (cbyte, cpayload) =
                build_leaf_chunk(batch_id, chunk_index, &restamped, &e2e_pcpb_witnesses(&restamped)).expect("chunk assembles");
            assert_eq!(validate_palw_overlay_payload(cbyte, &cpayload), Ok(()), "chunk {chunk_index} must pass isolation");
            let chunk = match parse_palw_overlay(cbyte, &cpayload).expect("chunk parses") {
                PalwOverlayEffect::LeafChunk(c) => c,
                other => panic!("expected a LeafChunk effect, got {other:?}"),
            };
            assert_eq!(
                apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), &store, &beacon, None, Some(&pcpb_ctx)),
                Ok(()),
                "chunk {chunk_index} built by the real producer was REJECTED by the real acceptance arm — \
                 miner/consensus drift in the leaf-Merkle construction (on-chain this is silent: the arm's \
                 error is discarded by `let _ =` in virtual_processor)"
            );
            chunk_payloads.push((cbyte, cpayload));
        }

        // ---- (3) EVERY leaf is stored, byte-identical to what the producer minted ----
        for leaf in &restamped {
            assert!(store.has_leaf(batch_id, leaf.leaf_index).unwrap(), "leaf {} was not stored", leaf.leaf_index);
            assert_eq!(
                store.leaf(batch_id, leaf.leaf_index).unwrap().leaf_hash(),
                leaf.leaf_hash(),
                "leaf {} was stored with different content than the producer minted",
                leaf.leaf_index
            );
        }

        // ---- (4) IDEMPOTENT REPLAY over the whole multi-chunk batch (§5.15.12) ----
        // The single-leaf case is covered by `chunk_index_squat_is_rejected_before_the_leaf_is_stored`;
        // this states it for a batch whose chunks were already fully applied, which is the shape a reorg
        // actually replays.
        for (cbyte, cpayload) in &chunk_payloads {
            let chunk = parse_palw_overlay(*cbyte, cpayload).expect("chunk re-parses");
            assert_eq!(
                apply_palw_overlay_effect(chunk, &store, &beacon, None, Some(&pcpb_ctx)),
                Ok(()),
                "an identical honest chunk must remain admissible — this is what makes the DENIAL half of \
                 the CHUNK-INDEX SQUAT closure true rather than merely argued"
            );
        }
        assert_eq!(store.leaf(batch_id, LEAF_COUNT - 1).unwrap().leaf_hash(), restamped[LEAF_COUNT as usize - 1].leaf_hash());

        // ---- (5) FIXED-POINT, NEGATIVE half (§5.15.12) ----
        //
        // The tree is opened by the `batch_id`-ZEROED projection of a leaf. The NON-projected
        // `leaf_hash()` — the one `resolve_palw_binding` deliberately uses for the eligibility draw — must
        // NOT verify. Asserted so that a future "these two leaf hashes are the same leaf, let's
        // de-duplicate them" cleanup fails LOUDLY here instead of either bricking every honest chunk (if
        // the arm switched to the populated hash) or re-opening the fixed point (if the resolver switched
        // to the projected one).
        let hashes: Vec<Hash64> = restamped
            .iter()
            .map(|l| {
                let mut p = l.clone();
                p.batch_id = Hash64::default();
                p.leaf_hash()
            })
            .collect();
        let subject = &restamped[0];
        let proof = palw_leaf_merkle_proof(&hashes, 0).expect("index 0 is in range");
        assert!(
            palw_verify_leaf_membership(&hashes[0], 0, LEAF_COUNT, &proof, &manifest.leaf_root),
            "the projected hash is the one that opens leaf_root"
        );
        assert_ne!(subject.leaf_hash(), hashes[0], "the projection must actually change the digest");
        assert!(
            !palw_verify_leaf_membership(&subject.leaf_hash(), 0, LEAF_COUNT, &proof, &manifest.leaf_root),
            "the batch_id-POPULATED leaf hash must NOT open leaf_root — see the FIXED-POINT notes on this \
             arm and on `resolve_palw_binding`; the two digests of one leaf are intentional"
        );
    }

    /// Cross-crate auditor quorum test. A certificate assembled by `misaka_palw_miner::audit` is
    /// processed by `verify_certificate_attestation` and the acceptance path.
    ///
    /// The component tests cover the verifier's forged-certificate and committee/sample re-derivation
    /// (`certificate_attestation_rederives_committee_sample_and_signatures`), the core quorum arithmetic
    /// (`certificate_stake_weighted_quorum`), the SEL-01 bond-split
    /// (`sel01_credential_aggregation_makes_bond_splitting_worthless`), and the miner-side producer
    /// (`independent_auditors_form_a_certificate_that_validates_verifies_and_reaches_quorum`). This test
    /// adds the cross-crate round trip over a producer-built batch. That is the shape
    /// `producer_built_batch_round_trips_through_the_real_acceptance_arm` closes for LEAF CHUNKS; this one
    /// closes for the certificate. `misaka-palw-miner` is a dev-dependency of this crate (its
    /// closure does not contain `kaspa-consensus`), so both implementations execute here.
    ///
    /// Construction and validation use the same derivation. The first assertion requires the producer's
    /// certificate to pass the verifier and detects construction/validation drift.
    ///
    /// Single-process coverage includes quorum acceptance, forged vote signatures, votes outside the
    /// re-derived slate, incorrect
    /// `auditor_set_commitment`, wrong `audit_sample_root`, a stake-short quorum, an omitted selected vote
    /// that cannot shrink the denominator, and the SEL-01 bond-split (splitting a credential's bond into
    /// many small outpoints does not change the re-derived slate or its commitment).
    /// Network partition recovery and multi-node reorg convergence remain integration-test concerns.
    #[test]
    fn producer_built_certificate_round_trips_through_verify_certificate_attestation() {
        use kaspa_consensus_core::palw::PalwProviderBondRecord;
        use kaspa_pq_validator_core::ValidatorKey;
        use misaka_palw_miner::audit::{
            AuditRound, Auditor, LeafAuditExecutor, QuorumPolicy, derive_audit_sample_root, execute_audit_round, run_audit_round,
            select_audit_slate, sign_vote,
        };
        use misaka_palw_miner::registration::{BatchPolicy, build_batch_manifest, build_leaf_chunk, restamp_leaves};
        use std::collections::HashSet;

        // A small (3-leaf, non-power-of-two) producer-built batch with DISTINCT `receipt_da_root`s, so the
        // re-derived sample root is a real function of the sampled leaves. The multi-chunk case is already
        // driven by `producer_built_batch_round_trips_through_the_real_acceptance_arm`; here the batch is
        // only the substrate the certificate certifies.
        const LEAF_COUNT: u32 = 3;
        const NET: u32 = 0x9107;
        const POV: u64 = 100;
        const COMMITTEE_SIZE: usize = 8; // >= candidate count ⇒ the weighted sample draws the whole slate
        const SAMPLE_SIZE: u32 = 8; // >= leaf_count ⇒ every leaf is sampled
        let seed = h(0x99);
        let empty: HashSet<Hash64> = HashSet::new();

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(256));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);

        let policy = BatchPolicy {
            // Keep registration above zero so the pre-registration adversary below can select a
            // non-bootstrap historical audit epoch with an otherwise fully valid signed quorum.
            registration_epoch: 5,
            registration_lead_epochs: 2,
            audit_window_epochs: 4,
            active_window_epochs: 6,
            min_leaf_bond_sompi: 0,
            max_batch_leaves: kaspa_consensus_core::palw::PALW_MAX_BATCH_LEAVES_V1 as u32,
        };
        let leaf_activation_epoch =
            policy.registration_epoch.saturating_add(policy.registration_lead_epochs).saturating_add(policy.audit_window_epochs);
        let leaf_expiry_epoch = leaf_activation_epoch.saturating_add(policy.active_window_epochs);
        // Reuse the leaf-chunk E2E's leaf fixture (`e2e_leaf`), but give each leaf a distinct DA root.
        let minted: Vec<PalwPublicLeafV1> = (0..LEAF_COUNT)
            .map(|i| {
                let mut leaf = e2e_leaf(i);
                leaf.registered_epoch = policy.registration_epoch;
                leaf.activation_epoch = leaf_activation_epoch;
                leaf.expiry_epoch = leaf_expiry_epoch;
                // Object V2 binds `receipt_v3_expires_epoch == expiry_epoch`, so a fixture that moves
                // the window has to move both — the stamp set them equal at the fixture's own values.
                // (The freshness window still holds: issued 2, anchor 2, registered 5 ⇒ 2+Δ ≤ 5 and
                // 5−2 ≤ w.)
                leaf.receipt_v3_expires_epoch = leaf_expiry_epoch;
                leaf.receipt_da_root = h(0xD0 + i as u8);
                leaf
            })
            .collect();

        // ---- the producer's MANIFEST + LEAF CHUNK(s), through the real acceptance arm ----
        let (batch_id, (mbyte, mpayload)) =
            build_batch_manifest(&minted, h(2), h(3), h(4), h(5), 0, &policy).expect("the fixture is a valid batch");
        let manifest = match parse_palw_overlay(mbyte, &mpayload).expect("manifest parses") {
            PalwOverlayEffect::Manifest(m) => m,
            other => panic!("expected a Manifest effect, got {other:?}"),
        };
        assert_eq!(manifest.activation_not_before_epoch, 11);
        assert_eq!(manifest.expiry_epoch, 17);
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(manifest.clone()), &store, &beacon, None, None).unwrap();
        let restamped = restamp_leaves(batch_id, &minted);
        for chunk_index in 0..manifest.chunk_count {
            let (cbyte, cpayload) =
                build_leaf_chunk(batch_id, chunk_index, &restamped, &e2e_pcpb_witnesses(&restamped)).expect("chunk assembles");
            let chunk = match parse_palw_overlay(cbyte, &cpayload).expect("chunk parses") {
                PalwOverlayEffect::LeafChunk(c) => c,
                other => panic!("expected a LeafChunk effect, got {other:?}"),
            };
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), &store, &beacon, None, Some(&pcpb_ctx)).unwrap();
        }
        // The on-chain leaves the verifier reads, in index order [0, leaf_count).
        let leaves: Vec<Arc<PalwPublicLeafV1>> = restamped.iter().cloned().map(Arc::new).collect();

        // ---- the PROVIDER-BOND view: three distinct credentials, aggregate 40 / 40 / 20 sompi ----
        // Credential 0 is deliberately split 10 + 30 across two bonds. Its canonical representative
        // carries only 10, so this fixture regresses the former selection/quorum mismatch: both producer
        // and verifier must retain its credential aggregate of 40 after selecting the representative.
        // Each auditor's ML-DSA-87 key is the SAME `ValidatorKey::from_seed` the certificate signs with, so
        // the record's `owner_public_key` is exactly the key the verifier checks each vote under.
        let seeds = [0x11u8, 0x22, 0x33];
        let representative_amounts = [10u64, 40, 20];
        let bond_op = |i: usize| TransactionOutpoint::new(h(0x40 + i as u8), 0);
        let mut provider_records: Vec<_> = (0..3usize)
            .map(|i| {
                let bond = bond_op(i);
                (
                    bond,
                    PalwProviderBondRecord {
                        version: 1,
                        bond_outpoint: bond,
                        owner_pubkey_hash: h(0x90 + i as u8),
                        owner_public_key: ValidatorKey::from_seed([seeds[i]; 32]).public_key().to_vec(),
                        operator_group_id: h(0xb0 + i as u8),
                        runtime_classes: vec![],
                        capacity_by_shape: vec![],
                        reward_key_root: h(0xc0 + i as u8),
                        amount_sompi: representative_amounts[i],
                        activation_daa_score: 0, // Active at POV.
                        created_daa_score: 0,
                        unbond_delay_epochs: 0,
                        unbond_request_daa_score: None,
                        slashed_at_daa_score: None,
                    },
                )
            })
            .collect();
        let split_sibling = TransactionOutpoint::new(h(0x80), 0);
        let mut split_sibling_record = provider_records[0].1.clone();
        split_sibling_record.bond_outpoint = split_sibling;
        split_sibling_record.amount_sompi = 30;
        provider_records.push((split_sibling, split_sibling_record));
        let view = ProviderBondView::from_records(provider_records);

        // The producer re-derives the SAME committee + sample root the verifier will, via the SAME
        // consensus-core primitives that `select_audit_slate` / `derive_audit_sample_root` compose — the
        // leaves carry no provider bond that resolves in this view, so both sides use EMPTY exclusion sets.
        let (slate, commitment) = select_audit_slate(&seed, &batch_id, &view, POV, &empty, &empty, COMMITTEE_SIZE);
        assert_eq!(slate.len(), 3, "committee_size >= candidates ⇒ the whole slate is drawn");
        assert_eq!(
            slate.iter().find(|member| member.representative == bond_op(0)).unwrap().weight,
            40,
            "selected quorum weight must retain the split credential's 10 + 30 aggregate"
        );
        let sample_root = derive_audit_sample_root(&seed, &batch_id, &restamped, SAMPLE_SIZE);

        let round = AuditRound {
            network_id: NET,
            batch_id,
            manifest_hash: manifest.content_id(),
            leaf_root: manifest.leaf_root,
            audit_beacon_epoch: 5,
            audit_sample_root: sample_root,
            passed_leaf_count: LEAF_COUNT,
            rejected_leaf_bitmap_root: h(0x44),
            certificate_epoch: 6,
            activation_epoch: manifest.activation_not_before_epoch,
            expiry_epoch: manifest.expiry_epoch,
            auditor_set_commitment: commitment,
        };
        // AUDIT-EXEC-01: a verdict is the OUTPUT of running the round over the beacon-selected
        // sample — the same sample `derive_audit_sample_root` above commits to — so this fixture
        // exercises the producer's real path rather than asserting `pass: true` beside it.
        struct FixedExecutor(bool);
        impl LeafAuditExecutor for FixedExecutor {
            fn audit_leaf(&mut self, _leaf_index: u32, _leaf: &PalwPublicLeafV1) -> Result<bool, String> {
                Ok(self.0)
            }
        }
        let auditor = |i: usize, pass: bool| {
            let verdict = execute_audit_round(&seed, &batch_id, &restamped, SAMPLE_SIZE, &mut FixedExecutor(pass))
                .expect("the round executes over the batch's leaves");
            (Auditor { key: ValidatorKey::from_seed([seeds[i]; 32]), bond: bond_op(i) }, verdict)
        };
        let ctx = PalwCertificateAttestationCtx {
            network_id: NET,
            pov_daa_score: POV,
            provider_bond_view: &view,
            prev_seed: Some(seed),
            inclusion_epoch: 6,
            inclusion_window_epochs: 16,
            committee_size: COMMITTEE_SIZE,
            sample_size: SAMPLE_SIZE,
            quorum_num: 2,
            quorum_den: 3,
        };

        // ---- (1) THE ROUND TRIP: the real producer's certificate is ACCEPTED by the real verifier ----
        let honest =
            run_audit_round(&round, &[auditor(0, true), auditor(1, true), auditor(2, true)], &slate, QuorumPolicy { num: 2, den: 3 })
                .expect("the honest slate reaches quorum, so the producer assembles a certificate");
        assert_eq!(honest.cert.approving_stake, 100, "producer PASS tally must use 40 + 40 + 20 aggregate stake");
        assert_eq!(
            verify_certificate_attestation(&honest.cert, &manifest, &ctx, &leaves),
            Ok(()),
            "a certificate built by the REAL miner quorum producer must be ACCEPTED by the REAL consensus \
             verifier on the FIRST try — a rejection here is a construction != validation drift (a real bug \
             to report), not a test to adjust: on-chain the arm's error is discarded and the lane silently \
             never certifies"
        );

        // The producer's stateless epoch relation alone permits an audit before registration. Build a
        // genuine 100/100 quorum over that historical epoch: every vote is correctly signed for the
        // declared epoch, the committee/sample/quorum are unchanged, and only the manifest cross-bind
        // is wrong. Consensus must reject it before the attacker-selected historical snapshot can win
        // canonical-certificate selection.
        let pre_registration_round = AuditRound { audit_beacon_epoch: manifest.registration_epoch - 1, ..round.clone() };
        let pre_registration = run_audit_round(
            &pre_registration_round,
            &[auditor(0, true), auditor(1, true), auditor(2, true)],
            &slate,
            QuorumPolicy { num: 2, den: 3 },
        )
        .expect("legacy stateless epoch checks admit the handcrafted pre-registration round");
        assert_eq!(pre_registration.cert.approving_stake, 100);
        assert_eq!(
            verify_certificate_attestation(&pre_registration.cert, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateAuditEpochOutsideManifestWindow),
            "a real signed quorum must not authorize an audit snapshot from before batch registration"
        );

        // The manifest's activation bound is exclusive. Transaction isolation already makes
        // `audit >= activation` structurally awkward (`certificate < activation`), but the
        // authoritative verifier is also called directly and must enforce the cross-bind itself.
        // Hand-shape the envelope and re-sign every vote for each boundary epoch so signature/quorum
        // failures cannot mask the interval rule.
        for audit_beacon_epoch in [manifest.activation_not_before_epoch, manifest.activation_not_before_epoch + 1] {
            let outside_round = AuditRound { audit_beacon_epoch, certificate_epoch: audit_beacon_epoch, ..round.clone() };
            let mut at_or_after_activation = honest.cert.clone();
            at_or_after_activation.audit_beacon_epoch = audit_beacon_epoch;
            at_or_after_activation.certificate_epoch = audit_beacon_epoch;
            at_or_after_activation.votes =
                [auditor(0, true), auditor(1, true), auditor(2, true)].iter().map(|(a, v)| sign_vote(&outside_round, a, v)).collect();
            at_or_after_activation.approving_stake = 100;
            let outside_ctx = PalwCertificateAttestationCtx { inclusion_epoch: audit_beacon_epoch, ..ctx };
            assert_eq!(
                verify_certificate_attestation(&at_or_after_activation, &manifest, &outside_ctx, &leaves),
                Err(PalwOverlayError::CertificateAuditEpochOutsideManifestWindow),
                "audit epoch {audit_beacon_epoch} must be strictly before manifest activation"
            );
        }

        // Cross-epoch mergeset regression: the tx was carried in epoch 6, while a chain block in epoch
        // 7 may accept that carrier later. Consensus must keep the producer's carrier coordinate; using
        // the accepting chain block would turn the same immutable certificate into a rejection.
        let accepting_chain_epoch = 7;
        assert_ne!(ctx.inclusion_epoch, accepting_chain_epoch);
        let wrong_accepting_block_ctx = PalwCertificateAttestationCtx {
            network_id: ctx.network_id,
            pov_daa_score: ctx.pov_daa_score,
            provider_bond_view: ctx.provider_bond_view,
            prev_seed: ctx.prev_seed,
            inclusion_epoch: accepting_chain_epoch,
            inclusion_window_epochs: ctx.inclusion_window_epochs,
            committee_size: ctx.committee_size,
            sample_size: ctx.sample_size,
            quorum_num: ctx.quorum_num,
            quorum_den: ctx.quorum_den,
        };
        assert_eq!(
            verify_certificate_attestation(&honest.cert, &manifest, &wrong_accepting_block_ctx, &leaves),
            Err(PalwOverlayError::CertificateInclusionEpochMismatch)
        );
        // ...and through the acceptance ARM (which additionally cross-binds `manifest_hash` / `leaf_root`
        // and resolves the leaves from the store), the certificate persists.
        let honest_hash = honest.cert.hash();
        apply_palw_overlay_effect(PalwOverlayEffect::Certificate(honest.cert.clone()), &store, &beacon, Some(&ctx), None).unwrap();
        assert_eq!(store.certificate(honest_hash).unwrap().passed_leaf_count, LEAF_COUNT);

        // The V2 votes do not sign these envelope fields, so the verifier must bind them to the
        // immutable manifest and the including block before any canonical certificate selection.
        let mut wrong_activation = honest.cert.clone();
        wrong_activation.activation_epoch += 1;
        assert_eq!(
            verify_certificate_attestation(&wrong_activation, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateActivationEpochMismatch)
        );
        let wrong_activation_hash = wrong_activation.hash();
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::Certificate(wrong_activation), &store, &beacon, Some(&ctx), None),
            Err(PalwOverlayError::CertificateActivationEpochMismatch)
        );
        assert!(store.certificate(wrong_activation_hash).is_err());

        let mut wrong_expiry = honest.cert.clone();
        wrong_expiry.expiry_epoch += 1;
        assert_eq!(
            verify_certificate_attestation(&wrong_expiry, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateExpiryEpochMismatch)
        );
        let wrong_expiry_hash = wrong_expiry.hash();
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::Certificate(wrong_expiry), &store, &beacon, Some(&ctx), None),
            Err(PalwOverlayError::CertificateExpiryEpochMismatch)
        );
        assert!(store.certificate(wrong_expiry_hash).is_err());

        let mut wrong_inclusion = honest.cert.clone();
        wrong_inclusion.certificate_epoch += 1;
        assert_eq!(
            verify_certificate_attestation(&wrong_inclusion, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateInclusionEpochMismatch)
        );
        let wrong_inclusion_hash = wrong_inclusion.hash();
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::Certificate(wrong_inclusion), &store, &beacon, Some(&ctx), None),
            Err(PalwOverlayError::CertificateInclusionEpochMismatch)
        );
        assert!(store.certificate(wrong_inclusion_hash).is_err());

        // ---- (2) forged signature on one vote ⇒ CertificateVoteSignatureInvalid ----
        let mut forged = honest.cert.clone();
        let siglen = forged.votes[0].signature.len();
        forged.votes[0].signature = vec![0u8; siglen]; // right length, garbage content
        assert_eq!(
            verify_certificate_attestation(&forged, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateVoteSignatureInvalid)
        );

        // ---- (3) a vote from OUTSIDE the re-derived slate ⇒ CertificateVoteOutsideCommittee ----
        // A genuine producer-signed vote whose bond is then re-pointed to a non-slate outpoint.
        let mut outsider = {
            let (a, v) = auditor(0, true);
            sign_vote(&round, &a, &v)
        };
        outsider.bond_outpoint = TransactionOutpoint::new(h(0xee), 0);
        let mut outside = honest.cert.clone();
        outside.votes = vec![outsider];
        assert_eq!(
            verify_certificate_attestation(&outside, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateVoteOutsideCommittee)
        );

        // ---- (4a) wrong auditor_set_commitment ⇒ CertificateAuditorSetMismatch ----
        let mut wrong_set = honest.cert.clone();
        wrong_set.auditor_set_commitment = h(0x7e);
        assert_eq!(
            verify_certificate_attestation(&wrong_set, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateAuditorSetMismatch)
        );

        // ---- (4b) wrong audit_sample_root ⇒ CertificateAuditSampleRootMismatch ----
        let mut wrong_sample = honest.cert.clone();
        wrong_sample.audit_sample_root = h(0x5e);
        assert_eq!(
            verify_certificate_attestation(&wrong_sample, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateAuditSampleRootMismatch)
        );

        // ---- (5) a stake-short quorum ⇒ CertificateQuorumNotReached ----
        // One 40-sompi PASS against two rejecting auditors: 40 pass of 100 total < 2/3. The producer's
        // `assemble_certificate` would REFUSE to build this (its own quorum gate), so the votes are
        // producer-SIGNED (`sign_vote`) and the certificate hand-shaped carrying the honest PASS tally — the
        // verifier still recomputes the tally from the bond view and rejects on quorum, not stake-mismatch.
        let short_votes: Vec<PalwAuditorVoteV2> =
            [auditor(0, true), auditor(1, false), auditor(2, false)].iter().map(|(a, v)| sign_vote(&round, a, v)).collect();
        let mut short = honest.cert.clone();
        short.votes = short_votes;
        short.approving_stake = 40; // == the recomputed PASS tally, so the quorum check (not the mismatch) fires
        assert_eq!(
            verify_certificate_attestation(&short, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateQuorumNotReached)
        );

        // ---- (5b) omitted selected votes do NOT shrink the quorum denominator ----
        // A single 40-sompi PASS is still measured against the full 40+40+20 selected slate. The old
        // participating-vote denominator incorrectly treated this as 40/40 and accepted it as 100%.
        let mut withheld = honest.cert.clone();
        withheld.votes = vec![{
            let (a, v) = auditor(0, true);
            sign_vote(&round, &a, &v)
        }];
        withheld.approving_stake = 40;
        assert_eq!(
            verify_certificate_attestation(&withheld, &manifest, &ctx, &leaves),
            Err(PalwOverlayError::CertificateQuorumNotReached),
            "missing selected-auditor votes must count against the 2/3 quorum"
        );

        // ---- (6) SEL-01 bond-split: splitting a credential's bond into many small outpoints does NOT
        //          change the re-derived slate or its commitment (the value the certificate must carry).
        // `verify_certificate_attestation` re-derives the committee via the SAME `select_auditor_committee`
        // that the producer's `select_audit_slate` composes, so this slate-equality IS the on-chain
        // anti-split property: a split world produces a byte-identical `auditor_set_commitment`, buying the
        // attacker no extra committee slot. `k = 3`, `N` divisible by 3; two decoy credentials so the
        // attacker actually competes for a size-2 committee.
        let split_rec = |cred: Hash64, grp: Hash64, amount: u64, bond: TransactionOutpoint| PalwProviderBondRecord {
            version: 1,
            bond_outpoint: bond,
            owner_pubkey_hash: cred,
            owner_public_key: vec![],
            operator_group_id: grp,
            runtime_classes: vec![],
            capacity_by_shape: vec![],
            reward_key_root: Hash64::default(),
            amount_sompi: amount,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbond_delay_epochs: 0,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        };
        const N: u64 = 900_000;
        let (ca, grp_a) = (h(0xA0), h(0x01));
        let decoy_b = split_rec(h(0xB0), h(0x02), 400_000, TransactionOutpoint::new(h(0xB0), 0));
        let decoy_d = split_rec(h(0xD0), h(0x03), 600_000, TransactionOutpoint::new(h(0xD0), 0));
        let whole_view = ProviderBondView::from_records([
            (TransactionOutpoint::new(ca, 0), split_rec(ca, grp_a, N, TransactionOutpoint::new(ca, 0))),
            (decoy_b.bond_outpoint, decoy_b.clone()),
            (decoy_d.bond_outpoint, decoy_d.clone()),
        ]);
        let k = 3u32;
        let mut split_records: Vec<(TransactionOutpoint, PalwProviderBondRecord)> = (0..k)
            .map(|i| {
                let bond = TransactionOutpoint::new(ca, i); // index 0 is the smallest ⇒ same representative
                (bond, split_rec(ca, grp_a, N / k as u64, bond))
            })
            .collect();
        split_records.push((decoy_b.bond_outpoint, decoy_b));
        split_records.push((decoy_d.bond_outpoint, decoy_d));
        let split_view = ProviderBondView::from_records(split_records);
        let (whole_slate, whole_commit) = select_audit_slate(&h(0x77), &h(0x42), &whole_view, POV, &empty, &empty, 2);
        let (split_slate, split_commit) = select_audit_slate(&h(0x77), &h(0x42), &split_view, POV, &empty, &empty, 2);
        assert_eq!(whole_slate, split_slate, "a bond-split must not change the beacon-selected auditor slate");
        assert_eq!(
            whole_commit, split_commit,
            "a bond-split must not change the auditor_set_commitment the certificate must carry — it wins no committee slot"
        );
    }

    /// kaspa-pq **ADR-0040 §5.15** — the EXACT proof-length bound, rejected in both directions and
    /// BEFORE any hashing.
    ///
    /// The context-free `validate_leaf_chunk` can only assert the static `<= 8` bound, having no
    /// manifest to read `leaf_count` from; the exact bound is what makes the proof for a given
    /// `(leaf, index, root)` unique, so it must live here, where the manifest is already loaded. A
    /// distinct error variant is what lets this test state "rejected on LENGTH" — with a single
    /// catch-all variant, a too-long proof that happened to fold to the right root would be
    /// indistinguishable from one that did not.
    #[test]
    fn membership_proof_length_is_exact_in_both_directions() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id;
        let expected = palw_leaf_merkle_depth(m.leaf_count);
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();

        let honest = chunk_with_proofs(bid, vec![leaf_in(bid, 0)]);
        for siblings in [Vec::new(), vec![honest.proofs[0].siblings[0], h(7)]] {
            let got = siblings.len();
            let mut bad = honest.clone();
            bad.proofs = vec![PalwLeafMembershipProofV1 { siblings }];
            assert_eq!(
                apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(bad), &store, &beacon, None, Some(&pcpb_ctx)),
                Err(PalwOverlayError::LeafMembershipProofLengthInvalid { leaf_index: 0, got, expected }),
                "a proof of length {got} (expected {expected}) must be refused on length alone"
            );
            assert!(!store.has_leaf(bid, 0).unwrap());
        }

        // A chunk with FEWER proofs than leaves cannot index-panic in consensus: `parse_palw_overlay` is
        // a bare Borsh decode, so this arm never relies on `validate_leaf_chunk` having run.
        let mut starved = chunk_with_proofs(bid, vec![leaf_in(bid, 0), leaf_in(bid, 1)]);
        starved.proofs.pop();
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(starved), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafProofCountMismatch { leaves: 2, proofs: 1 })
        );
        assert!(
            !store.has_leaf(bid, 0).unwrap() && !store.has_leaf(bid, 1).unwrap(),
            "a later-leaf validation error must not leave an earlier leaf partially persisted"
        );
    }

    /// kaspa-pq **ADR-0040 §5.15** — a member leaf's proof does not open it at ANOTHER index, and a
    /// non-member cannot borrow a member's slot.
    ///
    /// Two independent index bindings have to hold, and this states both:
    ///
    /// * **Cross-index proof reuse.** Leaf 0 is a genuine member, but presented with leaf 1's proof it
    ///   is refused, and vice versa. The direction bits come from the leaf's own `leaf_index`, never
    ///   from the payload, so the attacker has no free bits to grind a fold with.
    /// * **Relabelling.** A leaf whose content is NOT the batch's member at index 1, submitted at index
    ///   1 with index 1's genuine proof, is refused — the level-0 node is
    ///   `Hash64_k(leaf-merkle-leaf, leaf_index_le32 ‖ leaf_hash)`, so the index is bound inside the
    ///   node, on top of `PalwPublicLeafV1::leaf_hash` already committing `leaf_index` itself. The two
    ///   bindings are deliberately redundant; neither may be removed on the grounds that the other
    ///   exists.
    #[test]
    fn a_member_leaf_cannot_be_replayed_at_another_index() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id;
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();

        let hashes = fixture_leaf_hashes();
        let chunk_with = |leaf: PalwPublicLeafV1, proof_for: u32| {
            let witnesses = vec![pcpb_fixture().witness(leaf.leaf_index)];
            PalwLeafChunkV1 {
                version: kaspa_consensus_core::palw::PALW_LEAF_CHUNK_VERSION_V3,
                batch_id: bid,
                chunk_index: 0,
                leaves: vec![leaf],
                proofs: vec![palw_leaf_merkle_proof(&hashes, proof_for).unwrap()],
                witnesses,
            }
        };

        // ---- cross-index proof reuse: genuine member, wrong index's proof ----
        for (leaf_index, proof_for) in [(0u32, 1u32), (1, 0)] {
            assert_eq!(
                apply_palw_overlay_effect(
                    PalwOverlayEffect::LeafChunk(chunk_with(leaf_in(bid, leaf_index), proof_for)),
                    &store,
                    &beacon,
                    None,
            None
                ),
                Err(PalwOverlayError::LeafMembershipProofInvalid { leaf_index }),
                "leaf {leaf_index} must not open under leaf {proof_for}'s proof"
            );
        }

        // ---- relabelling: content that is not the member at index 1, submitted at index 1 ----
        let mut relabelled = leaf_in(bid, 1);
        relabelled.job_nullifier = h(0x77);
        assert_ne!(relabelled.leaf_hash(), leaf_in(bid, 1).leaf_hash(), "the fixture must actually differ");
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk_with(relabelled, 1)), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafMembershipProofInvalid { leaf_index: 1 })
        );

        assert!(!store.has_leaf(bid, 0).unwrap() && !store.has_leaf(bid, 1).unwrap(), "no rejected leaf may have been stored");
    }

    /// kaspa-pq **ADR-0040 §5.14.3 item 7 (P1-10 prerequisite)** — a leaf whose `registered_epoch` is not
    /// its manifest's `registration_epoch` is refused, *even though its membership proof verifies*.
    ///
    /// The fixture is what makes this test mean anything. `manifest_at_epoch` always builds `leaf_root`
    /// over `fixture_leaf_hashes()` — leaves stamped `FIXTURE_REGISTRATION_EPOCH` — so a manifest
    /// registered at `FIXTURE_REGISTRATION_EPOCH + 1` has genuinely-opening proofs for leaves that
    /// disagree with it. Delete the epoch check and this batch is accepted and STORED; that is asserted
    /// positively below by first confirming the same leaves round-trip under the matching manifest.
    ///
    /// Why the leaf cannot dodge this by restamping itself: `registered_epoch` is inside `leaf_hash`,
    /// `leaf_hash` opens `manifest.leaf_root` (§5.15/M2), and `leaf_root` is inside `content_id()` ==
    /// `batch_id`. Changing the leaf's epoch changes the batch it is a member of. So the author's only
    /// remaining valid construction is a batch whose leaves share one registration epoch.
    ///
    /// The acceptance arm does not determine the real acceptance epoch. It binds the
    /// leaf to the manifest; `PalwBatchManifestV1::admission_valid` (reached via
    /// `PalwBatchViewV1::apply_manifest`, which `check_palw_ticket`'s `view.resolvable_batch` requires
    /// before any header may mine the batch) binds the manifest to the carrier epoch.
    #[test]
    fn a_leaf_must_carry_its_manifests_registration_epoch() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);

        // ---- the SAME leaf content, under a manifest whose registration epoch disagrees ----
        let skewed = manifest_at_epoch(FIXTURE_REGISTRATION_EPOCH + 1);
        let skewed_bid = skewed.batch_id;
        assert_eq!(
            skewed.leaf_root,
            manifest().leaf_root,
            "the two fixtures must share a leaf_root, or this test would be proving a membership failure"
        );
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(skewed), &store, &beacon, None, None).unwrap();

        let leaves = vec![leaf_in(skewed_bid, 0), leaf_in(skewed_bid, 1)];
        // The proofs are DERIVED from the same ordered hash sequence the root was reduced from, so they
        // verify. The rejection below is therefore attributable to the epoch alone.
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk_with_proofs(skewed_bid, leaves)), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafRegistrationEpochMismatch {
                leaf_index: 0,
                leaf_registered_epoch: FIXTURE_REGISTRATION_EPOCH,
                manifest_registration_epoch: FIXTURE_REGISTRATION_EPOCH + 1,
            }),
            "a leaf registered at a different epoch than its batch must not be stored"
        );
        assert!(!store.has_leaf(skewed_bid, 0).unwrap(), "a rejected leaf must not have been written");

        // ---- the control: identical leaves, matching manifest, accepted ----
        let m = manifest();
        let bid = m.batch_id;
        assert_ne!(bid, skewed_bid, "the two manifests must be distinct batches");
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();
        let ok = vec![leaf_in(bid, 0), leaf_in(bid, 1)];
        apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk_with_proofs(bid, ok)), &store, &beacon, None, Some(&pcpb_ctx))
            .expect("the epoch-matched batch must still be accepted — the rule must not reject honest chunks");
        assert!(store.has_leaf(bid, 0).unwrap() && store.has_leaf(bid, 1).unwrap());
    }

    /// kaspa-pq **ADR-0040 §5.15.12 (WRITE-ONCE coverage must not be hollowed out)** — `insert_leaf`'s
    /// write-once check is still REACHED and still fires.
    ///
    /// After M2 the membership gate makes `LeafImmutabilityViolation` nearly unreachable through this
    /// arm — that is the intended consequence (§5.15.8), not a regression — because any chunk that
    /// passes the gate is byte-identical to the honest one. So the honest way to keep the write-once
    /// property under test is to occupy the slot from OUTSIDE the arm and then drive a fully valid chunk
    /// through the gate into `insert_leaf`. That is exactly the defence-in-depth ordering being
    /// asserted: gate first, store second, and the store still refuses.
    #[test]
    fn leaf_write_once_still_fires_after_the_membership_gate() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id;
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();

        // Occupy (bid, 0) with foreign content directly, bypassing the arm.
        let mut foreign = leaf_in(bid, 0);
        foreign.provider_a_reward_script = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0xbe, 0xef]));
        store.insert_leaf(bid, 0, Arc::new(foreign.clone())).unwrap();

        // The HONEST chunk now passes the membership gate and is refused by the store, not by the gate.
        let honest = chunk_with_proofs(bid, vec![leaf_in(bid, 0)]);
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(honest), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafImmutabilityViolation),
            "the gate must precede insert_leaf, and insert_leaf must still be the thing that refuses here"
        );
        assert_eq!(store.leaf(bid, 0).unwrap().leaf_hash(), foreign.leaf_hash());
    }

    /// kaspa-pq **ADR-0040 P1-2 (LEAF-01)** — the reward basis is immutable after acceptance.
    ///
    /// The audit's remedy was "freeze the leaf hash / reward scripts as an immutable snapshot at
    /// accepted-block time". P1-1's write-once store achieves the same property without copying: the
    /// bytes at `(batch_id, leaf_index)` cannot change once written, so the reward path's re-read
    /// necessarily returns what body validation proved.
    ///
    /// This test states that as the reward-relevant property specifically — the earlier G3 test proves
    /// the store refuses the write, this one proves the REWARD SCRIPTS a coinbase would derive are the
    /// ones that survive.
    #[test]
    fn reward_scripts_are_immutable_after_acceptance() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id;
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();

        let honest = leaf_in(bid, 0);
        let honest_a = honest.provider_a_reward_script.clone();
        let chunk = chunk_with_proofs(bid, vec![honest]);
        apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), &store, &beacon, None, Some(&pcpb_ctx)).unwrap();

        // The reward path reads the leaf by key at coinbase time. Attempt the theft: same key, different
        // payout. The re-read still yields the accepted scripts.
        //
        // ADR-0040 §5.15 — the REJECTING CHECK moved, the property did not. This used to be refused by
        // `insert_leaf` (write-once), i.e. only because the honest leaf was already there; it is now
        // refused by the M2 membership gate, on content, before the store is consulted at all. The
        // strictly stronger statement: the theft fails whether or not the thief goes first. Write-once
        // is still exercised, at `leaf_write_once_still_fires_after_the_membership_gate`.
        let mut thief = leaf_in(bid, 0);
        thief.provider_a_reward_script = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0xbe, 0xef]));
        let overwrite = chunk_with_proofs(bid, vec![thief.clone()]);
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(overwrite), &store, &beacon, None, Some(&pcpb_ctx)),
            Err(PalwOverlayError::LeafMembershipProofInvalid { leaf_index: 0 })
        );

        // ...and the ORIGINAL reason this fixture was rejected has NOT been retired, only outranked.
        // Asserted in place, because "the gate happens to fire first" is not the same claim as "the
        // store would still refuse": if a later slice ever loosens the gate, this line is what keeps
        // LEAF-01 from silently becoming unenforced. Driven straight at the store, since no chunk
        // carrying this content can reach `insert_leaf` through the arm any more — which is the point.
        //
        // §5.15.12 negative-fixture audit: `is_err()` was too loose to state the ORIGINAL reason — any
        // store fault would have satisfied it. The write-once property is specifically the ALREADY-EXISTS
        // refusal (`insert_leaf`'s content-address put-if-absent), which is also the exact predicate the
        // arm maps to `LeafImmutabilityViolation`.
        assert!(
            store.insert_leaf(bid, 0, Arc::new(thief)).is_err_and(|e| e.is_already_exists()),
            "write-once must independently refuse the same theft; the M2 gate outranks it, not replaces it"
        );

        assert_eq!(
            store.leaf(bid, 0).unwrap().provider_a_reward_script,
            honest_a,
            "the reward script a coinbase derives must be the one that was accepted"
        );
    }

    /// kaspa-pq **ADR-0040 P1-4 (BIND-02 / BIND-05)** — a certificate must bind to the batch it names.
    ///
    /// Downstream, `is_block_eligible_at` only asks whether `cert_hash.is_some()`, and
    /// `resolve_palw_binding` reads only the certificate's epoch window. So without this check, ANY
    /// certificate blob in the store could satisfy an algo-4 header for ANY batch — the certificate's
    /// `manifest_hash` / `leaf_root` were decoded and never compared to anything.
    ///
    /// The attestation path separately verifies ML-DSA votes, auditor selection, stake-weighted quorum
    /// and `audit_sample_root`.
    #[test]
    fn certificate_must_bind_to_its_batch_manifest_and_leaf_root() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let _pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let bid = m.batch_id;
        let real_leaf_root = m.leaf_root;
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();

        let base = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: bid,
            manifest_hash: bid,
            leaf_root: real_leaf_root,
            audit_beacon_epoch: 5,
            audit_sample_root: h(4),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(5),
            certificate_epoch: 6,
            activation_epoch: 7,
            expiry_epoch: 13,
            auditor_set_commitment: h(7),
            approving_stake: 0,
            votes: vec![PalwAuditorVoteV2 {
                bond_outpoint: TransactionOutpoint::new(h(8), 0),
                vote: 1,
                checked_leaf_bitmap_root: h(6),
                passed_leaf_count: 2,
                rejected_leaf_bitmap_root: h(5),
                signature: vec![],
            }],
        };

        // (1) a certificate for a batch with no admitted manifest cannot be persisted at all.
        let orphan = PalwBatchCertificateV2 { batch_id: h(0xde), ..base.clone() };
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::Certificate(orphan), &store, &beacon, None, None),
            Err(PalwOverlayError::UnknownBatch)
        );

        // (2) right batch, wrong manifest — it certifies a manifest that is not the one on chain.
        let wrong_manifest = PalwBatchCertificateV2 { manifest_hash: h(0xbb), ..base.clone() };
        let wrong_manifest_hash = wrong_manifest.hash();
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::Certificate(wrong_manifest), &store, &beacon, None, None),
            Err(PalwOverlayError::CertificateManifestMismatch)
        );
        assert!(store.certificate(wrong_manifest_hash).is_err(), "a mis-bound certificate must not be persisted");

        // (3) right batch and manifest, wrong leaf set — it attests to leaves that are not this batch's.
        let wrong_leaves = PalwBatchCertificateV2 { leaf_root: h(0xcc), ..base.clone() };
        assert_eq!(
            apply_palw_overlay_effect(PalwOverlayEffect::Certificate(wrong_leaves), &store, &beacon, None, None),
            Err(PalwOverlayError::CertificateLeafRootMismatch)
        );

        // (4) the correctly-bound certificate persists.
        let good_hash = base.hash();
        apply_palw_overlay_effect(PalwOverlayEffect::Certificate(base), &store, &beacon, None, None).unwrap();
        assert_eq!(store.certificate(good_hash).unwrap().leaf_root, real_leaf_root);
    }

    /// A certificate must be attested by the beacon-selected committee over beacon-selected on-chain
    /// leaves.
    ///
    /// AUTHSET-01 and SAMPLE-01 re-derive the declared roots from beacon-visible state. SEL-01 draws the
    /// committee from `ProviderBondView` and weights votes by ECON-03 `amount_sompi`.
    #[test]
    fn certificate_attestation_rederives_committee_sample_and_signatures() {
        use kaspa_consensus_core::palw::{PALW_AUDITOR_V2_MLDSA87_CONTEXT, PalwProviderBondRecord};
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;
        use std::collections::HashSet;

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let _pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);
        let m = manifest();
        let (bid, leaf_root) = (m.batch_id, m.leaf_root);
        apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m.clone()), &store, &beacon, None, None).unwrap();

        // Two on-chain leaves with DISTINCT `receipt_da_root`s, so the re-derived sample root is a real
        // function of the sampled set (not a constant). Inserted so the apply-path can enumerate them.
        let mut leaf0 = leaf_in(bid, 0);
        leaf0.receipt_da_root = h(0xd0);
        let mut leaf1 = leaf_in(bid, 1);
        leaf1.receipt_da_root = h(0xd1);
        let leaves = [Arc::new(leaf0), Arc::new(leaf1)];
        store.insert_leaf(bid, 0, leaves[0].clone()).unwrap();
        store.insert_leaf(bid, 1, leaves[1].clone()).unwrap();

        const NET: u32 = 7;
        const POV: u64 = 100;
        let seed = h(0x99);
        let op = |b: u8| TransactionOutpoint::new(h(b), 0);
        let kp = |s: u8| mldsa::generate_key_pair([s; 32]);

        // Three PROVIDER-bonded auditors, distinct credentials + operator groups, 40 / 40 / 20 sompi — so
        // any two reach 2/3 and any one alone does not. `owner_pubkey_hash` is the credential, distinct
        // from the batch's own provider bonds (op(6)/op(7), absent from this view), so none is excluded.
        let keys = [kp(0x11), kp(0x22), kp(0x33)];
        let amounts = [40u64, 40, 20];
        let bond_op = |i: usize| op(0x40 + i as u8);
        let view = ProviderBondView::from_records((0..3usize).map(|i| {
            let outpoint = bond_op(i);
            (
                outpoint,
                PalwProviderBondRecord {
                    version: 1,
                    bond_outpoint: outpoint,
                    owner_pubkey_hash: h(0x90 + i as u8),
                    owner_public_key: keys[i].verification_key.as_ref().to_vec(),
                    operator_group_id: h(0xb0 + i as u8),
                    runtime_classes: vec![],
                    capacity_by_shape: vec![],
                    reward_key_root: h(0xc0 + i as u8),
                    amount_sompi: amounts[i],
                    activation_daa_score: 0, // Active at POV.
                    created_daa_score: 0,
                    unbond_delay_epochs: 0,
                    unbond_request_daa_score: None,
                    slashed_at_daa_score: None,
                },
            )
        }));

        // committee_size ≥ 3 ⇒ the weighted sample draws EVERY candidate, so the slate is all three
        // (each credential's representative is its single outpoint). sample_size ≥ leaf_count ⇒ both
        // leaves are sampled. These are the honest, re-derived reference values the cert must carry.
        let empty: HashSet<Hash64> = HashSet::new();
        let (honest_slate, honest_commitment) =
            kaspa_consensus_core::palw::select_auditor_committee(&seed, &bid, &view, POV, &empty, &empty, 8);
        assert_eq!(honest_slate.len(), 3, "committee_size ≥ candidates ⇒ whole slate");
        let honest_indices = palw_deterministic_sample(&seed, &bid, 2, 8);
        assert_eq!(honest_indices, vec![0, 1]);
        let honest_sample_root = palw_audit_sample_root(&[leaves[0].receipt_da_root, leaves[1].receipt_da_root]);

        let ctx = PalwCertificateAttestationCtx {
            network_id: NET,
            pov_daa_score: POV,
            provider_bond_view: &view,
            prev_seed: Some(seed),
            inclusion_epoch: 6,
            inclusion_window_epochs: 16,
            committee_size: 8,
            sample_size: 8,
            quorum_num: 2,
            quorum_den: 3,
        };

        // A certificate carrying the re-derived commitment and sample root; only the votes vary.
        let cert = |votes: Vec<PalwAuditorVoteV2>, approving_stake: u128| PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: bid,
            manifest_hash: bid,
            leaf_root,
            audit_beacon_epoch: 5,
            audit_sample_root: honest_sample_root,
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(5),
            certificate_epoch: 6,
            activation_epoch: 7,
            expiry_epoch: 13,
            auditor_set_commitment: honest_commitment,
            approving_stake,
            votes,
        };
        // Sign a vote for auditor `i` over a chosen audit epoch and `sample_root`. Keeping the epoch
        // parameter here lets the manifest-window regressions below use otherwise genuine signatures
        // rather than relying on the new interval check to mask an invalid-signature fixture.
        let signed_over_at = |i: usize, vote: u8, audit_beacon_epoch: u64, sample_root: &Hash64| {
            let mut v = PalwAuditorVoteV2 {
                bond_outpoint: bond_op(i),
                vote,
                checked_leaf_bitmap_root: h(6),
                passed_leaf_count: 2,
                rejected_leaf_bitmap_root: h(5),
                signature: vec![],
            };
            let d = v.signing_hash(NET, &bid, audit_beacon_epoch, sample_root);
            v.signature = mldsa::sign(&keys[i].signing_key, d.as_bytes().as_slice(), PALW_AUDITOR_V2_MLDSA87_CONTEXT, [0x5au8; 32])
                .expect("sign")
                .as_ref()
                .to_vec();
            v
        };
        let signed_over = |i: usize, vote: u8, sample_root: &Hash64| signed_over_at(i, vote, 5, sample_root);
        let signed = |i: usize, vote: u8| signed_over(i, vote, &honest_sample_root);

        // ---- honest AND at quorum: 80 of 100 ⇒ accepted, and it persists through the apply path ----
        let good = cert(vec![signed(0, 1), signed(1, 1), signed(2, 0)], 80);
        assert_eq!(verify_certificate_attestation(&good, &m, &ctx, &leaves), Ok(()), "the honest re-derived path verifies");

        // ---- MANIFEST-AUDIT-INTERVAL: signed/quorate handcrafted certificates may not select a
        // historical snapshot from before this batch was registered. The fixture's registration is
        // epoch 1, so epoch 0 is outside the interval while still satisfying the envelope's legacy
        // `audit <= certificate < activation` ordering. Re-sign every vote over epoch 0 so the only
        // defect is the missing manifest cross-bind this regression covers.
        let mut before_registration = cert(
            vec![
                signed_over_at(0, 1, 0, &honest_sample_root),
                signed_over_at(1, 1, 0, &honest_sample_root),
                signed_over_at(2, 0, 0, &honest_sample_root),
            ],
            80,
        );
        before_registration.audit_beacon_epoch = 0;
        assert_eq!(
            verify_certificate_attestation(&before_registration, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateAuditEpochOutsideManifestWindow),
            "valid signatures and quorum must not authorize a pre-registration audit snapshot"
        );

        // The upper bound is exclusive. These certificates likewise carry genuine signatures and the
        // honest 80/100 quorum for their declared epoch; changing the inclusion context keeps the
        // authoritative verifier focused on the manifest interval even when called independently of
        // transaction-isolation validation.
        for audit_beacon_epoch in [m.activation_not_before_epoch, m.activation_not_before_epoch + 1] {
            let outside_ctx = PalwCertificateAttestationCtx { inclusion_epoch: audit_beacon_epoch, ..ctx };
            let mut at_or_after_activation = cert(
                vec![
                    signed_over_at(0, 1, audit_beacon_epoch, &honest_sample_root),
                    signed_over_at(1, 1, audit_beacon_epoch, &honest_sample_root),
                    signed_over_at(2, 0, audit_beacon_epoch, &honest_sample_root),
                ],
                80,
            );
            at_or_after_activation.audit_beacon_epoch = audit_beacon_epoch;
            at_or_after_activation.certificate_epoch = audit_beacon_epoch;
            assert_eq!(
                verify_certificate_attestation(&at_or_after_activation, &m, &outside_ctx, &leaves),
                Err(PalwOverlayError::CertificateAuditEpochOutsideManifestWindow),
                "audit epoch {audit_beacon_epoch} must be strictly before manifest activation"
            );
        }

        // ---- SUMMARY-BIND V2: the envelope cannot repackage the same signed votes under another
        // pass/reject summary.
        let mut repackaged = good.clone();
        repackaged.passed_leaf_count = 1;
        repackaged.rejected_leaf_bitmap_root = h(0xee);
        assert_eq!(
            verify_certificate_attestation(&repackaged, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateVoteSummaryMismatch)
        );

        // Copying the rewritten summary into each vote without actually re-signing also fails.
        for vote in &mut repackaged.votes {
            vote.passed_leaf_count = repackaged.passed_leaf_count;
            vote.rejected_leaf_bitmap_root = repackaged.rejected_leaf_bitmap_root;
        }
        assert_eq!(
            verify_certificate_attestation(&repackaged, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateVoteSignatureInvalid)
        );

        let mut impossible_count = good.clone();
        impossible_count.passed_leaf_count = 3;
        assert_eq!(
            verify_certificate_attestation(&impossible_count, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificatePassedLeafCountOutOfRange)
        );
        let good_hash = good.hash();
        apply_palw_overlay_effect(PalwOverlayEffect::Certificate(good), &store, &beacon, Some(&ctx), None).unwrap();
        assert_eq!(store.certificate(good_hash).unwrap().passed_leaf_count, 2);

        // ---- AUTHSET-01: a certificate declaring the WRONG auditor_set_commitment is rejected ----
        let mut wrong_committee = cert(vec![signed(0, 1), signed(1, 1)], 80);
        wrong_committee.auditor_set_commitment = h(0x7e);
        assert_eq!(
            verify_certificate_attestation(&wrong_committee, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateAuditorSetMismatch)
        );

        // ---- AUTHSET-01: a vote from OUTSIDE the beacon-selected slate is rejected ----
        let mut outsider = signed_over(0, 1, &honest_sample_root); // valid sig, but re-pointed to a non-slate bond
        outsider.bond_outpoint = op(0xee);
        assert_eq!(
            verify_certificate_attestation(&cert(vec![outsider], 40), &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateVoteOutsideCommittee)
        );

        // ---- SAMPLE-01: a certificate whose audit_sample_root is NOT the re-derived one is rejected ----
        let mut wrong_sample = cert(vec![signed(0, 1), signed(1, 1)], 80);
        wrong_sample.audit_sample_root = h(0x5e);
        assert_eq!(
            verify_certificate_attestation(&wrong_sample, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateAuditSampleRootMismatch)
        );

        // ---- SAMPLE-01: a vote signed over an ARBITRARY sample (not the re-derived root) is rejected.
        // The certificate still declares the honest root (so it passes the SAMPLE-01 equality), but the
        // vote signed a different root, so the digest — built from the re-derived root — does not verify.
        let forged_sample_vote = cert(vec![signed_over(0, 1, &h(0xaa)), signed(1, 1)], 80);
        assert_eq!(
            verify_certificate_attestation(&forged_sample_vote, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateVoteSignatureInvalid)
        );

        // ---- the classic forgery: right-length garbage signatures ----
        let garbage = cert(
            vec![
                PalwAuditorVoteV2 {
                    bond_outpoint: bond_op(0),
                    vote: 1,
                    checked_leaf_bitmap_root: h(6),
                    passed_leaf_count: 2,
                    rejected_leaf_bitmap_root: h(5),
                    signature: vec![0u8; 4627],
                },
                PalwAuditorVoteV2 {
                    bond_outpoint: bond_op(1),
                    vote: 1,
                    checked_leaf_bitmap_root: h(6),
                    passed_leaf_count: 2,
                    rejected_leaf_bitmap_root: h(5),
                    signature: vec![0u8; 4627],
                },
            ],
            80,
        );
        assert_eq!(
            verify_certificate_attestation(&garbage, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateVoteSignatureInvalid)
        );

        // ---- a real signature replayed under a DIFFERENT slate bond fails (sig binds to its own key) ----
        let mut stolen = signed(0, 1);
        stolen.bond_outpoint = bond_op(1);
        assert_eq!(
            verify_certificate_attestation(&cert(vec![stolen], 40), &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateVoteSignatureInvalid)
        );

        // ---- honest but SHORT of quorum: 40 of 100 passing < 2/3 ----
        let short = cert(vec![signed(0, 1), signed(1, 0), signed(2, 0)], 40);
        assert_eq!(verify_certificate_attestation(&short, &m, &ctx, &leaves), Err(PalwOverlayError::CertificateQuorumNotReached));

        // ---- honest PASS from one auditor, but the other selected auditors WITHHOLD ----
        // The denominator remains the entire 40+40+20 selected slate. Before this regression fix the
        // verifier summed only submitted votes, turning this into 40/40 and accepting a false quorum.
        let withheld = cert(vec![signed(0, 1)], 40);
        assert_eq!(verify_certificate_attestation(&withheld, &m, &ctx, &leaves), Err(PalwOverlayError::CertificateQuorumNotReached));

        // ---- one bond must not be counted twice ----
        let dup = cert(vec![signed(0, 1), signed(0, 1)], 80);
        assert_eq!(verify_certificate_attestation(&dup, &m, &ctx, &leaves), Err(PalwOverlayError::CertificateDuplicateVoteBond));

        // ---- ADR-0040 §12′: the DECLARED approving stake must equal the tally, both directions ----
        let inflated = cert(vec![signed(0, 1), signed(1, 1), signed(2, 0)], u128::MAX);
        assert_eq!(
            verify_certificate_attestation(&inflated, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateApprovingStakeMismatch)
        );
        let deflated = cert(vec![signed(0, 1), signed(1, 1), signed(2, 0)], 1);
        assert_eq!(
            verify_certificate_attestation(&deflated, &m, &ctx, &leaves),
            Err(PalwOverlayError::CertificateApprovingStakeMismatch)
        );

        // ---- §5.17.3: an UNRESOLVABLE audit-epoch seed fails closed ----
        let no_seed_ctx = PalwCertificateAttestationCtx { prev_seed: None, ..ctx };
        assert_eq!(
            verify_certificate_attestation(&cert(vec![signed(0, 1), signed(1, 1)], 80), &m, &no_seed_ctx, &leaves),
            Err(PalwOverlayError::CertificateAuditEpochSeedUnresolved)
        );

        // ---- §5.17.3: a certificate stale beyond the inclusion window is rejected ----
        let stale_ctx = PalwCertificateAttestationCtx { inclusion_epoch: 5 + 17, ..ctx };
        let mut stale_cert = cert(vec![signed(0, 1), signed(1, 1)], 80);
        stale_cert.certificate_epoch = stale_ctx.inclusion_epoch;
        assert_eq!(
            verify_certificate_attestation(&stale_cert, &m, &stale_ctx, &leaves),
            Err(PalwOverlayError::CertificateOutsideAuditInclusionWindow)
        );

        // ---- SEL-01 / order independence: the re-derivation is stable no matter the record order ----
        let reordered = ProviderBondView::from_records((0..3usize).rev().map(|i| {
            let outpoint = bond_op(i);
            (outpoint, view.get(&outpoint).unwrap().clone())
        }));
        let (_slate2, commitment2) =
            kaspa_consensus_core::palw::select_auditor_committee(&seed, &bid, &reordered, POV, &empty, &empty, 8);
        assert_eq!(commitment2, honest_commitment, "committee re-derivation is independent of record order");
    }

    /// §11.2: a beacon commit accumulates into the epoch; a matching reveal is recorded as valid; a reveal
    /// with no prior commit (or a wrong random) is inert (dropped, not recorded).
    #[test]
    fn apply_beacon_commit_reveal_accumulates() {
        use kaspa_consensus_core::palw::{PalwBeaconCommitV1, PalwBeaconRevealV1, beacon_commitment};
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
        let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
        // D3-b: the acceptance arm resolves clauses 11/12 against this staged context. An arm
        // called WITHOUT it rejects every leaf chunk (fail-closed), so tests stage it rather
        // than opting out of the gate.
        let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
        let _pcpb_ctx = pcpb_test_support::ctx(&pcpb_store);

        let bond = TransactionOutpoint::new(h(0x50), 0);
        let random = [7u8; 64];
        let commitment = beacon_commitment(9, &random, &bond);
        // commit for epoch 9 ⇒ accumulated.
        let commit = PalwBeaconCommitV1 { version: 1, epoch: 9, bond_outpoint: bond, commitment, signature: vec![] };
        apply_palw_overlay_effect(PalwOverlayEffect::BeaconCommit(commit), &store, &beacon, None, None).unwrap();
        assert_eq!(beacon.commitment_of(9, &bond).unwrap(), Some(commitment));
        assert_eq!(beacon.epoch_inputs(9).unwrap().valid_reveals.len(), 0);

        // a reveal with the WRONG random does not open the commit ⇒ not recorded.
        let bad = PalwBeaconRevealV1 { version: 1, epoch: 9, bond_outpoint: bond, random_64: [0u8; 64], signature: vec![] };
        apply_palw_overlay_effect(PalwOverlayEffect::BeaconReveal(bad), &store, &beacon, None, None).unwrap();
        assert_eq!(beacon.epoch_inputs(9).unwrap().valid_reveals.len(), 0);

        // the matching reveal ⇒ recorded as a valid reveal.
        let good = PalwBeaconRevealV1 { version: 1, epoch: 9, bond_outpoint: bond, random_64: random, signature: vec![] };
        let entropy = good.entropy_digest();
        apply_palw_overlay_effect(PalwOverlayEffect::BeaconReveal(good), &store, &beacon, None, None).unwrap();
        assert_eq!(beacon.epoch_inputs(9).unwrap().valid_reveals, vec![(bond, entropy)]);
        assert_ne!(entropy, commitment, "the public E-2 commitment must not be reused as R_E entropy");

        // a reveal for an epoch with no commit is inert.
        let orphan = PalwBeaconRevealV1 { version: 1, epoch: 20, bond_outpoint: bond, random_64: random, signature: vec![] };
        apply_palw_overlay_effect(PalwOverlayEffect::BeaconReveal(orphan), &store, &beacon, None, None).unwrap();
        assert_eq!(beacon.epoch_inputs(20).unwrap().valid_reveals.len(), 0);
    }

    /// §12.3: the R_E → eligibility_digest bridge. With no beacon seed carried at the selected parent,
    /// resolve returns None; once a state is written, the resolved digest equals the direct
    /// `eligibility_hash` over that seed (proving R_E makes clause 9 computable). NOT enforced anywhere.
    #[test]
    fn resolve_eligibility_from_beacon_seed() {
        use kaspa_consensus_core::palw::{PalwBeaconStateV1, eligibility_hash};
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let beacon = DbPalwBeaconStore::new(db, CachePolicy::Count(16));
        let sp = h(0x30);
        let (net, chain_commit, target, batch_id, leaf_index, leaf_hash, nf) = (0x9107u32, h(1), 42u64, h(2), 3u32, h(4), h(5));

        // no carried seed ⇒ None.
        assert_eq!(
            resolve_palw_eligibility(&beacon, sp, net, &chain_commit, target, &batch_id, leaf_index, &leaf_hash, &nf).unwrap(),
            None
        );

        // write a beacon state at the selected parent, then resolve.
        let seed = h(0x77);
        beacon
            .set_state(
                sp,
                Arc::new(PalwBeaconStateV1 {
                    version: 1,
                    epoch: 9,
                    seed,
                    dns_anchor: h(0),
                    anchor_blue_score: 0,
                    anchor_daa_score: 0,
                    anchor_overlay_root: h(0),
                    valid_reveals_root: h(0),
                    missing_commitments_root: h(0),
                    mode: 0,
                    degraded_epochs: 0,
                    valid_reveal_count: 0,
                    missing_commit_count: 0,
                }),
            )
            .unwrap();

        let got = resolve_palw_eligibility(&beacon, sp, net, &chain_commit, target, &batch_id, leaf_index, &leaf_hash, &nf).unwrap();
        let want = eligibility_hash(net, &seed, &chain_commit, target, &batch_id, leaf_index, &leaf_hash, &nf);
        assert_eq!(got, Some(want));
    }

    /// §12.1: the clause-6 bridge. No carried record ⇒ None; a record whose anchor is the zero
    /// bootstrap ⇒ None (fail-closed — no certificate is derivable); a record with a confirmed anchor
    /// ⇒ chain_commit over the anchor + the on-demand certificate digest, matching the direct pure
    /// computation. NOT enforced anywhere (C5 flips clauses 6–9 atomically).
    #[test]
    fn resolve_chain_commit_from_beacon_record() {
        use kaspa_consensus_core::palw::{BeaconDnsAnchor, PalwBeaconStateV1, chain_commit, dns_finality_certificate_hash_v1};
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let beacon = DbPalwBeaconStore::new(db, CachePolicy::Count(16));
        let (net, target) = (0x9107u32, 42u64);
        let state = |anchor: BeaconDnsAnchor| PalwBeaconStateV1 {
            version: 1,
            epoch: 9,
            seed: h(0x77),
            dns_anchor: anchor.hash,
            anchor_blue_score: anchor.blue_score,
            anchor_daa_score: anchor.daa_score,
            anchor_overlay_root: anchor.overlay_root,
            valid_reveals_root: h(0),
            missing_commitments_root: h(0),
            mode: 0,
            degraded_epochs: 0,
            valid_reveal_count: 0,
            missing_commit_count: 0,
        };

        // no carried record ⇒ None.
        assert_eq!(resolve_palw_chain_commit(&beacon, h(0x40), net, target).unwrap(), None);

        // zero bootstrap anchor ⇒ None (fail-closed).
        let sp_boot = h(0x41);
        beacon.set_state(sp_boot, Arc::new(state(BeaconDnsAnchor::UNCONFIRMED))).unwrap();
        assert_eq!(resolve_palw_chain_commit(&beacon, sp_boot, net, target).unwrap(), None);

        // confirmed anchor ⇒ Some(chain_commit(anchor, cert_v1(facts), S, net)).
        let anchor = BeaconDnsAnchor { hash: h(0x50), blue_score: 700, daa_score: 900, overlay_root: h(0x51) };
        let sp = h(0x42);
        beacon.set_state(sp, Arc::new(state(anchor))).unwrap();
        let want = chain_commit(&anchor.hash, &dns_finality_certificate_hash_v1(&anchor), target, net);
        assert_eq!(resolve_palw_chain_commit(&beacon, sp, net, target).unwrap(), Some(want));
    }

    /// §16.3: the clause-7 lane HOLD bridge. An absent lane-bits row (genesis / pre-activation parent)
    /// falls back to the lane's genesis bits; a carried row supplies the per-lane bits. NOT enforced.
    #[test]
    fn resolve_lane_hold_bits() {
        use crate::model::stores::palw_lane_bits::DbPalwLaneBitsStore;
        use kaspa_consensus_core::palw::{LaneDifficultyParams, PalwLaneBitsV1};
        use kaspa_consensus_core::pow_layer0::WorkLane;
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwLaneBitsStore::new(db, CachePolicy::Count(16));
        let params =
            LaneDifficultyParams { genesis_hash_bits: 0x1d00ffff, genesis_replica_bits: 0x1e00abcd, ..LaneDifficultyParams::INERT };
        let sp = h(0x60);

        // absent row ⇒ genesis lane bits per lane.
        assert_eq!(resolve_palw_lane_hold_bits(&store, sp, WorkLane::HashFloor, &params).unwrap(), 0x1d00ffff);
        assert_eq!(resolve_palw_lane_hold_bits(&store, sp, WorkLane::ReplicaPalw, &params).unwrap(), 0x1e00abcd);

        // carried row ⇒ that block's per-lane bits.
        store.set(sp, PalwLaneBitsV1 { hash_bits: 0x1c00aaaa, replica_bits: 0x1b00bbbb }).unwrap();
        assert_eq!(resolve_palw_lane_hold_bits(&store, sp, WorkLane::HashFloor, &params).unwrap(), 0x1c00aaaa);
        assert_eq!(resolve_palw_lane_hold_bits(&store, sp, WorkLane::ReplicaPalw, &params).unwrap(), 0x1b00bbbb);
    }

    /// §18.1: `resolve_palw_binding` reads the leaf + certificate a header names and packs them into the
    /// pure verify inputs; store absence fails closed. The resolved binding drives
    /// `verify_palw_ticket_store_facts` so a matching header passes clauses 1–5 and a wrong nullifier is
    /// rejected — the concrete verify_palw_ticket ↔ PalwStore bridge.
    #[test]
    fn resolve_binding_and_store_facts() {
        use kaspa_consensus_core::palw::verify_palw_ticket_store_facts;
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db, CachePolicy::Count(64));

        // absent leaf/cert fail closed.
        assert_eq!(resolve_palw_binding(h(1), 0, h(9), 7, &store), Err(PalwBindingError::LeafAbsent));

        // populate a leaf + certificate.
        store.insert_leaf(h(1), 0, Arc::new(leaf(0))).unwrap();
        let cert = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: h(1),
            manifest_hash: h(2),
            leaf_root: h(3),
            audit_beacon_epoch: 5,
            audit_sample_root: h(4),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(5),
            certificate_epoch: 6,
            activation_epoch: 6,
            expiry_epoch: 20,
            auditor_set_commitment: h(7),
            approving_stake: 0,
            votes: vec![],
        };
        let cert_hash = cert.hash();
        store.insert_certificate(cert_hash, Arc::new(cert)).unwrap();
        // leaf present but cert hash unknown ⇒ CertAbsent.
        assert_eq!(resolve_palw_binding(h(1), 0, h(99), 7, &store), Err(PalwBindingError::CertAbsent));

        let mut unsupported = store.certificate(cert_hash).unwrap().as_ref().clone();
        unsupported.version = 1;
        let unsupported_hash = unsupported.hash();
        store.insert_certificate(unsupported_hash, Arc::new(unsupported)).unwrap();
        assert_eq!(resolve_palw_binding(h(1), 0, unsupported_hash, 7, &store), Err(PalwBindingError::CertUnsupportedVersion(1)));

        // kaspa-pq **ADR-0040 CERT-BATCH — REJECT: a certificate belonging to a DIFFERENT batch.**
        //
        // `resolve_palw_binding` looks the certificate up by hash alone, so without the cross-bind a
        // header could name any stored certificate and inherit its window. Here batch h(1)'s leaf is
        // present and batch h(0x21)'s certificate is a perfectly valid, stored, resolvable blob — the
        // ONLY thing wrong is that it certifies someone else's batch. It must be attributably rejected,
        // not silently accepted and not conflated with "absent".
        let foreign = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: h(0x21),
            manifest_hash: h(0x22),
            leaf_root: h(0x23),
            audit_beacon_epoch: 5,
            audit_sample_root: h(0x24),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(0x25),
            certificate_epoch: 6,
            activation_epoch: 0,
            expiry_epoch: u64::MAX,
            auditor_set_commitment: h(0x26),
            approving_stake: 0,
            votes: vec![],
        };
        let foreign_hash = foreign.hash();
        store.insert_certificate(foreign_hash, Arc::new(foreign)).unwrap();
        assert_eq!(
            resolve_palw_binding(h(1), 0, foreign_hash, 7, &store),
            Err(PalwBindingError::CertBatchMismatch),
            "a certificate that certifies another batch must not resolve for this one"
        );
        // ...and the resolver stays honest in the other direction: the batch's OWN certificate resolves.
        assert!(resolve_palw_binding(h(1), 0, cert_hash, 7, &store).is_ok());

        // ADR-0040 CERT-BATCH — certificates are write-once by content, mirroring leaves. Re-inserting
        // identical content is idempotent; a different blob at the same key fails closed.
        store.insert_certificate(foreign_hash, store.certificate(foreign_hash).unwrap()).unwrap();
        let mut collider = (*store.certificate(foreign_hash).unwrap()).clone();
        collider.expiry_epoch = 1;
        assert!(
            store.insert_certificate(foreign_hash, Arc::new(collider)).is_err(),
            "a differing certificate at an existing content key must be refused, not silently overwrite"
        );

        // full resolution: leaf(0) commits raw nullifier h(3), proof_type 1, activation 7, expiry 13.
        let resolved = resolve_palw_binding(h(1), 0, cert_hash, /*target_daa_interval*/ 42, &store).unwrap();
        assert_eq!(resolved.binding.ticket_nullifier_commitment, kaspa_consensus_core::palw::ticket_nullifier_commitment(&h(3)));
        assert_eq!(resolved.binding.proof_type, 1);
        assert_eq!(resolved.binding.leaf_activation_epoch, 7);
        assert_eq!(resolved.binding.leaf_expiry_epoch, 13);
        assert_eq!(resolved.binding.target_daa_interval, 42);
        assert_eq!(resolved.leaf_hash, leaf(0).leaf_hash());

        // clauses 1–5 over the resolved binding: epoch 10 ∈ [7,13) leaf & [6,20) cert, interval matches.
        let cert_active = resolved.cert_activation_epoch <= 10 && 10 < resolved.cert_expiry_epoch;
        assert!(verify_palw_ticket_store_facts(&h(3), 1, 42, &resolved.binding, cert_active, 10).is_ok());
        // a header whose nullifier disagrees with the resolved leaf is rejected.
        assert!(verify_palw_ticket_store_facts(&h(4), 1, 42, &resolved.binding, cert_active, 10).is_err());
        // epoch outside the leaf window is rejected (LeafNotActive at epoch 13).
        assert!(verify_palw_ticket_store_facts(&h(3), 1, 42, &resolved.binding, cert_active, 13).is_err());
    }

    // =========================================================================================
    // ADR-0040 ECON-03 leg 5 — the AUTHORIZED EXIT.
    //
    // These exercise `palw_provider_unbond_request_authorized` directly — the per-request predicate the
    // acceptance-time `ProviderUnbondAuthFilter` (virtual_processor/utxo_validation.rs) consults to
    // SKIP an unauthorized `0x37` transaction. The predicate's four checks are the security core; a
    // `false` verdict now means the carrying transaction is not accepted (and mutates nothing) rather
    // than the whole block being rejected — but WHAT the predicate accepts vs. refuses is unchanged, so
    // these cases still pin the rule. The block-stays-valid / record-unchanged consequences of a skip
    // are pinned separately in `provider_unbond_auth_filter` (utxo_validation.rs tests).
    // =========================================================================================

    /// Builds a bonded provider and the machinery to sign (or mis-sign) exits for it.
    #[allow(clippy::type_complexity)]
    fn econ03_unbond_fixture() -> (
        libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
        libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
        TransactionOutpoint,
        kaspa_consensus_core::palw::ProviderBondView,
    ) {
        use kaspa_consensus_core::palw::{PALW_PAYLOAD_VERSION_V1, PalwProviderBondPayloadV1, ProviderBondView};
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_PROVIDER_BOND;
        use kaspa_consensus_core::tx::Transaction;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        let owner = mldsa::generate_key_pair([0x11; 32]);
        let attacker = mldsa::generate_key_pair([0x22; 32]);

        let payload = PalwProviderBondPayloadV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            owner_public_key: owner.verification_key.as_ref().to_vec(),
            operator_group_id: h(1),
            runtime_classes: vec![h(2)],
            capacity_by_shape: vec![(1, 10)],
            reward_key_root: h(4),
            amount_sompi: 1_000,
            unbond_delay_epochs: 10,
        };
        let bond_tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_PROVIDER_BOND, 0, borsh::to_vec(&payload).unwrap());
        let outpoint = TransactionOutpoint::new(bond_tx.id(), 0);
        let mut view = ProviderBondView::new();
        // Accepted at DAA 500 ⇒ Active from 500 onward.
        view.apply(&kaspa_consensus_core::palw::palw_provider_bond_mutations_from_accepted_txs(&[bond_tx], 500, 1_000, 4));
        (owner, attacker, outpoint, view)
    }

    const ECON03_NET: u32 = 7;

    /// A `0x37` transaction carrying `req`.
    fn econ03_unbond_tx(req: &kaspa_consensus_core::palw::PalwProviderUnbondRequestV1) -> kaspa_consensus_core::tx::Transaction {
        kaspa_consensus_core::tx::Transaction::new(
            0,
            vec![],
            vec![],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_PROVIDER_UNBOND,
            0,
            borsh::to_vec(req).unwrap(),
        )
    }

    /// An unbond request for `outpoint`, signed by `key` under `context` — so a test can vary the
    /// signer and the signing context independently of the key the request CLAIMS.
    fn econ03_signed_request(
        key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
        claimed_pubkey: Vec<u8>,
        outpoint: TransactionOutpoint,
        network_id: u32,
        context: &[u8],
    ) -> kaspa_consensus_core::palw::PalwProviderUnbondRequestV1 {
        use kaspa_consensus_core::palw::{PALW_PAYLOAD_VERSION_V1, PalwProviderUnbondRequestV1};
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;
        let mut req = PalwProviderUnbondRequestV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            bond_outpoint: outpoint,
            owner_public_key: claimed_pubkey,
            signature: vec![],
        };
        let d = req.signing_hash(network_id);
        req.signature = mldsa::sign(&key.signing_key, d.as_bytes().as_slice(), context, [0x5a; 32]).expect("sign").as_ref().to_vec();
        req
    }

    /// Unbonding regression case. Unbonding ejects a provider from the active set.
    /// If anyone could publish a `0x37` naming someone else's bond, knocking every honest provider
    /// out of selection would cost a transaction fee. Each assertion is a distinct forgery attempt.
    #[test]
    fn econ03_only_the_bond_owner_can_request_an_exit() {
        use kaspa_consensus_core::dns_finality::STAKE_ATTESTATION_SIG_LEN;
        use kaspa_consensus_core::palw::{PALW_PAYLOAD_VERSION_V1, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT, PalwProviderUnbondRequestV1};

        let (owner, attacker, outpoint, view) = econ03_unbond_fixture();
        let owner_pk = owner.verification_key.as_ref().to_vec();
        let pov = 600; // the bond is Active here.

        // ---- (0) the honest case: the owner's own signature authorizes the exit. ----
        let good = econ03_signed_request(&owner, owner_pk.clone(), outpoint, ECON03_NET, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT);
        assert!(palw_provider_unbond_request_authorized(&good, &view, ECON03_NET, pov));

        // ---- (1) UNSIGNED: right-length zeros. The whole class of "shape passed, so it is fine". ----
        let unsigned = PalwProviderUnbondRequestV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            bond_outpoint: outpoint,
            owner_public_key: owner_pk.clone(),
            signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
        };
        // It passes the stateless SHAPE check — which is exactly why this rule must exist.
        assert_eq!(kaspa_consensus_core::palw::validate_palw_overlay_payload(0x37, &borsh::to_vec(&unsigned).unwrap()), Ok(()));
        assert!(!palw_provider_unbond_request_authorized(&unsigned, &view, ECON03_NET, pov));

        // ---- (2) WRONG KEY: the attacker signs its own request, claiming its own key. ----
        // The key is checked against the RECORD, not against the request's own claim.
        let attacker_pk = attacker.verification_key.as_ref().to_vec();
        let wrong_key = econ03_signed_request(&attacker, attacker_pk, outpoint, ECON03_NET, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT);
        assert!(!palw_provider_unbond_request_authorized(&wrong_key, &view, ECON03_NET, pov));

        // ---- (3) KEY SUBSTITUTION: attacker signs but CLAIMS the owner's key, so check (3) passes
        // and only the signature stands between the attacker and the honest provider's exit. ----
        let substituted =
            econ03_signed_request(&attacker, owner_pk.clone(), outpoint, ECON03_NET, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT);
        assert!(!palw_provider_unbond_request_authorized(&substituted, &view, ECON03_NET, pov));

        // ---- (4) CROSS-NETWORK REPLAY: a valid authorization from another network. ----
        let other_net =
            econ03_signed_request(&owner, owner_pk.clone(), outpoint, ECON03_NET + 1, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT);
        assert!(!palw_provider_unbond_request_authorized(&other_net, &view, ECON03_NET, pov));

        // ---- (5) CROSS-BOND REPLAY: the owner's signature re-aimed at a different bond. The
        // digest covers bond_outpoint, so moving it invalidates the signature. ----
        let mut cross_bond = good.clone();
        cross_bond.bond_outpoint = TransactionOutpoint::new(outpoint.transaction_id, outpoint.index + 1);
        // (that bond does not resolve either, which is itself a rejection — check (1))
        assert!(!palw_provider_unbond_request_authorized(&cross_bond, &view, ECON03_NET, pov));

        // ---- (6) UNKNOWN BOND: an exit naming a bond nobody registered is rejected, not ignored. ----
        let phantom = TransactionOutpoint::new(econ03_unbond_tx(&good).id(), 3);
        let ghost = econ03_signed_request(&owner, owner_pk, phantom, ECON03_NET, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT);
        assert!(!palw_provider_unbond_request_authorized(&ghost, &view, ECON03_NET, pov));
    }

    /// A bond that is not `Pending`/`Active` has no exit to request. Rejecting the second request
    /// keeps at most ONE unbond mutation per bond per chain, which is what keeps `ProviderBondView`
    /// apply/revert exact inverses — and a `Slashed` bond is forfeit, not exiting.
    #[test]
    fn econ03_exit_is_refused_unless_the_bond_is_pending_or_active() {
        use kaspa_consensus_core::palw::{PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT, PalwProviderBondMutation};

        let (owner, _attacker, outpoint, view) = econ03_unbond_fixture();
        let owner_pk = owner.verification_key.as_ref().to_vec();
        let req = econ03_signed_request(&owner, owner_pk, outpoint, ECON03_NET, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT);

        // Pending (before activation at 500) — an exit may be requested.
        assert!(palw_provider_unbond_request_authorized(&req, &view, ECON03_NET, 499));
        // Active.
        assert!(palw_provider_unbond_request_authorized(&req, &view, ECON03_NET, 500));

        // Already Unbonding ⇒ refused, so the mutation cannot apply twice.
        let mut unbonding = view.clone();
        unbonding.apply(&[PalwProviderBondMutation::Unbond(outpoint, 600)]);
        assert!(!palw_provider_unbond_request_authorized(&req, &unbonding, ECON03_NET, 600));
        // ...but before the stamp it is still Active at that point of view, and still exitable.
        assert!(palw_provider_unbond_request_authorized(&req, &unbonding, ECON03_NET, 599));

        // Slashed ⇒ refused. A forfeit bond has no exit.
        let mut slashed = view.clone();
        slashed.apply(&[PalwProviderBondMutation::Slash(outpoint, 700)]);
        assert!(!palw_provider_unbond_request_authorized(&req, &slashed, ECON03_NET, 700));
    }

    /// Two authorized exits for ONE bond in a single block both pass — they are judged against the
    /// same point of view. Both carry the block's `accepted_daa_score`, so the registry producer
    /// canonicalizes them to one first-only mutation. This makes persisted apply/revert a strict
    /// one-to-one inverse while retaining the same final state.
    #[test]
    fn econ03_duplicate_exits_in_one_block_are_canonicalized() {
        use kaspa_consensus_core::palw::{
            PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT, PalwProviderBondMutation, palw_provider_bond_mutations_from_accepted_txs,
            provider_bond_release_daa_score,
        };

        let (owner, _attacker, outpoint, view) = econ03_unbond_fixture();
        let owner_pk = owner.verification_key.as_ref().to_vec();
        let req = econ03_signed_request(&owner, owner_pk, outpoint, ECON03_NET, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT);
        let txs = vec![econ03_unbond_tx(&req), econ03_unbond_tx(&req)];

        // Both are authorized against the pre-block view (each judged independently by the skip filter).
        assert!(palw_provider_unbond_request_authorized(&req, &view, ECON03_NET, 600));

        // The producer emits one canonical Unbond mutation. The txs are byte-identical here (a body
        // cannot actually carry the same tx id twice), which is the strictest form of duplicate
        // input; independently funded requests for the same outpoint take the same branch.
        let muts = palw_provider_bond_mutations_from_accepted_txs(&txs, 600, 1_000, 4);
        let unbonds: Vec<_> = muts.iter().filter(|m| matches!(m, PalwProviderBondMutation::Unbond(..))).collect();
        assert_eq!(unbonds.len(), 1);

        let mut applied = view.clone();
        applied.apply(&muts);
        assert_eq!(applied.get(&outpoint).unwrap().unbond_request_daa_score, Some(600));
        assert_eq!(provider_bond_release_daa_score(applied.get(&outpoint).unwrap(), 100), Some(600 + 10 * 100));

        // Reverting the canonical mutation restores the pre-block view exactly.
        applied.revert(&muts);
        assert_eq!(applied, view);
    }

    /// The dedicated signing context is what stops a signature made for a DIFFERENT object being
    /// presented as an unbond authorization. Distinctness is asserted as a table property in
    /// `signature_domains`; this is the operational half — a real signature over the very same
    /// digest under any other registered context must NOT verify here.
    #[test]
    fn econ03_unbond_context_is_not_confusable_with_any_other_signing_domain() {
        use kaspa_consensus_core::palw::PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT;
        use kaspa_consensus_core::signature_domains::SIGNATURE_DOMAINS;

        let (owner, _attacker, outpoint, view) = econ03_unbond_fixture();
        let owner_pk = owner.verification_key.as_ref().to_vec();
        let pov = 600;

        let mut others = 0usize;
        for d in SIGNATURE_DOMAINS {
            if d.context == PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT {
                continue;
            }
            others += 1;
            // The bond owner's own key, the correct network, the correct bond, the correct digest —
            // everything right except the context.
            let req = econ03_signed_request(&owner, owner_pk.clone(), outpoint, ECON03_NET, d.context);
            assert!(
                !palw_provider_unbond_request_authorized(&req, &view, ECON03_NET, pov),
                "a signature under {} must not authorize an unbond",
                d.object
            );
        }
        assert!(others >= 10, "every other registered domain must be exercised, saw {others}");
    }

    // =========================================================================================
    // ADR-MA §17.3 — activation-certificate validator attestation
    // =========================================================================================

    mod activation_certificate {
        use super::h;
        use crate::processes::palw::verify_compute_set_activation_certificate;
        use kaspa_consensus_core::dns_finality::{ActiveBondView, StakeBondRecord};
        use kaspa_consensus_core::palw_compute_set::{
            ComputeSetRegistryError, PALW_COMPUTE_SET_ACTIVATION_CERT_VERSION, PALW_COMPUTE_SET_ACTIVATION_MLDSA87_CONTEXT,
            PalwComputeSetActivationCertificateV1, PalwComputeSetActivationVoteV1, palw_activation_validator_set_commitment,
            palw_activation_votes_root,
        };
        use kaspa_consensus_core::tx::TransactionOutpoint;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        pub(super) const NET: u32 = 111;
        pub(super) const POV: u64 = 700;

        pub(super) struct Validator {
            pub(super) bond: StakeBondRecord,
            pub(super) signing_key: mldsa::MLDSA87SigningKey,
        }

        /// The `ActiveBondView` these validators form (shared with the §17.4 governance tests).
        pub(super) fn bond_view(validators: &[Validator]) -> ActiveBondView {
            ActiveBondView::from_records(validators.iter().map(|v| (v.bond.bond_outpoint, v.bond.clone())))
        }

        pub(super) fn validator(tag: u8, amount: u64) -> Validator {
            let kp = mldsa::generate_key_pair([tag; 32]);
            let pubkey = kp.verification_key.as_ref().to_vec();
            let bond = StakeBondRecord {
                version: 1,
                bond_outpoint: TransactionOutpoint::new(h(tag), 0),
                owner_pubkey_hash: h(tag.wrapping_add(0x40)),
                validator_pubkey_hash: kaspa_consensus_core::dns_finality::validator_id_from_pubkey(&pubkey),
                validator_pubkey: pubkey,
                amount,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [tag; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: kaspa_consensus_core::dns_finality::BondStatus::Active,
                last_attested_epoch: None,
                dormant_at_daa_score: None,
                dormant_at_epoch: None,
            };
            Validator { bond, signing_key: kp.signing_key }
        }

        /// A quorum-meeting certificate over `validators`, with `passing` of them voting PASS
        /// (the rest voting reject), every declared commitment computed honestly.
        fn certified(validators: &[Validator], passing: usize) -> (PalwComputeSetActivationCertificateV1, ActiveBondView) {
            let mut active: Vec<StakeBondRecord> = validators.iter().map(|v| v.bond.clone()).collect();
            active.sort_by(|a, b| {
                (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
            });
            let commitment = palw_activation_validator_set_commitment(&active).unwrap();
            let total: u128 = active.iter().map(|b| b.amount as u128).sum();
            let mut cert = PalwComputeSetActivationCertificateV1 {
                version: PALW_COMPUTE_SET_ACTIVATION_CERT_VERSION,
                compute_set_id: h(0xd1),
                descriptor_hash: h(0xd1),
                conformance_result_root: h(0xd2),
                auditor_capacity_evidence_root: h(0xd3),
                artifact_reproducibility_root: h(0xd4),
                validator_set_commitment: commitment,
                approving_stake: 0,
                total_selected_stake: total,
                effective_from_daa: 1_000,
                votes_root: Hash64::default(),
                votes: vec![],
            };
            let mut votes: Vec<PalwComputeSetActivationVoteV1> = validators
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let mut vote = PalwComputeSetActivationVoteV1 {
                        bond_outpoint: v.bond.bond_outpoint,
                        vote: if i < passing { 1 } else { 0 },
                        signature: vec![],
                    };
                    let digest = vote.signing_hash(NET, &cert);
                    let sig =
                        mldsa::sign(&v.signing_key, digest.as_byte_slice(), PALW_COMPUTE_SET_ACTIVATION_MLDSA87_CONTEXT, [0x11; 32])
                            .expect("mldsa sign");
                    vote.signature = sig.as_ref().to_vec();
                    vote
                })
                .collect();
            votes.sort_by(|a, b| {
                (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
            });
            cert.approving_stake = votes
                .iter()
                .filter(|v| v.vote == 1)
                .map(|v| validators.iter().find(|x| x.bond.bond_outpoint == v.bond_outpoint).unwrap().bond.amount as u128)
                .sum();
            cert.votes_root = palw_activation_votes_root(&votes).unwrap();
            cert.votes = votes;
            let view = ActiveBondView::from_records(validators.iter().map(|v| (v.bond.bond_outpoint, v.bond.clone())));
            (cert, view)
        }

        use kaspa_hashes::Hash64;

        fn expect_reject(result: Result<(), ComputeSetRegistryError>, needle: &str) {
            match result {
                Err(ComputeSetRegistryError::CertificateAttestationInvalid(_, reason)) => {
                    assert!(reason.contains(needle), "expected rejection about {needle:?}, got {reason:?}")
                }
                other => panic!("expected CertificateAttestationInvalid({needle:?}), got {other:?}"),
            }
        }

        /// Green path: three validators, all passing, 2/3 quorum met, every commitment honest.
        #[test]
        fn honest_certificate_verifies() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (cert, view) = certified(&validators, 3);
            verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3).expect("honest certificate");
            // 2-of-3 stake passing also meets a 2/3 quorum (equality reaches).
            let (cert, view) = certified(&validators, 2);
            verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3).expect("exact-threshold certificate");
        }

        #[test]
        fn quorum_shortfall_is_rejected() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (cert, view) = certified(&validators, 1);
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "quorum");
        }

        #[test]
        fn single_key_cannot_activate() {
            // §17.3's whole point: one operator key — one bond, however staked — passing alone
            // meets no 2/3 quorum unless it IS ≥2/3 of the entire active set.
            let validators = [validator(1, 100), validator(2, 300), validator(3, 100)];
            let (cert, view) = certified(&validators, 1); // only the 100-stake validator passes
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "quorum");
        }

        #[test]
        fn vote_flip_fails_signature() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (mut cert, view) = certified(&validators, 2);
            // Flip the reject vote to pass WITHOUT re-signing; keep the declared commitments
            // consistent with the flipped tally so the signature check is what trips.
            let flipped = cert.votes.iter().position(|v| v.vote == 0).unwrap();
            cert.votes[flipped].vote = 1;
            cert.approving_stake = 300;
            cert.votes_root = palw_activation_votes_root(&cert.votes).unwrap();
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "signature");
        }

        #[test]
        fn declared_commitments_are_recomputed() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            // Inflated approving_stake.
            let (mut cert, view) = certified(&validators, 2);
            cert.approving_stake = 300;
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "approving_stake");
            // Wrong votes_root.
            let (mut cert, view) = certified(&validators, 3);
            cert.votes_root = h(0x99);
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "votes_root");
            // Stale validator-set commitment (a bond joined after authoring).
            let (cert, _) = certified(&validators[..2], 2);
            let (_, grown_view) = certified(&validators, 3);
            expect_reject(verify_compute_set_activation_certificate(&cert, &grown_view, NET, POV, 2, 3), "validator_set_commitment");
            // Wrong declared total.
            let (mut cert, view) = certified(&validators, 3);
            cert.total_selected_stake = 1;
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "total_selected_stake");
        }

        #[test]
        fn votes_must_resolve_to_active_bonds() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            // A vote from an outpoint the bond view has never seen.
            let (mut cert, view) = certified(&validators, 3);
            cert.votes[0].bond_outpoint = TransactionOutpoint::new(h(0x7f), 0);
            cert.votes.sort_by(|a, b| {
                (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
            });
            cert.votes_root = palw_activation_votes_root(&cert.votes).unwrap();
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "unknown or inactive");
            // A slashed bond stops voting (and moves the active-set commitment, which is the
            // first mismatch on the verification path).
            let (cert, _) = certified(&validators, 3);
            let mut slashed = validators[0].bond.clone();
            slashed.slashed_at_daa_score = Some(10);
            let view = ActiveBondView::from_records(
                [(slashed.bond_outpoint, slashed)]
                    .into_iter()
                    .chain(validators[1..].iter().map(|v| (v.bond.bond_outpoint, v.bond.clone()))),
            );
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "validator_set_commitment");
            // An empty vote list certifies nothing.
            let (mut cert, view) = certified(&validators, 3);
            cert.votes.clear();
            expect_reject(verify_compute_set_activation_certificate(&cert, &view, NET, POV, 2, 3), "no embedded votes");
        }
    }

    // =========================================================================================
    // ADR-MA §17.4 — governance envelope attestation (policy / allocation / halt)
    // =========================================================================================

    mod governance_envelope {
        use super::activation_certificate::{NET, POV, Validator, bond_view, validator};
        use super::h;
        use crate::processes::palw::verify_palw_governance_envelope;
        use kaspa_consensus_core::palw_compute_set::{
            ComputeSetRegistryError, PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION, PALW_GOVERNANCE_ENVELOPE_VERSION,
            PALW_GOVERNANCE_MLDSA87_CONTEXT, PALW_MODEL_ALLOCATION_PLAN_VERSION, PalwComputeSetEmergencyHaltV1,
            PalwGovernanceActionV1, PalwGovernanceEnvelopeV1, PalwGovernanceVoteV1, PalwModelAllocationEntryV1,
            PalwModelAllocationPlanV1, palw_activation_validator_set_commitment, palw_governance_votes_root,
        };
        use kaspa_consensus_core::tx::TransactionOutpoint;
        use kaspa_hashes::Hash64;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        fn halt_action(set: u8) -> PalwGovernanceActionV1 {
            PalwGovernanceActionV1::EmergencyHalt(PalwComputeSetEmergencyHaltV1 {
                version: PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION,
                compute_set_id: h(set),
                evidence_root: Hash64::default(),
            })
        }

        /// A quorum-meeting envelope over `validators`, `approving` of them voting approve.
        fn governed(
            action: PalwGovernanceActionV1,
            validators: &[Validator],
            approving: usize,
        ) -> (PalwGovernanceEnvelopeV1, kaspa_consensus_core::dns_finality::ActiveBondView) {
            let mut active: Vec<_> = validators.iter().map(|v| v.bond.clone()).collect();
            active.sort_by(|a, b| {
                (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
            });
            let mut envelope = PalwGovernanceEnvelopeV1 {
                version: PALW_GOVERNANCE_ENVELOPE_VERSION,
                action,
                validator_set_commitment: palw_activation_validator_set_commitment(&active).unwrap(),
                approving_stake: 0,
                total_selected_stake: active.iter().map(|b| b.amount as u128).sum(),
                votes_root: Hash64::default(),
                votes: vec![],
            };
            let mut votes: Vec<PalwGovernanceVoteV1> = validators
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let mut vote = PalwGovernanceVoteV1 {
                        bond_outpoint: v.bond.bond_outpoint,
                        vote: if i < approving { 1 } else { 0 },
                        signature: vec![],
                    };
                    let digest = vote.signing_hash(NET, &envelope);
                    let sig = mldsa::sign(&v.signing_key, digest.as_byte_slice(), PALW_GOVERNANCE_MLDSA87_CONTEXT, [0x11; 32])
                        .expect("mldsa sign");
                    vote.signature = sig.as_ref().to_vec();
                    vote
                })
                .collect();
            votes.sort_by(|a, b| {
                (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
            });
            envelope.approving_stake = votes
                .iter()
                .filter(|v| v.vote == 1)
                .map(|v| validators.iter().find(|x| x.bond.bond_outpoint == v.bond_outpoint).unwrap().bond.amount as u128)
                .sum();
            envelope.votes_root = palw_governance_votes_root(&votes).unwrap();
            envelope.votes = votes;
            (envelope, bond_view(validators))
        }

        fn expect_reject(result: Result<(), ComputeSetRegistryError>, needle: &str) {
            match result {
                Err(ComputeSetRegistryError::GovernanceAttestationInvalid(reason)) => {
                    assert!(reason.contains(needle), "expected rejection about {needle:?}, got {reason:?}")
                }
                other => panic!("expected GovernanceAttestationInvalid({needle:?}), got {other:?}"),
            }
        }

        #[test]
        fn honest_governance_envelope_verifies() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (envelope, view) = governed(halt_action(0xd1), &validators, 3);
            verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3).expect("honest envelope");
            let (envelope, view) = governed(halt_action(0xd1), &validators, 2);
            verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3).expect("exact-threshold envelope");
        }

        /// REG-AUTH-03: the halt path's whole point — one party cannot stop an arbitrary set.
        #[test]
        fn single_key_cannot_halt_a_set() {
            let validators = [validator(1, 100), validator(2, 300), validator(3, 100)];
            let (envelope, view) = governed(halt_action(0xd1), &validators, 1);
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3), "quorum");
        }

        /// REG-AUTH-01/02: an unsigned action is not merely under-quorate, it carries nothing to
        /// verify — the pre-fix code path applied exactly this.
        #[test]
        fn unsigned_action_is_rejected() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (mut envelope, view) = governed(halt_action(0xd1), &validators, 3);
            envelope.votes.clear();
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3), "no embedded votes");
        }

        /// A vote is bound to the exact action bytes: swapping the cargo under collected
        /// signatures must not verify.
        #[test]
        fn votes_do_not_transfer_to_another_action() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (mut envelope, view) = governed(halt_action(0xd1), &validators, 3);
            envelope.action = halt_action(0xd2);
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3), "signature");
        }

        /// Domain separation: an activation vote's signature cannot be reused as a governance
        /// vote, and a governance vote is scoped to its own action kind.
        #[test]
        fn governance_votes_are_domain_separated() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            // Same validators, same tallies, but the action changes KIND: the band byte and the
            // action hash both move, so no collected signature survives the swap.
            let (halt_envelope, view) = governed(halt_action(0xd1), &validators, 3);
            let mut plan = PalwModelAllocationPlanV1 {
                version: PALW_MODEL_ALLOCATION_PLAN_VERSION,
                plan_id: Hash64::default(),
                sequence: 1,
                effective_from_daa: 1_000,
                entries: vec![PalwModelAllocationEntryV1 { compute_set_id: h(0xd1), target_share_bps: 10_000 }],
            };
            plan.plan_id = plan.derive_plan_id();
            let mut swapped = halt_envelope.clone();
            swapped.action = PalwGovernanceActionV1::AllocationPlan(plan);
            expect_reject(verify_palw_governance_envelope(&swapped, &view, NET, POV, 2, 3), "signature");
        }

        #[test]
        fn declared_commitments_are_recomputed() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (mut envelope, view) = governed(halt_action(0xd1), &validators, 2);
            envelope.approving_stake = 300;
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3), "approving_stake");

            let (mut envelope, view) = governed(halt_action(0xd1), &validators, 3);
            envelope.votes_root = h(0x99);
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3), "votes_root");

            // Stale validator-set commitment (a bond joined after authoring).
            let (envelope, _) = governed(halt_action(0xd1), &validators[..2], 2);
            let (_, grown_view) = governed(halt_action(0xd1), &validators, 3);
            expect_reject(verify_palw_governance_envelope(&envelope, &grown_view, NET, POV, 2, 3), "validator_set_commitment");

            let (mut envelope, view) = governed(halt_action(0xd1), &validators, 3);
            envelope.total_selected_stake = 1;
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3), "total_selected_stake");
        }

        #[test]
        fn votes_must_resolve_to_active_bonds() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (mut envelope, view) = governed(halt_action(0xd1), &validators, 3);
            envelope.votes[0].bond_outpoint = TransactionOutpoint::new(h(0x7f), 0);
            envelope.votes.sort_by(|a, b| {
                (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
            });
            envelope.votes_root = palw_governance_votes_root(&envelope.votes).unwrap();
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 3), "unknown or inactive");
        }

        /// The ADR-0040 P0-5 vacuity guards, inherited verbatim.
        #[test]
        fn degenerate_quorum_parameters_fail_closed() {
            let validators = [validator(1, 100), validator(2, 100), validator(3, 100)];
            let (envelope, view) = governed(halt_action(0xd1), &validators, 3);
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 0, 3), "degenerate");
            expect_reject(verify_palw_governance_envelope(&envelope, &view, NET, POV, 2, 0), "degenerate");
        }
    }

    /// ADR-0045 D3-b, design memo §8 — the grind / seat-binding / fail-closed negatives at the
    /// ACCEPTANCE coordinate (`apply_palw_overlay_effect`, LeafChunk arm), i.e. against the real
    /// clause-11/12 wiring rather than the pure predicates alone.
    ///
    /// Every adversarial fixture is a SELF-CONSISTENT batch: the manifest is built over the
    /// tampered leaves' own Merkle projection, so the membership gate passes and the first
    /// rejection is the PCPB clause under test — a fixture the membership gate caught would prove
    /// nothing about clauses 11/12 (the §5.15.12 rebuild discipline).
    mod pcpb_clause_negatives {
        use super::*;
        use crate::model::stores::palw_pcpb::DbPalwPcpbStore;
        use kaspa_consensus_core::palw::{
            PALW_DISPATCH_KIND_SELF_SERIAL, PALW_LEAF_CHUNK_VERSION_V3, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, PalwDispatchEvidence,
            PalwLeafChunkV1, PalwLeafPcpbWitnessV1, PalwProviderSnapshotEntry, PalwSnapshotWitnessSet, SelfSerialProof,
            palw_build_snapshot_witnesses, palw_job_challenge, palw_pcpb_derive_b, palw_pcpb_receipt_preimage, palw_provider_id,
            palw_provider_pk_hash,
        };
        use kaspa_consensus_core::tx::TransactionOutpoint;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;
        use pcpb_test_support::{DELTA, K, NETWORK_ID, W};

        /// A self-consistent batch for adversarial leaves: manifest derived FROM the leaves (root,
        /// count, registration epoch), content-addressed, with per-leaf derived proofs.
        fn adversarial_batch(
            mut leaves: Vec<PalwPublicLeafV1>,
            witnesses: Vec<PalwLeafPcpbWitnessV1>,
        ) -> (PalwBatchManifestV1, PalwLeafChunkV1) {
            let registration_epoch = leaves[0].registered_epoch;
            let hashes: Vec<Hash64> = leaves
                .iter()
                .map(|l| {
                    let mut projected = l.clone();
                    projected.batch_id = Hash64::default();
                    projected.leaf_hash()
                })
                .collect();
            let mut m = PalwBatchManifestV1 {
                version: 1,
                batch_id: h(1),
                registration_epoch,
                model_profile_id: h(2),
                runtime_class_id: h(3),
                leaf_count: leaves.len() as u32,
                chunk_count: 1,
                leaf_root: palw_leaf_merkle_root(&hashes),
                descriptor_root: h(5),
                total_leaf_bond_sompi: 0,
                audit_policy_id: h(6),
                activation_not_before_epoch: registration_epoch + 3,
                expiry_epoch: registration_epoch + 9,
            };
            m.batch_id = m.content_id();
            for leaf in leaves.iter_mut() {
                leaf.batch_id = m.batch_id;
            }
            let proofs = leaves.iter().map(|l| palw_leaf_merkle_proof(&hashes, l.leaf_index).expect("index in range")).collect();
            let chunk = PalwLeafChunkV1 {
                version: PALW_LEAF_CHUNK_VERSION_V3,
                batch_id: m.batch_id,
                chunk_index: 0,
                leaves,
                proofs,
                witnesses,
            };
            (m, chunk)
        }

        /// Admit a manifest+chunk pair against a staged context and return the chunk's outcome.
        fn admit(
            store: &DbPalwStore,
            beacon: &DbPalwBeaconStore,
            pcpb: &PalwPcpbAcceptanceCtx<'_>,
            m: PalwBatchManifestV1,
            chunk: PalwLeafChunkV1,
        ) -> Result<(), PalwOverlayError> {
            apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), store, beacon, None, None).unwrap();
            apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), store, beacon, None, Some(pcpb))
        }

        /// §8 "grind 系 / sentinel 拒否" — clause 11 at acceptance. `issued_epoch` cannot be
        /// declared freely (the challenge must RE-DERIVE under `R_issued`), the freshness window is
        /// enforced at its exact boundaries, and the Object-V1 zero sentinel is structurally
        /// underivable.
        #[test]
        fn pcpb_clause11_acceptance_rejects_free_epochs_and_sentinels() {
            let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
            let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
            let pcpb_store = pcpb_fixture().staged_store(&db, &beacon);
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            // Seeds for the epochs the negatives re-derive under (0 and 1 are not staged by the
            // fixture; without them the free-declaration cases would fail as unresolvable-context
            // instead of as the challenge mismatch actually under test).
            let mut batch = rocksdb::WriteBatch::default();
            beacon.set_beacon_seed_batch(&mut batch, 0, h(0xA0)).unwrap();
            beacon.set_beacon_seed_batch(&mut batch, 1, h(0xA1)).unwrap();
            db.write(batch).unwrap();
            let wits = |n: u32| (0..n).map(|i| pcpb_fixture().witness(i)).collect::<Vec<_>>();

            // Baseline: the untampered leaves in their own self-consistent batch ACCEPT — and the
            // fixture geometry (issued 2, Δ 2, registered 4) sits exactly on the `anchor+Δ ==
            // registered` boundary, so the boundary's accept side is pinned here.
            let (m, chunk) = adversarial_batch(vec![leaf_raw(0), leaf_raw(1)], wits(2));
            admit(&store, &beacon, &pctx, m, chunk).expect("the self-consistent baseline batch must accept");

            // (a) Free-declared issued epoch: the leaf claims issued=1 while its challenge was
            // derived at 2. R_1 exists, the freshness window admits it — but the challenge does not
            // re-derive, which is the H-10 replay hole clause 11 exists to close.
            let mut l = leaf_raw(0);
            l.receipt_v3_issued_epoch = 1;
            let (m, chunk) = adversarial_batch(vec![l], wits(1));
            assert_eq!(admit(&store, &beacon, &pctx, m, chunk), Err(PalwOverlayError::LeafChallengeMismatch { leaf_index: 0 }));

            // (b) A tampered challenge value under honest epochs.
            let mut l = leaf_raw(0);
            l.receipt_v3_job_challenge = h(0xFF);
            let (m, chunk) = adversarial_batch(vec![l], wits(1));
            assert_eq!(admit(&store, &beacon, &pctx, m, chunk), Err(PalwOverlayError::LeafChallengeMismatch { leaf_index: 0 }));

            // (c) §8 "sentinel 拒否": the Object-V1 shape (zero challenge, zero issued epoch) is
            // structurally underivable — no beacon seed hashes the witness triple to zero.
            let mut l = leaf_raw(0);
            l.receipt_v3_job_challenge = Hash64::default();
            l.receipt_v3_issued_epoch = 0;
            let (m, chunk) = adversarial_batch(vec![l], wits(1));
            assert_eq!(admit(&store, &beacon, &pctx, m, chunk), Err(PalwOverlayError::LeafChallengeMismatch { leaf_index: 0 }));

            // (d) Freshness `anchor + Δ ≤ registered`, violated by one epoch: issued 3 ⇒ 3+2 > 4.
            let mut l = leaf_raw(0);
            l.receipt_v3_issued_epoch = 3;
            let (m, chunk) = adversarial_batch(vec![l], wits(1));
            assert_eq!(
                admit(&store, &beacon, &pctx, m, chunk),
                Err(PalwOverlayError::LeafChallengeStale { leaf_index: 0, issued_epoch: 3, anchor_epoch: 3, registered_epoch: 4 })
            );

            // (e) Freshness `registered − issued ≤ w` at w EXACTLY (6): registered 8, issued 2 —
            // accepted, against the same staged snapshot/draw context.
            let mut l0 = leaf_raw(0);
            l0.registered_epoch = 8;
            let mut l1 = leaf_raw(1);
            l1.registered_epoch = 8;
            let (m, chunk) = adversarial_batch(vec![l0, l1], wits(2));
            admit(&store, &beacon, &pctx, m, chunk).expect("registered − issued == w exactly is inside the window");

            // (f) ...and w+1 is not.
            let mut l = leaf_raw(0);
            l.registered_epoch = 9;
            let (m, chunk) = adversarial_batch(vec![l], wits(1));
            assert_eq!(
                admit(&store, &beacon, &pctx, m, chunk),
                Err(PalwOverlayError::LeafChallengeStale { leaf_index: 0, issued_epoch: 2, anchor_epoch: 2, registered_epoch: 9 })
            );
        }

        /// §8 "窓外 fail-closed" — every unresolvable clause-11/12 context read is a REJECTION,
        /// never a waiver: no context at all, an unretained issued-epoch seed, an unretained
        /// snapshot, an unclosed/unretained draw beacon, and an anchor below the snapshot lag.
        #[test]
        fn pcpb_clause12_acceptance_fails_closed_on_unresolvable_context() {
            let fixture = pcpb_fixture();

            // (a) No PCPB context: the WHOLE chunk is refused (verification that cannot run is a
            // refusal, not a waiver).
            let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
            let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
            fixture.staged_store(&db, &beacon);
            let (m, chunk) = adversarial_batch(vec![leaf_raw(0)], vec![fixture.witness(0)]);
            apply_palw_overlay_effect(PalwOverlayEffect::Manifest(m), &store, &beacon, None, None).unwrap();
            assert_eq!(
                apply_palw_overlay_effect(PalwOverlayEffect::LeafChunk(chunk), &store, &beacon, None, None),
                Err(PalwOverlayError::LeafPcpbContextUnresolvable {
                    leaf_index: 0,
                    epoch: fixture.issued_epoch,
                    what: "acceptance ran without a PCPB context"
                })
            );

            // Selective staging for the remaining cases: each env stages everything EXCEPT the one
            // read whose absence is under test.
            let stage_env = |issued_seed: bool, snapshot: bool, draw_seed: bool| {
                let (lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
                let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
                let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
                let pcpb_store = DbPalwPcpbStore::new(db.clone(), kaspa_database::prelude::CachePolicy::Count(64));
                let mut batch = rocksdb::WriteBatch::default();
                if issued_seed {
                    beacon.set_beacon_seed_batch(&mut batch, fixture.issued_epoch, fixture.issued_seed).unwrap();
                }
                if snapshot {
                    pcpb_store.set_snapshot_batch(&mut batch, fixture.snapshot_epoch, fixture.witnesses.commitment).unwrap();
                }
                if draw_seed {
                    beacon.set_beacon_seed_batch(&mut batch, fixture.draw_epoch, fixture.draw_seed).unwrap();
                }
                db.write(batch).unwrap();
                (lt, db, store, beacon, pcpb_store)
            };

            // (b) Issued-epoch seed unretained ⇒ clause 11's re-derivation cannot run.
            let (_lt1, _db1, store, beacon, pcpb_store) = stage_env(false, true, true);
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let (m, chunk) = adversarial_batch(vec![leaf_raw(0)], vec![fixture.witness(0)]);
            assert_eq!(
                admit(&store, &beacon, &pctx, m, chunk),
                Err(PalwOverlayError::LeafPcpbContextUnresolvable {
                    leaf_index: 0,
                    epoch: fixture.issued_epoch,
                    what: "issued-epoch beacon seed"
                })
            );

            // (c) Provider snapshot unretained at anchor − k.
            let (_lt2, _db2, store, beacon, pcpb_store) = stage_env(true, false, true);
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let (m, chunk) = adversarial_batch(vec![leaf_raw(0)], vec![fixture.witness(0)]);
            assert_eq!(
                admit(&store, &beacon, &pctx, m, chunk),
                Err(PalwOverlayError::LeafPcpbContextUnresolvable {
                    leaf_index: 0,
                    epoch: fixture.snapshot_epoch,
                    what: "provider snapshot"
                })
            );

            // (d) Post-commit draw beacon unclosed / unretained at anchor + Δ.
            let (_lt3, _db3, store, beacon, pcpb_store) = stage_env(true, true, false);
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let (m, chunk) = adversarial_batch(vec![leaf_raw(0)], vec![fixture.witness(0)]);
            assert_eq!(
                admit(&store, &beacon, &pctx, m, chunk),
                Err(PalwOverlayError::LeafPcpbContextUnresolvable {
                    leaf_index: 0,
                    epoch: fixture.draw_epoch,
                    what: "post-commit beacon seed"
                })
            );

            // (e) An anchor below the snapshot lag (`anchor − k` underflows): issued 1 with k = 2.
            // The challenge is re-derived at epoch 1 so clause 11 passes and the underflow is the
            // first failing read — the genesis boundary condition of design memo §10.2.
            let (_lt4, db4, store, beacon, pcpb_store) = stage_env(true, true, true);
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let seed1 = h(0xA1);
            let mut batch = rocksdb::WriteBatch::default();
            beacon.set_beacon_seed_batch(&mut batch, 1, seed1).unwrap();
            db4.write(batch).unwrap();
            let w0 = fixture.witness(0);
            let mut l = leaf_raw(0);
            l.receipt_v3_issued_epoch = 1;
            l.registered_epoch = 3;
            l.receipt_v3_job_challenge = palw_job_challenge(
                NETWORK_ID,
                1,
                &seed1,
                &w0.scheduler_job_id,
                &w0.requester_credential,
                &w0.request_commitment,
                l.shape_id,
            );
            l.job_nullifier = l.receipt_v3_job_challenge;
            let (m, chunk) = adversarial_batch(vec![l], vec![w0]);
            assert_eq!(
                admit(&store, &beacon, &pctx, m, chunk),
                Err(PalwOverlayError::LeafPcpbContextUnresolvable {
                    leaf_index: 0,
                    epoch: 1,
                    what: "anchor predates the snapshot lag"
                })
            );
        }

        /// §8 "provider 束縛" (external) — the leaf's DECLARED seats are bound to the drawn
        /// providers at acceptance: naming any other bonded provider in either seat is refused even
        /// though the witness itself is honest.
        #[test]
        fn pcpb_clause12_acceptance_binds_declared_seats_external() {
            let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
            let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
            let fixture = pcpb_fixture();
            let pcpb_store = fixture.staged_store(&db, &beacon);
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let drawn_a = fixture.witnesses.slots[fixture.slot_a].entry.provider_id;
            let drawn_b = fixture.witnesses.slots[fixture.slot_b].entry.provider_id;
            let other =
                |not: [Hash64; 2]| *fixture.bonds.iter().find(|o| !not.contains(&palw_provider_id(o))).expect("4 providers, 2 drawn");

            // Seat A swapped to an undrawn (but bonded, real) provider.
            let mut l = leaf_raw(0);
            l.provider_a_bond = other([drawn_a, drawn_b]);
            let (m, chunk) = adversarial_batch(vec![l], vec![fixture.witness(0)]);
            assert_eq!(admit(&store, &beacon, &pctx, m, chunk), Err(PalwOverlayError::LeafDispatchEvidenceInvalid { leaf_index: 0 }));

            // Seat B swapped likewise.
            let mut l = leaf_raw(0);
            l.provider_b_bond = other([drawn_a, drawn_b]);
            let (m, chunk) = adversarial_batch(vec![l], vec![fixture.witness(0)]);
            assert_eq!(admit(&store, &beacon, &pctx, m, chunk), Err(PalwOverlayError::LeafDispatchEvidenceInvalid { leaf_index: 0 }));
        }

        // ---- self-serial world -----------------------------------------------------------------

        /// Self-branch geometry inside the shared windows (W 6, K 2, Δ 2): anchor (= a_commit
        /// epoch) 5, issued 5, registered 8, snapshot at 3, draw at 7.
        const SELF_ANCHOR: u64 = 5;
        const SELF_REGISTERED: u64 = 8;

        struct SelfFix {
            bonds: Vec<TransactionOutpoint>,
            keys: Vec<mldsa::MLDSA87KeyPair>,
            ws: PalwSnapshotWitnessSet,
            issued_seed: Hash64,
            draw_seed: Hash64,
        }

        fn self_fix() -> SelfFix {
            let mut keys = Vec::new();
            let mut bonds = Vec::new();
            let mut entries = Vec::new();
            for i in 0..4u8 {
                let kp = mldsa::generate_key_pair([0x40 + i; 32]);
                let outpoint = TransactionOutpoint::new(h(0xB0 + i), i as u32);
                entries.push(PalwProviderSnapshotEntry {
                    provider_id: palw_provider_id(&outpoint),
                    ml_dsa_pk_hash: palw_provider_pk_hash(kp.verification_key.as_ref()),
                    bond_sompi: 250,
                    reward_script_commitment: h(0xD0 + i),
                });
                bonds.push(outpoint);
                keys.push(kp);
            }
            SelfFix { bonds, keys, ws: palw_build_snapshot_witnesses(&entries), issued_seed: h(0x21), draw_seed: h(0x22) }
        }

        impl SelfFix {
            /// A leaf + witness pair for `a_commit`, honest under this fixture's staged world.
            fn leaf_and_witness(&self, a_commit: Hash64) -> (PalwPublicLeafV1, PalwLeafPcpbWitnessV1) {
                let b_slot = self.ws.select(&palw_pcpb_derive_b(&self.draw_seed, &a_commit)).expect("non-empty snapshot");
                let b_id = self.ws.slots[b_slot].entry.provider_id;
                let b_key = self.bonds.iter().position(|o| palw_provider_id(o) == b_id).unwrap();
                let a_slot = (0..self.ws.slots.len()).find(|&i| i != b_slot).unwrap();
                let a_id = self.ws.slots[a_slot].entry.provider_id;
                let a_bond = *self.bonds.iter().find(|o| palw_provider_id(o) == a_id).unwrap();
                let preimage = palw_pcpb_receipt_preimage(&a_commit, b"self-acceptance-tail");
                let sig = mldsa::sign(&self.keys[b_key].signing_key, &preimage, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x77; 32])
                    .expect("sign");
                let (job, cred, req) = (h(0xE5), h(0xE6), h(0xE7));
                let mut leaf = leaf_raw(0);
                leaf.registered_epoch = SELF_REGISTERED;
                leaf.dispatch_kind = PALW_DISPATCH_KIND_SELF_SERIAL;
                leaf.a_commit = a_commit;
                leaf.a_commit_epoch = SELF_ANCHOR;
                leaf.provider_a_bond = a_bond;
                leaf.provider_b_bond = self.bonds[b_key];
                leaf.provider_snapshot_root = self.ws.commitment.snapshot_root;
                leaf.assignment_proof_root = self.ws.commitment.assignment_root;
                leaf.receipt_v3_issued_epoch = SELF_ANCHOR;
                leaf.receipt_v3_job_challenge =
                    palw_job_challenge(NETWORK_ID, SELF_ANCHOR, &self.issued_seed, &job, &cred, &req, leaf.shape_id);
                leaf.job_nullifier = leaf.receipt_v3_job_challenge;
                let a = &self.ws.slots[a_slot];
                let b = &self.ws.slots[b_slot];
                let witness = PalwLeafPcpbWitnessV1 {
                    scheduler_job_id: job,
                    requester_credential: cred,
                    request_commitment: req,
                    dispatch: PalwDispatchEvidence::SelfSerial(SelfSerialProof {
                        a_commit,
                        a_entry: a.entry.clone(),
                        a_snapshot_membership: a.snapshot_membership.clone(),
                        b_entry: b.entry.clone(),
                        b_snapshot_membership: b.snapshot_membership.clone(),
                        b_interval: b.interval.clone(),
                        b_assignment_membership: b.assignment_membership.clone(),
                        b_ml_dsa_pk: self.keys[b_key].verification_key.as_ref().to_vec(),
                        b_receipt_preimage: preimage,
                        b_signature: sig.as_ref().to_vec(),
                    }),
                };
                (leaf, witness)
            }

            /// Stage seeds + snapshot (+ optionally an A-commit registry row) into fresh stores.
            fn stage(
                &self,
                db: &std::sync::Arc<kaspa_database::prelude::DB>,
                beacon: &DbPalwBeaconStore,
                pcpb: &DbPalwPcpbStore,
                acommit_row: Option<(Hash64, u64)>,
            ) {
                let mut batch = rocksdb::WriteBatch::default();
                beacon.set_beacon_seed_batch(&mut batch, SELF_ANCHOR, self.issued_seed).unwrap();
                beacon.set_beacon_seed_batch(&mut batch, SELF_ANCHOR + DELTA, self.draw_seed).unwrap();
                pcpb.set_snapshot_batch(&mut batch, SELF_ANCHOR - K, self.ws.commitment).unwrap();
                if let Some((anchor, epoch)) = acommit_row {
                    pcpb.set_acommit_batch(&mut batch, &anchor, epoch).unwrap();
                }
                db.write(batch).unwrap();
            }
        }

        fn self_env(
            fix: &SelfFix,
            acommit_row: Option<(Hash64, u64)>,
        ) -> (kaspa_database::utils::DbLifetime, std::sync::Arc<kaspa_database::prelude::DB>, DbPalwStore, DbPalwBeaconStore, DbPalwPcpbStore)
        {
            let (lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let store = DbPalwStore::new(db.clone(), CachePolicy::Count(64));
            let beacon = DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(64));
            let pcpb_store = DbPalwPcpbStore::new(db.clone(), kaspa_database::prelude::CachePolicy::Count(64));
            fix.stage(&db, &beacon, &pcpb_store, acommit_row);
            (lt, db, store, beacon, pcpb_store)
        }

        /// §8 "a_commit 事後登録" — the self arm's on-chain ordering anchor at acceptance, in both
        /// directions of the `row ≤ declared` rule, plus the absent-row fail-closed case, plus the
        /// self-branch seat bindings (drawn-B substitution and the unbonded A).
        #[test]
        fn pcpb_clause12_acceptance_selfserial_registry_and_seats() {
            let fix = self_fix();
            let a_commit = h(0xAC);

            // Honest, row == declared: accepted end-to-end (the baseline every negative perturbs).
            let (leaf, witness) = fix.leaf_and_witness(a_commit);
            let (_lt, _db, store, beacon, pcpb_store) = self_env(&fix, Some((a_commit, SELF_ANCHOR)));
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let (m, chunk) = adversarial_batch(vec![leaf.clone()], vec![witness.clone()]);
            admit(&store, &beacon, &pctx, m, chunk).expect("the honest self-serial batch must accept");

            // (a) NO registry row: the anchor was never on-chain — refused, fail-closed.
            let (_lt, _db, store, beacon, pcpb_store) = self_env(&fix, None);
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let (m, chunk) = adversarial_batch(vec![leaf.clone()], vec![witness.clone()]);
            assert_eq!(
                admit(&store, &beacon, &pctx, m, chunk),
                Err(PalwOverlayError::LeafACommitUnanchored { leaf_index: 0, declared_epoch: SELF_ANCHOR, registry_epoch: None })
            );

            // (b) Registry row LATER than the declared epoch (row 6 > declared 5): the leaf claims
            // its commitment predates a beacon it was actually registered after — the exact "late
            // anchor borrows an already-known beacon" direction. Refused.
            let (_lt, _db, store, beacon, pcpb_store) = self_env(&fix, Some((a_commit, SELF_ANCHOR + 1)));
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let (m, chunk) = adversarial_batch(vec![leaf.clone()], vec![witness.clone()]);
            assert_eq!(
                admit(&store, &beacon, &pctx, m, chunk),
                Err(PalwOverlayError::LeafACommitUnanchored {
                    leaf_index: 0,
                    declared_epoch: SELF_ANCHOR,
                    registry_epoch: Some(SELF_ANCHOR + 1)
                })
            );

            // (c) Registry row EARLIER than the declared epoch (row 3 ≤ declared 5): ACCEPTED —
            // `row ≤ declared` is the deliberate rule (both coordinates carry the argument): the
            // commitment provably predates `R_{declared+Δ}`, so B still post-dates the commit; the
            // residual freedom (which epoch's pair to condition on) is the same w-bounded choice
            // the external branch's issued-epoch already has, and equality would only re-price it
            // through multiple cheap anchors instead of removing it.
            let (_lt, _db, store, beacon, pcpb_store) = self_env(&fix, Some((a_commit, SELF_ANCHOR - 2)));
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let (m, chunk) = adversarial_batch(vec![leaf.clone()], vec![witness.clone()]);
            admit(&store, &beacon, &pctx, m, chunk).expect("row ≤ declared must accept (the documented ≤ rule)");

            // (d) Declared seat B ≠ drawn B (another bonded provider): refused by the seat binding.
            let (_lt, _db, store, beacon, pcpb_store) = self_env(&fix, Some((a_commit, SELF_ANCHOR)));
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let drawn_b = palw_provider_id(&leaf.provider_b_bond);
            let a_id = palw_provider_id(&leaf.provider_a_bond);
            let other = *fix.bonds.iter().find(|o| ![drawn_b, a_id].contains(&palw_provider_id(o))).unwrap();
            let mut swapped = leaf.clone();
            swapped.provider_b_bond = other;
            let (m, chunk) = adversarial_batch(vec![swapped], vec![witness.clone()]);
            assert_eq!(admit(&store, &beacon, &pctx, m, chunk), Err(PalwOverlayError::LeafDispatchEvidenceInvalid { leaf_index: 0 }));

            // (e) Unbonded A self-order: seat A names a bond outside the snapshot and the witness
            // fabricates a matching entry — membership cannot verify, so clause 12 refuses. Without
            // this, an unbonded A mints with only B's bond at stake.
            let (_lt, _db, store, beacon, pcpb_store) = self_env(&fix, Some((a_commit, SELF_ANCHOR)));
            let pctx = pcpb_test_support::ctx(&pcpb_store);
            let outsider = TransactionOutpoint::new(h(0xEE), 9);
            let mut unbonded = leaf.clone();
            unbonded.provider_a_bond = outsider;
            let mut forged = witness.clone();
            if let PalwDispatchEvidence::SelfSerial(p) = &mut forged.dispatch {
                p.a_entry = PalwProviderSnapshotEntry {
                    provider_id: palw_provider_id(&outsider),
                    ml_dsa_pk_hash: h(0xEF),
                    bond_sompi: 250,
                    reward_script_commitment: h(0xF0),
                };
            }
            let (m, chunk) = adversarial_batch(vec![unbonded], vec![forged]);
            assert_eq!(admit(&store, &beacon, &pctx, m, chunk), Err(PalwOverlayError::LeafDispatchEvidenceInvalid { leaf_index: 0 }));
        }

        /// The staged-store constants this module leans on really are the shared ones (a drift in
        /// `pcpb_test_support` would silently re-scope every window boundary above).
        #[test]
        fn pcpb_negative_battery_window_constants_are_the_shared_ones() {
            assert_eq!((W, K, DELTA), (6, 2, 2), "window constants moved — re-derive every boundary case in this module");
            let f = pcpb_fixture();
            assert_eq!(f.registered_epoch, FIXTURE_REGISTRATION_EPOCH);
            assert_eq!(f.issued_epoch, FIXTURE_REGISTRATION_EPOCH - DELTA);
            assert_eq!(f.snapshot_epoch, f.issued_epoch - K);
            assert_eq!(f.draw_epoch, f.issued_epoch + DELTA);
        }
    }
}

/// ADR-0045 D3-b — shared PCPB test scaffolding: build a leaf/witness/context triple that the real
/// clause-11/12 arm ACCEPTS, so tests whose subject is something else (store re-announce, lifecycle
/// e2e) can keep exercising their own property, and clause tests can perturb one field at a time
/// from a known-good baseline.
///
/// Everything here is derived, never asserted: the challenge is re-derived with the same
/// `palw_job_challenge` the verifier calls, and the leaf's declared provider bonds are set to the
/// providers the beacon draw actually selects. A change that breaks producer/verifier agreement
/// therefore breaks these fixtures loudly instead of silently pinning a stale expectation.
#[cfg(test)]
pub(crate) mod pcpb_test_support {
    use kaspa_consensus_core::palw::{
        BeaconAssignedProof, PALW_DISPATCH_KIND_BEACON_ASSIGNED, PalwDispatchEvidence, PalwLeafPcpbWitnessV1,
        PalwProviderSnapshotEntry, PalwPublicLeafV1, PalwSnapshotWitnessSet, palw_assignment_draw_seed, palw_build_snapshot_witnesses,
        palw_job_challenge, palw_provider_id,
    };
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_hashes::Hash64;

    pub(crate) const NETWORK_ID: u32 = 110;
    pub(crate) const W: u64 = 6;
    pub(crate) const K: u64 = 2;
    pub(crate) const DELTA: u64 = 2;

    fn th(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    /// A bonded provider set whose ids come from real bond outpoints — the same derivation
    /// (`palw_provider_id`) the acceptance arm applies to `leaf.provider_{a,b}_bond`.
    pub(crate) struct PcpbFixture {
        pub bonds: Vec<TransactionOutpoint>,
        pub witnesses: PalwSnapshotWitnessSet,
        /// `(issued_epoch, seed)` and `(draw_epoch, seed)` rows the beacon store must hold.
        pub issued_epoch: u64,
        pub issued_seed: Hash64,
        pub draw_epoch: u64,
        pub draw_seed: Hash64,
        pub snapshot_epoch: u64,
        pub registered_epoch: u64,
        /// The providers the draw selected, in seat order.
        pub slot_a: usize,
        pub slot_b: usize,
    }

    /// The acceptance context a staged store implies, with this module's window constants.
    pub(crate) fn ctx(
        store: &crate::model::stores::palw_pcpb::DbPalwPcpbStore,
    ) -> super::PalwPcpbAcceptanceCtx<'_> {
        super::PalwPcpbAcceptanceCtx {
            pcpb_store: store,
            network_id: NETWORK_ID,
            freshness_window_epochs: W,
            snapshot_lag_epochs: K,
            post_commit_delta_epochs: DELTA,
        }
    }

    /// Build a fixture anchored at `registered_epoch` whose external-branch draw picks two distinct
    /// providers (searching the draw beacon for one, exactly as an honest scheduler would wait for
    /// the epoch whose beacon assigns it a usable pair).
    pub(crate) fn fixture(registered_epoch: u64) -> PcpbFixture {
        let issued_epoch = registered_epoch - DELTA;
        let snapshot_epoch = issued_epoch - K;
        let draw_epoch = issued_epoch + DELTA;
        let bonds: Vec<TransactionOutpoint> = (0..4u8).map(|i| TransactionOutpoint::new(th(0xB0 + i), i as u32)).collect();
        let entries: Vec<PalwProviderSnapshotEntry> = bonds
            .iter()
            .enumerate()
            .map(|(i, outpoint)| PalwProviderSnapshotEntry {
                provider_id: palw_provider_id(outpoint),
                ml_dsa_pk_hash: th(0xC0 + i as u8),
                bond_sompi: 250,
                reward_script_commitment: th(0xD0 + i as u8),
            })
            .collect();
        let witnesses = palw_build_snapshot_witnesses(&entries);
        let (mut draw_seed, mut slot_a, mut slot_b) = (th(0x40), 0usize, 0usize);
        for k in 0..64u8 {
            draw_seed = th(0x40u8.wrapping_add(k));
            let (a, b) = (
                witnesses.select(&palw_assignment_draw_seed(&draw_seed, 0)),
                witnesses.select(&palw_assignment_draw_seed(&draw_seed, 1)),
            );
            if let (Some(a), Some(b)) = (a, b)
                && a != b
            {
                (slot_a, slot_b) = (a, b);
                break;
            }
        }
        assert_ne!(slot_a, slot_b, "fixture: no beacon drew two distinct providers");
        PcpbFixture {
            bonds,
            witnesses,
            issued_epoch,
            issued_seed: th(0x11),
            draw_epoch,
            draw_seed,
            snapshot_epoch,
            registered_epoch,
            slot_a,
            slot_b,
        }
    }

    impl PcpbFixture {
        /// The bond outpoint of a drawn slot — what the leaf must declare in that seat.
        fn bond_of(&self, slot: usize) -> TransactionOutpoint {
            let id = self.witnesses.slots[slot].entry.provider_id;
            *self.bonds.iter().find(|o| palw_provider_id(o) == id).expect("every snapshot id came from a bond")
        }

        /// The per-leaf challenge preimage triple. `scheduler_job_id` varies with the leaf index so a
        /// batch's leaves carry DISTINCT challenges (and therefore distinct `job_nullifier`s), which is
        /// what an honest scheduler produces — one lease per job, not one per epoch.
        fn preimage(leaf_index: u32) -> (Hash64, Hash64, Hash64) {
            let mut job = [0u8; 64];
            job[0] = 0xE1;
            job[1..5].copy_from_slice(&leaf_index.to_le_bytes());
            (Hash64::from_bytes(job), th(0xE2), th(0xE3))
        }

        /// Stamp a leaf into the PCPB world this fixture describes: the drawn provider seats, the
        /// committed roots, the anchor epochs, and a challenge that RE-DERIVES under `R_{issued}`.
        ///
        /// It also switches the leaf to receipt DA **Object V2**, because D3-b makes that mandatory:
        /// clause 11 re-derives `receipt_v3_job_challenge`, and `validate_public_leaf`'s Object-V1 arm
        /// requires that very field to be a zero sentinel — the two cannot both hold, so a V1 leaf can
        /// no longer be stored (design memo §10.1).
        pub(crate) fn stamp(&self, leaf: &mut PalwPublicLeafV1) {
            leaf.provider_a_bond = self.bond_of(self.slot_a);
            leaf.provider_b_bond = self.bond_of(self.slot_b);
            leaf.dispatch_kind = PALW_DISPATCH_KIND_BEACON_ASSIGNED;
            leaf.a_commit = Hash64::default();
            leaf.a_commit_epoch = 0;
            leaf.provider_snapshot_root = self.witnesses.commitment.snapshot_root;
            leaf.assignment_proof_root = self.witnesses.commitment.assignment_root;
            leaf.registered_epoch = self.registered_epoch;
            leaf.receipt_da_object_version = kaspa_consensus_core::palw::da::PALW_RECEIPT_DA_OBJECT_VERSION_V2;
            leaf.receipt_v3_compute_set_id = th(0xC5);
            leaf.receipt_v3_issued_epoch = self.issued_epoch;
            leaf.receipt_v3_expires_epoch = leaf.expiry_epoch;
            let (job, cred, req) = Self::preimage(leaf.leaf_index);
            let challenge =
                palw_job_challenge(NETWORK_ID, self.issued_epoch, &self.issued_seed, &job, &cred, &req, leaf.shape_id);
            leaf.receipt_v3_job_challenge = challenge;
            // Object-V2 requires the leaf's job nullifier to BE its challenge.
            leaf.job_nullifier = challenge;
        }

        /// The witness that opens the leaf stamped at `leaf_index`.
        pub(crate) fn witness(&self, leaf_index: u32) -> PalwLeafPcpbWitnessV1 {
            let (scheduler_job_id, requester_credential, request_commitment) = Self::preimage(leaf_index);
            PalwLeafPcpbWitnessV1 {
                scheduler_job_id,
                requester_credential,
                request_commitment,
                dispatch: PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof {
                    slot_a: self.witnesses.slots[self.slot_a].clone(),
                    slot_b: self.witnesses.slots[self.slot_b].clone(),
                }),
            }
        }

        /// Build a PCPB store on `db` and stage this fixture's context into it — the one-call setup
        /// every acceptance test needs before driving the real arm.
        pub(crate) fn staged_store(
            &self,
            db: &std::sync::Arc<kaspa_database::prelude::DB>,
            beacon: &crate::model::stores::palw_beacon::DbPalwBeaconStore,
        ) -> crate::model::stores::palw_pcpb::DbPalwPcpbStore {
            let store = crate::model::stores::palw_pcpb::DbPalwPcpbStore::new(
                db.clone(),
                kaspa_database::prelude::CachePolicy::Count(64),
            );
            self.stage(db, beacon, &store);
            store
        }

        /// Stage the beacon seeds + provider snapshot this fixture's leaves resolve against.
        pub(crate) fn stage(
            &self,
            db: &std::sync::Arc<kaspa_database::prelude::DB>,
            beacon: &crate::model::stores::palw_beacon::DbPalwBeaconStore,
            pcpb: &crate::model::stores::palw_pcpb::DbPalwPcpbStore,
        ) {
            let mut batch = rocksdb::WriteBatch::default();
            beacon.set_beacon_seed_batch(&mut batch, self.issued_epoch, self.issued_seed).unwrap();
            beacon.set_beacon_seed_batch(&mut batch, self.draw_epoch, self.draw_seed).unwrap();
            pcpb.set_snapshot_batch(&mut batch, self.snapshot_epoch, self.witnesses.commitment).unwrap();
            db.write(batch).unwrap();
        }
    }
}
