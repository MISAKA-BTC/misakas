//! ADR-0039 PALW Replica-GEMM audited-compute lane — on-chain wire format.
//!
//! This module freezes the **consensus-facing** PALW types (design v0.2 §9, §10, §11, §12, §24):
//! the public leaf / manifest / chunk / certificate / provider-bond / beacon / authorization /
//! revocation payloads carried on the PALW overlay subnetworks (`0x30-0x37`, see
//! [`crate::subnets`]), plus the security-critical domain-separated hash helpers
//! (`leaf_hash`, `chain_commit`, `eligibility_hash`, `beacon_seed`,
//! `private_match_commitment`) and the network [`PalwParams`]. (`slot_digest` was removed by ADR-0040
//! TGT-02; its retired keyed domain is pinned as reserved by `retired_slot_domain_is_never_reused`.)
//!
//! Everything here is **inert** until the PALW activation fence — no header is minted on the PALW
//! lane, and no live validator path consumes these types yet. The point of landing them first is
//! design §33: *freeze the wire format and test vectors before any hot-path / GPU code*. The
//! provider-side, off-chain runtime identity (the two `MISAKA-QW4/QW9-PALW-v1` tiers, GEMM-trace and
//! output commitments) lives in `misaka-mil-core`'s `palw` module.
//!
//! All multi-field preimages are unambiguous: fixed-width fields (`Hash64` = 64 B, integers as
//! little-endian) are concatenated directly; variable-length fields are `u64`-LE length-prefixed.
//! Every hash is a keyed BLAKE2b-512 (`blake2b_512_keyed`) under a disjoint `misaka-palw-v1/*`
//! domain so no PALW hash can be replayed as any other hash in the system.

/// DA-01 receipt-object, chunk-proof, sampling, and challenge lifecycle primitives.
/// Kept as a child module so the new wire/state machine can evolve independently of
/// the already-large PALW header and batch implementation.
pub mod da;
pub mod search_snapshot;

use crate::BlueWorkType;
use crate::dns_finality::{STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN, p2pkh_mldsa87_spk, validator_id_from_pubkey};
use crate::pow_layer0::{POW_ALGO_ID_BLAKE2B_SHA3, POW_ALGO_ID_PALW_REPLICA, WorkLane};
use crate::tx::{ScriptPublicKey, Transaction, TransactionOutpoint, TransactionOutput};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{HASH64_SIZE, Hash64, blake2b_512_address_payload, blake2b_512_keyed};
use kaspa_math::Uint512;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::{self, Display, Formatter};

// =============================================================================================
// Domain separators (keyed BLAKE2b-512 keys, ≤ 64 bytes — pinned in tests).
// =============================================================================================

/// `leaf_hash = Hash64_k(leaf, borsh(PalwPublicLeafV1))` — the on-chain leaf descriptor hash
/// committed before the beacon (design §9.2, invariant I-2).
pub const PALW_LEAF_DOMAIN: &[u8] = b"misaka-palw-v1/leaf";
/// `chain_commit(S)` fork-binding digest (design §12.1, invariant I-4).
pub const PALW_CHAIN_COMMIT_DOMAIN: &[u8] = b"misaka-palw-chain-commit-v1";
/// kaspa-pq ADR-0040 **TGT-02/TGT-03 — the retired `PALW_SLOT_DOMAIN`, kept as a tombstone.**
///
/// `slot_digest` and its domain were deleted (zero production callers; see the deletion note above
/// `chain_commit`). This string is RESERVED: it must never be reused for a different derivation, or a
/// future keyed hash could collide with digests produced by the removed one.
///
/// It is a `const` rather than a comment so the reservation is ENFORCEABLE — a comment cannot fail a
/// build. `retired_slot_domain_is_never_reused` asserts no live domain equals it.
pub const PALW_RETIRED_SLOT_DOMAIN: &[u8] = b"misaka-palw-slot-v1";
/// `eligibility_hash` one-shot draw (design §12.3).
pub const PALW_ELIGIBILITY_DOMAIN: &[u8] = b"misaka-palw-eligibility-v1";
/// ADR-0040 P1-6 — keyed-hash domain for the block-authorization preimage, signing hash and authority
/// key hash. Separate from the eligibility and leaf domains so the three can never be confused.
pub const PALW_AUTHORIZATION_DOMAIN: &[u8] = b"misaka-palw-authorization-v1";
/// ADR-0040 ECON-03 (leg 5) — keyed-hash domain for the provider-unbond authorization preimage.
pub const PALW_PROVIDER_UNBOND_DOMAIN: &[u8] = b"misaka-palw-provider-unbond-v1";
/// `R_E` epoch beacon seed (design §11.2).
pub const PALW_BEACON_DOMAIN: &[u8] = b"misaka-palw-beacon-v1";
/// `PalwBeaconCommitV1.commitment = Hash64_k(beacon-commit, epoch ‖ random_64 ‖ bond)` (design §11.2).
pub const PALW_BEACON_COMMIT_DOMAIN: &[u8] = b"misaka-palw-beacon-commit-v1";
/// `dns_finality_certificate_hash_v1` — clause 6's confirmation-evidence digest over ANCHOR-pure facts
/// (design §12.1; panel-frozen v1 preimage: `anchor_hash ‖ blue ‖ daa ‖ anchor_overlay_root`).
pub const PALW_DNS_CERT_DOMAIN: &[u8] = b"misaka-palw-dns-cert-v1";
/// `PalwAuditorVoteV2` signing message (§10.1, I-14 DA-possession binding). The V2 digest additionally
/// binds the certificate summary (`passed_leaf_count`, `rejected_leaf_bitmap_root`), so an assembler
/// cannot reuse valid votes in a certificate that claims different audit results.
///
/// An auditor's vote signature
/// covers `audit_sample_root`, but the possession property that was MEANT to buy — "a certificate cannot
/// be signed without first fetching the beacon-selected receipt chunks" — DOES NOT hold yet, because
/// ADR-0040 SAMPLE-01 / §5.17: consensus NOW re-derives `audit_sample_root` at
/// `verify_certificate_attestation` — `palw_audit_sample_root` over the beacon-selected on-chain leaves'
/// `receipt_da_root`, and every vote is verified over the RE-DERIVED root, not the declared one. Because
/// the sampled receipt CHUNKS are off-chain DA, this is the §5.17.6 REDEFINITION (a root over on-chain
/// per-leaf DA commitments), not the stronger I-14 possession property, which is unprovable on-chain.
pub const PALW_AUDITOR_VOTE_V2_DOMAIN: &[u8] = b"misaka-palw-auditor-vote-v2";
/// Retired V1 vote domain. Reserved forever so no future signing object can reinterpret old digests.
pub const PALW_RETIRED_AUDITOR_VOTE_V1_DOMAIN: &[u8] = b"misaka-palw-auditor-vote-v1";
/// `ticket_nullifier_commitment = Hash64_k(ticket-nullifier-commit, ticket_nullifier)` (§12.3, I-13) —
/// the leaf publishes only this commitment; the raw `ticket_nullifier` is disclosed at header-use time.
/// A one-way commitment (the 64-byte nullifier is not guessable), so a third party who reads the public
/// leaf CANNOT compute `eligibility_hash` in advance and pre-list the epoch's interval winners.
pub const PALW_TICKET_NULLIFIER_COMMIT_DOMAIN: &[u8] = b"misaka-palw-ticket-nf-commit-v1";
/// `leaf_root = Hash64_k(leaf-root, count ‖ apex)`, the final commitment to the manifest's ordered
/// leaf set (ADR-0040 §5.15.4).
///
/// Prefixing the count makes the commitment order- and count-sensitive while retaining domain
/// separation from other PALW digests. Leaf membership is verified before insertion into the
/// content-addressed store.
pub const PALW_LEAF_ROOT_DOMAIN: &[u8] = b"misaka-palw-leaf-root-v1";
/// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2) — level-0 node of the leaf Merkle tree:
/// `Hash64_k(leaf-merkle-leaf, leaf_index_le32 ‖ leaf_hash)`. Binding the index inside the node is what
/// makes a valid leaf non-replayable at a different index.
pub const PALW_LEAF_MERKLE_LEAF_DOMAIN: &[u8] = b"misaka-palw-leaf-merkle-leaf-v1";
/// kaspa-pq ADR-0040 §5.15 — internal node of the leaf Merkle tree: `Hash64_k(leaf-merkle-node, l ‖ r)`.
/// DISJOINT from [`PALW_LEAF_MERKLE_LEAF_DOMAIN`], so an internal digest can never be passed off as a
/// leaf digest (leaf/internal confusion).
pub const PALW_LEAF_MERKLE_NODE_DOMAIN: &[u8] = b"misaka-palw-leaf-merkle-node-v1";
/// kaspa-pq ADR-0040 §5.15 — the uniform padding constant `H_EMPTY = Hash64_k(leaf-merkle-empty, "")`
/// used to fill level 0 out to `2^d`. Uniform padding (NOT tail duplication) removes the odd-arity
/// second-preimage family and gives every proof exactly `d` siblings.
pub const PALW_LEAF_MERKLE_EMPTY_DOMAIN: &[u8] = b"misaka-palw-leaf-merkle-empty-v1";
/// `batch_id = content_id = Hash64_k(batch-id, borsh(manifest with batch_id zeroed))` (§9.2) — C4
/// content-addressing: `batch_id` must equal the hash of the manifest's OWN content (batch_id excluded,
/// as it is self-referential), so no two forks can register different manifests under one `batch_id`.
pub const PALW_BATCH_ID_DOMAIN: &[u8] = b"misaka-palw-batch-id-v1";

/// ADR-0039 D15 — post-commitment pair-binding partner-B derivation domain.
pub const PALW_PCPB_DOMAIN: &[u8] = b"misaka-palw-pcpb-v1";

/// ADR-0040 §5.14.7 — PCPB verifiable-evidence keyed-BLAKE2b domains (all INERT; pinned by
/// `domain_strings_are_pinned_and_fit_key_limit` + `palw_keyed_domains_are_pairwise_distinct`). See
/// [`palw_dispatch_evidence_valid`].
pub const PALW_SNAPSHOT_LEAF_DOMAIN: &[u8] = b"misaka-palw-pcpb-snapshot-leaf-v1";
pub const PALW_SNAPSHOT_NODE_DOMAIN: &[u8] = b"misaka-palw-pcpb-snapshot-node-v1";
pub const PALW_ASSIGNMENT_LEAF_DOMAIN: &[u8] = b"misaka-palw-pcpb-assign-leaf-v1";
pub const PALW_ASSIGNMENT_NODE_DOMAIN: &[u8] = b"misaka-palw-pcpb-assign-node-v1";
pub const PALW_ASSIGNMENT_DRAW_DOMAIN: &[u8] = b"misaka-palw-pcpb-assign-draw-v1";
pub const PALW_PROVIDER_PK_HASH_DOMAIN: &[u8] = b"misaka-palw-pcpb-provider-pk-v1";
/// ADR-0045 D3-b — the canonical PCPB provider identity `Hash64_k(provider-id, bond_outpoint)`; see
/// [`palw_provider_id`].
pub const PALW_PROVIDER_ID_DOMAIN: &[u8] = b"misaka-palw-pcpb-provider-id-v1";
/// ADR-0045 D3-b clause 11 — the job-scoped challenge derivation domain. DELIBERATELY the byte
/// string the bridge's Seam 1 has used since it landed (`mil/bridge/src/challenge.rs`), promoted
/// here so the VERIFIER owns the derivation and the producer delegates — never two copies that can
/// drift. See [`palw_job_challenge`].
pub const PALW_JOB_CHALLENGE_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/job-challenge";
/// ML-DSA-87 FIPS-204 `ctx` for the PCPB partner-B receipt signature (disjoint from the other PALW ctxs).
pub const PALW_PCPB_RECEIPT_MLDSA87_CONTEXT: &[u8] = b"PALWPcpbReceiptV1";
/// Fixed prefix the signed partner-B receipt preimage must carry, immediately followed by A_commit (64 B).
pub const PALW_PCPB_RECEIPT_TAG: &[u8] = b"misaka-palw-pcpb-receipt-binds-a-commit-v1";
/// ML-DSA signing hash for [`PalwBeaconCommitV1`]. Separate from both the commitment construction
/// and reveal signature domains, so a signature is not reusable across beacon operations.
pub const PALW_BEACON_COMMIT_SIGNING_DOMAIN: &[u8] = b"misaka-palw-beacon-commit-sign-v1";
/// ML-DSA signing hash for [`PalwBeaconRevealV1`].
pub const PALW_BEACON_REVEAL_SIGNING_DOMAIN: &[u8] = b"misaka-palw-beacon-reveal-sign-v1";
/// ML-DSA-87 context used for both beacon operations. Commit/reveal replay separation lives in the
/// distinct signing-hash domains above; this context keeps PALW beacon signatures disjoint from DNS
/// attestations, unbond requests, and transaction-script signatures at the FIPS-204 layer as well.
pub const PALW_BEACON_MLDSA87_CONTEXT: &[u8] = b"PALWBeaconV1";

/// kaspa-pq ADR-0040 P1-3 (CERT-01) — libcrux ML-DSA-87 `ctx` for a batch-certificate auditor vote.
/// Distinct from [`PALW_BEACON_MLDSA87_CONTEXT`] so a beacon-commit signature can never be replayed as
/// an audit vote (and vice versa) even if the two signing preimages ever collide.
pub const PALW_AUDITOR_V2_MLDSA87_CONTEXT: &[u8] = b"PALWAuditorVoteV2";
/// Retired V1 vote signature context. Reserved forever to make the V2 cutover replay-incompatible.
pub const PALW_RETIRED_AUDITOR_V1_MLDSA87_CONTEXT: &[u8] = b"PALWAuditorVoteV1";

/// kaspa-pq ADR-0040 P1-6 — libcrux ML-DSA-87 `ctx` for a per-block ticket authorization. Distinct from
/// the beacon and auditor contexts so an authorization can never be replayed as either.
pub const PALW_AUTHORIZATION_MLDSA87_CONTEXT: &[u8] = b"PALWBlockAuthorizationV1";
/// ADR-0040 ECON-03 (leg 5) — the provider-unbond authorization context. Disjoint from every other
/// row in [`crate::signature_domains::SIGNATURE_DOMAINS`], and in particular from the DNS
/// `UNBOND_REQUEST_CONTEXT`: a DNS stake-bond unbond authorization must not be replayable to release
/// a PALW provider bond, nor the reverse. Follows the surrounding PALW naming convention (see the
/// known-inconsistency note in `signature_domains`).
pub const PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT: &[u8] = b"PALWProviderUnbondV1";
/// Off-chain PALW Receipt v3 ML-DSA-87 context. The wire implementation lives in `misaka-palw`,
/// while this duplicate is the consensus-core signature-domain registry row. Their byte equality is
/// pinned by `consensus/tests/receipt_v3_domain_alignment.rs` without introducing a core → PALW crate
/// dependency (which would invert the existing layering).
pub const PALW_RECEIPT_V3_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-v3/receipt/mldsa87";
/// Off-chain scheduler-signed COMPUTE JobSpec transport envelope (worker wire schema
/// `misaka.palw.testnet-jobspec.v2+scheduler-mldsa87`). Distinct from
/// [`PALW_RECEIPT_V3_MLDSA87_CONTEXT`] so a receipt signature can never be replayed as a job-spec
/// authorization or vice versa, and from the on-chain search assignment/anchor contexts. The
/// signing digest is the runtime's slot-independent framed `testnet-jobspec-signing/v2`
/// keyed-BLAKE2b — a HASH domain, which stays in its own namespace per the
/// `signature_domains` module note. Like Receipt v3 this is an off-chain wire contract, so it
/// follows the slash convention.
pub const PALW_COMPUTE_JOBSPEC_V2_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-v3/jobspec/mldsa87";
/// Digest of an opened reveal's secret entropy. This is deliberately distinct from
/// [`PALW_BEACON_COMMIT_DOMAIN`]: the public commitment is known in `E-2` and therefore MUST NOT be
/// reused as the `R_E` entropy input once the reveal arrives in `E-1`.
pub const PALW_BEACON_REVEAL_ENTROPY_DOMAIN: &[u8] = b"misaka-palw-beacon-reveal-entropy-v1";
/// `valid_reveals_root` — canonical-sorted keyed hash of the `(bond, reveal_entropy_digest)` set
/// whose reveal validly opened its commit this epoch (design §11.2, a `beacon_seed` input).
pub const PALW_BEACON_REVEALS_ROOT_DOMAIN: &[u8] = b"misaka-palw-beacon-reveals-root-v1";
/// `missing_commitments_root` — canonical-sorted keyed hash of the `(bond, commitment)` set that
/// committed but did not validly reveal this epoch (design §11.2, a `beacon_seed` input; the missing
/// set is what a later slice slashes).
pub const PALW_BEACON_MISSING_ROOT_DOMAIN: &[u8] = b"misaka-palw-beacon-missing-root-v1";
/// `private_match_commitment` binding the two provider receipts to the leaf (design §24.2).
pub const PALW_MATCH_DOMAIN: &[u8] = b"misaka-palw-match-v1";
/// `receipt_hash = Hash64_k(replica-receipt, borsh(ReplicaExecutionReceiptV1))` — leaf-committed
/// provider receipt hash (design §24.1).
pub const PALW_RECEIPT_DOMAIN: &[u8] = b"misaka-palw-replica-receipt-v1";
/// Provider-pair selection from the prior-epoch beacon seed (design §8.1).
pub const PALW_PROVIDER_SELECT_DOMAIN: &[u8] = b"misaka-palw-provider-select-v1";
/// Auditor selection weight from the prior-epoch beacon seed (design §10.2).
pub const PALW_AUDITOR_SELECT_DOMAIN: &[u8] = b"misaka-palw-auditor-select-v1";
/// Commitment over a certificate's selected auditor set (design §10.2) — the value a
/// [`PalwBatchCertificateV2::auditor_set_commitment`] carries. See [`auditor_set_commitment`].
pub const PALW_AUDITOR_SET_DOMAIN: &[u8] = b"misaka-palw-auditor-set-v1";
/// R4 anti-griefing (design §24.5) — deterministic per-mismatch escalation draw from the audit beacon.
pub const PALW_MISMATCH_ESCALATE_DOMAIN: &[u8] = b"misaka-palw-mismatch-escalate-v1";
/// SEL-01 (ADR-0040 §5.17.5) — the per-round draw of the credential-aggregated, bond-weighted,
/// non-replacement sampler ([`palw_weighted_sample_without_replacement`]). Distinct from
/// [`PALW_AUDITOR_SELECT_DOMAIN`] (the SUPERSEDED unweighted per-outpoint score) and
/// [`PALW_PROVIDER_SELECT_DOMAIN`] (the SUPERSEDED uniform index) so the weighted draw can never
/// collide with either derivation it replaces.
pub const PALW_WEIGHTED_DRAW_DOMAIN: &[u8] = b"misaka-palw-weighted-draw-v1";
/// SAMPLE-01 (ADR-0040 §5.17.6) — the per-round draw of the deterministic on-chain leaf sampler
/// ([`palw_deterministic_sample`]) that picks WHICH batch leaves the audit round covers. Distinct from
/// every other PALW keyed domain (asserted by `palw_keyed_domains_are_pairwise_distinct`).
pub const PALW_AUDIT_SAMPLE_DOMAIN: &[u8] = b"misaka-palw-audit-sample-v1";
/// SAMPLE-01 (ADR-0040 §5.17.6) — the canonical commitment domain for the re-derived `audit_sample_root`:
/// a keyed hash over the beacon-selected leaves' `receipt_da_root` values in ascending-index order
/// ([`palw_audit_sample_root`]). This is the REDEFINITION §5.17.6 fixes — an on-chain vector commitment
/// over per-leaf DA roots, which consensus CAN re-derive, replacing the unenforceable off-chain
/// receipt-chunk possession property (I-14). Distinct from every other PALW keyed domain.
pub const PALW_AUDIT_SAMPLE_ROOT_DOMAIN: &[u8] = b"misaka-palw-audit-sample-root-v1";

// =============================================================================================
// Proof type (design §20.2). Header carries `palw_proof_type: u8`; keep the wire byte pinned to the
// OPEN discriminants {1, 3} (borsh's positional enum index would NOT preserve these, so the on-wire
// representation is a plain `u8` and this enum is a typed view over it).
//
// ADR-0039 (2026-07-19): PALW is **all-open / publicly verifiable**. The content-HIDING proof types are
// REMOVED — no TEE (`TeeRateLimitedV1 = 2`) and no witness-hiding argument (`WitnessHidingArgumentV1 =
// 4`). Every mint-grade leaf must be checkable by anyone: replica-exact is public + reproducible, and a
// transparent argument has no trusted setup and hides nothing. A leaf carrying discriminant 2 or 4 is
// REJECTED (`from_u8` → `None`). Privacy is a product line that simply does NOT mint (solo / off-protocol
// tier, content never leaves the device), never an on-chain content-hiding feature.
// =============================================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwProofType {
    /// k=2 replica-exact agreement — the genesis lane. OPEN: the answer tokens are public and
    /// bit-reproducible by anyone with the model, so the reproducibility IS the proof (nothing hidden).
    ReplicaExactV1 = 1,
    /// Transparent (no-trusted-setup, publicly verifiable — STARK-style) argument of the same
    /// computation. OPEN: anyone can check the argument; the content is not hidden.
    TransparentArgumentV1 = 3,
}

impl PalwProofType {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[inline]
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::ReplicaExactV1),
            3 => Some(Self::TransparentArgumentV1),
            // 2 (TEE) and 4 (witness-hiding) are REMOVED — PALW is all-open; a leaf with those is rejected.
            _ => None,
        }
    }
}

// =============================================================================================
// ADR-0039 D15 — self-ordered jobs as first-class: the two PURE, INERT consensus deltas.
// (challenge-binding freshness + dispatch-proof validity). Not yet wired into `verify_palw_ticket`;
// they land with the D15 activation, alongside the challenge-in-context runtime + PCPB dispatch. Pure
// functions of committed fields — no store reads, no wall-clock — so both are inert-landable.
// =============================================================================================

/// D15 (i) / ADR-0045 D3-b clause 11 (pure half) — **challenge-binding freshness**. A mint-grade leaf's
/// committed challenge epoch (`receipt_v3_issued_epoch`), its dispatch anchor epoch (`a_commit_epoch`
/// for the self branch, the challenge epoch itself for the external branch), and its acceptance-pinned
/// `registered_epoch` must satisfy
///
/// ```text
/// issued ≤ anchor  ∧  anchor + Δ ≤ registered  ∧  registered − issued ≤ w
/// ```
///
/// so a cached/replayed computation under a stale challenge cannot mint (upper bound `w`), and the
/// post-commit draw beacon `R_{anchor+Δ}` provably post-dates the anchor (lower bound `Δ` — without it
/// the "future" beacon may already be known at anchor time and partner-B selection is grindable). The
/// window is non-empty because `PalwParams::is_structurally_valid` enforces `w ≥ Δ`. All three epochs
/// are consensus-derived or content-sealed (item 7 pinned `registered_epoch`; both others are inside
/// `content_id()`), so no operand is free to the producer. Supersedes the Δ-less two-epoch form — the
/// spec change ADR-0040 §5.14.7.6 predicted (the check GREW; the old form had no lower bound).
#[inline]
pub fn palw_challenge_fresh(issued_epoch: u64, anchor_epoch: u64, registered_epoch: u64, w: u64, delta: u64) -> bool {
    issued_epoch <= anchor_epoch
        && anchor_epoch.checked_add(delta).is_some_and(|draw| draw <= registered_epoch)
        && registered_epoch - issued_epoch <= w
}

/// D15 — recompute the PCPB partner-B derivation `B = f(R_{E+Δ}, A_commit)` from the POST-commit beacon and
/// A's escrow-locked receipt commitment. Domain-separated; the verifier compares this to the leaf's claimed
/// partner-B so A cannot pre-select a sybil B *after* seeing the answer.
#[inline]
pub fn palw_pcpb_derive_b(post_commit_beacon: &Hash64, a_commit: &Hash64) -> Hash64 {
    let mut p = Vec::with_capacity(2 * HASH64_SIZE);
    p.extend_from_slice(post_commit_beacon.as_byte_slice());
    p.extend_from_slice(a_commit.as_byte_slice());
    blake2b_512_keyed(PALW_PCPB_DOMAIN, &p)
}

// The legacy declared-`bool` `PalwDispatchProof` / `palw_dispatch_proof_valid` pair was REMOVED by the
// D3-b wiring slice, exactly as §5.14.7.9 scheduled ("除去は wiring 原子スライスが行う"): its external
// arm was a `true && true` tautology and its self arm a caller assertion (§5.14.2). The verifiable
// replacement is [`PalwDispatchEvidence`] + [`palw_dispatch_evidence_valid`] below.

// =============================================================================================
// ADR-0040 §5.14.7.1 — REDESIGNED dispatch proof as VERIFIABLE EVIDENCE (PURE, INERT).
//
// Replaces the caller-asserted `bool`s of `PalwDispatchProof` (§5.14.2 degeneracy) with values the
// verifier RE-DERIVES from the consensus-resolved beacon + provider snapshot. **Zero production callers**
// — this is the §5.14.5-style zero-risk landing of §5.14.3 item 3's core artifact; the WIRING of it
// (LeafV2 fields + `verify_palw_ticket` clauses 6/7/9) remains the atomic cutover §5.14.4 forbids
// splitting.
//
// Soundness hinge: the leaf carries only COMMITMENTS/ROOTS; the heavy witnesses ride in the receipt DA
// object / leaf-chunk and are re-derived here. `palw_dispatch_evidence_valid`'s clause 0 binds the
// leaf-committed roots to INDEPENDENTLY-resolved on-chain roots, so nothing here trusts a leaf-supplied
// value as its own proof. ML-DSA verification is INJECTED (`PalwMlDsaVerifier`) so consensus-core's prod
// dependency surface is unchanged (libcrux stays a dev-dep, exercised by the unit tests).
// =============================================================================================

/// Injected ML-DSA-87 verifier. The self-arm signature check calls this — it never stores or trusts a
/// caller-asserted "signature ok" verdict (that IS the §5.14.2 degeneracy). Production binds a
/// libcrux-backed impl at the clause-7 wiring slice (the PORTABLE verify, for cross-CPU determinism —
/// audit H-2); the unit tests bind the real libcrux verifier via the consensus-core dev-dependency, so
/// the self arm is exercised against genuine ML-DSA keypairs, not a stub.
pub trait PalwMlDsaVerifier {
    fn verify(&self, verification_key: &[u8], message: &[u8], context: &[u8], signature: &[u8]) -> bool;
}

/// A bottom-up keyed-BLAKE2b Merkle membership witness. `index` is the leaf's position (LSB-first path);
/// `siblings` are the co-path hashes, bottom to top. Mirrors the existing PALW leaf-merkle node
/// construction (the fold direction binds the index). Pure — no store reads.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwMerkleMembership {
    pub index: u32,
    pub leaf: Hash64,
    pub siblings: Vec<Hash64>,
}

impl PalwMerkleMembership {
    fn fold_to_root(&self, node_domain: &[u8]) -> Hash64 {
        let mut node = self.leaf;
        let mut idx = self.index;
        for sib in &self.siblings {
            let mut p = Vec::with_capacity(2 * HASH64_SIZE);
            if idx & 1 == 0 {
                push_hash(&mut p, &node);
                push_hash(&mut p, sib);
            } else {
                push_hash(&mut p, sib);
                push_hash(&mut p, &node);
            }
            node = blake2b_512_keyed(node_domain, &p);
            idx >>= 1;
        }
        node
    }

    /// True iff the witness opens `expected_leaf` at `index` up to `root` under `node_domain`.
    pub fn verifies(&self, root: &Hash64, expected_leaf: &Hash64, node_domain: &[u8]) -> bool {
        self.leaf == *expected_leaf && self.fold_to_root(node_domain) == *root
    }
}

/// One bonded provider as committed in an epoch bond-weighted snapshot (E−k). `ml_dsa_pk_hash` binds the
/// self-arm signing key; `reward_script_commitment` is the payout target (kept as a commitment so this
/// pure module needs no `ScriptPublicKey` serialization).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwProviderSnapshotEntry {
    pub provider_id: Hash64,
    pub ml_dsa_pk_hash: Hash64,
    pub bond_sompi: u64,
    pub reward_script_commitment: Hash64,
}

/// One provider's half-open bond interval `[cumulative_lo, cumulative_lo + bond_sompi)` in the assignment
/// tree — the deterministic provider-id-sorted cumulative partition of `[0, total_bond)`. Committed under
/// `assignment_root`, itself a deterministic derivation of the snapshot, so it is NOT independently
/// attacker-chosen once clause 0 binds it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwAssignmentInterval {
    pub provider_id: Hash64,
    pub bond_sompi: u64,
    pub cumulative_lo: u128,
}

/// External/parallel arm: one provider slot, re-derivable. Carries the snapshot entry + its membership,
/// and the assignment interval + its membership. The verifier re-runs the weighted draw — no `bool`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct SlotAssignmentWitness {
    pub entry: PalwProviderSnapshotEntry,
    pub snapshot_membership: PalwMerkleMembership,
    pub interval: PalwAssignmentInterval,
    pub assignment_membership: PalwMerkleMembership,
}

/// External arm proof: two beacon-drawn slots (must be distinct providers).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct BeaconAssignedProof {
    pub slot_a: SlotAssignmentWitness,
    pub slot_b: SlotAssignmentWitness,
}

/// Self/serial arm: B is drawn by the POST-commit beacon (via `palw_pcpb_derive_b`, so A cannot pre-pick a
/// sybil B after seeing the answer) AND B's ML-DSA-87 receipt binds `a_commit` — CHECKED, not declared.
///
/// D3-b addition (design memo §4 clause 12): the arm also carries **A's own snapshot membership**
/// (`a_entry` + `a_snapshot_membership`). Self-ordering is a bonded-provider privilege — without this,
/// an unbonded A could self-order into the mint path with only B's bond at stake.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct SelfSerialProof {
    pub a_commit: Hash64,
    pub a_entry: PalwProviderSnapshotEntry,
    pub a_snapshot_membership: PalwMerkleMembership,
    pub b_entry: PalwProviderSnapshotEntry,
    pub b_snapshot_membership: PalwMerkleMembership,
    pub b_interval: PalwAssignmentInterval,
    pub b_assignment_membership: PalwMerkleMembership,
    pub b_ml_dsa_pk: Vec<u8>,
    pub b_receipt_preimage: Vec<u8>,
    pub b_signature: Vec<u8>,
}

/// ADR-0040 §5.14.7.1 — the redesigned dispatch evidence. Replaces [`PalwDispatchProof`].
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub enum PalwDispatchEvidence {
    BeaconAssigned(BeaconAssignedProof),
    SelfSerial(SelfSerialProof),
}

/// `Hash64_k(snapshot-leaf, provider_id ‖ ml_dsa_pk_hash ‖ bond_le64 ‖ reward_script_commitment)`.
pub fn palw_snapshot_entry_hash(e: &PalwProviderSnapshotEntry) -> Hash64 {
    let mut p = Vec::with_capacity(3 * HASH64_SIZE + 8);
    push_hash(&mut p, &e.provider_id);
    push_hash(&mut p, &e.ml_dsa_pk_hash);
    p.extend_from_slice(&e.bond_sompi.to_le_bytes());
    push_hash(&mut p, &e.reward_script_commitment);
    blake2b_512_keyed(PALW_SNAPSHOT_LEAF_DOMAIN, &p)
}

/// `Hash64_k(assign-leaf, provider_id ‖ bond_le64 ‖ cumulative_lo_le128)`.
pub fn palw_assignment_interval_hash(iv: &PalwAssignmentInterval) -> Hash64 {
    let mut p = Vec::with_capacity(HASH64_SIZE + 8 + 16);
    push_hash(&mut p, &iv.provider_id);
    p.extend_from_slice(&iv.bond_sompi.to_le_bytes());
    p.extend_from_slice(&iv.cumulative_lo.to_le_bytes());
    blake2b_512_keyed(PALW_ASSIGNMENT_LEAF_DOMAIN, &p)
}

/// `Hash64_k(provider-pk, ml_dsa_pk)` — binds a self-arm verification key to its committed provider id.
pub fn palw_provider_pk_hash(ml_dsa_pk: &[u8]) -> Hash64 {
    blake2b_512_keyed(PALW_PROVIDER_PK_HASH_DOMAIN, ml_dsa_pk)
}

/// Canonical partner-B receipt preimage: `TAG ‖ a_commit ‖ tail`. B signs THIS; the `TAG ‖ a_commit`
/// prefix is what [`palw_receipt_embeds_a_commit`] checks, so the binding is verified, not declared.
pub fn palw_pcpb_receipt_preimage(a_commit: &Hash64, tail: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(PALW_PCPB_RECEIPT_TAG.len() + HASH64_SIZE + tail.len());
    p.extend_from_slice(PALW_PCPB_RECEIPT_TAG);
    push_hash(&mut p, a_commit);
    p.extend_from_slice(tail);
    p
}

/// True iff `preimage` begins with the PCPB receipt tag immediately followed by `a_commit`'s 64 bytes.
pub fn palw_receipt_embeds_a_commit(preimage: &[u8], a_commit: &Hash64) -> bool {
    let tag = PALW_PCPB_RECEIPT_TAG;
    let need = tag.len() + HASH64_SIZE;
    preimage.len() >= need && &preimage[..tag.len()] == tag && &preimage[tag.len()..need] == a_commit.as_byte_slice()
}

/// `reduce128(H) mod total_bond` — low 128 bits of the seed reduced into `[0, total_bond)`. The modulo
/// bias is ≈ `total_bond / 2^128` (negligible for any realistic bond); the production draw MAY switch to
/// rejection sampling without changing this evidence shape.
fn reduce_hash_to_bond(h: &Hash64, total_bond: u128) -> u128 {
    let b = h.as_byte_slice();
    let lo: [u8; 16] = b[..16].try_into().expect("Hash64 is 64 bytes");
    u128::from_le_bytes(lo) % total_bond.max(1)
}

/// External-arm per-slot draw seed `H_k(assign-draw, beacon ‖ slot_index)`. Producers derive the slot's
/// draw with this; the verifier re-derives the same seed inside [`palw_dispatch_evidence_valid`].
pub fn palw_assignment_draw_seed(beacon: &Hash64, slot_index: u8) -> Hash64 {
    let mut p = Vec::with_capacity(HASH64_SIZE + 1);
    push_hash(&mut p, beacon);
    p.push(slot_index);
    blake2b_512_keyed(PALW_ASSIGNMENT_DRAW_DOMAIN, &p)
}

fn interval_contains(iv: &PalwAssignmentInterval, ticket: u128) -> bool {
    let hi = iv.cumulative_lo.saturating_add(iv.bond_sompi as u128);
    iv.cumulative_lo <= ticket && ticket < hi
}

/// One slot's full re-derivation: entry ∈ snapshot, interval ∈ assignment, entry/interval agree on
/// (provider_id, bond), and the weighted-draw ticket falls in the interval. No `bool` is trusted.
fn slot_witness_valid(
    w: &SlotAssignmentWitness,
    snapshot_root: &Hash64,
    assignment_root: &Hash64,
    total_bond: u128,
    draw_seed: &Hash64,
) -> bool {
    let ticket = reduce_hash_to_bond(draw_seed, total_bond);
    w.entry.provider_id == w.interval.provider_id
        && w.entry.bond_sompi == w.interval.bond_sompi
        && w.snapshot_membership.verifies(snapshot_root, &palw_snapshot_entry_hash(&w.entry), PALW_SNAPSHOT_NODE_DOMAIN)
        && w.assignment_membership.verifies(assignment_root, &palw_assignment_interval_hash(&w.interval), PALW_ASSIGNMENT_NODE_DOMAIN)
        && interval_contains(&w.interval, ticket)
}

/// [`PALW_DISPATCH_KIND_BEACON_ASSIGNED`] on a leaf commits it to the external arm ([`BeaconAssignedProof`]).
pub const PALW_DISPATCH_KIND_BEACON_ASSIGNED: u8 = 0;
/// [`PALW_DISPATCH_KIND_SELF_SERIAL`] commits the leaf to the self arm ([`SelfSerialProof`]).
pub const PALW_DISPATCH_KIND_SELF_SERIAL: u8 = 1;

/// The canonical provider identity in PCPB snapshots: `Hash64_k(provider-id, bond_txid ‖ bond_index)`.
/// Keyed by the bond OUTPOINT — the same coordinate the reward gate (`active_provider_bond_at`) and the
/// registry itself use — so a leaf's declared `provider_{a,b}_bond` maps to a snapshot entry with no
/// extra indirection. Per-bond intervals are linear in bond value, so one owner splitting collateral
/// across bonds gains nothing (and the SEL-01 floor prices the split).
pub fn palw_provider_id(bond_outpoint: &TransactionOutpoint) -> Hash64 {
    let mut p = Vec::with_capacity(HASH64_SIZE + 4);
    push_hash(&mut p, &bond_outpoint.transaction_id);
    p.extend_from_slice(&bond_outpoint.index.to_le_bytes());
    blake2b_512_keyed(PALW_PROVIDER_ID_DOMAIN, &p)
}

/// ADR-0045 D3-b clause 11 — the job-scoped challenge, re-derived by the verifier. Binds the epoch
/// beacon (freshness has a real anchor), the scheduler job, the requester credential, the request
/// commitment, and the shape — the exact §537 recipe Seam 1 implemented; the preimage layout is
/// byte-identical to the bridge's original so every already-issued lease/receipt keeps verifying.
/// Without this re-derivation `receipt_v3_issued_epoch` is a free declaration and clause 11's
/// freshness window is hollow (cached-activation replay, audit H-10).
#[allow(clippy::too_many_arguments)]
pub fn palw_job_challenge(
    network_id: u32,
    beacon_epoch: u64,
    beacon_seed: &Hash64,
    scheduler_job_id: &Hash64,
    requester_credential: &Hash64,
    request_commitment: &Hash64,
    shape_id: u16,
) -> Hash64 {
    let mut preimage = Vec::with_capacity(4 + 8 + HASH64_SIZE * 4 + 2);
    preimage.extend_from_slice(&network_id.to_le_bytes());
    preimage.extend_from_slice(&beacon_epoch.to_le_bytes());
    push_hash(&mut preimage, beacon_seed);
    push_hash(&mut preimage, scheduler_job_id);
    push_hash(&mut preimage, requester_credential);
    push_hash(&mut preimage, request_commitment);
    preimage.extend_from_slice(&shape_id.to_le_bytes());
    blake2b_512_keyed(PALW_JOB_CHALLENGE_DOMAIN, &preimage)
}

/// The leaf-committed facts clause 12 verifies dispatch evidence AGAINST — every field is inside
/// `leaf_hash` (→ `leaf_root` → `content_id() == batch_id`), so none is free to the evidence carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDispatchLeafFacts {
    pub snapshot_root: Hash64,
    pub assignment_root: Hash64,
    pub a_commit: Hash64,
    pub dispatch_kind: u8,
    /// `palw_provider_id(leaf.provider_a_bond)` / `..b_bond` — the DECLARED seats the drawn providers
    /// must actually occupy. Without these the evidence proves "some pair was drawn" while the leaf
    /// names whoever it likes (delegation laundering, the exact forgery PCPB exists to kill).
    pub provider_a_id: Hash64,
    pub provider_b_id: Hash64,
}

/// ADR-0045 D3-b clause 12 — **dispatch-evidence validity** (PURE). Every branch is a value the verifier
/// re-derives from consensus-resolved inputs; NO caller-asserted `bool` is trusted (the declared-`bool`
/// ancestor of this function was removed by this slice). `resolved` is the independently store-resolved
/// snapshot commitment of epoch `anchor − k`; `post_commit_beacon` is `R_{anchor + Δ}` from the seed
/// history (both fail-closed at the call site); `leaf` carries the LeafV2-committed facts; `verifier` is
/// the injected ML-DSA-87 check.
pub fn palw_dispatch_evidence_valid<V: PalwMlDsaVerifier>(
    ev: &PalwDispatchEvidence,
    resolved: &PalwSnapshotCommitment,
    post_commit_beacon: &Hash64,
    leaf: &PalwDispatchLeafFacts,
    verifier: &V,
) -> bool {
    // Clause 0 (soundness hinge): the leaf-committed roots MUST equal the independently-resolved on-chain
    // roots. Without this the evidence degenerates to trusting an attacker-supplied root (§5.14.2).
    if leaf.snapshot_root != resolved.snapshot_root || leaf.assignment_root != resolved.assignment_root {
        return false;
    }
    if resolved.total_bond == 0 {
        return false; // an empty snapshot cannot assign any slot
    }
    match ev {
        PalwDispatchEvidence::BeaconAssigned(p) => {
            // The branch must be the one the leaf committed to — evidence is not swappable after the fact.
            if leaf.dispatch_kind != PALW_DISPATCH_KIND_BEACON_ASSIGNED {
                return false;
            }
            // External leaves carry the self-arm sentinels; a non-zero a_commit on this branch is a
            // category error (and would open an unanchored self flow wearing external clothes).
            if leaf.a_commit != Hash64::default() {
                return false;
            }
            slot_witness_valid(
                &p.slot_a,
                &resolved.snapshot_root,
                &resolved.assignment_root,
                resolved.total_bond,
                &palw_assignment_draw_seed(post_commit_beacon, 0),
            ) && slot_witness_valid(
                &p.slot_b,
                &resolved.snapshot_root,
                &resolved.assignment_root,
                resolved.total_bond,
                &palw_assignment_draw_seed(post_commit_beacon, 1),
            )
            // two DISTINCT providers: the external arm is a real 2-of-N draw, not a `true && true` tautology.
            && p.slot_a.entry.provider_id != p.slot_b.entry.provider_id
            // …and the drawn providers are the leaf's DECLARED seats, in seat order (slot 0 = A, slot 1 = B).
            && p.slot_a.entry.provider_id == leaf.provider_a_id
            && p.slot_b.entry.provider_id == leaf.provider_b_id
        }
        PalwDispatchEvidence::SelfSerial(p) => {
            if leaf.dispatch_kind != PALW_DISPATCH_KIND_SELF_SERIAL {
                return false;
            }
            if p.a_commit != leaf.a_commit || p.a_commit == Hash64::default() {
                return false;
            }
            // A is a bonded provider in the SAME snapshot (self-ordering is a bonded privilege), sits in
            // the leaf's A seat, and is not its own partner.
            if p.a_entry.provider_id != leaf.provider_a_id
                || p.a_entry.provider_id == p.b_entry.provider_id
                || !p.a_snapshot_membership.verifies(
                    &resolved.snapshot_root,
                    &palw_snapshot_entry_hash(&p.a_entry),
                    PALW_SNAPSHOT_NODE_DOMAIN,
                )
            {
                return false;
            }
            // B is selected by the POST-commit beacon (A cannot pre-pick a sybil B after seeing the answer).
            let seed = palw_pcpb_derive_b(post_commit_beacon, &p.a_commit);
            let ticket = reduce_hash_to_bond(&seed, resolved.total_bond);
            p.b_entry.provider_id == leaf.provider_b_id
                && p.b_entry.provider_id == p.b_interval.provider_id
                && p.b_entry.bond_sompi == p.b_interval.bond_sompi
                && p.b_snapshot_membership.verifies(
                    &resolved.snapshot_root,
                    &palw_snapshot_entry_hash(&p.b_entry),
                    PALW_SNAPSHOT_NODE_DOMAIN,
                )
                && p.b_assignment_membership.verifies(
                    &resolved.assignment_root,
                    &palw_assignment_interval_hash(&p.b_interval),
                    PALW_ASSIGNMENT_NODE_DOMAIN,
                )
                && interval_contains(&p.b_interval, ticket)
                // the signing key binds to the committed provider id, and B genuinely signed a receipt that
                // embeds A_commit — the ordering evidence is CHECKED here, not declared by a `bool`.
                && palw_provider_pk_hash(&p.b_ml_dsa_pk) == p.b_entry.ml_dsa_pk_hash
                && palw_receipt_embeds_a_commit(&p.b_receipt_preimage, &p.a_commit)
                && verifier.verify(&p.b_ml_dsa_pk, &p.b_receipt_preimage, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, &p.b_signature)
        }
    }
}

// =============================================================================================
// ADR-0040 §5.14.7.4 (item 5) — CANONICAL provider snapshot + assignment builder (PURE, INERT).
//
// The producer/verifier "other side" of `palw_dispatch_evidence_valid`'s clause 0. Builds the epoch
// bond-weighted snapshot root and the DETERMINISTIC assignment tree (cumulative bond partition of
// `[0, total_bond)` in provider-id order) from a provider set, plus every membership witness a producer
// needs to assemble a `SlotAssignmentWitness` / `SelfSerialProof`. The assignment tree being a pure
// function of the sorted snapshot is exactly what lets the wiring slice resolve `snapshot_root` on-chain
// and RECOMPUTE `assignment_root` — so it is never independently attacker-chosen. **Zero production
// callers**: no on-chain snapshot commit exists yet (that is the atomic cutover).
// =============================================================================================

/// The committed roots of one epoch snapshot. `provider_count` binds the retained set size.
///
/// D3-b: persisted per epoch by the provider-snapshot history store (the clause-0 "independently
/// resolved" side) and carried in the pruning snapshot payload, hence the serde/borsh derives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwSnapshotCommitment {
    pub snapshot_root: Hash64,
    pub assignment_root: Hash64,
    pub total_bond: u128,
    pub provider_count: u32,
}

/// The full canonical snapshot: the committed roots plus one ready-to-use [`SlotAssignmentWitness`] per
/// retained provider, in canonical (provider-id-sorted) order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSnapshotWitnessSet {
    pub commitment: PalwSnapshotCommitment,
    pub slots: Vec<SlotAssignmentWitness>,
}

impl PalwSnapshotWitnessSet {
    /// The index of the provider a bond-weighted draw `seed` selects, or `None` if the snapshot is empty.
    /// Mirrors the interval containment in [`palw_dispatch_evidence_valid`]: producers pick the slot's
    /// provider with this, the verifier re-derives the same containment. Because the intervals partition
    /// `[0, total_bond)` with no gap or overlap, at most one provider is ever returned.
    pub fn select(&self, seed: &Hash64) -> Option<usize> {
        if self.commitment.total_bond == 0 {
            return None;
        }
        let ticket = reduce_hash_to_bond(seed, self.commitment.total_bond);
        self.slots.iter().position(|s| interval_contains(&s.interval, ticket))
    }
}

/// Bottom-up keyed-BLAKE2b tree levels for `leaves` (duplicate-last padding for odd widths — matching the
/// fold in [`PalwMerkleMembership::fold_to_root`]). `levels[0]` are the leaves; the last level is the root.
fn palw_pcpb_merkle_levels(leaves: &[Hash64], node_domain: &[u8]) -> Vec<Vec<Hash64>> {
    let mut levels = vec![leaves.to_vec()];
    while levels.last().unwrap().len() > 1 {
        let cur = levels.last().unwrap();
        let mut next = Vec::with_capacity(cur.len().div_ceil(2));
        let mut i = 0;
        while i < cur.len() {
            let l = cur[i];
            let r = if i + 1 < cur.len() { cur[i + 1] } else { cur[i] };
            let mut p = Vec::with_capacity(2 * HASH64_SIZE);
            push_hash(&mut p, &l);
            push_hash(&mut p, &r);
            next.push(blake2b_512_keyed(node_domain, &p));
            i += 2;
        }
        levels.push(next);
    }
    levels
}

/// The membership witness for `index` in a tree produced by [`palw_pcpb_merkle_levels`] (LSB-first path,
/// duplicate-last padding — the inverse of [`PalwMerkleMembership::fold_to_root`]).
fn palw_pcpb_merkle_membership(levels: &[Vec<Hash64>], index: usize) -> PalwMerkleMembership {
    let leaf = levels[0][index];
    let mut siblings = Vec::with_capacity(levels.len().saturating_sub(1));
    let mut idx = index;
    for level in &levels[..levels.len() - 1] {
        let sib = if idx & 1 == 0 { if idx + 1 < level.len() { level[idx + 1] } else { level[idx] } } else { level[idx - 1] };
        siblings.push(sib);
        idx >>= 1;
    }
    PalwMerkleMembership { index: index as u32, leaf, siblings }
}

/// ADR-0040 §5.14.7.4 — build the canonical snapshot + assignment commitment and per-provider witnesses.
/// Order-INDEPENDENT (entries are sorted by `provider_id`); entries with `bond_sompi == 0` are excluded
/// (they can never be drawn). The assignment intervals cumulatively partition `[0, total_bond)` with no
/// gap or overlap, so exactly one interval contains any ticket. PURE — no store reads, no wall-clock. An
/// empty (or all-zero-bond) set yields zero roots / `total_bond == 0`, which `palw_dispatch_evidence_valid`
/// rejects at clause 0.
pub fn palw_build_snapshot_witnesses(entries: &[PalwProviderSnapshotEntry]) -> PalwSnapshotWitnessSet {
    let mut kept: Vec<PalwProviderSnapshotEntry> = entries.iter().filter(|e| e.bond_sompi > 0).cloned().collect();
    kept.sort_by(|a, b| a.provider_id.as_byte_slice().cmp(b.provider_id.as_byte_slice()));

    if kept.is_empty() {
        return PalwSnapshotWitnessSet {
            commitment: PalwSnapshotCommitment {
                snapshot_root: Hash64::default(),
                assignment_root: Hash64::default(),
                total_bond: 0,
                provider_count: 0,
            },
            slots: Vec::new(),
        };
    }

    let mut intervals = Vec::with_capacity(kept.len());
    let mut cum: u128 = 0;
    for e in &kept {
        intervals.push(PalwAssignmentInterval { provider_id: e.provider_id, bond_sompi: e.bond_sompi, cumulative_lo: cum });
        cum += e.bond_sompi as u128;
    }

    let snap_leaves: Vec<Hash64> = kept.iter().map(palw_snapshot_entry_hash).collect();
    let assign_leaves: Vec<Hash64> = intervals.iter().map(palw_assignment_interval_hash).collect();
    let snap_levels = palw_pcpb_merkle_levels(&snap_leaves, PALW_SNAPSHOT_NODE_DOMAIN);
    let assign_levels = palw_pcpb_merkle_levels(&assign_leaves, PALW_ASSIGNMENT_NODE_DOMAIN);
    let snapshot_root = snap_levels.last().unwrap()[0];
    let assignment_root = assign_levels.last().unwrap()[0];

    let slots = (0..kept.len())
        .map(|i| SlotAssignmentWitness {
            entry: kept[i].clone(),
            snapshot_membership: palw_pcpb_merkle_membership(&snap_levels, i),
            interval: intervals[i].clone(),
            assignment_membership: palw_pcpb_merkle_membership(&assign_levels, i),
        })
        .collect();

    PalwSnapshotWitnessSet {
        commitment: PalwSnapshotCommitment { snapshot_root, assignment_root, total_bond: cum, provider_count: kept.len() as u32 },
        slots,
    }
}

// =============================================================================================
// Preimage helpers.
// =============================================================================================

#[inline]
fn push_hash(buf: &mut Vec<u8>, h: &Hash64) {
    buf.extend_from_slice(h.as_byte_slice());
}

#[inline]
fn push_var(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    buf.extend_from_slice(bytes);
}

/// Interpret the leading 8 bytes of a digest as a little-endian `u64` (used for the modular slot
/// draw). Deterministic across platforms.
#[inline]
fn digest_low_u64(h: &Hash64) -> u64 {
    let b = h.as_byte_slice();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

// =============================================================================================
// Security-critical hash helpers (all keyed, all fixed-width preimages).
// =============================================================================================

/// `chain_commit(S)` (design §12.1). Miner-independent: derived from a DNS-finalized checkpoint fixed
/// *before* the target slot, so a producer cannot re-roll a ticket per fork (invariant I-4).
pub fn chain_commit(
    dns_finalized_checkpoint: &Hash64,
    dns_finality_certificate: &Hash64,
    target_interval: u64,
    network_id: u32,
) -> Hash64 {
    let mut p = Vec::with_capacity(2 * HASH64_SIZE + 8 + 4);
    push_hash(&mut p, dns_finalized_checkpoint);
    push_hash(&mut p, dns_finality_certificate);
    p.extend_from_slice(&target_interval.to_le_bytes());
    p.extend_from_slice(&network_id.to_le_bytes());
    blake2b_512_keyed(PALW_CHAIN_COMMIT_DOMAIN, &p)
}

// kaspa-pq ADR-0040 **TGT-02 + TGT-03 — `slot_digest` / `target_daa_interval` REMOVED (resolved by
// deletion). These were one site, not two: `active_window_intervals` (TGT-03) existed ONLY as a
// parameter of `target_daa_interval`, so deleting the function dissolves TGT-03 with it. There is no
// params field and no admission path by that name to add a `== 0` rejection to.**
//
// Both had zero non-test callers. Deleting them rather than "wiring them into a real path" is the
// point of the finding, not a dodge: the derivation WAS implemented in
// `body_processor/body_validation_in_context.rs` and then deliberately removed, because it introduced a
// SECOND target-interval rule that contradicted clause 5 of `verify_palw_ticket_store_facts`. Clause 5
// requires `header.daa_score == binding.target_daa_interval`; a slot draw is unrelated to `daa_score`,
// so every honest block failed with `IntervalMismatch`. Wiring these back in is a request to
// re-introduce a known-broken rule.
//
// **Where the capability actually comes from.** The interval is still consensus-derived, just not from
// a slot draw: `header.palw_target_daa_interval` (a live, fully-wired header field — do not confuse it
// with the deleted helper) is pinned by clause 5 to the block's own `daa_score`, which is itself
// validated post-GHOSTDAG as a function of the block's past. A miner therefore cannot name a
// favourable interval, which is exactly the property invariant I-3 wanted. That is recorded at the
// refutation of TGT-01 in `body_validation_in_context.rs`.

/// `eligibility_hash` — the one-shot draw (design §12.3). Returns a 512-bit digest; acceptance
/// compares `Uint512(eligibility_hash) <= target_512(bits)` in the difficulty slice.
#[allow(clippy::too_many_arguments)]
pub fn eligibility_hash(
    network_id: u32,
    eligibility_beacon: &Hash64,
    chain_commit: &Hash64,
    target_interval: u64,
    batch_id: &Hash64,
    leaf_index: u32,
    leaf_hash: &Hash64,
    ticket_nullifier: &Hash64,
) -> Hash64 {
    let mut p = Vec::with_capacity(4 * HASH64_SIZE + 4 + 8 + 4);
    p.extend_from_slice(&network_id.to_le_bytes());
    push_hash(&mut p, eligibility_beacon);
    push_hash(&mut p, chain_commit);
    p.extend_from_slice(&target_interval.to_le_bytes());
    push_hash(&mut p, batch_id);
    p.extend_from_slice(&leaf_index.to_le_bytes());
    push_hash(&mut p, leaf_hash);
    push_hash(&mut p, ticket_nullifier);
    blake2b_512_keyed(PALW_ELIGIBILITY_DOMAIN, &p)
}

/// ADR-0039 §12.3 — the one-shot eligibility DRAW acceptance: `Uint512(eligibility_digest) <=
/// target_512(bits)` AND the canonical algo-4 `nonce == low64(ticket_nullifier)` (the nonce is pinned
/// to the nullifier so it cannot be ground, I-3). Pure; `eligibility_digest` is [`eligibility_hash`].
/// ADR-0039 §12.3 / I-13 — the leaf's public commitment to its `ticket_nullifier`. The raw nullifier is
/// disclosed only when the ticket's header is minted; verification checks
/// `ticket_nullifier_commitment(header.palw_ticket_nullifier) == leaf.ticket_nullifier_commitment`. This
/// makes a ticket's future eligibility computable ONLY by the ticket holder (who knows the raw
/// nullifier), never by a third party reading the on-chain leaf — closing the pre-computable-winner
/// targeted-DoS / censorship / bribery channel.
pub fn ticket_nullifier_commitment(ticket_nullifier: &Hash64) -> Hash64 {
    blake2b_512_keyed(PALW_TICKET_NULLIFIER_COMMIT_DOMAIN, ticket_nullifier.as_byte_slice())
}

pub fn palw_eligibility_win(eligibility_digest: &Hash64, bits: u32, nonce: u64, ticket_nullifier: &Hash64) -> bool {
    let e = Uint512::from_le_bytes(*eligibility_digest.as_byte_slice());
    let target = Uint512::from_compact_target_bits_512(bits);
    e <= target && nonce == digest_low_u64(ticket_nullifier)
}

/// The resolved leaf/certificate facts a Header-v3 must match (design §14.2). The stores
/// (`leaf_store` / `certificate_store` / `beacon_store`) produce this; [`verify_palw_ticket`] is the
/// pure predicate over it, so consensus construction and validation share one acceptance rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTicketBinding {
    /// I-13: the leaf's published commitment. The verify checks
    /// `ticket_nullifier_commitment(header.palw_ticket_nullifier) == this` (not raw equality).
    pub ticket_nullifier_commitment: Hash64,
    pub proof_type: u8,
    /// Leaf active window (epochs): `[leaf_activation_epoch, leaf_expiry_epoch)`.
    pub leaf_activation_epoch: u64,
    pub leaf_expiry_epoch: u64,
    /// The single DAA interval this leaf may draw in (§12.2). Must equal the header's `daa_score`.
    pub target_daa_interval: u64,
}

/// The first §14.2 rule an algo-4 header violates, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTicketReject {
    NullifierMismatch,
    ProofTypeMismatch,
    LeafNotActive,
    CertNotActive,
    IntervalMismatch,
    ChainCommitMismatch,
    LaneBitsMismatch,
    ComputeCapExhausted,
    EligibilityMiss,
}

/// ADR-0039 §14.2 — the pure acceptance predicate for an algo-4 (PALW) header, given its own fields,
/// the resolved leaf/cert `binding`, and the consensus-derived expectations (`cert_active` =
/// `cert.is_active_at(daa_score)`, `epoch` = `ctx.epoch(daa_score)`, `expected_chain_commit` /
/// `expected_bits` from the lagged checkpoint + lane DAA, `compute_headroom_positive` = `E`-cap not
/// exhausted). Deterministic and store-free, so the header-verifier and the mining-template builder
/// apply the identical rule (construction == validation). Returns the first violated rule.
#[allow(clippy::too_many_arguments)]
pub fn verify_palw_ticket(
    h_nullifier: &Hash64,
    h_proof_type: u8,
    h_chain_commit: &Hash64,
    h_bits: u32,
    h_nonce: u64,
    h_daa_score: u64,
    eligibility_digest: &Hash64,
    binding: &PalwTicketBinding,
    cert_active: bool,
    epoch: u64,
    expected_chain_commit: &Hash64,
    expected_bits: u32,
    compute_headroom_positive: bool,
) -> Result<(), PalwTicketReject> {
    // Clauses 1–5 (nullifier / proof-type / leaf-active / cert-active / interval) — the store+epoch
    // resolvable subset, shared with the header pipeline (see `verify_palw_ticket_store_facts`).
    verify_palw_ticket_store_facts(h_nullifier, h_proof_type, h_daa_score, binding, cert_active, epoch)?;
    if h_chain_commit != expected_chain_commit {
        return Err(PalwTicketReject::ChainCommitMismatch);
    }
    if h_bits != expected_bits {
        return Err(PalwTicketReject::LaneBitsMismatch);
    }
    // §5.4: a zero-headroom compute lane produces / accepts no algo-4 block (no free blue score).
    if !compute_headroom_positive {
        return Err(PalwTicketReject::ComputeCapExhausted);
    }
    if !palw_eligibility_win(eligibility_digest, h_bits, h_nonce, h_nullifier) {
        return Err(PalwTicketReject::EligibilityMiss);
    }
    Ok(())
}

/// ADR-0039 §14.2 clauses 1–5 — the subset of the algo-4 acceptance rule that is fully determined by
/// the header, the store-resolved leaf/cert `binding`, and the consensus `epoch`, with no dependency on
/// the beacon (`R_E`), lane-DAA retarget, lagged checkpoint, or compute-cap state. [`verify_palw_ticket`]
/// runs this first; the header pipeline runs *only* this while those later consensus-state slices are
/// still inert, so there is a single, non-divergent source for these five rules. Returns the first
/// violated clause.
pub fn verify_palw_ticket_store_facts(
    h_nullifier: &Hash64,
    h_proof_type: u8,
    h_daa_score: u64,
    binding: &PalwTicketBinding,
    cert_active: bool,
    epoch: u64,
) -> Result<(), PalwTicketReject> {
    // I-13: the header DISCLOSES the raw nullifier; it must open the leaf's published commitment.
    if ticket_nullifier_commitment(h_nullifier) != binding.ticket_nullifier_commitment {
        return Err(PalwTicketReject::NullifierMismatch);
    }
    if h_proof_type != binding.proof_type {
        return Err(PalwTicketReject::ProofTypeMismatch);
    }
    if !(binding.leaf_activation_epoch <= epoch && epoch < binding.leaf_expiry_epoch) {
        return Err(PalwTicketReject::LeafNotActive);
    }
    if !cert_active {
        return Err(PalwTicketReject::CertNotActive);
    }
    if h_daa_score != binding.target_daa_interval {
        return Err(PalwTicketReject::IntervalMismatch);
    }
    Ok(())
}

/// ADR-0039 §9.3 / I-2 — a batch is block-eligible only once **every** leaf chunk is on-chain: a batch
/// missing any of its manifest's `chunk_count` chunks stays `Incomplete` and can never certify (no
/// hidden leaves). Tracks which chunk indices have been seen; deterministic and idempotent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwBatchChunkTracker {
    chunk_count: u32,
    received: BTreeSet<u32>,
}

impl PalwBatchChunkTracker {
    pub fn new(chunk_count: u32) -> Self {
        Self { chunk_count, received: BTreeSet::new() }
    }

    /// Record a chunk index; returns `false` if out of range (`>= chunk_count`) or already recorded.
    pub fn record(&mut self, chunk_index: u32) -> bool {
        if chunk_index >= self.chunk_count {
            return false;
        }
        self.received.insert(chunk_index)
    }

    /// True once all `chunk_count` chunks are present (the batch may leave `Incomplete`).
    pub fn is_complete(&self) -> bool {
        self.received.len() as u32 == self.chunk_count
    }

    pub fn missing_count(&self) -> u32 {
        self.chunk_count - self.received.len() as u32
    }
}

/// ADR-0039 §18 (I-6) — the data-availability check that a batch's on-chain / pruning-bundle state is
/// complete enough to verify without out-of-band data: the manifest, all leaf chunks, the certificate,
/// and the beacon state must all be present. Pure boolean over the resolved presence flags.
pub fn palw_da_bundle_complete(
    manifest_present: bool,
    chunks: &PalwBatchChunkTracker,
    certificate_present: bool,
    beacon_present: bool,
) -> bool {
    manifest_present && chunks.is_complete() && certificate_present && beacon_present
}

/// A mining-template algo-4 candidate ticket: the resolved eligibility digest + the canonical nonce +
/// its nullifier, from an Active ticket in the miner's inventory (design §22).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTemplateCandidate {
    pub eligibility_digest: Hash64,
    pub nonce: u64,
    pub ticket_nullifier: Hash64,
}

/// ADR-0039 §22 — pick the algo-4 ticket the mining template should use for the current interval: the
/// first candidate (in the caller's canonical inventory order) whose one-shot eligibility draw **wins**
/// at the current lane `bits`, using the EXACT validation predicate [`palw_eligibility_win`] so the
/// template a miner builds and the header a validator accepts agree. Returns the winning index, or
/// `None` if no Active ticket draws this interval (⇒ the miner produces an algo-3 template instead).
pub fn palw_select_template_ticket(candidates: &[PalwTemplateCandidate], bits: u32) -> Option<usize> {
    candidates.iter().position(|c| palw_eligibility_win(&c.eligibility_digest, bits, c.nonce, &c.ticket_nullifier))
}

/// ADR-0039 §22 / C6 — build the mining-template candidate for one Active ticket by computing its
/// eligibility-draw digest and canonical nonce **exactly the way the validator does** (clause 9): the
/// digest is [`eligibility_hash`] over the lagged `R_E` (`eligibility_beacon` = the finality-buried
/// anchor's `palw_beacon_seed`), the consensus-derived `chain_commit`, the target interval, and the leaf
/// identity; the nonce is pinned to `low64(ticket_nullifier)` (I-3). Because the template's selection
/// (`palw_select_template_ticket`) and the validator's acceptance (`verify_palw_ticket`) share these two
/// functions, **construction == validation** is structural — a header built from a winning candidate
/// satisfies clauses 6/9 by construction (proved by `template_ticket_construction_equals_validation`).
#[allow(clippy::too_many_arguments)]
pub fn palw_template_candidate(
    network_id: u32,
    eligibility_beacon: &Hash64,
    chain_commit: &Hash64,
    target_interval: u64,
    batch_id: &Hash64,
    leaf_index: u32,
    leaf_hash: &Hash64,
    ticket_nullifier: &Hash64,
) -> PalwTemplateCandidate {
    PalwTemplateCandidate {
        eligibility_digest: eligibility_hash(
            network_id,
            eligibility_beacon,
            chain_commit,
            target_interval,
            batch_id,
            leaf_index,
            leaf_hash,
            ticket_nullifier,
        ),
        nonce: digest_low_u64(ticket_nullifier),
        ticket_nullifier: *ticket_nullifier,
    }
}

/// `R_E` epoch beacon seed (design §11.2). The reveal / missing-commitment sets are pre-reduced to
/// canonical roots by the caller so the preimage is fixed-width.
pub fn beacon_seed(
    prev_seed: &Hash64,
    dns_finalized_anchor: &Hash64,
    valid_reveals_root: &Hash64,
    missing_commitments_root: &Hash64,
    epoch: u64,
) -> Hash64 {
    let mut p = Vec::with_capacity(4 * HASH64_SIZE + 8);
    push_hash(&mut p, prev_seed);
    push_hash(&mut p, dns_finalized_anchor);
    push_hash(&mut p, valid_reveals_root);
    push_hash(&mut p, missing_commitments_root);
    p.extend_from_slice(&epoch.to_le_bytes());
    blake2b_512_keyed(PALW_BEACON_DOMAIN, &p)
}

/// `R_fallback` degraded-mode seed (design §11.3): used only when the primary commit-reveal quorum is
/// unavailable. Derived from a **lagged wide** selected-chain window (NOT the current tip, to resist
/// tip-grinding) — the caller pre-reduces the window's block hashes to a Merkle root. This is NOT
/// fully unbiasable, so the compute-work multiplier is reduced (or 0) while a fallback seed is active.
pub fn beacon_fallback_seed(prev_seed: &Hash64, finalized_anchor: &Hash64, lagged_window_root: &Hash64, epoch: u64) -> Hash64 {
    let mut p = Vec::with_capacity(3 * HASH64_SIZE + 8);
    push_hash(&mut p, prev_seed);
    push_hash(&mut p, finalized_anchor);
    push_hash(&mut p, lagged_window_root);
    p.extend_from_slice(&epoch.to_le_bytes());
    blake2b_512_keyed(PALW_BEACON_DOMAIN, &p)
}

/// Degraded-mode decision for the DNS PALW beacon (design §11.3). Drives whether new batches may
/// activate and whether algo-4 blocks are accepted this epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBeaconMode {
    /// Primary commit-reveal quorum reached: full seed, new batches may activate, full compute weight.
    Healthy,
    /// DNS quality low / quorum short, still inside the grace window: existing Active tickets keep the
    /// previous seed, NO new batch activates, reduced compute weight.
    DegradedGrace,
    /// Grace exhausted: algo-4 blocks are invalid this epoch; the algo-3 hash lane continues.
    Halted,
}

/// Decide the beacon mode from DNS health + quorum + how many epochs the degradation has lasted
/// (design §11.3). `grace_epochs` is [`PalwParams::dns_degraded_grace_epochs`].
pub fn beacon_mode(dns_healthy: bool, quorum_reached: bool, degraded_epochs: u64, grace_epochs: u64) -> PalwBeaconMode {
    if dns_healthy && quorum_reached {
        PalwBeaconMode::Healthy
    } else if degraded_epochs <= grace_epochs {
        PalwBeaconMode::DegradedGrace
    } else {
        PalwBeaconMode::Halted
    }
}

/// ADR-0039 §11.3 (K5 fallback policy) — the effective per-leaf compute-work multiplier given the
/// beacon mode. Full `base_scale` while `Healthy`; HALVED during the `DegradedGrace` window; ZERO once
/// `Halted`. Monotone: `Healthy >= DegradedGrace >= Halted`.
///
/// **K5 wiring decision (design panel, recorded):** the GHOSTDAG compute credit
/// (`normalize_palw_work` at the header stage) stays FLAT on Header v3 and does NOT consume this
/// policy. Three structural reasons: (1) header-only IBD runs the credit for every header BEFORE any of
/// its history is body/virtual-validated locally, so any beacon-derived input would consume fields not
/// yet S2-authenticated (the unverified-field grinding hazard); (2) proof-level GHOSTDAG managers pin
/// PALW inert (`with_level`), so a mode-aware level-0 credit would diverge from the proof view; (3) the
/// only header-pure lagged signal (buried-anchor seed-carry, below) is BINARY (Healthy vs not) and
/// cannot express this tri-level policy. §11.3's "reduced compute weight" is therefore enforced by the
/// LAYERED gates instead — the S2-site `PalwLaneHalted` chain-block rule, the body-stage clause-10
/// lagged halt indicator, the `WorkRewardClass::ReplicaPalwHalted` zero-pay classification, and the
/// `advance_epoch_gated` activation freeze — with the residual weight exposure bounded by the permanent
/// `E = H + min(C, 4H)` cap (≤ 5× hash). Mode-aware WEIGHT is an explicit activation-time decision
/// (header-committed mode = Header v4 = re-genesis). This fn remains the template/candidate policy and
/// the future weight hook.
pub fn effective_compute_work_scale(base_scale: u64, mode: PalwBeaconMode) -> u64 {
    match mode {
        PalwBeaconMode::Healthy => base_scale,
        PalwBeaconMode::DegradedGrace => base_scale / 2,
        PalwBeaconMode::Halted => 0,
    }
}

/// ADR-0039 §11.3 (K5, the LAGGED BINARY beacon-health signal) — the length of the seed CARRY run
/// ending at the newest sample, over per-PALW-epoch `(epoch, seed)` samples in ascending epoch order
/// (one representative — the newest buried header — per epoch; see the body-stage sampler).
///
/// Soundness: `derive_beacon_epoch_state` carries the seed VERBATIM in `DegradedGrace`/`Halted` and
/// always advances it through the keyed hash chain in `Healthy` (the epoch is folded into the preimage,
/// so even an identical input set yields a fresh seed). Endpoint equality therefore certifies that NO
/// Healthy advance happened in the spanned interval: `seed(E_newest) == seed(e)` ⇒ every epoch in
/// `(e, E_newest]` was non-Healthy, and the run is `E_newest - e` for the OLDEST such equal sample.
/// A run `> grace_epochs` certifies the newest sampled epoch was `Halted`.
///
/// LAGGED + fail-open by construction: with `< 2` samples (short history, activation boundary, pruned
/// walk) the run is `0` — never a false halt. And because the samples are finality-BURIED, the signal
/// trails reality by ~the attestation lag: it misses a just-started halt (the reward gate + weight cap
/// cover that window) and keeps certifying for ~lag epochs after a Healthy recovery (the template MUST
/// consult the same predicate — [`palw_template_lane_open`] — or it would build self-rejecting blocks).
pub fn palw_seed_carry_run(samples: &[(u64, Hash64)]) -> u64 {
    let Some(&(newest_epoch, newest_seed)) = samples.last() else { return 0 };
    let mut oldest_equal = newest_epoch;
    for &(epoch, seed) in samples.iter().rev().skip(1) {
        if seed != newest_seed {
            break;
        }
        oldest_equal = epoch;
    }
    newest_epoch.saturating_sub(oldest_equal)
}

/// ADR-0039 §11.3 (K5) — may a `Certified` batch flip `Active` at this block, judged from the same
/// lagged buried samples? `true` iff the two newest distinct-epoch seeds DIFFER (a buried Healthy
/// advance exists — the beacon was recently healthy enough to admit new batches). `false` fail-closed
/// on `< 2` samples. Binary is exactly sufficient here: §11.3 freezes NEW batch activation during BOTH
/// `DegradedGrace` and `Halted` (existing Active tickets are untouched), and the seed-carry signal is
/// precisely "non-Healthy" — no tri-level resolution needed, unlike the weight policy above.
pub fn palw_lagged_activation_open(samples: &[(u64, Hash64)]) -> bool {
    let mut it = samples.iter().rev();
    let (Some(&(_, newest)), Some(&(_, prev))) = (it.next(), it.next()) else { return false };
    newest != prev
}

/// ADR-0039 §11.3 (K5, template-side c==v twin) — MUST be consulted by the algo-4 template candidate
/// constructor before emitting a ticket: the lane is open iff the block's OWN derived beacon mode is not
/// `Halted` (the S2-site `PalwLaneHalted` rule) AND the buried seed-carry run does not exceed grace (the
/// body-stage clause-10 rule). The second conjunct is what prevents post-recovery self-bricking: for
/// ~the attestation lag after a Healthy recovery the buried run still certifies halted, clause 10 still
/// rejects, and a template consulting only the exact mode would build blocks its own validation refuses.
pub fn palw_template_lane_open(derived_mode: u8, buried_carry_run: u64, grace_epochs: u64) -> bool {
    derived_mode != PalwBeaconMode::Halted.to_u8() && buried_carry_run <= grace_epochs
}

/// `PalwBeaconCommitV1.commitment = Hash64_k(beacon-commit, epoch ‖ random_64 ‖ bond_tx ‖ bond_idx)`
/// (design §11.2). Binds the reveal to the epoch and the committing bond.
pub fn beacon_commitment(epoch: u64, random_64: &[u8; 64], bond: &TransactionOutpoint) -> Hash64 {
    let mut p = Vec::with_capacity(8 + 64 + HASH64_SIZE + 4);
    p.extend_from_slice(&epoch.to_le_bytes());
    p.extend_from_slice(random_64);
    push_hash(&mut p, &bond.transaction_id);
    p.extend_from_slice(&bond.index.to_le_bytes());
    blake2b_512_keyed(PALW_BEACON_COMMIT_DOMAIN, &p)
}

/// Target-epoch lead for a commit included while PALW epoch `P` is current: it commits for `P + 2`.
pub const PALW_BEACON_COMMIT_LEAD_EPOCHS: u64 = 2;
/// Target-epoch lead for a reveal included while PALW epoch `P` is current: it reveals for `P + 1`.
pub const PALW_BEACON_REVEAL_LEAD_EPOCHS: u64 = 1;

/// The only target epoch a commit may name while `current_epoch` is active. `None` at the `u64`
/// boundary rather than wrapping/saturating into an ambiguous epoch.
#[inline]
pub fn beacon_commit_target_epoch(current_epoch: u64) -> Option<u64> {
    current_epoch.checked_add(PALW_BEACON_COMMIT_LEAD_EPOCHS)
}

/// The only target epoch a reveal may name while `current_epoch` is active.
#[inline]
pub fn beacon_reveal_target_epoch(current_epoch: u64) -> Option<u64> {
    current_epoch.checked_add(PALW_BEACON_REVEAL_LEAD_EPOCHS)
}

/// True exactly in the `E-2` commit phase for target epoch `E`.
#[inline]
pub fn beacon_commit_phase_accepts(current_epoch: u64, target_epoch: u64) -> bool {
    beacon_commit_target_epoch(current_epoch) == Some(target_epoch)
}

/// True exactly in the `E-1` reveal phase for target epoch `E`.
#[inline]
pub fn beacon_reveal_phase_accepts(current_epoch: u64, target_epoch: u64) -> bool {
    beacon_reveal_target_epoch(current_epoch) == Some(target_epoch)
}

/// Digest the secret opened by a valid reveal before it enters `valid_reveals_root`. The raw secret
/// is included in this distinct-domain preimage, so two different valid openings produce different
/// epoch seeds even if a caller accidentally supplies their earlier public commitments elsewhere.
pub fn beacon_reveal_entropy_digest(epoch: u64, random_64: &[u8; 64], bond: &TransactionOutpoint) -> Hash64 {
    let mut p = Vec::with_capacity(8 + 64 + HASH64_SIZE + 4);
    p.extend_from_slice(&epoch.to_le_bytes());
    p.extend_from_slice(random_64);
    push_hash(&mut p, &bond.transaction_id);
    p.extend_from_slice(&bond.index.to_le_bytes());
    blake2b_512_keyed(PALW_BEACON_REVEAL_ENTROPY_DOMAIN, &p)
}

/// Canonical-sorted keyed hash of a `(bond, value_digest)` set (design §11.2). Both `beacon_seed`
/// roots use this fixed-width shape, but `value_digest` has different semantics and a different root
/// domain: reveal-entropy digest for valid reveals, public commitment for missing reveals. The set is
/// sorted by `(transaction_id, index)` — the SAME canonical order the `(epoch ‖ bond)` store keys use —
/// then each entry is appended as `transaction_id ‖ index_le ‖ value_digest`, `u64`-LE count-prefixed
/// so the digest is collision-free across cardinalities. Deterministic and caller-order-independent.
fn beacon_bond_digest_set_root(domain: &[u8], entries: &[(TransactionOutpoint, Hash64)]) -> Hash64 {
    let mut sorted: Vec<&(TransactionOutpoint, Hash64)> = entries.iter().collect();
    sorted.sort_by(|a, b| a.0.transaction_id.as_byte_slice().cmp(b.0.transaction_id.as_byte_slice()).then(a.0.index.cmp(&b.0.index)));
    let mut p = Vec::with_capacity(8 + sorted.len() * (HASH64_SIZE + 4 + HASH64_SIZE));
    p.extend_from_slice(&(sorted.len() as u64).to_le_bytes());
    for (outpoint, value_digest) in sorted {
        push_hash(&mut p, &outpoint.transaction_id);
        p.extend_from_slice(&outpoint.index.to_le_bytes());
        push_hash(&mut p, value_digest);
    }
    blake2b_512_keyed(domain, &p)
}

/// `valid_reveals_root` — the `beacon_seed` input over the epoch's validly-opened reveal entropy
/// digests (§11.2). A public `beacon_commitment` is not a valid entry here.
pub fn beacon_valid_reveals_root(entries: &[(TransactionOutpoint, Hash64)]) -> Hash64 {
    beacon_bond_digest_set_root(PALW_BEACON_REVEALS_ROOT_DOMAIN, entries)
}

/// `missing_commitments_root` — the `beacon_seed` input over the epoch's committed-but-unrevealed
/// bonds (§11.2). The missing set is `commits(E)` minus the bonds that validly revealed.
pub fn beacon_missing_commitments_root(entries: &[(TransactionOutpoint, Hash64)]) -> Hash64 {
    beacon_bond_digest_set_root(PALW_BEACON_MISSING_ROOT_DOMAIN, entries)
}

/// ADR-0039 §11.2 — the beacon commit-reveal quorum: the epoch reached quorum iff the **stake-weighted**
/// revealed tally reaches the `num/den` fraction of the total committed stake. Stake-weighted (not a raw
/// count) so bond-splitting cannot Sybil the quorum — consistent with the certificate quorum
/// ([`PalwBatchCertificateV2::quorum_reached`]). `stake_of` resolves a committing bond to its stake in
/// the epoch's DNS bond view. `den` must be > 0; ties go to reached (`>=`). Feeds `beacon_mode`'s
/// `quorum_reached` bool.
pub fn beacon_quorum_reached(
    committed: &[TransactionOutpoint],
    revealed: &[TransactionOutpoint],
    num: u16,
    den: u16,
    stake_of: impl Fn(&TransactionOutpoint) -> u128,
) -> bool {
    // ADR-0040 QUORUM-02: `num == 0` is the same vacuity as `den == 0` from the other side — the RHS of
    // the cross-multiplied comparison becomes 0, so an empty reveal set would "reach" quorum. Guarded
    // here for the same reason it is guarded in `PalwBatchCertificateV2::quorum_reached` (P0-5).
    if den == 0 || num == 0 {
        return false;
    }
    let committed_stake: u128 = committed.iter().map(&stake_of).sum();
    // §11.3: an epoch with no committed stake (total participant dropout) is NOT a reached quorum — it
    // is degraded. Without this, the vacuous `0 >= 0` would advance the seed as Healthy over an empty
    // reveal set, masking a dropout as a healthy epoch.
    if committed_stake == 0 {
        return false;
    }
    let revealed_stake: u128 = revealed.iter().map(&stake_of).sum();
    revealed_stake.saturating_mul(den as u128) >= committed_stake.saturating_mul(num as u128)
}

/// `private_match_commitment` (design §24.2): the leaf commits this; the body (receipts, trace) stays
/// in receipt DA and a later canary dispute checks equality against it.
pub fn private_match_commitment(
    output_commitment: &Hash64,
    canonical_gemm_trace_root: &Hash64,
    operation_schedule_commitment: &Hash64,
    job_set_commitment: &Hash64,
    receipt_a_hash: &Hash64,
    receipt_b_hash: &Hash64,
) -> Hash64 {
    let mut p = Vec::with_capacity(6 * HASH64_SIZE);
    for h in [
        output_commitment,
        canonical_gemm_trace_root,
        operation_schedule_commitment,
        job_set_commitment,
        receipt_a_hash,
        receipt_b_hash,
    ] {
        push_hash(&mut p, h);
    }
    blake2b_512_keyed(PALW_MATCH_DOMAIN, &p)
}

// =============================================================================================
// Provider execution receipt (off-chain artifact, hashed into the leaf) — design §24.1.
// =============================================================================================

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ReplicaExecutionReceiptV1 {
    pub version: u16,
    pub provider_bond: TransactionOutpoint,
    pub session_public_key: Vec<u8>,
    pub job_nullifier: Hash64,
    pub job_set_commitment: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_id: u16,
    pub quantum_count: u16,
    pub output_commitment: Hash64,
    /// Canonical Compute v1 §17.5 fix 1 — a **model-opaque** commitment: the fold of the off-chain trace
    /// (op-ids, GEMM schedule, integer accumulators — all owned by `compute_spec_version`). Consensus
    /// borsh-serializes/hashes this 64-byte blob and MUST NEVER re-derive or field-parse it; the validity
    /// kernel (`verify_palw_ticket`) never reads it. Keeping it opaque is what makes model/MoE/VLM changes
    /// a spec-version rollout rather than a consensus fork.
    pub canonical_gemm_trace_root: Hash64,
    pub operation_schedule_commitment: Hash64,
    pub receipt_da_root: Hash64,
    pub completed_at_epoch: u64,
    /// ML-DSA-87 signature over [`Self::signing_hash`] under the receipt context. Verification wiring
    /// lands in the audit slice; here it is a wire field only.
    pub signature: Vec<u8>,
}

impl ReplicaExecutionReceiptV1 {
    /// The message a provider signs: every field **except** `signature`, domain-separated and
    /// length-prefixed. Kept separate from [`Self::hash`] so the leaf can commit the full receipt
    /// (incl. signature) while the signature covers only the semantic fields.
    pub fn signing_hash(&self) -> Hash64 {
        let mut p = Vec::with_capacity(256);
        p.extend_from_slice(&self.version.to_le_bytes());
        push_hash(&mut p, &self.provider_bond.transaction_id);
        p.extend_from_slice(&self.provider_bond.index.to_le_bytes());
        push_var(&mut p, &self.session_public_key);
        for h in [&self.job_nullifier, &self.job_set_commitment, &self.model_profile_id, &self.runtime_class_id] {
            push_hash(&mut p, h);
        }
        p.extend_from_slice(&self.shape_id.to_le_bytes());
        p.extend_from_slice(&self.quantum_count.to_le_bytes());
        for h in [&self.output_commitment, &self.canonical_gemm_trace_root, &self.operation_schedule_commitment, &self.receipt_da_root]
        {
            push_hash(&mut p, h);
        }
        p.extend_from_slice(&self.completed_at_epoch.to_le_bytes());
        blake2b_512_keyed(PALW_RECEIPT_DOMAIN, &p)
    }

    /// Full-receipt hash committed by the leaf (`receipt_a_hash` / `receipt_b_hash`).
    pub fn hash(&self) -> Hash64 {
        blake2b_512_keyed(PALW_RECEIPT_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }
}

/// Private match record (body in receipt DA; only its commitment is public) — design §24.2.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ReplicaMatchRecordV1 {
    pub receipt_a_hash: Hash64,
    pub receipt_b_hash: Hash64,
    pub job_nullifier: Hash64,
    pub output_commitment: Hash64,
    pub canonical_gemm_trace_root: Hash64,
    pub operation_schedule_commitment: Hash64,
    pub matched_at_epoch: u64,
}

// =============================================================================================
// Public leaf, manifest, chunk (design §9.2, §9.3).
// =============================================================================================

/// One public leaf descriptor — everything a validator needs to state-lookup a ticket, with NO
/// prompt/output/receipt body. Published before the beacon (I-2). Design §24 / §9.2.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwPublicLeafV1 {
    pub version: u16,
    pub batch_id: Hash64,
    pub leaf_index: u32,
    /// A scheduled work unit cannot mint another leaf.
    pub job_nullifier: Hash64,
    /// I-13 winner secrecy: the leaf publishes only the COMMITMENT
    /// `ticket_nullifier_commitment(ticket_nullifier)`, never the raw nullifier. The header discloses the
    /// raw nullifier at mint time (also carried first-class in Header v3); verification binds
    /// `ticket_nullifier_commitment(header.palw_ticket_nullifier) == this`. So only the ticket holder can
    /// pre-compute the ticket's future eligibility — a third party reading the public leaf cannot.
    pub ticket_nullifier_commitment: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_id: u16,
    pub quantum_count: u16,
    /// [`PalwProofType`] discriminant.
    pub proof_type: u8,
    pub provider_a_bond: TransactionOutpoint,
    pub provider_b_bond: TransactionOutpoint,
    pub provider_a_reward_script: ScriptPublicKey,
    pub provider_b_reward_script: ScriptPublicKey,
    pub ticket_authority_pk_hash: Hash64,
    pub private_match_commitment: Hash64,
    /// Canonical receipt DA schema. V1 retains the closed-net legacy receipts; V2 carries the
    /// node-owned Receipt-v3 pair required by public Header-v4 networks.
    pub receipt_da_object_version: u16,
    pub receipt_da_root: Hash64,
    /// Canonical object-v1 byte length committed by `receipt_da_root`. These metadata fields are
    /// consensus-visible so buried-beacon sampling and timeout slashing never depend on whether a
    /// particular validator happened to fetch the off-chain object first.
    pub receipt_da_object_len: u32,
    pub receipt_da_chunk_count: u16,
    /// Selected-chain expectations for Object V2. They are part of `leaf_hash`, so workers cannot
    /// choose a compute set, job challenge, or validity window after the leaf is committed. All four
    /// fields are canonical zero sentinels for legacy Object V1.
    pub receipt_v3_compute_set_id: Hash64,
    pub receipt_v3_job_challenge: Hash64,
    pub receipt_v3_issued_epoch: u64,
    pub receipt_v3_expires_epoch: u64,
    pub registered_epoch: u64,
    pub activation_epoch: u64,
    pub expiry_epoch: u64,
    pub leaf_bond_sompi: u64,
    /// ADR-0045 D3-b (docs/palw-pcpb-leaf-v2-wiring-design.md §1) — the PCPB leaf commitments.
    /// The challenge commitment / challenge epoch are NOT new fields: `receipt_v3_job_challenge` /
    /// `receipt_v3_issued_epoch` carry them (Seam 1's `derive_job_challenge` already binds the
    /// job-scoped triple). All five below ride `leaf_hash → leaf_root → content_id() == batch_id`,
    /// so they are sealed by the same M2/item-7 discipline as `registered_epoch`.
    ///
    /// Self branch (`dispatch_kind == PALW_DISPATCH_KIND_SELF_SERIAL`): A's escrow-locked receipt
    /// commitment. External branch: the all-zero sentinel.
    pub a_commit: Hash64,
    /// Self branch: the epoch the `PalwACommitV1` anchor was ACCEPTED on-chain (clause 12 checks
    /// equality against the registry — both directions of epoch-grinding die on that equality).
    /// External branch: 0 sentinel.
    pub a_commit_epoch: u64,
    /// The bond-weighted provider snapshot root of epoch `anchor − k` (anchor = `a_commit_epoch`
    /// for self, `receipt_v3_issued_epoch` for external). Clause 0 of the dispatch-evidence
    /// verifier requires equality with the independently store-resolved root.
    pub provider_snapshot_root: Hash64,
    /// The assignment-tree root deterministically derived from that snapshot (provider-id sort →
    /// cumulative bond intervals). Same clause-0 equality.
    pub assignment_proof_root: Hash64,
    /// [`PALW_DISPATCH_KIND_BEACON_ASSIGNED`] or [`PALW_DISPATCH_KIND_SELF_SERIAL`]. Committed on
    /// the leaf so the chunk-side evidence branch cannot be swapped after the fact.
    pub dispatch_kind: u8,
}

impl PalwPublicLeafV1 {
    /// `leaf_hash` = keyed hash of the full leaf descriptor. Self-contained (the leaf holds no
    /// self-reference), so the borsh encoding is a faithful preimage.
    pub fn leaf_hash(&self) -> Hash64 {
        blake2b_512_keyed(PALW_LEAF_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }

    pub fn proof_type(&self) -> Option<PalwProofType> {
        PalwProofType::from_u8(self.proof_type)
    }
}

/// Batch manifest — fixes the leaf/chunk counts and roots before the beacon (design §9.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBatchManifestV1 {
    pub version: u16,
    pub batch_id: Hash64,
    pub registration_epoch: u64,
    pub model_profile_id: Hash64,
    pub runtime_class_id: Hash64,
    pub leaf_count: u32,
    pub chunk_count: u16,
    pub leaf_root: Hash64,
    pub descriptor_root: Hash64,
    pub total_leaf_bond_sompi: u64,
    pub audit_policy_id: Hash64,
    pub activation_not_before_epoch: u64,
    pub expiry_epoch: u64,
}

impl PalwBatchManifestV1 {
    /// ADR-0039 §9.2 — the batch's CONTENT identity: a keyed hash of the manifest with its
    /// (self-referential) `batch_id` field zeroed. A valid manifest must satisfy `batch_id ==
    /// content_id()` (see [`Self::batch_id_is_content_derived`]); this makes `batch_id` a collision-
    /// resistant content address, so `(batch_id, leaf_index)` / `batch_id` store keys are fork-safe
    /// (C4 design panel: without this, `batch_id` is an attacker-chosen FIELD and two forks can register
    /// different manifests under one key, last-writer-wins).
    pub fn content_id(&self) -> Hash64 {
        let mut canonical = self.clone();
        canonical.batch_id = Hash64::default();
        blake2b_512_keyed(PALW_BATCH_ID_DOMAIN, &borsh::to_vec(&canonical).expect("borsh"))
    }

    /// True iff `batch_id` equals the content id (the content-addressing invariant).
    #[inline]
    pub fn batch_id_is_content_derived(&self) -> bool {
        self.batch_id == self.content_id()
    }

    /// ADR-0039 §9.2/§9.3 — the pure admission predicate for a manifest accepted at `accept_epoch`. C4
    /// panel: `apply_palw_overlay_effect`'s Manifest arm currently does ZERO validation, so a manifest
    /// with `expiry_epoch = u64::MAX` (or a forged `batch_id`) is admissible and pins its view entry
    /// forever. This bounds every window and content-addresses the batch. `chunk_span(leaf_count,
    /// max_chunk)` = the exact number of chunks the leaves require.
    #[allow(clippy::too_many_arguments)]
    pub fn admission_valid(
        &self,
        accept_epoch: u64,
        max_batch_leaves: u32,
        max_leaf_chunk_leaves: u16,
        registration_lead_epochs: u64,
        active_window_epochs: u64,
        audit_window_epochs: u64,
        min_leaf_bond_sompi: u64,
    ) -> bool {
        if self.version != 1 || !self.batch_id_is_content_derived() {
            return false;
        }
        if self.leaf_count == 0 || self.leaf_count > max_batch_leaves || max_leaf_chunk_leaves == 0 {
            return false;
        }
        // ADR-0040 P1-11 (AO-02): `chunks_present` is a FIXED 256-bit bitmap, so a parameter set whose
        // chunk count could exceed 256 would index out of bounds in `apply_leaf_chunk`. Unreachable at
        // the shipped params (256 leaves / 64 per chunk ⇒ 4 chunks), but "unreachable at today's
        // parameters" is not the same as "impossible" — bound it structurally here so a future params
        // change cannot silently reintroduce a panic on an attacker-supplied index.
        if self.chunk_count as usize > PALW_CHUNK_BITMAP_BITS {
            return false;
        }
        // §7 economic floor (R3 c_saved calibration): the aggregate bond must cover the per-leaf floor for
        // every leaf. Without this, a batch can register `leaf_count` leaves against a token bond, and the
        // forgery-EV inequality — which must dominate `R + c_saved`, not just `R` — never holds. Aggregate
        // (not per-leaf) is checked here because the manifest fixes only the total before the leaf chunks
        // arrive; the per-leaf split is enforced where leaves are admitted.
        if self.total_leaf_bond_sompi < (self.leaf_count as u64).saturating_mul(min_leaf_bond_sompi) {
            return false;
        }
        // chunk_count must be exactly ceil(leaf_count / max_leaf_chunk_leaves) — no hidden/padded chunks.
        let expected_chunks = self.leaf_count.div_ceil(max_leaf_chunk_leaves as u32);
        if self.chunk_count as u32 != expected_chunks {
            return false;
        }
        // §11.2.1-style phase freeze: registration epoch is the acceptance epoch (miner cannot re-aim).
        if self.registration_epoch != accept_epoch {
            return false;
        }
        // Activation is at/after registration + the mandatory lead; the active window is bounded (so no
        // `expiry_epoch = u64::MAX` pins the batch view forever).
        let min_activation = self.registration_epoch.saturating_add(registration_lead_epochs).saturating_add(audit_window_epochs);
        if self.activation_not_before_epoch < min_activation {
            return false;
        }
        // kaspa-pq **ADR-0040 P1-11 (DOS-04)** — bound activation from ABOVE as well.
        //
        // Bounding `expiry_epoch` relative to `activation_not_before_epoch` (below) is not sufficient on
        // its own: with activation itself unbounded, a manifest could name `activation_not_before_epoch =
        // 10^18` and a correspondingly distant expiry, and `palw_batch_referenceable` would then have to
        // RETAIN that entry — for practical purposes forever. Because `PalwBatchViewV1` is cloned and
        // re-persisted on every block, a flood of such manifests is permanent, per-block amplified state
        // that only transaction fees rate-limit.
        //
        // The scheduling slack is one lead window: a batch may be aimed at the earliest legal epoch or up
        // to `registration_lead_epochs` beyond it, which is all a producer needs to line a batch up with a
        // future audit round. Anything further ahead is indistinguishable from pinning.
        let max_activation = min_activation.saturating_add(registration_lead_epochs);
        if self.activation_not_before_epoch > max_activation {
            return false;
        }
        self.expiry_epoch > self.activation_not_before_epoch
            && self.expiry_epoch <= self.activation_not_before_epoch.saturating_add(active_window_epochs)
    }
}

/// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2) — a leaf's membership proof against
/// `manifest.leaf_root`: the `d` sibling digests, bottom-up.
///
/// Sibling proof only. Direction bits are derived from the leaf's own `leaf_index`
/// (see [`palw_verify_leaf_membership`]) and are deliberately NOT carried on the wire — a
/// path-direction field would be attacker-chosen data that widens the set of accepted folds for a
/// given leaf. Nothing here is free: index comes from the leaf, count comes from the manifest.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwLeafMembershipProofV1 {
    pub siblings: Vec<Hash64>,
}

impl PalwLeafMembershipProofV1 {
    #[inline]
    pub fn len(&self) -> usize {
        self.siblings.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.siblings.is_empty()
    }
}

/// A chunk of ≤ [`PALW_MAX_LEAVES_PER_CHUNK`] public leaves (design §9.3).
///
/// kaspa-pq ADR-0040 §5.15 — **v2**: `proofs[i]` is the membership proof of `leaves[i]` against the
/// batch manifest's `leaf_root`. v1 (no proofs) is REJECTED outright rather than parsed leniently with
/// an empty `proofs`: a lenient parse would reopen the whole CHUNK-INDEX SQUAT hole, since the
/// acceptance gate would then have nothing to check.
///
/// ADR-0045 D3-b — **v3**: `witnesses[i]` additionally carries leaf `i`'s PCPB evidence (challenge
/// preimage + dispatch evidence), verified by the acceptance arm's clauses 11/12 BEFORE `insert_leaf`.
/// The same lenient-parse rule applies one version up: v2 (no witnesses) is rejected outright, or the
/// PCPB gate would have nothing to check — the exact partial-gating state D3-b forbids.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwLeafChunkV1 {
    pub version: u16,
    pub batch_id: Hash64,
    pub chunk_index: u16,
    pub leaves: Vec<PalwPublicLeafV1>,
    /// Index-aligned with `leaves` (both are ordered by strictly increasing `leaf_index`).
    pub proofs: Vec<PalwLeafMembershipProofV1>,
    /// Index-aligned with `leaves` — the per-leaf PCPB evidence (v3).
    pub witnesses: Vec<PalwLeafPcpbWitnessV1>,
}

/// ADR-0045 D3-b (design memo §5) — one leaf's PCPB evidence as carried by a v3 chunk. WIRE ONLY:
/// chunks are transaction payloads, never bincode-persisted, so this rides no DB format pin. The
/// challenge triple re-derives `receipt_v3_job_challenge` under `R_{issued_epoch}` (clause 11's heavy
/// half — without it `issued_epoch` is a free declaration and freshness is hollow); `dispatch` is the
/// clause-12 evidence re-run against the store-resolved snapshot + post-commit beacon.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwLeafPcpbWitnessV1 {
    pub scheduler_job_id: Hash64,
    pub requester_credential: Hash64,
    pub request_commitment: Hash64,
    pub dispatch: PalwDispatchEvidence,
}

/// Chunk-size cap. **v3 (D3-b) lowered this 64 → 24**: a worst-case per-leaf SelfSerial witness is
/// ≈ 13 KiB (ML-DSA-87 pk 2592 B + sig 4627 B + receipt preimage + two memberships), and 24 leaves
/// keep the fullest possible chunk comfortably inside [`PALW_MAX_OVERLAY_PAYLOAD_BYTES`] — asserted
/// by an exact-encode test, not estimated. Manifest chunk arithmetic (`expected_chunks`) follows this
/// constant, so the cut is a re-genesis format change riding the same cutover as the leaf fields.
pub const PALW_MAX_LEAVES_PER_CHUNK: usize = 24;

/// ADR-0045 D3-b (design memo §2.1) — the PCPB self-serial ordering anchor (subnetwork `0x45`,
/// [`crate::subnets::SUBNETWORK_ID_PALW_ACOMMIT`]). Registers `a_commit` on-chain so the draw beacon
/// `R_{a_commit_epoch + Δ}` provably post-dates it; clause 12 requires the registry's accepted epoch to
/// EQUAL the leaf's declared `a_commit_epoch` (both directions of epoch-grinding die on that equality).
///
/// Deliberately unsigned and content-keyed: WHO registered is meaningless (front-running a mempool
/// anchor merely pays the victim's fee — the M2 content argument), WHEN is everything, and the "when"
/// is the ACCEPTING block's consensus-derived epoch, never a declared field. Spam is priced by
/// ordinary tx fees and swept by the registry's retention window.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwACommitV1 {
    pub version: u16,
    pub a_commit: Hash64,
}

/// Context-free validation of a `0x45` A-commit anchor payload: strict decode, v1 version, non-zero
/// commitment (the self arm rejects a zero `a_commit` at clause 12, so a zero anchor could never be
/// used — refuse it at the door instead of storing a dead row).
pub fn validate_palw_acommit_tx(payload: &[u8]) -> Result<(), PalwTxError> {
    if payload.len() > PALW_MAX_OVERLAY_PAYLOAD_BYTES {
        return Err(PalwTxError::PayloadTooLarge { len: payload.len(), max: PALW_MAX_OVERLAY_PAYLOAD_BYTES });
    }
    let anchor: PalwACommitV1 = decode_palw_payload(payload)?;
    check_palw_version(anchor.version)?;
    if anchor.a_commit == Hash64::default() {
        return Err(PalwTxError::InvalidField("acommit.a_commit"));
    }
    Ok(())
}

/// ADR-0040 P1-11 (AO-02) — the width of `PalwBatchLifecycleV1::chunks_present` in bits. A batch's
/// `chunk_count` must not exceed it, or `apply_leaf_chunk` would index outside the fixed `[u64; 4]`.
pub const PALW_CHUNK_BITMAP_BITS: usize = 256;

// =============================================================================================
// Certificate + auditor vote (design §10.1, §24.4).
// =============================================================================================

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwAuditorVoteV2 {
    pub bond_outpoint: TransactionOutpoint,
    /// 1 = pass, 0 = reject.
    pub vote: u8,
    pub checked_leaf_bitmap_root: Hash64,
    /// Certificate-wide count independently approved by this auditor.
    pub passed_leaf_count: u32,
    /// Certificate-wide rejected-leaf bitmap commitment independently approved by this auditor.
    pub rejected_leaf_bitmap_root: Hash64,
    /// ML-DSA-87 signature (verification in the audit slice).
    pub signature: Vec<u8>,
}

impl PalwAuditorVoteV2 {
    /// ADR-0039 §10.1 / I-14 — the message an auditor signs. It binds the vote to the certificate's
    /// batch + audit-beacon epoch + **`audit_sample_root`** (the beacon-selected receipt-chunk
    /// commitment) + the auditor's identity + which leaves it checked + the exact certificate-level
    /// pass/reject summary. The `signature` field itself is excluded (it covers this digest).
    ///
    /// ADR-0040 SAMPLE-01 boundary. `verify_certificate_attestation` independently
    /// re-derives `audit_sample_root` from the beacon-selected leaves' on-chain DA commitments and
    /// verifies each selected auditor's ML-DSA-87 signature over that root. A producer therefore cannot
    /// substitute an arbitrary root while retaining valid votes.
    ///
    /// This closes the enforceable commitment-coverage property, not I-14's stronger off-chain data
    /// possession claim: consensus still cannot infer that an auditor fetched the receipt chunks merely
    /// from a valid signature over their committed roots. Receipt DA/challenge evidence remains an
    /// activation blocker for a public, valuable network.
    pub fn signing_hash(&self, network_id: u32, batch_id: &Hash64, audit_beacon_epoch: u64, audit_sample_root: &Hash64) -> Hash64 {
        let mut p = Vec::with_capacity(5 * HASH64_SIZE + 4 + 8 + 4 + 1 + 4);
        p.extend_from_slice(&network_id.to_le_bytes());
        push_hash(&mut p, batch_id);
        p.extend_from_slice(&audit_beacon_epoch.to_le_bytes());
        push_hash(&mut p, audit_sample_root);
        push_hash(&mut p, &self.bond_outpoint.transaction_id);
        p.extend_from_slice(&self.bond_outpoint.index.to_le_bytes());
        p.push(self.vote);
        push_hash(&mut p, &self.checked_leaf_bitmap_root);
        p.extend_from_slice(&self.passed_leaf_count.to_le_bytes());
        push_hash(&mut p, &self.rejected_leaf_bitmap_root);
        blake2b_512_keyed(PALW_AUDITOR_VOTE_V2_DOMAIN, &p)
    }
}

// =============================================================================================
// R4 — mismatch attribution (anti-griefing, design §24.5).
//
// The k=2 replica rule credits a leaf only when both replicas agree. Non-agreement alone is
// therefore a griefing vector: a malicious provider paired with an honest one can deliberately
// emit a wrong output so NEITHER is credited, burning the honest partner's real GPU work at no
// cost to itself. The v0.1 design "just doesn't credit the winner" — which punishes the victim as
// hard as the attacker. R4 makes non-agreement ATTRIBUTABLE: a deterministic fraction of mismatches
// (plus every repeat-offender bond) is escalated to a reference-runtime re-run, and the bond of the
// party whose committed output deviates from the reference result is slashed. The honest partner,
// whose output matches the reference, keeps its bond. This is a pure decision layer — the escalation
// draw, the attribution verdict, and the slash-target set. The re-run itself and the per-provider
// mismatch counter are off-protocol inputs (design §24.5); consensus only checks the verdict.
// =============================================================================================

/// A committed non-agreement between the two replicas of one leaf (design §24.5). `output_a` /
/// `output_b` are the providers' leaf-committed output hashes; a record is only meaningful when they
/// differ (that IS the mismatch).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwMismatchRecordV1 {
    pub batch_id: Hash64,
    pub leaf_index: u32,
    pub provider_a: TransactionOutpoint,
    pub provider_b: TransactionOutpoint,
    pub output_a: Hash64,
    pub output_b: Hash64,
}

/// The attribution verdict after a reference-runtime re-run resolves a mismatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwMismatchVerdict {
    /// `output_a` deviates from the reference result — slash `provider_a`.
    SlashA,
    /// `output_b` deviates — slash `provider_b`.
    SlashB,
    /// Neither committed output matches the reference — both deviated; slash both.
    SlashBoth,
    /// Not actually a mismatch (`output_a == output_b`); nothing to attribute.
    NotAMismatch,
}

/// R4 parameters (design §24.5). Inert placeholder escalates nothing (`0` ppm, threshold `0` disables
/// the repeat-offender path) so activation is byte-neutral; calibrated at re-genesis to the measured
/// collusion cost (see the ADR c_saved / anti-griefing calibration note).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwMismatchParams {
    /// Baseline fraction of mismatches escalated to a reference re-run, in parts-per-million.
    pub escalation_rate_ppm: u32,
    /// A provider bond that has accrued at least this many prior mismatches is escalated
    /// unconditionally (0 disables the repeat-offender path).
    pub repeat_offender_threshold: u32,
}

impl PalwMismatchParams {
    pub const INERT: PalwMismatchParams = PalwMismatchParams { escalation_rate_ppm: 0, repeat_offender_threshold: 0 };
}

impl PalwMismatchRecordV1 {
    /// The deterministic escalation draw for this mismatch under `audit_beacon_seed`. Every node
    /// derives the same value, so escalation cannot be steered by a provider. Returns a value in
    /// `[0, 1_000_000)`; the mismatch is baseline-escalated iff it is `< escalation_rate_ppm`.
    pub fn escalation_draw(&self, audit_beacon_seed: &Hash64) -> u32 {
        let mut p = Vec::with_capacity(HASH64_SIZE + HASH64_SIZE + 4);
        push_hash(&mut p, audit_beacon_seed);
        push_hash(&mut p, &self.batch_id);
        p.extend_from_slice(&self.leaf_index.to_le_bytes());
        let d = blake2b_512_keyed(PALW_MISMATCH_ESCALATE_DOMAIN, &p);
        let b = d.as_byte_slice();
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]) % 1_000_000
    }

    /// True iff this mismatch is escalated to a reference-runtime re-run: either the deterministic
    /// baseline draw hits, or one of the two providers is at/over the repeat-offender threshold. The
    /// per-provider mismatch counts are supplied by the off-protocol tracker (design §24.5).
    pub fn is_escalated(
        &self,
        audit_beacon_seed: &Hash64,
        params: &PalwMismatchParams,
        prior_mismatches_a: u32,
        prior_mismatches_b: u32,
    ) -> bool {
        if self.output_a == self.output_b {
            return false; // not a mismatch — nothing to escalate
        }
        if self.escalation_draw(audit_beacon_seed) < params.escalation_rate_ppm {
            return true;
        }
        let repeat = params.repeat_offender_threshold;
        repeat != 0 && (prior_mismatches_a >= repeat || prior_mismatches_b >= repeat)
    }

    /// Attribute the mismatch given the reference runtime's authoritative output hash. The party whose
    /// committed output differs from the reference is the deviator. Because `output_a != output_b`, at
    /// most one can match the reference — so the honest partner is never slashed.
    pub fn attribute(&self, reference_output: &Hash64) -> PalwMismatchVerdict {
        if self.output_a == self.output_b {
            return PalwMismatchVerdict::NotAMismatch;
        }
        let a_ok = self.output_a == *reference_output;
        let b_ok = self.output_b == *reference_output;
        match (a_ok, b_ok) {
            (true, false) => PalwMismatchVerdict::SlashB,
            (false, true) => PalwMismatchVerdict::SlashA,
            _ => PalwMismatchVerdict::SlashBoth, // neither matches (both cannot, since a != b)
        }
    }

    /// The bond outpoints to slash for a verdict — the deterministic input to the slash slice.
    pub fn slash_targets(&self, verdict: PalwMismatchVerdict) -> Vec<TransactionOutpoint> {
        match verdict {
            PalwMismatchVerdict::SlashA => vec![self.provider_a],
            PalwMismatchVerdict::SlashB => vec![self.provider_b],
            PalwMismatchVerdict::SlashBoth => vec![self.provider_a, self.provider_b],
            PalwMismatchVerdict::NotAMismatch => vec![],
        }
    }
}

/// Certificate attesting a DNS-selected auditor quorum confirmed the batch facts (design §10.1).
/// It is **not** a proof of inference; it is a set of attested facts (I-10 / design §1.2).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBatchCertificateV2 {
    pub version: u16,
    pub batch_id: Hash64,
    pub manifest_hash: Hash64,
    pub leaf_root: Hash64,
    pub audit_beacon_epoch: u64,
    pub audit_sample_root: Hash64,
    /// Certificate summary signed identically by every embedded V2 auditor vote.
    pub passed_leaf_count: u32,
    /// Certificate summary signed identically by every embedded V2 auditor vote.
    pub rejected_leaf_bitmap_root: Hash64,
    pub certificate_epoch: u64,
    pub activation_epoch: u64,
    pub expiry_epoch: u64,
    pub auditor_set_commitment: Hash64,
    /// kaspa-pq **ADR-0040 §12′** — the stake-weighted PASS tally this certificate claims.
    ///
    /// DECLARED here rather than derived at every read site, because the supersession comparator must be
    /// evaluable wherever a certificate is (including the body-stage view builder, which has no bond
    /// view). It is not trusted: `verify_certificate_attestation` recomputes the tally from the active
    /// bond view and REJECTS any certificate whose declared value disagrees, so the field is a
    /// commitment, not an input. It is covered by [`Self::hash`] (which hashes the whole borsh
    /// encoding), so two certificates differing only in this field are different objects.
    pub approving_stake: u128,
    pub votes: Vec<PalwAuditorVoteV2>,
}

impl PalwBatchCertificateV2 {
    /// Cached-lookup key: the hash a Header v3 references as `palw_epoch_certificate_hash`.
    pub fn hash(&self) -> Hash64 {
        blake2b_512_keyed(PALW_LEAF_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }

    /// True iff `activation_epoch <= epoch < expiry_epoch` (design §14.2 `is_active_at`).
    #[inline]
    pub fn is_active_at(&self, epoch: u64) -> bool {
        self.activation_epoch <= epoch && epoch < self.expiry_epoch
    }

    /// ADR-0039 §10.1/§10.2 — the stake-weighted PASS tally over the certificate's votes. `stake_of`
    /// resolves an auditor bond outpoint to its stake in the audit-epoch DNS bond view (0 if the bond
    /// is not an eligible auditor). Only `vote == 1` (pass) counts. Deterministic: every node computes
    /// the same tally from the same bond view.
    pub fn pass_stake(&self, stake_of: impl Fn(&TransactionOutpoint) -> u128) -> u128 {
        self.votes.iter().filter(|v| v.vote == 1).map(|v| stake_of(&v.bond_outpoint)).sum()
    }

    /// ADR-0039 §10.2 — the certificate is quorum-valid iff the stake-weighted PASS tally reaches the
    /// `num/den` fraction of the total eligible auditor stake (testnet 2/3). `den` must be > 0; ties go
    /// to reached (>=). This is the check every node runs at batch activation before caching the
    /// certificate hash.
    ///
    /// ADR-0040 P0-5 — the two vacuity guards. Without them the cross-multiplied comparison degenerates
    /// to `0 >= 0`, i.e. **every** certificate reaches quorum:
    ///   * `total_auditor_stake == 0` (no eligible auditor stake at all) — the RHS is 0, so a certificate
    ///     with zero PASS votes passes. This is the same class of bug the sibling
    ///     [`beacon_quorum_reached`] already guards (its `committed_stake == 0` early-out); it was simply
    ///     never ported here.
    ///   * `num == 0` (a degenerate "0/den" threshold) — the RHS is 0 for the same reason, so any
    ///     misconfigured or attacker-influenced threshold of zero admits everything. `beacon_quorum_reached`
    ///     does **not** guard this either; ADR-0040 QUORUM-02 fixes both call sites together.
    /// A zero threshold is never a legitimate configuration, so both are fail-closed rejections rather
    /// than saturating arithmetic.
    pub fn quorum_reached(
        &self,
        total_auditor_stake: u128,
        num: u16,
        den: u16,
        stake_of: impl Fn(&TransactionOutpoint) -> u128,
    ) -> bool {
        if den == 0 || num == 0 || total_auditor_stake == 0 {
            return false;
        }
        // pass_stake * den >= total * num  (cross-multiplied to avoid fractional rounding).
        self.pass_stake(stake_of).saturating_mul(den as u128) >= total_auditor_stake.saturating_mul(num as u128)
    }
}

// =============================================================================================
// Provider bond, block authorization, beacon, revocation (design §24.3, §12.4, §11.2, §9.5).
// =============================================================================================

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwProviderBondPayloadV1 {
    pub version: u16,
    /// ML-DSA-87 public key.
    pub owner_public_key: Vec<u8>,
    pub operator_group_id: Hash64,
    pub runtime_classes: Vec<Hash64>,
    pub capacity_by_shape: Vec<(u16, u32)>,
    pub reward_key_root: Hash64,
    pub amount_sompi: u64,
    pub unbond_delay_epochs: u64,
}

/// Authorized provider-bond exit on subnetwork `0x37`
/// (`SUBNETWORK_ID_PALW_PROVIDER_UNBOND`).
///
/// # Lock release
///
/// The value lock ([`validate_provider_bond_tx`]) plus the spend gate make a provider's output-0
/// unspendable for the life of the bond. This request provides the corresponding release path.
///
/// # Authorization
///
/// The signature under [`PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT`] binds the network and bond outpoint,
/// preventing third-party unbond requests and cross-bond or cross-network replay.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwProviderUnbondRequestV1 {
    pub version: u16,
    /// The bond being released — its creating tx's output-0 outpoint.
    pub bond_outpoint: TransactionOutpoint,
    /// The bond owner's 2592-byte ML-DSA-87 public key. Bound to the bond at acceptance:
    /// `validator_id_from_pubkey(owner_public_key) == record.owner_pubkey_hash`.
    pub owner_public_key: Vec<u8>,
    /// ML-DSA-87 signature over [`Self::signing_hash`].
    pub signature: Vec<u8>,
}

impl PalwProviderUnbondRequestV1 {
    /// The digest the bond owner signs. Excludes `signature`; INCLUDES `owner_public_key` so a
    /// signature cannot be re-presented under a substituted key, and `network_id` + `bond_outpoint`
    /// so it is neither cross-network nor cross-bond replayable.
    pub fn signing_hash(&self, network_id: u32) -> Hash64 {
        let mut p = Vec::with_capacity(4 + 2 + HASH64_SIZE + 4 + self.owner_public_key.len() + 8);
        p.extend_from_slice(&network_id.to_le_bytes());
        p.extend_from_slice(&self.version.to_le_bytes());
        push_hash(&mut p, &self.bond_outpoint.transaction_id);
        p.extend_from_slice(&self.bond_outpoint.index.to_le_bytes());
        push_var(&mut p, &self.owner_public_key);
        blake2b_512_keyed(PALW_PROVIDER_UNBOND_DOMAIN, &p)
    }
}

// =============================================================================================
// ADR-0040 ECON-03 legs 2 & 3 — the provider-bond RECORD and the point-of-view READ.
//
// Transposed from the DNS stake bond (`StakeBondRecord`, `bond_mutations_from_accepted_txs`,
// `effective_bond_status`, `ActiveBondView` in `dns_finality`). Inventing a second, different bond
// mechanism in the same codebase would itself be a defect, so the shape is deliberately identical
// and the two can be diffed against each other.
// =============================================================================================

/// Status of a PALW provider bond. Mirrors [`crate::dns_finality::BondStatus`] minus `Dormant`
/// (the DNS dormancy fence is an attestation-liveness mechanism with no PALW analogue).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwProviderBondStatus {
    /// Committed, but `activation_daa_score` not yet reached.
    #[default]
    Pending = 0,
    /// Active collateral — the only status that backs a provider reward.
    Active = 1,
    /// Owner submitted an authorized unbond request; released after the delay.
    Unbonding = 2,
    /// Slashed; principal forfeit.
    Slashed = 3,
}

/// kaspa-pq **ADR-0040 (ECON-03, leg 2) — the registry entry derived from an ACCEPTED provider-bond
/// transaction.** Persisted in the `PalwProviderBond` store (prefix 241) keyed by `bond_outpoint`.
///
/// Every field here is either derived from the chain (`activation_daa_score`, `created_daa_score`) or
/// carried from a payload whose `amount_sompi` was already cross-checked against locked coins by
/// [`validate_provider_bond_tx`]. That ordering is the point: merely storing a self-declared payload
/// would create a NUMBER IN A DATABASE — still attacker-chosen, still unbacked, but now read by
/// consensus as if it meant something, which is strictly worse than discarding it. The record is
/// trustworthy only because the value lock ran first, at a coordinate where rejection is loud.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwProviderBondRecord {
    pub version: u16,
    /// Identifies the bond uniquely: output-0 of its creating transaction — the very output the
    /// value lock pinned and the spend gate keeps unspent.
    pub bond_outpoint: TransactionOutpoint,
    /// `validator_id_from_pubkey(owner_public_key)` — the identity the unbond authorizer checks.
    pub owner_pubkey_hash: Hash64,
    /// Retained so the unbond signature can be verified without re-reading the creating tx.
    pub owner_public_key: Vec<u8>,
    pub operator_group_id: Hash64,
    pub runtime_classes: Vec<Hash64>,
    pub capacity_by_shape: Vec<(u16, u32)>,
    pub reward_key_root: Hash64,
    /// The bond's collateral, in sompi. REAL: equal to the value of the locked output-0.
    pub amount_sompi: u64,
    /// The payload declares no activation, so this is simply the acceptance DAA — non-retroactivity
    /// is free here, where the DNS bond needed an explicit forward clamp against a declared field.
    pub activation_daa_score: u64,
    pub created_daa_score: u64,
    /// Clamped UP to the network floor, so an operator cannot shorten its own exit lock (and thus
    /// its slashable window) by declaring a tiny delay.
    pub unbond_delay_epochs: u64,
    pub unbond_request_daa_score: Option<u64>,
    pub slashed_at_daa_score: Option<u64>,
    // DELIBERATE DEVIATION FROM THE DNS PRECEDENT — there is NO mutable `status` field here, and the
    // omission is load-bearing rather than cosmetic.
    //
    // `StakeBondRecord` carries one, so its apply/revert path must re-derive that cache from the
    // authoritative DAA stamps after every mutation. The PALW record avoids the redundant cache
    // entirely, reducing the state that a registry writer must keep synchronized.
    //
    // Rather than transpose a redundant mutable answer into a new mechanism, the field is dropped.
    // Status is ALWAYS derived via `effective_provider_bond_status(record, pov)`, which is a pure
    // function of DAA stamps, so `apply`/`revert` become exact inverses by construction and there is
    // no second, staler answer to the status question for a caller to reach for by accident.
    // `econ03_view_apply_and_revert_are_exact_inverses` is the test that fails if this is reinstated.
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwProviderBondRecord {}

/// One mutation to the provider-bond registry, derived from an accepted transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwProviderBondMutation {
    Insert(TransactionOutpoint, PalwProviderBondRecord),
    Unbond(TransactionOutpoint, u64),
    Slash(TransactionOutpoint, u64),
}

/// kaspa-pq **ADR-0040 (ECON-03, leg 3) — the status READ, and the reason it is order-independent.**
///
/// Status is DERIVED from DAA stamps by a pure DAA-vs-DAA comparison chain, never read from the
/// mutable cached `status` field. That is the whole order-independence argument at
/// this level: the same `(record, pov)` yields the same answer on every node regardless of what order
/// branches were processed, whereas a stored flag diverges across reorg paths.
///
/// Precedence mirrors [`crate::dns_finality::effective_bond_status`]: slashed → unbonding →
/// activation.
pub fn effective_provider_bond_status(record: &PalwProviderBondRecord, pov_daa_score: u64) -> PalwProviderBondStatus {
    if record.slashed_at_daa_score.is_some_and(|s| pov_daa_score >= s) {
        return PalwProviderBondStatus::Slashed;
    }
    if record.unbond_request_daa_score.is_some_and(|u| pov_daa_score >= u) {
        return PalwProviderBondStatus::Unbonding;
    }
    if pov_daa_score >= record.activation_daa_score { PalwProviderBondStatus::Active } else { PalwProviderBondStatus::Pending }
}

/// `true` iff the bond is `Active` at `pov_daa_score` — the eligibility predicate any consumer of
/// provider collateral must apply.
pub fn is_provider_bond_active_at(record: &PalwProviderBondRecord, pov_daa_score: u64) -> bool {
    effective_provider_bond_status(record, pov_daa_score) == PalwProviderBondStatus::Active
}

/// kaspa-pq **ADR-0040 (ECON-03, leg 5) — when a requested exit actually releases.**
///
/// `None` until an authorized unbond request has been recorded. Mirrors
/// [`crate::dns_finality::bond_release_daa_score`], with one difference forced by the payload: the DNS
/// bond stores its delay in BLOCKS, whereas [`PalwProviderBondPayloadV1::unbond_delay_epochs`] is in
/// PALW epochs, so the caller supplies `epoch_length_daa` (`Params::palw_epoch_length_daa`, 100 on
/// every preset) to reach the same clock the DAA stamps are on.
///
/// The delay used is [`PalwProviderBondRecord::unbond_delay_epochs`], which acceptance already clamped
/// UP to the network floor. Reading the clamped field rather than the payload is the entire point: an
/// operator that declares a one-epoch delay must still serve the floor, because the exit delay IS the
/// slashable window and a self-shortened window is a self-granted immunity.
///
/// `saturating_add`/`saturating_mul`: a pathological delay or request height saturates at `u64::MAX`
/// (never releasable) rather than wrapping to an early — i.e. immediate — release.
pub fn provider_bond_release_daa_score(record: &PalwProviderBondRecord, epoch_length_daa: u64) -> Option<u64> {
    record.unbond_request_daa_score.map(|u| u.saturating_add(record.unbond_delay_epochs.saturating_mul(epoch_length_daa.max(1))))
}

/// `true` iff the bond has requested an exit AND the clamped delay has elapsed at `pov_daa_score`.
///
/// This is the predicate the leg-4 spend gate must consult: it is the ONLY condition under which
/// output-0 may leave the UTXO set. Both halves are required — `Unbonding` alone means the request
/// was made, not that the window has passed.
pub fn is_provider_bond_releasable_at(record: &PalwProviderBondRecord, pov_daa_score: u64, epoch_length_daa: u64) -> bool {
    effective_provider_bond_status(record, pov_daa_score) == PalwProviderBondStatus::Unbonding
        && provider_bond_release_daa_score(record, epoch_length_daa).is_some_and(|release| pov_daa_score >= release)
}

/// kaspa-pq **ADR-0040 (ECON-03, leg 2)** — map accepted transactions to registry mutations.
///
/// `min_provider_bond_sompi` is the SEL-01 anti-split floor. Without a floor, splitting a bond is free,
/// and a free split is a Sybil hole regardless of how selection later weights it.
///
/// Provider-bond floor enforcement. This function has a production caller:
/// `stage_palw_provider_bond_mutations` (virtual_processor/processor.rs) drives it from each
/// selected-chain block's accepted transactions to write the registry at prefix 241, and
/// `palw_provider_bond_mutations_for_chain_block` re-derives it for the reorg revert. A sub-floor bond
/// is DROPPED here, so it never enters the registry, never resolves `Active`, and therefore — via
/// `palw_work_reward_class`'s ECON-03 collateral-resolution rule — can never back a paid leaf.
///
/// Limitation: a sub-floor `ProviderBond`
/// transaction remains VALID ON-CHAIN. The isolation validator (`validate_provider_bond_tx`) enforces
/// the value lock and `amount_sompi != 0`, not the floor, because the floor is a network parameter and
/// isolation validity must not depend on one. Such a transaction is simply economically inert: it locks
/// the owner's own coins to the owner's own script and buys nothing.
///
/// `PalwBatchAdmissionParams::is_consistent_for_activation` rejects a zero floor, and every activated
/// preset asserts that invariant.
///
/// `unbond_floor_epochs` clamps the operator-declared delay UP, for the same reason the DNS bond
/// clamps `unbonding_period_blocks`.
///
/// The caller decides which txs count as accepted, so this stays unit-testable. It is a function of
/// the chain (that block's acceptance data and its own header DAA) and never of arrival order.
/// ADR-0045 D3-b — the A-commit anchors accepted by one chain block's transactions (subnetwork
/// `0x45`). Strict-decoded and shape-checked here as well as at isolation: an anchor that fails the
/// shape rule is a fee-paying no-op, never a registry row (the beacon-op semantics). Deduplicated
/// first-only so the registry writer has one mutation per anchor to reconcile.
pub fn palw_acommit_anchors_from_accepted_txs(txs: &[crate::tx::Transaction]) -> Vec<Hash64> {
    let mut anchors = Vec::new();
    let mut seen = HashSet::new();
    for tx in txs {
        if tx.subnetwork_id.palw_pcpb_tx_kind().is_none() {
            continue;
        }
        if validate_palw_acommit_tx(&tx.payload).is_err() {
            continue;
        }
        if let Ok(anchor) = borsh::from_slice::<PalwACommitV1>(&tx.payload)
            && seen.insert(anchor.a_commit)
        {
            anchors.push(anchor.a_commit);
        }
    }
    anchors
}

pub fn palw_provider_bond_mutations_from_accepted_txs(
    txs: &[crate::tx::Transaction],
    accepted_daa_score: u64,
    min_provider_bond_sompi: u64,
    unbond_floor_epochs: u64,
) -> Vec<PalwProviderBondMutation> {
    let mut muts = Vec::new();
    // More than one independently valid request for the same bond can be accepted in one
    // mergeset because authorization is deliberately evaluated against the common selected-parent
    // view. They all carry this chain block's same `accepted_daa_score`, so only the first request
    // has a distinct registry effect. Canonical accepted-tx order makes first-only dedup
    // deterministic and, critically, gives the persisted writer one mutation to invert on reorg.
    let mut seen_unbonds = HashSet::new();
    for tx in txs {
        let Some(kind_byte) = tx.subnetwork_id.palw_tx_kind() else { continue };
        match PalwTxKind::from_subnetwork_byte(kind_byte) {
            Some(PalwTxKind::ProviderBond) => {
                if let Ok(payload) = borsh::from_slice::<PalwProviderBondPayloadV1>(&tx.payload) {
                    // Anti-split floor: sub-minimum bonds are DROPPED, not clamped. Admitting them
                    // at a raised amount would mint collateral that no output locks.
                    if payload.amount_sompi < min_provider_bond_sompi {
                        continue;
                    }
                    let outpoint = TransactionOutpoint::new(tx.id(), 0);
                    muts.push(PalwProviderBondMutation::Insert(
                        outpoint,
                        PalwProviderBondRecord {
                            version: payload.version,
                            bond_outpoint: outpoint,
                            owner_pubkey_hash: validator_id_from_pubkey(&payload.owner_public_key),
                            owner_public_key: payload.owner_public_key,
                            operator_group_id: payload.operator_group_id,
                            runtime_classes: payload.runtime_classes,
                            capacity_by_shape: payload.capacity_by_shape,
                            reward_key_root: payload.reward_key_root,
                            amount_sompi: payload.amount_sompi,
                            activation_daa_score: accepted_daa_score,
                            created_daa_score: accepted_daa_score,
                            unbond_delay_epochs: payload.unbond_delay_epochs.max(unbond_floor_epochs),
                            unbond_request_daa_score: None,
                            slashed_at_daa_score: None,
                        },
                    ));
                }
            }
            Some(PalwTxKind::ProviderUnbond) => {
                // Authorization (owner-key binding + signature + Pending/Active precondition) is a
                // block-validity rule (`palw_provider_unbond_authorized`, consensus/src/processes/palw.rs),
                // so any request reaching here in a VALID block is already authorized and applies once.
                //
                // The decode goes through the SAME helper the authorizer uses. If the two decoded
                // independently, a payload one accepted and the other skipped would be an unauthorized
                // mutation — the producer/verifier drift this ADR has been bitten by repeatedly.
                if let Some(req) = decode_provider_unbond_request(tx)
                    && seen_unbonds.insert(req.bond_outpoint)
                {
                    muts.push(PalwProviderBondMutation::Unbond(req.bond_outpoint, accepted_daa_score));
                }
            }
            _ => {}
        }
    }
    muts
}

/// The single decode point for a `0x37` provider-unbond transaction.
///
/// Both the registry PRODUCER (`palw_provider_bond_mutations_from_accepted_txs`) and the
/// authorization VERIFIER (`palw_provider_unbond_authorized`) route through this, so the set of
/// transactions they act on is identical by construction rather than by two matching copies of the
/// same three lines. A tx the verifier skipped but the producer mutated on would be an unauthorized
/// state transition; a tx the producer skipped but the verifier rejected on would be a spurious
/// block invalidity. Neither is reachable while there is one decoder.
///
/// Non-`0x37` transactions and undecodable payloads yield `None`. Undecodable is unreachable inside a
/// valid block — [`validate_provider_unbond`] rejects it at the isolation coordinate — so this is a
/// defensive skip, not a second opinion.
fn decode_provider_unbond_request(tx: &crate::tx::Transaction) -> Option<PalwProviderUnbondRequestV1> {
    let kind_byte = tx.subnetwork_id.palw_tx_kind()?;
    if PalwTxKind::from_subnetwork_byte(kind_byte) != Some(PalwTxKind::ProviderUnbond) {
        return None;
    }
    borsh::from_slice::<PalwProviderUnbondRequestV1>(&tx.payload).ok()
}

/// kaspa-pq **ADR-0040 (ECON-03, leg 5)** — every provider-unbond request among `txs`, paired with the
/// id of the transaction carrying it. Mirrors [`crate::dns_finality::unbond_requests_from_accepted_txs`].
///
/// Consumed by `palw_provider_unbond_authorized` (consensus/src/processes/palw.rs), which is where the
/// signature and the bond binding are checked — those need a point of view, which this crate has no
/// access to.
pub fn palw_provider_unbond_requests_from_accepted_txs(
    txs: &[crate::tx::Transaction],
) -> Vec<(crate::tx::TransactionId, PalwProviderUnbondRequestV1)> {
    txs.iter().filter_map(|tx| decode_provider_unbond_request(tx).map(|req| (tx.id(), req))).collect()
}

/// kaspa-pq **ADR-0040 (ECON-03, leg 3) — the per-block provider-bond view.**
///
/// An in-memory snapshot of the provider-bond set as-of a specific block, composed along that
/// block's selected-chain prefix — the exact shape of [`crate::dns_finality::ActiveBondView`], and
/// for the same reason: a validity rule must NOT read the global store at prefix 241, because that
/// store reflects whichever branch was committed most recently. A point-of-view-dependent read would
/// chain-split. The store is written by the walk and read ONLY to seed it.
///
/// [`Self::apply`] and [`Self::revert`] are exact inverses (`revert` iterates in reverse, so a
/// `Slash`/`Unbond` whose `Insert` is reverted in the same diff is handled), and they mirror the
/// persisted staging byte-for-byte, so the in-memory view and the on-disk store cannot diverge.
/// Consequence: two nodes reaching the same block by different reorg paths hold identical views.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderBondView {
    bonds: std::collections::HashMap<TransactionOutpoint, PalwProviderBondRecord>,
}

impl ProviderBondView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the walk from an existing `(outpoint, record)` set — the store snapshot at the previous
    /// sink, the same anchor `accumulated_diff` starts from. Records are inserted verbatim so the
    /// seed matches the store.
    pub fn from_records(records: impl IntoIterator<Item = (TransactionOutpoint, PalwProviderBondRecord)>) -> Self {
        Self { bonds: records.into_iter().collect() }
    }

    /// Apply one block's mutations in tx order (the forward walk).
    pub fn apply(&mut self, mutations: &[PalwProviderBondMutation]) {
        for mutation in mutations {
            match mutation {
                PalwProviderBondMutation::Insert(outpoint, record) => {
                    self.bonds.insert(*outpoint, record.clone());
                }
                PalwProviderBondMutation::Unbond(outpoint, daa) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        record.unbond_request_daa_score = Some(*daa);
                    }
                }
                PalwProviderBondMutation::Slash(outpoint, daa) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        record.slashed_at_daa_score = Some(*daa);
                    }
                }
            }
        }
    }

    /// Revert one block's mutations in REVERSE order (the backward walk) — the exact inverse of
    /// [`Self::apply`].
    pub fn revert(&mut self, mutations: &[PalwProviderBondMutation]) {
        for mutation in mutations.iter().rev() {
            match mutation {
                PalwProviderBondMutation::Insert(outpoint, _) => {
                    self.bonds.remove(outpoint);
                }
                PalwProviderBondMutation::Unbond(outpoint, _) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        record.unbond_request_daa_score = None;
                    }
                }
                PalwProviderBondMutation::Slash(outpoint, _) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        record.slashed_at_daa_score = None;
                    }
                }
            }
        }
    }

    /// Resolves ECON-03 collateral at a point of view. The returned record's `amount_sompi` was
    /// verified against locked coins rather than supplied by a payload. Returns `None` when the
    /// outpoint is unknown or the bond is not `Active` at the requested DAA score.
    pub fn active_provider_bond_at(&self, outpoint: &TransactionOutpoint, pov_daa_score: u64) -> Option<&PalwProviderBondRecord> {
        let record = self.bonds.get(outpoint)?;
        is_provider_bond_active_at(record, pov_daa_score).then_some(record)
    }

    /// Resolved collateral of a bond at a point of view, or ZERO if it does not resolve. This is the
    /// function a reward path must consult: an unbacked, unknown, unbonding or slashed bond is worth
    /// nothing, and saying so as `0` rather than as an absent lookup makes the "no backing ⇒ no
    /// collateral" property total.
    pub fn resolved_collateral_at(&self, outpoint: &TransactionOutpoint, pov_daa_score: u64) -> u64 {
        self.active_provider_bond_at(outpoint, pov_daa_score).map_or(0, |r| r.amount_sompi)
    }

    /// Raw lookup regardless of status (diagnostics / tests).
    pub fn get(&self, outpoint: &TransactionOutpoint) -> Option<&PalwProviderBondRecord> {
        self.bonds.get(outpoint)
    }

    /// Static-audit finding H-01 — resolve which sponsor, if any, a manifest carrier proved.
    ///
    /// `spent_scripts` is the script of every output the carrier consumed. If one of them is the
    /// `provider_bond_lock_spk` of a bond that is ACTIVE at `pov_daa_score`, that bond's owner is the
    /// sponsor. Nothing else is checked because nothing else is needed: spending that script already
    /// required an ML-DSA-87 signature whose sighash preimage covers the transaction's
    /// `subnetwork_id` and its entire `payload`, so the spend IS the owner authorizing THIS manifest.
    /// Consensus reads a result the script engine produced; it does not re-verify a signature.
    ///
    /// Ties are resolved by taking the FIRST match in the caller's iteration order, which must be
    /// input order — a carrier spending two sponsors' scripts must attribute deterministically.
    ///
    /// Returns [`PALW_SPONSOR_UNATTRIBUTED`] when nothing matches, and the caller decides whether
    /// that is fatal (past the fence it is).
    pub fn resolve_manifest_sponsor<'a>(
        &self,
        spent_scripts: impl IntoIterator<Item = &'a ScriptPublicKey>,
        pov_daa_score: u64,
    ) -> (u64, u64) {
        // Built once per call from the ACTIVE bonds only. `bonds` is bounded by the registry, and a
        // manifest carrier is rare, so the linear build is cheaper than caching something that would
        // have to be invalidated per point of view.
        let index: std::collections::HashMap<ScriptPublicKey, (u64, u64)> = self
            .bonds
            .values()
            .filter(|record| is_provider_bond_active_at(record, pov_daa_score))
            .map(|record| (provider_bond_lock_spk(&record.owner_public_key), palw_manifest_sponsor_tag(&record.owner_public_key)))
            .collect();
        for script in spent_scripts {
            if let Some(tag) = index.get(script) {
                // A real key cannot hash to the sentinel; refusing it keeps "unattributed" a single
                // unambiguous value rather than something an astronomically lucky key could forge.
                if *tag != PALW_SPONSOR_UNATTRIBUTED {
                    return *tag;
                }
            }
        }
        PALW_SPONSOR_UNATTRIBUTED
    }

    pub fn records(&self) -> Vec<PalwProviderBondRecord> {
        self.bonds.values().cloned().collect()
    }

    /// Total collateral of all bonds `Active` at `pov_daa_score`.
    pub fn total_active_provider_stake_at(&self, pov_daa_score: u64) -> u64 {
        self.bonds
            .values()
            .filter(|b| is_provider_bond_active_at(b, pov_daa_score))
            .fold(0u64, |acc, b| acc.saturating_add(b.amount_sompi))
    }

    pub fn len(&self) -> usize {
        self.bonds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bonds.is_empty()
    }
}

/// Cross-fork double-use authorization (design §12.4). The ticket authority ML-DSA-signs a
/// domain-separated header preimage (with `palw_authorization_hash = 0`); a second signature over a
/// different header commitment is slashing evidence.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBlockAuthorizationV1 {
    pub version: u16,
    pub batch_id: Hash64,
    pub leaf_index: u32,
    pub ticket_nullifier: Hash64,
    pub header_preimage_commitment: Hash64,
    pub authority_public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

/// kaspa-pq **ADR-0040 (AUTH-TXSHAPE) — the ONE way to build the carrying transaction.**
///
/// The authorization travels in the block body as a transaction on subnetwork `0x38`, and consensus
/// pins its shape in two places: `check_palw_block_authorization_shape` at transaction isolation
/// (which runs FIRST, so a shape violation is reported as such rather than as whichever generic rule
/// an illegal input/output happens to trip on the way) and body clause 7, which additionally requires
/// it to be the LAST transaction and re-decodes the payload to compare against
/// `header.palw_authorization_hash`.
///
/// Every one of those pins is a property of the *encoding*, not of the signature. So a producer that
/// hand-rolls the transaction can drift from the verifier without any test noticing — the signature
/// still verifies, and the block is still rejected. This function exists so there is exactly one
/// encoder in the tree and `construction == validation` is a fact about the code rather than a claim
/// in a comment.
///
/// It deliberately takes the *typed* authorization rather than a `Vec<u8>` payload: an API that
/// accepted pre-encoded bytes would let a caller append trailing bytes, which is precisely one of the
/// deviations clause 7 rejects.
///
/// The shape, all of it fixed here and none of it caller-selectable: `version = TX_VERSION`, no
/// inputs, no outputs, `lock_time = 0`, `subnetwork_id = SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION`
/// (`0x38`), `gas = 0`, and `payload` the canonical borsh encoding of `auth`. [`Transaction::new`]
/// leaves the storage-mass commitment unset (`0`), which is what clause 7's `mass() != 0` rejection
/// requires.
///
/// Borsh encoding of this fixed-shape struct cannot fail: every field is a `u16`/`u32`/`Hash64`/
/// `Vec<u8>`, none of which has a fallible writer, and the sink is an in-memory `Vec`.
pub fn build_palw_authorization_transaction(auth: &PalwBlockAuthorizationV1) -> Transaction {
    Transaction::new(
        crate::constants::TX_VERSION,
        vec![],
        vec![],
        0,
        crate::subnets::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION,
        0,
        borsh::to_vec(auth).expect("borsh encoding of a fixed-shape PALW authorization is infallible"),
    )
}

/// Canonical AUTH-02 header commitment for a ticket authorization.
///
/// # Security property
///
/// `eligibility_hash` does not bind block content, and a winning header discloses its raw
/// `ticket_nullifier`. The authorization therefore binds the complete canonical header preimage.
///
/// # Signature requirement
///
/// Binding payout data into `eligibility_hash` would permit grinding over payout scripts. An authority
/// signature binds a fixed block preimage without adding a new draw coordinate.
///
/// # Committed data
///
/// The binding covers the complete canonical header preimage rather than an allowlist of fields. See
/// [`crate::hashing::header::palw_authorization_commitment`] for the substitutions and why they are the
/// only two. A header field added in future is bound automatically, because it is already in the
/// preimage the block hash is computed over.
pub fn palw_header_preimage_commitment(network_id: u32, header: &crate::header::Header, authed_root: &Hash64) -> Hash64 {
    crate::hashing::header::palw_authorization_commitment(network_id, header, authed_root)
}

impl PalwBlockAuthorizationV1 {
    /// `palw_authorization_hash` = hash of the completed authorization payload (design §12.4).
    pub fn hash(&self) -> Hash64 {
        blake2b_512_keyed(PALW_LEAF_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }

    /// ADR-0040 P1-6 — the ML-DSA-87 message the ticket authority signs.
    ///
    /// Excludes `signature` (it covers this digest) and `authority_public_key` is INCLUDED, so a
    /// signature cannot be re-presented under a substituted key. Mirrors the beacon commit/reveal
    /// pattern, and uses its own context (`PALW_AUTHORIZATION_MLDSA87_CONTEXT`) so an authorization
    /// signature can never be replayed as a beacon or an audit vote.
    pub fn signing_hash(&self, network_id: u32) -> Hash64 {
        let mut p = Vec::with_capacity(4 + 2 + 3 * HASH64_SIZE + 4 + self.authority_public_key.len());
        p.extend_from_slice(&network_id.to_le_bytes());
        p.extend_from_slice(&self.version.to_le_bytes());
        push_hash(&mut p, &self.batch_id);
        p.extend_from_slice(&self.leaf_index.to_le_bytes());
        push_hash(&mut p, &self.ticket_nullifier);
        push_hash(&mut p, &self.header_preimage_commitment);
        p.extend_from_slice(&(self.authority_public_key.len() as u32).to_le_bytes());
        p.extend_from_slice(&self.authority_public_key);
        blake2b_512_keyed(PALW_AUTHORIZATION_DOMAIN, &p)
    }

    /// ADR-0040 P1-6 (AUTH-03) — does this authorization bind the leaf's declared authority?
    ///
    /// `leaf.ticket_authority_pk_hash` had ZERO production readers, so the field named an authority that
    /// nothing checked. This is the check: the authorization's public key must hash to it.
    pub fn binds_leaf_authority(&self, leaf_ticket_authority_pk_hash: &Hash64) -> bool {
        blake2b_512_keyed(PALW_AUTHORIZATION_DOMAIN, &self.authority_public_key) == *leaf_ticket_authority_pk_hash
    }

    /// ADR-0040 (AUTH-02) — the pure, signature-free half of authorization validity.
    ///
    /// Checks that the authorization is *about* this block and this ticket. The ML-DSA verification is
    /// the caller's (it needs the crypto crate); splitting it this way keeps the binding logic unit
    /// testable without a signer, and makes the two failure modes distinguishable in the reject reason.
    ///
    /// `authed_root` is the merkle root over every transaction EXCEPT the 0x38 authorization
    /// transaction — see [`palw_header_preimage_commitment`] for why that substitution is necessary.
    /// The three redundant scalar equalities below are kept deliberately: they are already covered by
    /// the total header commitment, but they make a mismatched ticket coordinate distinguishable from a
    /// mismatched header at the call site, and they cost nothing.
    pub fn binds_header(&self, network_id: u32, header: &crate::header::Header, authed_root: &Hash64) -> bool {
        self.version == 1
            && self.batch_id == header.palw_batch_id
            && self.leaf_index == header.palw_leaf_index
            && self.ticket_nullifier == header.palw_ticket_nullifier
            && self.header_preimage_commitment == palw_header_preimage_commitment(network_id, header, authed_root)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBeaconCommitV1 {
    pub version: u16,
    pub epoch: u64,
    pub bond_outpoint: TransactionOutpoint,
    pub commitment: Hash64,
    pub signature: Vec<u8>,
}

impl PalwBeaconCommitV1 {
    /// Domain-separated ML-DSA message digest. Binds the operation to the network and every semantic
    /// field while excluding `signature` itself. `network_id` is the consensus PALW network number
    /// used by the other PALW hash preimages; callers must resolve it from chain params.
    pub fn signing_hash(&self, network_id: u32) -> Hash64 {
        let mut p = Vec::with_capacity(4 + 2 + 8 + HASH64_SIZE + 4 + HASH64_SIZE);
        p.extend_from_slice(&network_id.to_le_bytes());
        p.extend_from_slice(&self.version.to_le_bytes());
        p.extend_from_slice(&self.epoch.to_le_bytes());
        push_hash(&mut p, &self.bond_outpoint.transaction_id);
        p.extend_from_slice(&self.bond_outpoint.index.to_le_bytes());
        push_hash(&mut p, &self.commitment);
        blake2b_512_keyed(PALW_BEACON_COMMIT_SIGNING_DOMAIN, &p)
    }

    /// Contextual phase predicate: a commit carried in epoch `P` may target only `P + 2`.
    #[inline]
    pub fn is_in_phase(&self, current_epoch: u64) -> bool {
        beacon_commit_phase_accepts(current_epoch, self.epoch)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBeaconRevealV1 {
    pub version: u16,
    pub epoch: u64,
    pub bond_outpoint: TransactionOutpoint,
    pub random_64: [u8; 64],
    pub signature: Vec<u8>,
}

impl PalwBeaconRevealV1 {
    /// Domain-separated ML-DSA message digest. This domain is intentionally distinct from
    /// [`PalwBeaconCommitV1::signing_hash`], so a commit signature cannot authorize a reveal.
    pub fn signing_hash(&self, network_id: u32) -> Hash64 {
        let mut p = Vec::with_capacity(4 + 2 + 8 + HASH64_SIZE + 4 + 64);
        p.extend_from_slice(&network_id.to_le_bytes());
        p.extend_from_slice(&self.version.to_le_bytes());
        p.extend_from_slice(&self.epoch.to_le_bytes());
        push_hash(&mut p, &self.bond_outpoint.transaction_id);
        p.extend_from_slice(&self.bond_outpoint.index.to_le_bytes());
        p.extend_from_slice(&self.random_64);
        blake2b_512_keyed(PALW_BEACON_REVEAL_SIGNING_DOMAIN, &p)
    }

    /// True iff this reveal matches a prior [`PalwBeaconCommitV1::commitment`] (design §11.2).
    pub fn matches_commit(&self, commitment: &Hash64) -> bool {
        beacon_commitment(self.epoch, &self.random_64, &self.bond_outpoint) == *commitment
    }

    /// Distinct-domain digest of the opened secret used by `valid_reveals_root` and hence `R_E`.
    #[inline]
    pub fn entropy_digest(&self) -> Hash64 {
        beacon_reveal_entropy_digest(self.epoch, &self.random_64, &self.bond_outpoint)
    }

    /// Contextual phase predicate: a reveal carried in epoch `P` may target only `P + 1`.
    #[inline]
    pub fn is_in_phase(&self, current_epoch: u64) -> bool {
        beacon_reveal_phase_accepts(current_epoch, self.epoch)
    }
}

/// ADR-0039 §12.1 — the frozen facts of a DNS-confirmed anchor threaded through the beacon recurrence
/// (design-panel resolution for clause 6). Every field is a property of the ANCHOR BLOCK ITSELF
/// (header-committed, lag-buried, confirmation-depth-cleared, strictly before any target interval it
/// certifies), so the certificate digest over them cannot be ground by boundary-block producers: the
/// bond view / attestation set at the deriving boundary never enters the preimage. `overlay_root` is
/// the anchor's own header-committed `overlay_commitment_root` — it binds WHICH bond/stake state
/// confirmed (the materialized `validator_set_commitment` intent) without any churnable input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BeaconDnsAnchor {
    pub hash: Hash64,
    pub blue_score: u64,
    pub daa_score: u64,
    pub overlay_root: Hash64,
}

impl BeaconDnsAnchor {
    /// The pre-confirmation sentinel (all zero). Fail-closed: no certificate is derivable from it.
    pub const UNCONFIRMED: Self = Self {
        hash: Hash64::from_bytes([0u8; HASH64_SIZE]),
        blue_score: 0,
        daa_score: 0,
        overlay_root: Hash64::from_bytes([0u8; HASH64_SIZE]),
    };

    #[inline]
    pub fn is_confirmed(&self) -> bool {
        self.hash != Hash64::default()
    }
}

/// ADR-0039 §12.1 (option (b), panel-frozen v1) — the `dns_finality_certificate_hash` fed to
/// [`chain_commit`]: a domain-separated digest of the confirmed anchor's own frozen facts. Deliberately
/// EXCLUDED (each was a constructed grinding/split channel): any bond-set commitment over a boundary-time
/// view (2^n re-rolls via cheap self-bond inclusion), any `confirmation_epoch` (2-valued at boundaries,
/// undefined for carried anchors), and any raw work/stake depth (grows per block). A future v2 carrying a
/// real certificate object slots in behind a new domain without changing [`chain_commit`]'s signature
/// (its second argument stays an opaque digest).
pub fn dns_finality_certificate_hash_v1(anchor: &BeaconDnsAnchor) -> Hash64 {
    let mut p = Vec::with_capacity(2 * HASH64_SIZE + 16);
    push_hash(&mut p, &anchor.hash);
    p.extend_from_slice(&anchor.blue_score.to_le_bytes());
    p.extend_from_slice(&anchor.daa_score.to_le_bytes());
    push_hash(&mut p, &anchor.overlay_root);
    blake2b_512_keyed(PALW_DNS_CERT_DOMAIN, &p)
}

/// ADR-0039 §12.1 — static params-consistency predicate discharging the §12.1 LOOKBACK inequality
/// ("the checkpoint lag exceeds DNS finality + max shallow reorg") plus non-vacuous confirmation depths.
/// A PALW **re-genesis preflight / C5 activation gate** — deliberately SEPARATE from
/// `dns_v3_params_consistent` (which live nets evaluate on every DNS state update: current testnet DNS
/// presets have `lag + backoff = 120 < max_reorg_horizon = 300` and would be instantly deactivated).
/// The PALW re-genesis must recalibrate `attestation_lag_blue_score`/`backoff` to satisfy this. Both
/// sides of the burial comparison are blue-score-denominated (`max_reorg_horizon_blocks` bounds the
/// abandonable chain suffix, whose blue score is bounded by its block count).
#[allow(clippy::too_many_arguments)]
pub fn palw_checkpoint_params_consistent(
    attestation_lag_blue_score: u64,
    attestation_anchor_backoff_blue_score: u64,
    max_reorg_horizon_blocks: u64,
    finality_depth: u64,
    pruning_depth: u64,
    required_work_depth: BlueWorkType,
    required_stake_depth: u128,
) -> bool {
    let burial = attestation_lag_blue_score.saturating_add(attestation_anchor_backoff_blue_score);
    // C6 SLICE 5 — the re-genesis BAND the finality-buried anchor must sit in:
    //   max(max_reorg_horizon, finality_depth)  <=  burial  <  pruning_depth
    // (i) burial >= max_reorg_horizon: outlast the deepest legal reorg (else a deep reorg re-rolls
    //     chain_commit). (ii) burial >= finality_depth: the anchor's selected-chain identity is
    //     externally SETTLED by DNS finality — this collapses the clause-9 forged-seed residual (a
    //     chain-disqualified block cannot be the canonical settled anchor without a finality violation)
    //     into the standing I-4 trust chain_commit already depends on. (iii) burial < pruning_depth: the
    //     anchor's header + reachability must survive on pruned nodes, or the body-stage clause-6/9 read
    //     becomes unrunnable (a liveness break). (iv) the confirmation predicate must not be vacuous —
    //     with both depths zero, `is_dns_confirmed` passes immediately and a "confirmed" anchor is merely
    //     lag-ready. The band is non-empty only if `pruning_depth > max(max_reorg_horizon, finality_depth)`
    //     (holds by design); a re-genesis must recalibrate lag/backoff to land inside it.
    burial >= max_reorg_horizon_blocks
        && burial >= finality_depth
        && burial < pruning_depth
        && (required_work_depth > BlueWorkType::from(0u64) || required_stake_depth > 0)
}

/// ADR-0039 §11.2 / §18.2 — the per-epoch derived beacon state persisted once per chain block (the
/// block carries its epoch's active `R_E`). Every field is fixed-width so the whole record is a
/// borsh + serde POD (no bare `[u8; 64]` — `random_64` is deliberately NOT here: reveal acceptance
/// reduces it to [`beacon_reveal_entropy_digest`], which is committed by `valid_reveals_root`).
///
/// The `*_root` + `dns_anchor` + `epoch` fields make each entry **self-verifying**: a pruned node with
/// the prior epoch's `seed` can recompute `seed == beacon_seed(prev_seed, dns_anchor, valid_reveals_root,
/// missing_commitments_root, epoch)` from the §18.3 proof bundle without replaying the raw commit/reveal
/// txs. `degraded_epochs` is the per-block-carried grace counter `beacon_mode` consumes (`= parent+1`
/// while degraded, else `0`), so the mode recurrence is reorg-safe without a global counter.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBeaconStateV1 {
    pub version: u16,
    pub epoch: u64,
    /// `R_E` (or the carried `R_{E-1}` on a non-boundary block / `DegradedGrace`).
    pub seed: Hash64,
    /// The DNS-finalized anchor folded into this epoch's seed (`beacon_seed` arg 2).
    pub dns_anchor: Hash64,
    /// The anchor's own frozen coordinates + header-committed overlay root (§12.1 clause-6 facts;
    /// storage option (ii): the record stays self-verifying — the certificate digest is derivable from
    /// these fields alone, and the carried-anchor path never re-reads the anchor header).
    pub anchor_blue_score: u64,
    pub anchor_daa_score: u64,
    pub anchor_overlay_root: Hash64,
    pub valid_reveals_root: Hash64,
    pub missing_commitments_root: Hash64,
    /// [`PalwBeaconMode`] discriminant (0 = Healthy, 1 = DegradedGrace, 2 = Halted).
    pub mode: u8,
    /// Consecutive degraded epochs feeding `beacon_mode` (`0` when Healthy).
    pub degraded_epochs: u64,
    /// Diagnostics (not seed inputs): counts behind the two roots.
    pub valid_reveal_count: u32,
    pub missing_commit_count: u32,
    /// Static-audit finding C-01 — the epochs THIS block closed, with their seeds.
    ///
    /// Usually empty (a non-boundary block) or one entry. More only when a mergeset advances DAA
    /// across several epoch boundaries at once, which closes every epoch in between; those epochs
    /// have no block of their own, so a back-pointer chain alone could never answer for them. That
    /// is the case a naive "walk to the previous boundary" design silently gets wrong.
    pub closed_seeds: Vec<(u64, Hash64)>,
    /// The nearest ancestor chain block whose `closed_seeds` is non-empty; `None` ONLY at the true
    /// origin of this history (genesis / the activation fence). When the selected parent has no row
    /// at all — the pruned-joiner boundary — this points AT the parent, so the walk terminates in
    /// [`PalwForkRelativeOutcome::Buried`] rather than claiming the epoch was never closed.
    ///
    /// This is what makes `R_E` a FORK-RELATIVE read. The epoch-keyed history store is
    /// idempotent-by-VALUE, not write-once — its own doc says the value is "a function of the chain
    /// that first closed this epoch" — so two forks legitimately write different seeds under one
    /// epoch key, and a PCPB clause reading that key resolves whichever fork wrote last. Walking
    /// this chain from the CANDIDATE's selected parent instead makes the answer a function of the
    /// candidate's own past, which is what clause 11 always claimed to be.
    pub prev_closer: Option<crate::BlockHash>,
}

/// Static-audit finding C-01 — the per-chain-block row of the PCPB context that the provider-bond
/// reconciliation derives: the epochs THIS block closed a provider snapshot for, plus the pointer
/// that makes the history walkable.
///
/// Deliberately a SEPARATE row from [`PalwBeaconStateV1`] rather than two more fields on it. The
/// beacon state is header-committed (`palw_overlay_commitment_root_v1` borsh-serializes it) and is
/// written by `commit_palw_beacon_state`, which runs BEFORE the provider-bond registry reconciles
/// the same chain path — the snapshot commitment simply does not exist yet at that coordinate.
/// Folding it in would make a header commitment depend on a later pipeline stage.
///
/// The two fields carry the identical contract as their beacon twins, for the identical reason;
/// see [`PalwBeaconStateV1::closed_seeds`] / [`PalwBeaconStateV1::prev_closer`].
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwPcpbChainStateV1 {
    pub version: u16,
    /// The epochs THIS block closed, with the provider snapshot commitment each closed with. A Vec
    /// for the same reason `closed_seeds` is: one mergeset can cross several epoch boundaries, and
    /// the epochs in between never get a block of their own.
    pub closed_snapshots: Vec<(u64, PalwSnapshotCommitment)>,
    /// The nearest ancestor chain block with a non-empty `closed_snapshots`, `None` only at the
    /// true origin of this history (genesis / the activation fence). A block whose selected parent
    /// has NO row at all points AT the parent instead — see [`PalwForkRelativeOutcome::Buried`].
    pub prev_closer: Option<crate::BlockHash>,
}

/// One block's row in a fork-relative closed-epoch history. Implemented by [`PalwBeaconStateV1`]
/// (seeds) and [`PalwPcpbChainStateV1`] (provider snapshots) so both walk through one resolver —
/// the termination rules below are subtle enough that a second copy would drift from the first.
pub trait PalwClosedEpochChain {
    type Value: Copy;
    fn closed_epochs(&self) -> &[(u64, Self::Value)];
    fn prev_closer(&self) -> Option<crate::BlockHash>;
}

impl PalwClosedEpochChain for PalwBeaconStateV1 {
    type Value = Hash64;
    fn closed_epochs(&self) -> &[(u64, Hash64)] {
        &self.closed_seeds
    }
    fn prev_closer(&self) -> Option<crate::BlockHash> {
        self.prev_closer
    }
}

impl PalwClosedEpochChain for PalwPcpbChainStateV1 {
    type Value = PalwSnapshotCommitment;
    fn closed_epochs(&self) -> &[(u64, PalwSnapshotCommitment)] {
        &self.closed_snapshots
    }
    fn prev_closer(&self) -> Option<crate::BlockHash> {
        self.prev_closer
    }
}

/// What a fork-relative walk concluded. Three outcomes, not two, because "this history does not
/// answer" and "this history is TRUNCATED here" are different facts with different safe handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwForkRelativeOutcome<V> {
    /// The candidate's own history closed the epoch with this value. Authoritative.
    Resolved(V),
    /// The walk ran off the end of the RETAINED chain: it reached a block whose row is gone, which
    /// happens only where the pruning pass deleted it (or, for a pruned joiner, below the boundary
    /// it imported). The target epoch is therefore buried under the pruning point — final, identical
    /// on every honest node, and beyond reorg — so the caller may fall back to the epoch-keyed
    /// buried history, which is exactly what the pruning-snapshot carry exists to populate.
    ///
    /// This is NOT a licence to read the shared store in general: a live fork's epochs are always
    /// answered above, by a retained row, before the walk can ever descend this far.
    Buried,
    /// This candidate's own past never closed the epoch (or the hop budget ran out). REFUSE — a
    /// live value substituted for an unresolvable epoch is the grindable state D3-b forbids.
    Refused,
}

/// Static-audit finding C-01 — resolve an epoch's value FORK-RELATIVELY by walking the closed-epoch
/// chain backwards from a starting block's row.
///
/// `read` returns a block's row; the walk starts at `from` (always the CANDIDATE block's selected
/// parent, never the node's virtual tip) and follows [`PalwClosedEpochChain::prev_closer`].
///
/// Why a walk and not an epoch-keyed lookup: the epoch-keyed histories are idempotent-by-VALUE.
/// Their own contract says the stored value is "a function of the chain that first closed this
/// epoch", so two forks that both close epoch `E` legitimately write DIFFERENT values under the same
/// key, and whichever committed last wins for EVERY reader — including a PCPB clause validating a
/// block on the other fork. That made leaf acceptance depend on fork receive order, sink-search order
/// and reorg path, which is a consensus split with no tx-level cause: the 2026-08-02 partition was
/// triggered by exactly this class of divergence (there, from a rule skew rather than a store read).
///
/// `max_hops` bounds the walk at the retention window; exhausting it is [`PalwForkRelativeOutcome::Refused`],
/// never a shorter answer.
pub fn palw_resolve_closed_epoch_fork_relative<S: PalwClosedEpochChain, E>(
    from: Hash64,
    epoch: u64,
    max_hops: usize,
    mut read: impl FnMut(Hash64) -> Result<Option<S>, E>,
) -> Result<PalwForkRelativeOutcome<S::Value>, E> {
    let mut cursor = Some(from);
    for _ in 0..max_hops {
        // A `None` pointer is the ORIGIN of this history (genesis / the activation fence): the epoch
        // was never closed here at all. A MISSING ROW is the opposite claim — the writer points at a
        // rowless parent on purpose — and means the history is truncated by pruning.
        let Some(block) = cursor else { return Ok(PalwForkRelativeOutcome::Refused) };
        let Some(state) = read(block)? else { return Ok(PalwForkRelativeOutcome::Buried) };
        // A block records the epochs IT closed. `epoch` is answered by whichever ancestor closed it —
        // including the multi-epoch case, where one block closes several and no block sits between.
        if let Some((_, value)) = state.closed_epochs().iter().find(|(closed, _)| *closed == epoch) {
            return Ok(PalwForkRelativeOutcome::Resolved(*value));
        }
        // Walked strictly past the target: this block closed an OLDER epoch. Every chain closes
        // EVERY epoch it spans (a multi-epoch mergeset records each one), so an older closer proves
        // the target was never closed in this history — refuse instead of descending to the floor
        // and mistaking the pruning boundary for an answer.
        if state.closed_epochs().iter().any(|(closed, _)| *closed < epoch) {
            return Ok(PalwForkRelativeOutcome::Refused);
        }
        cursor = state.prev_closer();
    }
    Ok(PalwForkRelativeOutcome::Refused)
}

#[cfg(test)]
mod fork_relative_history_tests {
    use super::*;
    use std::collections::HashMap;

    fn state(epoch: u64, closed: Vec<(u64, Hash64)>, prev: Option<Hash64>) -> PalwBeaconStateV1 {
        PalwBeaconStateV1 {
            version: 1,
            epoch,
            seed: Hash64::default(),
            closed_seeds: closed,
            prev_closer: prev,
            dns_anchor: Hash64::default(),
            anchor_blue_score: 0,
            anchor_daa_score: 0,
            anchor_overlay_root: Hash64::default(),
            valid_reveals_root: Hash64::default(),
            missing_commitments_root: Hash64::default(),
            mode: 0,
            degraded_epochs: 0,
            valid_reveal_count: 0,
            missing_commit_count: 0,
        }
    }
    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }
    fn walk(chain: &HashMap<Hash64, PalwBeaconStateV1>, from: Hash64, epoch: u64) -> PalwForkRelativeOutcome<Hash64> {
        palw_resolve_closed_epoch_fork_relative::<PalwBeaconStateV1, ()>(from, epoch, 64, |b| Ok(chain.get(&b).cloned())).unwrap()
    }
    fn resolved(seed: Hash64) -> PalwForkRelativeOutcome<Hash64> {
        PalwForkRelativeOutcome::Resolved(seed)
    }

    /// THE finding. Two forks both close epoch 7 with different seeds. An epoch-keyed store answers
    /// whichever wrote last — the same answer for BOTH forks, so one of them validates a leaf
    /// against the other's randomness and the two nodes disagree about block validity. Walking each
    /// candidate's OWN chain gives each fork its own seed, which is what clause 11 always meant.
    #[test]
    fn two_forks_closing_one_epoch_resolve_to_their_own_seeds() {
        let (seed_a, seed_b) = (h(0xAA), h(0xBB));
        let mut chain = HashMap::new();
        chain.insert(h(1), state(6, vec![(6, h(0x60))], None));
        // Fork A: block 2 closes epoch 7 as 0xAA. Fork B: block 3 closes the SAME epoch as 0xBB.
        chain.insert(h(2), state(7, vec![(7, seed_a)], Some(h(1))));
        chain.insert(h(3), state(7, vec![(7, seed_b)], Some(h(1))));

        assert_eq!(walk(&chain, h(2), 7), resolved(seed_a));
        assert_eq!(walk(&chain, h(3), 7), resolved(seed_b));
        // Their shared past still resolves identically on both — divergence is confined to the
        // epochs the forks actually disagree about.
        assert_eq!(walk(&chain, h(2), 6), walk(&chain, h(3), 6));
    }

    /// A mergeset can advance DAA across several epoch boundaries at once, closing every epoch in
    /// between. Those epochs get NO block of their own, so a design that only followed a
    /// "previous boundary block" pointer would answer `None` for them and reject honest leaves.
    /// This is the case that makes `closed_seeds` a Vec rather than a single entry.
    #[test]
    fn a_multi_epoch_jump_answers_for_every_epoch_it_closed() {
        let mut chain = HashMap::new();
        chain.insert(h(1), state(3, vec![(3, h(0x30))], None));
        chain.insert(h(2), state(6, vec![(4, h(0x40)), (5, h(0x50)), (6, h(0x60))], Some(h(1))));
        for (epoch, seed) in [(4u64, h(0x40)), (5, h(0x50)), (6, h(0x60)), (3, h(0x30))] {
            assert_eq!(walk(&chain, h(2), epoch), resolved(seed), "epoch {epoch} must resolve");
        }
    }

    /// Blocks that close nothing carry the pointer forward, so a long run of them costs ONE hop.
    /// Without that the walk would be O(blocks) and the hop bound would reject honest leaves.
    #[test]
    fn non_closing_blocks_do_not_consume_hops() {
        let mut chain = HashMap::new();
        chain.insert(h(1), state(5, vec![(5, h(0x50))], None));
        // Model what the WRITER produces: a non-closing block inherits its parent's pointer rather
        // than pointing at its parent, so the whole run points at the last closer, h(1).
        for i in 2..40u8 {
            chain.insert(h(i), state(5, vec![], Some(h(1))));
        }
        let prev = h(39);
        // 38 non-closing blocks, resolved with a hop budget of 2.
        assert_eq!(
            palw_resolve_closed_epoch_fork_relative::<PalwBeaconStateV1, ()>(prev, 5, 2, |b| Ok(chain.get(&b).cloned())).unwrap(),
            resolved(h(0x50))
        );
    }

    /// Fail-closed in every direction: an epoch this history never closed, an epoch in the future
    /// and an exhausted hop budget all REFUSE. A live-value substitution here is the grindable
    /// state D3-b forbids.
    #[test]
    fn unresolvable_epochs_fail_closed() {
        let mut chain = HashMap::new();
        chain.insert(h(1), state(4, vec![(4, h(0x40))], None));
        chain.insert(h(2), state(9, vec![(9, h(0x90))], Some(h(1))));

        assert_eq!(walk(&chain, h(2), 7), PalwForkRelativeOutcome::Refused, "an epoch this chain never closed must refuse");
        assert_eq!(walk(&chain, h(2), 12), PalwForkRelativeOutcome::Refused, "a future epoch must refuse");
        assert_eq!(
            palw_resolve_closed_epoch_fork_relative::<PalwBeaconStateV1, ()>(h(2), 4, 1, |b| Ok(chain.get(&b).cloned())).unwrap(),
            PalwForkRelativeOutcome::Refused,
            "an exhausted hop budget must refuse rather than answer from a shorter walk"
        );
    }

    /// The distinction that keeps a pruned node from fork-relativity's one real cost: reaching a
    /// block whose row is GONE is `Buried`, not `Refused`. Rows disappear only where the pruning
    /// pass deleted them, so the target epoch is below the pruning point — final, past every reorg
    /// horizon, and answerable from the epoch-keyed history the pruning snapshot carries.
    ///
    /// A `None` pointer is the opposite claim and stays `Refused`: that is the ORIGIN of the
    /// history (genesis / the activation fence), where a fallback would read a live shared row and
    /// hand a young fork its sibling's value — the very finding this walk closes.
    #[test]
    fn a_truncated_history_is_buried_and_an_origin_is_refused() {
        let mut chain = HashMap::new();
        // h(9) is retained; its pointer names h(8), whose row the pruning pass deleted.
        chain.insert(h(9), state(20, vec![(20, h(0xC0))], Some(h(8))));
        assert_eq!(walk(&chain, h(9), 20), resolved(h(0xC0)), "retained epochs never reach the boundary");
        assert_eq!(walk(&chain, h(9), 11), PalwForkRelativeOutcome::Buried, "a pruned ancestor means buried, not absent");
        // The pruned joiner's shape: the candidate's own selected parent is the imported boundary,
        // which has no row in THIS history yet. Same conclusion — consult the carried history.
        assert_eq!(walk(&chain, h(0xEE), 9), PalwForkRelativeOutcome::Buried, "a rowless start block is the boundary");

        // Same walk, same target epoch, but the history ends at its origin instead of at a pruning
        // boundary. No fallback is permitted here.
        let mut young = HashMap::new();
        young.insert(h(9), state(20, vec![(20, h(0xC0))], None));
        assert_eq!(walk(&young, h(9), 11), PalwForkRelativeOutcome::Refused, "an origin must refuse, never fall back");
    }

    /// One resolver, two row types. The provider-snapshot chain has the same shape and the same
    /// finding — two forks that both close epoch 7 write different commitments under one epoch key —
    /// so it walks through the identical code rather than a second copy that could drift.
    #[test]
    fn the_provider_snapshot_chain_walks_through_the_same_resolver() {
        let commitment = |b: u8| PalwSnapshotCommitment {
            snapshot_root: h(b),
            assignment_root: h(b.wrapping_add(1)),
            total_bond: b as u128,
            provider_count: 2,
        };
        let row = |closed: Vec<(u64, PalwSnapshotCommitment)>, prev: Option<Hash64>| PalwPcpbChainStateV1 {
            version: 1,
            closed_snapshots: closed,
            prev_closer: prev,
        };
        let mut chain = HashMap::new();
        chain.insert(h(1), row(vec![(6, commitment(0x60))], None));
        chain.insert(h(2), row(vec![(7, commitment(0xAA))], Some(h(1))));
        chain.insert(h(3), row(vec![(7, commitment(0xBB))], Some(h(1))));
        let walk_snap = |from: Hash64, epoch: u64| {
            palw_resolve_closed_epoch_fork_relative::<PalwPcpbChainStateV1, ()>(from, epoch, 64, |b| Ok(chain.get(&b).cloned()))
                .unwrap()
        };

        assert_eq!(walk_snap(h(2), 7), PalwForkRelativeOutcome::Resolved(commitment(0xAA)));
        assert_eq!(walk_snap(h(3), 7), PalwForkRelativeOutcome::Resolved(commitment(0xBB)));
        assert_eq!(walk_snap(h(2), 6), walk_snap(h(3), 6), "the shared past still agrees");
        assert_eq!(walk_snap(h(2), 5), PalwForkRelativeOutcome::Refused, "an epoch below this origin refuses");
    }
}

impl PalwBeaconMode {
    /// Wire discriminant for [`PalwBeaconStateV1::mode`].
    pub fn to_u8(self) -> u8 {
        match self {
            PalwBeaconMode::Healthy => 0,
            PalwBeaconMode::DegradedGrace => 1,
            PalwBeaconMode::Halted => 2,
        }
    }

    /// Inverse of [`Self::to_u8`]; `None` for an unknown discriminant (fail-closed at the caller).
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(PalwBeaconMode::Healthy),
            1 => Some(PalwBeaconMode::DegradedGrace),
            2 => Some(PalwBeaconMode::Halted),
            _ => None,
        }
    }
}

impl PalwBeaconStateV1 {
    /// The record's carried anchor facts (the [`dns_finality_certificate_hash_v1`] inputs).
    pub fn anchor(&self) -> BeaconDnsAnchor {
        BeaconDnsAnchor {
            hash: self.dns_anchor,
            blue_score: self.anchor_blue_score,
            daa_score: self.anchor_daa_score,
            overlay_root: self.anchor_overlay_root,
        }
    }

    /// ADR-0039 §11.3 (K5 wiring) — the first PALW epoch of the CURRENT halted run, or `None` if this
    /// state is not `Halted`. Closed-form from the grace recurrence: `degraded_epochs` counts the
    /// consecutive non-Healthy epochs ending at `self.epoch`, and the mode flips to `Halted` exactly when
    /// `degraded_epochs > grace_epochs` — so the first HALTED epoch of the run is
    /// `epoch + grace_epochs + 1 - degraded_epochs`. A source block whose minting epoch is
    /// `>= halted_since(..)` was minted under a halted beacon (within THIS run); epochs before the run
    /// started are outside this state's memory and must be treated conservatively (not halted) by the
    /// caller. Saturating throughout (a `degraded_epochs` inconsistency can under- but never over-reach).
    pub fn halted_since(&self, grace_epochs: u64) -> Option<u64> {
        (self.mode == PalwBeaconMode::Halted.to_u8())
            .then(|| self.epoch.saturating_add(grace_epochs).saturating_add(1).saturating_sub(self.degraded_epochs))
    }

    /// Clause 6's `dns_finality_certificate_hash` derived on demand from the carried anchor facts.
    /// Returns `None` while no DNS-confirmed anchor has entered the recurrence (the zero
    /// bootstrap anchor certifies nothing — `chain_commit` over a degenerate zero-cert would be
    /// reproducible by every private fork, voiding I-4 exactly when the chain is weakest). The C5
    /// atomic flip rejects algo-4 while this is `None`.
    pub fn dns_certificate_hash(&self) -> Option<Hash64> {
        let anchor = self.anchor();
        anchor.is_confirmed().then(|| dns_finality_certificate_hash_v1(&anchor))
    }
}

/// ADR-0039 §18.3 — one epoch's finalized beacon facts in the pruning-proof `beacon_chain`. It carries
/// exactly the inputs a historic verifier needs to (a) re-check the `R_E` hash-chain step
/// (`seed == beacon_seed(prev_seed, dns_anchor, valid_reveals_root, missing_commitments_root, epoch)`)
/// and (b) re-derive that epoch's `chain_commit` from the anchor facts — without the raw commit/reveal
/// set. It is the compact projection of a [`PalwBeaconStateV1`] onto its seed-relevant fields (the two
/// diagnostic counts are dropped; the `version` is fixed at `1`). Frozen wire type (§33 freeze-first);
/// the pruning verifier + P2P flow that consume it are the D3 slice.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBeaconCheckpointV1 {
    pub epoch: u64,
    /// `R_E` for this epoch (the seed a same-epoch algo-4 ticket's eligibility draws over).
    pub seed: Hash64,
    pub dns_anchor: Hash64,
    pub anchor_blue_score: u64,
    pub anchor_daa_score: u64,
    pub anchor_overlay_root: Hash64,
    pub valid_reveals_root: Hash64,
    pub missing_commitments_root: Hash64,
    /// [`PalwBeaconMode`] discriminant + consecutive degraded epochs — needed so a verifier replays the
    /// exact `DegradedGrace` recurrence, not just the healthy path.
    pub mode: u8,
    pub degraded_epochs: u64,
}

impl PalwBeaconCheckpointV1 {
    /// Project a finalized per-epoch beacon state onto its pruning-proof checkpoint.
    pub fn from_state(s: &PalwBeaconStateV1) -> Self {
        Self {
            epoch: s.epoch,
            seed: s.seed,
            dns_anchor: s.dns_anchor,
            anchor_blue_score: s.anchor_blue_score,
            anchor_daa_score: s.anchor_daa_score,
            anchor_overlay_root: s.anchor_overlay_root,
            valid_reveals_root: s.valid_reveals_root,
            missing_commitments_root: s.missing_commitments_root,
            mode: s.mode,
            degraded_epochs: s.degraded_epochs,
        }
    }

    /// The carried anchor facts (the [`dns_finality_certificate_hash_v1`] inputs).
    pub fn anchor(&self) -> BeaconDnsAnchor {
        BeaconDnsAnchor {
            hash: self.dns_anchor,
            blue_score: self.anchor_blue_score,
            daa_score: self.anchor_daa_score,
            overlay_root: self.anchor_overlay_root,
        }
    }

    /// True iff this checkpoint's `seed` is the correct hash-chain successor of `prev_seed` under the
    /// §11.2 recurrence. The verifier walks `beacon_chain` in ascending epoch order checking this at each
    /// step (the first checkpoint's `prev_seed` is the bundle's `from_epoch − 1` boundary seed).
    pub fn seed_follows(&self, prev_seed: &Hash64) -> bool {
        self.seed == beacon_seed(prev_seed, &self.dns_anchor, &self.valid_reveals_root, &self.missing_commitments_root, self.epoch)
    }
}

/// ADR-0039 §18.3 / SUMMARY-BIND V2 — the PALW slice of the pruning proof: enough on-chain-equivalent data for a pruned
/// node to recompute historic algo-4 ticket validity (batch/leaf existence, certificate quorum,
/// activation/expiry, the beacon chain, target interval, chain_commit, eligibility hash, component work,
/// nullifier dedup) without the full history. V2 is an explicit wire break because its certificates
/// carry summary-bound V2 votes; legacy V1 bundles are not decoded as this type. The builder, pruning
/// verifier, and P2P request/response flow (§18.4) remain the D3 slice.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwEpochProofBundleV2 {
    /// Exact V2 envelope revision. V1 bundles did not carry this prefix and are not accepted.
    pub version: u16,
    pub from_epoch: u64,
    pub to_epoch: u64,
    pub beacon_chain: Vec<PalwBeaconCheckpointV1>,
    pub batch_manifests: Vec<PalwBatchManifestV1>,
    pub leaf_chunks: Vec<PalwLeafChunkV1>,
    pub certificates: Vec<PalwBatchCertificateV2>,
    pub revocations: Vec<PalwRevocationV1>,
    /// The frontier commitment of the active-nullifier set at `to_epoch` (§15.2), so the verifier can
    /// continue dedup from a pruned boundary without every historic nullifier.
    pub nullifier_frontier_root: Hash64,
    /// Canonical Compute v1 §17.5 fix 3 (I-12) — the compute-set records active over `[from_epoch,
    /// to_epoch]`. TAIL-appended (never reorder the fields above — borsh is positional). **Carried-but-
    /// unread** until the D3 builder + pruning verifier + re-genesis wire it: a pruned verifier recomputing
    /// historic `E` must apply each epoch's per-set `effective_compute_work_scale()` (via each record's
    /// activation/deprecation window) instead of a single global scale, so class-as-data does not
    /// contradict I-12. Default empty ⇒ the historic recompute falls back to the flat const scale, exactly
    /// as today; no shipped preset produces a bundle, so this is byte-neutral in practice.
    pub active_set_records: Vec<PalwComputeSetRecordV1>,
}

impl PalwEpochProofBundleV2 {
    /// Structural check that the `beacon_chain` is a contiguous ascending run over `[from_epoch,
    /// to_epoch]` whose seeds chain from `boundary_prev_seed` (the seed finalized at `from_epoch − 1`).
    /// Content checks (batch/cert/leaf existence, quorum, windows) are the pruning verifier's job; this
    /// is the beacon-chain linkage the verifier runs first.
    pub fn beacon_chain_links(&self, boundary_prev_seed: &Hash64) -> bool {
        if self.version != PALW_EPOCH_PROOF_BUNDLE_VERSION_V2 {
            return false;
        }
        if self.to_epoch < self.from_epoch {
            return false;
        }
        if self.beacon_chain.len() as u64 != self.to_epoch - self.from_epoch + 1 {
            return false;
        }
        let mut prev = *boundary_prev_seed;
        for (i, cp) in self.beacon_chain.iter().enumerate() {
            if cp.epoch != self.from_epoch + i as u64 || !cp.seed_follows(&prev) {
                return false;
            }
            prev = cp.seed;
        }
        true
    }
}

/// ADR-0039 §18.2 / D3 — the PALW frontier a pruned-IBD joiner needs to validate the first post-pruning-
/// point block: the pruning point's own block-keyed PALW state. Without it, `versioned_overlay_
/// commitment_root` cannot reconstruct the selected-parent transition state, the algo-4 ticket check has
/// no batch view to gate on, and the cross-ancestor nullifier dedup seeds empty (re-opening reuse of a
/// still-active pre-pp ticket).
///
/// Commitment boundary: this rides its own singleton store
/// (`DbPalwPrunedFrontierStore`, a fresh prefix), **not** the bincode-persisted
/// `PruningPointOverlaySnapshot` wrapper. That preserves the legacy wrapper encoding and Header-v3's
/// pinned commitment bytes. On a re-genesis Header-v4 network, this frontier is one component of
/// `PalwSelectedParentStateV2`; the first post-pp child's `overlay_commitment_root` therefore authenticates
/// it with the rest of the live PALW state before a transition is accepted (`c == v`). The complete
/// pruning transport additionally carries the beacon accumulator, provider view, paid-work window,
/// anti-spam closure, and DA state; its writer/import/recovery path is atomic and bounded.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwPrunedFrontierV1 {
    /// `beacon_state(pruning_point)` — the R_E seed + DNS anchor; MUST be seeded or the first post-pp v3
    /// block's overlay-root recompute panics.
    pub beacon_state: Option<PalwBeaconStateV1>,
    /// `overlay_view(pruning_point)` — the fork-relative batch lifecycle the ticket check (C5) gates on.
    pub overlay_view: Option<PalwBatchViewV1>,
    /// `lane_bits(pruning_point)` — the carried per-lane difficulty at the boundary (informational; the
    /// enforced clause-7 difficulty is a pure header-window function).
    pub lane_bits: Option<PalwLaneBitsV1>,
    /// `active_nullifiers(pruning_point)` — the retention window, so a joiner still detects reuse of a
    /// pre-pp ticket that is inside the window (else pruning would forget them and re-open double-use).
    pub active_nullifiers: PalwActiveNullifierSet,
}

impl PalwPrunedFrontierV1 {
    /// True iff the frontier carries no PALW state — the shipped-preset / pre-activation case (so a
    /// capture on a non-PALW net produces the byte-neutral empty default).
    pub fn is_empty(&self) -> bool {
        self.beacon_state.is_none() && self.overlay_view.is_none() && self.lane_bits.is_none() && self.active_nullifiers.is_empty()
    }
}

/// The inputs the epoch-boundary derivation gathers from the beacon store (past-relative) before
/// calling [`derive_beacon_epoch_state`]. Kept as an explicit struct so the derivation stays a pure
/// function of already-resolved sets (no store handle), unit-testable in isolation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeaconEpochInputs {
    /// Every `(bond, commitment)` that committed FOR this epoch (from the commit store).
    pub commits: Vec<(TransactionOutpoint, Hash64)>,
    /// The subset of `commits` whose reveal validly opened it (`matches_commit`), as
    /// `(bond, reveal_entropy_digest)`. The public commitment MUST NOT be placed in this vector.
    pub valid_reveals: Vec<(TransactionOutpoint, Hash64)>,
}

impl BeaconEpochInputs {
    /// `commits` minus the bonds present in `valid_reveals` — the committed-but-unrevealed set whose root
    /// feeds `beacon_seed` (and which a later slice slashes). Deterministic (preserves `commits` order;
    /// the root re-sorts canonically).
    pub fn missing_commitments(&self) -> Vec<(TransactionOutpoint, Hash64)> {
        let revealed: BTreeSet<(Hash64, u32)> = self.valid_reveals.iter().map(|(o, _)| (o.transaction_id, o.index)).collect();
        self.commits.iter().filter(|(o, _)| !revealed.contains(&(o.transaction_id, o.index))).cloned().collect()
    }
}

/// ADR-0039 §11.2 — derive epoch `E`'s beacon state from the resolved commit/reveal sets, the prior
/// epoch's seed, and the DNS-finalized anchor. Pure (store-free): the caller has already gathered
/// [`BeaconEpochInputs`] past-relative to the deriving block. Computes the two roots, decides the mode
/// (`beacon_mode` from DNS health + stake quorum + carried grace counter), and advances the seed —
/// `beacon_seed` on `Healthy`, else the previous seed is carried (design §11.3: `DegradedGrace` reuses
/// `R_{E-1}`; `Halted` also carries it, with algo-4 acceptance gated off elsewhere). `stake_of` resolves
/// a committing bond to its epoch stake. Returns the record persisted for the boundary block.
#[allow(clippy::too_many_arguments)]
pub fn derive_beacon_epoch_state(
    epoch: u64,
    prev_seed: &Hash64,
    anchor: &BeaconDnsAnchor,
    inputs: &BeaconEpochInputs,
    dns_healthy: bool,
    prev_degraded_epochs: u64,
    grace_epochs: u64,
    quorum_num: u16,
    quorum_den: u16,
    stake_of: impl Fn(&TransactionOutpoint) -> u128,
) -> PalwBeaconStateV1 {
    let valid_reveals_root = beacon_valid_reveals_root(&inputs.valid_reveals);
    let missing = inputs.missing_commitments();
    let missing_commitments_root = beacon_missing_commitments_root(&missing);

    let committed: Vec<TransactionOutpoint> = inputs.commits.iter().map(|(o, _)| *o).collect();
    let revealed: Vec<TransactionOutpoint> = inputs.valid_reveals.iter().map(|(o, _)| *o).collect();
    let quorum = beacon_quorum_reached(&committed, &revealed, quorum_num, quorum_den, &stake_of);

    // Grace counter recurrence: reset on Healthy, else +1 (bounded by the mode decision).
    let degraded_epochs = if dns_healthy && quorum { 0 } else { prev_degraded_epochs.saturating_add(1) };
    let mode = beacon_mode(dns_healthy, quorum, degraded_epochs, grace_epochs);

    let seed = match mode {
        // §11.2: full commit-reveal seed advance. The seed preimage folds only the anchor HASH — the
        // clause-6 cert facts ride alongside in the record without altering R_E's semantics.
        PalwBeaconMode::Healthy => beacon_seed(prev_seed, &anchor.hash, &valid_reveals_root, &missing_commitments_root, epoch),
        // §11.3: grace/halt reuse the previous seed (no new unbiasable randomness this epoch).
        PalwBeaconMode::DegradedGrace | PalwBeaconMode::Halted => *prev_seed,
    };

    PalwBeaconStateV1 {
        version: 1,
        epoch,
        seed,
        // Static-audit C-01: the fork-relative chain is stitched by the CALLER, which is the only
        // place that knows this block's selected parent and the full set of epochs it closed. This
        // pure derivation returns the record for ONE epoch and leaves both fields empty.
        closed_seeds: Vec::new(),
        prev_closer: None,
        dns_anchor: anchor.hash,
        anchor_blue_score: anchor.blue_score,
        anchor_daa_score: anchor.daa_score,
        anchor_overlay_root: anchor.overlay_root,
        valid_reveals_root,
        missing_commitments_root,
        mode: mode.to_u8(),
        degraded_epochs,
        valid_reveal_count: inputs.valid_reveals.len() as u32,
        missing_commit_count: missing.len() as u32,
    }
}

/// ADR-0039 §11.2 / §18.1 — the per-epoch on-chain accumulation the beacon store persists (keyed by
/// epoch): every commitment that committed FOR this epoch, plus the subset that validly revealed
/// (`matches_commit` at reveal-accept time). Valid reveals retain the distinct-domain `Hash64`
/// [`beacon_reveal_entropy_digest`] of `random_64`; raw secrets need not remain in the state. At the
/// epoch boundary this maps to
/// [`BeaconEpochInputs`] and feeds [`derive_beacon_epoch_state`].
///
/// Mainnet, testnet-10, simnet and devnet use the inactive `u64::MAX` fence. The three PALW presets use
/// an activation score of 0 and write the beacon accumulator per chain block. This encoding is part of
/// the database-versioned PALW state.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBeaconEpochAccumV1 {
    pub version: u16,
    pub commits: Vec<(TransactionOutpoint, Hash64)>,
    /// `(bond, reveal_entropy_digest)` entries; never `(bond, commitment)`.
    pub valid_reveals: Vec<(TransactionOutpoint, Hash64)>,
}

impl PalwBeaconEpochAccumV1 {
    pub fn new() -> Self {
        Self { version: 1, commits: Vec::new(), valid_reveals: Vec::new() }
    }

    /// Record a commit for `bond`. Idempotent on the bond outpoint (a bond commits once per epoch — a
    /// duplicate is ignored, keeping the first-seen commitment).
    pub fn record_commit(&mut self, bond: TransactionOutpoint, commitment: Hash64) {
        if !self.commits.iter().any(|(o, _)| *o == bond) {
            self.commits.push((bond, commitment));
        }
    }

    /// The commitment `bond` committed for this epoch, if any (used to check a reveal's `matches_commit`).
    pub fn commitment_of(&self, bond: &TransactionOutpoint) -> Option<Hash64> {
        self.commits.iter().find(|(o, _)| o == bond).map(|(_, c)| *c)
    }

    /// Record the entropy digest of a reveal that validly opened `bond`'s commitment. Idempotent on
    /// the bond outpoint. The caller computes this with [`PalwBeaconRevealV1::entropy_digest`] only
    /// after `matches_commit` succeeds.
    pub fn record_valid_reveal(&mut self, bond: TransactionOutpoint, reveal_entropy_digest: Hash64) {
        if !self.valid_reveals.iter().any(|(o, _)| *o == bond) {
            self.valid_reveals.push((bond, reveal_entropy_digest));
        }
    }

    /// Map to the pure derivation inputs (design §11.2).
    pub fn to_inputs(&self) -> BeaconEpochInputs {
        BeaconEpochInputs { commits: self.commits.clone(), valid_reveals: self.valid_reveals.clone() }
    }
}

impl Default for PalwBeaconEpochAccumV1 {
    fn default() -> Self {
        Self::new()
    }
}

/// Non-retroactive revocation (design §9.5): invalidates only future unused leaves from
/// `effective_daa_score` onward.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwRevocationV1 {
    pub version: u16,
    pub batch_id: Hash64,
    pub effective_daa_score: u64,
    pub reason_code: u16,
    pub evidence_hash: Hash64,
}

// =============================================================================================
// PALW overlay stateless payload admission (subnetworks 0x30..0x37).
// =============================================================================================

/// The only PALW payload version accepted by the v1 overlay wire format.
pub const PALW_PAYLOAD_VERSION_V1: u16 = 1;

/// Breaking certificate/vote wire revision that binds certificate summary fields into every auditor
/// signature. V1 votes and certificates are intentionally unsupported and must be regenerated.
pub const PALW_BATCH_CERTIFICATE_VERSION_V2: u16 = 2;
/// Breaking pruning-envelope revision carrying summary-bound V2 certificates.
pub const PALW_EPOCH_PROOF_BUNDLE_VERSION_V2: u16 = 2;
/// kaspa-pq ADR-0040 §5.15 — the leaf-chunk payload version that carried membership proofs. As of the
/// D3-b cutover this is a REJECTED historical version (kept as a named constant so the rejection tests
/// and the acceptance arm can say precisely WHICH version they refuse); the live version is
/// [`PALW_LEAF_CHUNK_VERSION_V3`].
pub const PALW_LEAF_CHUNK_VERSION_V2: u16 = 2;
/// ADR-0045 D3-b — the leaf-chunk payload version that carries membership proofs AND per-leaf PCPB
/// witnesses.
///
/// This is a LEAF-CHUNK-ONLY bump. [`check_palw_version`] is shared by every payload kind and enforces
/// v1; `validate_leaf_chunk` substitutes its own check for that shared one. The V2 certificate likewise
/// has its own exact check; every remaining arm stays v1. Never widen the shared check to "1 or 2 or 3".
/// v2 (no witnesses) is rejected for the same reason v2 rejected v1: a lenient parse defaulting the new
/// vector to empty would ship the exact partial-gating state D3-b forbids.
pub const PALW_LEAF_CHUNK_VERSION_V3: u16 = 3;
/// Static upper bound on a membership proof's length, from [`PALW_MAX_BATCH_LEAVES_V1`] = 256
/// (`ceil(log2(256)) = 8`). See `validate_leaf_chunk` for why the EXACT bound lives elsewhere.
pub const PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN: usize = 8;
/// Static upper bound on a PCPB snapshot/assignment membership path (2^32 providers is absurd; the
/// bound exists so nested-vector decoding of a hostile witness stays O(bounded) before any hashing).
pub const PALW_MAX_PCPB_MEMBERSHIP_SIBLINGS: usize = 32;
/// Static upper bound on the self-arm partner-B receipt preimage carried in a PCPB witness. The honest
/// preimage is `TAG ‖ a_commit ‖ tail` where the tail is a receipt signing preimage (≈ 3.2 KiB with an
/// ML-DSA-87 session key); 8 KiB leaves headroom without letting one witness eat the payload cap.
pub const PALW_MAX_PCPB_RECEIPT_PREIMAGE_BYTES: usize = 8 * 1024;
/// Hard per-transaction PALW payload cap, checked before Borsh decoding. The largest object is a V2
/// certificate containing ML-DSA-87 votes; 512 KiB leaves room for the frozen hard vote cap while
/// preventing an unbounded payload from reaching nested-vector decoding.
pub const PALW_MAX_OVERLAY_PAYLOAD_BYTES: usize = 512 * 1024;
/// Hard wire cap independent of the smaller per-network `PalwParams::max_batch_leaves` policy.
pub const PALW_MAX_BATCH_LEAVES_V1: usize = 256;
/// Hard wire cap for provider metadata vectors.
pub const PALW_MAX_PROVIDER_RUNTIME_CLASSES_V1: usize = 64;
pub const PALW_MAX_PROVIDER_CAPACITY_ENTRIES_V1: usize = 256;
/// Hard wire cap. A network may select fewer auditors through `PalwParams::auditor_count`.
pub const PALW_MAX_AUDITOR_VOTES_V2: usize = 64;
/// Reward scripts embedded in public leaves are metadata, not transaction outputs. Bound them here
/// so one leaf cannot consume the whole payload cap with an unusable script.
pub const PALW_MAX_REWARD_SCRIPT_BYTES_V1: usize = 1024;

/// Typed view of the reserved PALW subnetwork byte band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTxKind {
    ProviderBond,
    BatchManifest,
    LeafChunk,
    BatchCertificate,
    Revocation,
    BeaconCommit,
    BeaconReveal,
    /// ADR-0040 ECON-03 leg 5 — the authorized provider exit ([`PalwProviderUnbondRequestV1`]).
    /// Its wire payload IS frozen and bound to [`PalwProviderBondPayloadV1::owner_public_key`];
    /// authorization is contextual (`palw_provider_unbond_authorized`).
    ProviderUnbond,
    /// ADR-0040 P1-6 — per-block ticket authorization (`PalwBlockAuthorizationV1`).
    BlockAuthorization,
    /// DA-01 bonded on-chain availability challenge (0x3a). Byte 0x39 remains strictly reserved for
    /// future cross-fork slashing evidence and is intentionally absent from this enum.
    DaChallenge,
    /// DA-01 provider-owner signed chunk response (0x3b).
    DaResponse,
    /// DA-01 objective post-deadline timeout evidence (0x3c).
    DaTimeoutEvidence,
    /// Node-anchored web-search availability challenge (0x3d, `PalwSearchChallengeTxV1`). May carry
    /// the scheduler-signed registration proof (assignment + signed anchor) that lazily registers
    /// the challenged obligation against the bonded scheduler registry.
    SearchChallenge,
    /// Search-availability chunk response (0x3e, `PalwSearchResponseTxV1`), proof-self-authorizing.
    SearchResponse,
    /// Search-availability post-deadline timeout evidence (0x3f, `PalwSearchTimeoutTxV1`).
    SearchTimeout,
}

impl PalwTxKind {
    #[inline]
    pub const fn from_subnetwork_byte(value: u8) -> Option<Self> {
        Some(match value {
            0x30 => Self::ProviderBond,
            0x31 => Self::BatchManifest,
            0x32 => Self::LeafChunk,
            0x33 => Self::BatchCertificate,
            0x34 => Self::Revocation,
            0x35 => Self::BeaconCommit,
            0x36 => Self::BeaconReveal,
            0x37 => Self::ProviderUnbond,
            0x38 => Self::BlockAuthorization,
            // 0x39 is reserved by ADR-0040 for cross-fork slashing. Never reuse it for DA.
            0x3a => Self::DaChallenge,
            0x3b => Self::DaResponse,
            0x3c => Self::DaTimeoutEvidence,
            0x3d => Self::SearchChallenge,
            0x3e => Self::SearchResponse,
            0x3f => Self::SearchTimeout,
            _ => return None,
        })
    }
}

/// Stateless PALW overlay payload failure. Context-dependent rules (activation fence, beacon phase,
/// past-relative bond ownership/stake, signature verification, and duplicate-on-chain state) are
/// deliberately absent and must run in contextual validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTxError {
    Decode,
    UnsupportedKind(u8),
    PayloadTooLarge {
        len: usize,
        max: usize,
    },
    UnsupportedVersion(u16),
    InvalidPublicKeyLen(usize),
    InvalidSignatureLen(usize),
    InvalidCount {
        field: &'static str,
        count: usize,
        min: usize,
        max: usize,
    },
    InvalidField(&'static str),
    NonCanonical(&'static str),
    /// ADR-0040 ECON-03: a `ProviderBond` tx declared an amount but created no output-0 to lock it.
    MissingProviderBondOutput,
    /// ADR-0040 ECON-03: output-0 exists but does not lock the declared `amount_sompi`.
    ProviderBondOutputValueMismatch {
        expected: u64,
        got: u64,
    },
    /// ADR-0040 ECON-03: output-0 locks the right value to the wrong key — collateral the bond's
    /// owner does not control is not that owner's collateral.
    ProviderBondOutputScriptMismatch,
}

impl Display for PalwTxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode => write!(f, "PALW overlay payload failed to decode"),
            Self::UnsupportedKind(kind) => write!(f, "PALW overlay subnetwork kind 0x{kind:02x} has no frozen v1 payload"),
            Self::PayloadTooLarge { len, max } => write!(f, "PALW overlay payload length {len} exceeds {max}"),
            Self::UnsupportedVersion(version) => write!(f, "unsupported PALW overlay payload version {version}"),
            Self::InvalidPublicKeyLen(len) => write!(f, "PALW ML-DSA-87 public key length {len} is invalid"),
            Self::InvalidSignatureLen(len) => write!(f, "PALW ML-DSA-87 signature length {len} is invalid"),
            Self::InvalidCount { field, count, min, max } => {
                write!(f, "PALW {field} count {count} is outside {min}..={max}")
            }
            Self::InvalidField(field) => write!(f, "PALW field {field} is invalid"),
            Self::NonCanonical(field) => write!(f, "PALW field {field} is not canonically ordered/unique"),
            Self::MissingProviderBondOutput => write!(f, "PALW provider bond declares an amount but has no output-0 locking it"),
            Self::ProviderBondOutputValueMismatch { expected, got } => {
                write!(f, "PALW provider bond output-0 value {got} does not lock the declared amount {expected}")
            }
            Self::ProviderBondOutputScriptMismatch => {
                write!(f, "PALW provider bond output-0 is not locked to the owner's P2PKH-ML-DSA script")
            }
        }
    }
}

#[inline]
fn decode_palw_payload<T: BorshDeserialize>(payload: &[u8]) -> Result<T, PalwTxError> {
    // `borsh::from_slice` is strict: trailing bytes fail rather than being ignored.
    borsh::from_slice(payload).map_err(|_| PalwTxError::Decode)
}

#[inline]
fn check_palw_version(version: u16) -> Result<(), PalwTxError> {
    if version == PALW_PAYLOAD_VERSION_V1 { Ok(()) } else { Err(PalwTxError::UnsupportedVersion(version)) }
}

#[inline]
fn check_count(field: &'static str, count: usize, min: usize, max: usize) -> Result<(), PalwTxError> {
    if (min..=max).contains(&count) { Ok(()) } else { Err(PalwTxError::InvalidCount { field, count, min, max }) }
}

#[inline]
fn cmp_outpoint(a: &TransactionOutpoint, b: &TransactionOutpoint) -> Ordering {
    a.transaction_id.as_byte_slice().cmp(b.transaction_id.as_byte_slice()).then(a.index.cmp(&b.index))
}

fn validate_provider_bond(payload: &[u8]) -> Result<(), PalwTxError> {
    decode_and_check_provider_bond(payload).map(|_| ())
}

/// kaspa-pq **ADR-0040 (ECON-03)** — the canonical lock target of a provider bond: the P2PKH-ML-DSA
/// script that output-0 of a `ProviderBond` tx must pay, DERIVED from the payload's own
/// `owner_public_key` rather than declared alongside it.
///
/// This is the one place the PALW bond deliberately DEPARTS from the DNS stake bond's shape, and the
/// departure is a strengthening: [`crate::dns_finality::StakeBondPayload`] carries only *hashes*, so
/// it had to append an `owner_reward_spk_payload: [u8; 64]` field for `validate_stake_bond_tx` to
/// have a script to compare against — a second declared field that must itself be bound to the key.
/// [`PalwProviderBondPayloadV1`] already carries the full 2592-byte `owner_public_key`, so the lock
/// target is a pure function of a field that is already there. The spend payload deliberately uses
/// [`blake2b_512_address_payload`], the same keyed hash `OP_BLAKE2B_512` recomputes over the unlock
/// public key. It must not use [`validator_id_from_pubkey`]: that unkeyed digest is the overlay
/// credential identity, not a spend-address hash, and putting it in P2PKH would freeze the collateral
/// permanently. Consequences: the borsh layout of `PalwProviderBondPayloadV1` does NOT move (its layout
/// pin stays green), and there is no key↔script binding check to forget, because a mismatch is
/// unrepresentable.
/// Static-audit finding H-01 — the sponsor identity a batch-manifest view slot is charged to.
///
/// Keyed on the OWNER KEY, deliberately, not on the bond outpoint: splitting one operator's stake
/// across many bonds collapses to ONE tag and buys zero extra slots. Splitting across many KEYS does
/// buy slots, but each key then needs its own bonded collateral, which is exactly the cost the quota
/// is there to impose.
///
/// The value is the leading 128 bits of the same `blake2b_512_address_payload` that
/// [`provider_bond_lock_spk`] hashes into the P2PKH script, so the tag is derivable from either side
/// of the sponsorship proof (the bond record's key, or the script the carrier spent) without a
/// second derivation rule to keep in sync. 128 bits is a collision margin no adversary can work
/// against — a collision would let two operators share one quota, which helps neither.
pub fn palw_manifest_sponsor_tag(owner_public_key: &[u8]) -> (u64, u64) {
    let payload = blake2b_512_address_payload(owner_public_key);
    let bytes = payload.as_bytes();
    (
        u64::from_be_bytes(bytes[0..8].try_into().expect("address payload is 64 bytes")),
        u64::from_be_bytes(bytes[8..16].try_into().expect("address payload is 64 bytes")),
    )
}

/// The sponsor tag meaning "no sponsor recorded" — every pre-fence lifecycle row decodes to this.
pub const PALW_SPONSOR_UNATTRIBUTED: (u64, u64) = (0, 0);

/// Static-audit finding H-01 — the sponsorship facts `apply_manifest` needs, resolved by the caller
/// from the chain block's own coordinate.
///
/// Passed as one struct rather than three scalars so a caller cannot supply a tag while forgetting
/// to say whether sponsorship is required — the combination `{tag: UNATTRIBUTED, required: true}` is
/// the fail-closed case and must stay expressible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PalwManifestSponsorshipV1 {
    /// The sponsor resolved for THIS manifest carrier, or [`PALW_SPONSOR_UNATTRIBUTED`].
    pub tag: (u64, u64),
    /// Concurrent view slots one sponsor may hold. `0` disables the quota.
    pub max_view_batches_per_sponsor: u32,
    /// Whether an unsponsored manifest must be refused (i.e. the POV block is at or past the fence).
    pub required: bool,
}

impl PalwManifestSponsorshipV1 {
    /// The pre-fence value: nothing resolved, nothing required, no quota. Writing this produces a
    /// lifecycle row byte-identical to what every pre-fence block wrote.
    pub const INERT: Self = Self { tag: PALW_SPONSOR_UNATTRIBUTED, max_view_batches_per_sponsor: 0, required: false };
}

pub fn provider_bond_lock_spk(owner_public_key: &[u8]) -> ScriptPublicKey {
    p2pkh_mldsa87_spk(&blake2b_512_address_payload(owner_public_key).as_bytes())
}

/// kaspa-pq **ADR-0040 (ECON-03) — the value lock. This is the whole difference between a number and
/// collateral**, and it is `validate_stake_bond_tx` (dns_finality.rs) transposed leg-for-leg.
///
/// # What was wrong
///
/// [`PalwProviderBondPayloadV1::amount_sompi`] was a bare SELF-DECLARED integer: the only economic
/// check in the entire admission path was `amount_sompi != 0`. Nothing compared it to any UTXO, so a
/// provider with an empty wallet could declare `u64::MAX` and the transaction was valid — while the
/// 77 % `PALW_PROVIDER_BASE_BPS` coinbase carve was paid out against it, gated on nothing more than
/// a distinctness check between two outpoints that need not resolve to anything
/// (`leaf.provider_a_bond != leaf.provider_b_bond`). That second half — the reward path paying against
/// unresolved outpoints — is closed separately, at the reward coordinate: see
/// `palw_work_reward_class`'s ECON-03 rule and [`crate::coinbase::WorkRewardClass::ReplicaPalwUnbackedCollateral`].
///
/// # What makes it real (and what this ALONE does not)
///
/// Output-0 of the very transaction that declares the bond must LOCK the declared amount: its
/// `value` must equal `amount_sompi`, and its `script_public_key` must be the owner's own
/// P2PKH-ML-DSA script ([`provider_bond_lock_spk`]). The amount is therefore no longer trusted — it
/// is cross-checked against real, owner-controlled coins the same transaction creates, which is what
/// gives a future slashing rule something to consume.
///
/// This check alone is NOT sufficient and is not claimed to be: without the spend gate keeping that
/// output unspent for the bond's life, output-0 would be locked for exactly one block and the
/// collateral would evaporate. See `ProviderBondSpendFilter::locks` in `utxo_validation.rs` for leg 4
/// and [`PalwProviderUnbondRequestV1`] for the authorized delayed exit (leg 5).
///
/// Stateless — it inspects only the transaction's own output-0 — so it runs in the isolation
/// validator at the same coordinate as the DNS arm, where a rejection is LOUD. The admission
/// decision deliberately does not live in the overlay-effect arm, whose `Result` is discarded by the
/// caller (`let _ = ...`) and would therefore fail silently.
pub fn validate_provider_bond_tx(payload: &[u8], outputs: &[TransactionOutput]) -> Result<(), PalwTxError> {
    let bond = decode_and_check_provider_bond(payload)?;
    let output0 = outputs.first().ok_or(PalwTxError::MissingProviderBondOutput)?;
    if output0.value != bond.amount_sompi {
        return Err(PalwTxError::ProviderBondOutputValueMismatch { expected: bond.amount_sompi, got: output0.value });
    }
    if output0.script_public_key != provider_bond_lock_spk(&bond.owner_public_key) {
        return Err(PalwTxError::ProviderBondOutputScriptMismatch);
    }
    Ok(())
}

fn decode_and_check_provider_bond(payload: &[u8]) -> Result<PalwProviderBondPayloadV1, PalwTxError> {
    let bond: PalwProviderBondPayloadV1 = decode_palw_payload(payload)?;
    check_palw_version(bond.version)?;
    if bond.owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(PalwTxError::InvalidPublicKeyLen(bond.owner_public_key.len()));
    }
    check_count("provider.runtime_classes", bond.runtime_classes.len(), 1, PALW_MAX_PROVIDER_RUNTIME_CLASSES_V1)?;
    if !bond.runtime_classes.windows(2).all(|w| w[0].as_byte_slice() < w[1].as_byte_slice()) {
        return Err(PalwTxError::NonCanonical("provider.runtime_classes"));
    }
    check_count("provider.capacity_by_shape", bond.capacity_by_shape.len(), 1, PALW_MAX_PROVIDER_CAPACITY_ENTRIES_V1)?;
    if !bond.capacity_by_shape.windows(2).all(|w| w[0].0 < w[1].0) {
        return Err(PalwTxError::NonCanonical("provider.capacity_by_shape"));
    }
    if bond.capacity_by_shape.iter().any(|(_, capacity)| *capacity == 0) {
        return Err(PalwTxError::InvalidField("provider.capacity_by_shape.capacity"));
    }
    if bond.amount_sompi == 0 {
        return Err(PalwTxError::InvalidField("provider.amount_sompi"));
    }
    if bond.unbond_delay_epochs == 0 {
        return Err(PalwTxError::InvalidField("provider.unbond_delay_epochs"));
    }
    Ok(bond)
}

/// ADR-0040 ECON-03 (leg 5): stateless shape of a provider-unbond request — decodability, version,
/// and the owner key / signature lengths. The ML-DSA-87 signature and the bond binding
/// (`validator_id_from_pubkey(owner_public_key) == record.owner_pubkey_hash`, plus the Pending/Active
/// precondition) need a point of view and are therefore enforced by the block-validity authorizer
/// `palw_provider_unbond_authorized`, mirroring the DNS `validate_stake_unbond_payload` /
/// `unbond_request_authorized` split.
pub(crate) fn validate_provider_unbond(payload: &[u8]) -> Result<(), PalwTxError> {
    let req: PalwProviderUnbondRequestV1 = decode_palw_payload(payload)?;
    check_palw_version(req.version)?;
    if req.owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(PalwTxError::InvalidPublicKeyLen(req.owner_public_key.len()));
    }
    if req.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(req.signature.len()));
    }
    Ok(())
}

fn validate_manifest(payload: &[u8]) -> Result<(), PalwTxError> {
    let manifest: PalwBatchManifestV1 = decode_palw_payload(payload)?;
    check_palw_version(manifest.version)?;
    check_count("manifest.leaf_count", manifest.leaf_count as usize, 1, PALW_MAX_BATCH_LEAVES_V1)?;
    let expected_chunks = (manifest.leaf_count as usize).div_ceil(PALW_MAX_LEAVES_PER_CHUNK) as u16;
    if manifest.chunk_count != expected_chunks {
        return Err(PalwTxError::InvalidField("manifest.chunk_count"));
    }
    if !(manifest.registration_epoch < manifest.activation_not_before_epoch
        && manifest.activation_not_before_epoch < manifest.expiry_epoch)
    {
        return Err(PalwTxError::InvalidField("manifest.epoch_range"));
    }
    Ok(())
}

/// Returns whether a PALW reward script is representable as a coinbase output.
///
/// ## Admission requirement
///
/// A leaf's `provider_{a,b}_reward_script` is emitted as a coinbase output by
/// `expected_coinbase_transaction` when a descendant merges the algo-4 source. But every coinbase output
/// must independently satisfy two rules enforced on every block in isolation:
///
///   * `check_transaction_pq_output_classes` requires the ML-DSA-87 P2PKH class;
///   * `check_coinbase_in_isolation` — the script must be `<= coinbase_payload_script_public_key_max_len`
///     (150 bytes on every preset).
///
/// ## Exact template
///
/// `is_pq_standard()` admits exactly one class, and that class has exactly one byte layout: the 69-byte
/// template built by [`crate::dns_finality::p2pkh_mldsa87_spk`] (ADR-0019 §8). Matching the template
/// exactly satisfies both rules without adding a script parser dependency to consensus-core.
pub fn palw_reward_script_is_coinbase_representable(spk: &ScriptPublicKey) -> bool {
    // ADR-0019 §8 template opcodes, mirroring `p2pkh_mldsa87_spk`.
    const OP_DUP: u8 = 0x76;
    const OP_BLAKE2B_512: u8 = 0xc4;
    const OP_DATA64: u8 = 0x40;
    const OP_EQUAL_VERIFY: u8 = 0x88;
    const OP_CHECKSIG_MLDSA87: u8 = 0xa6;
    const P2PKH_MLDSA87_SCRIPT_LEN: usize = 69;

    if spk.version() != 0 {
        return false;
    }
    let s = spk.script();
    s.len() == P2PKH_MLDSA87_SCRIPT_LEN
        && s[0] == OP_DUP
        && s[1] == OP_BLAKE2B_512
        && s[2] == OP_DATA64
        && s[67] == OP_EQUAL_VERIFY
        && s[68] == OP_CHECKSIG_MLDSA87
    // s[3..67] is the free 64-byte BLAKE2b-512 payload — any value is payable.
}

/// ADR-0040 P1-6 — isolation validity for a ticket authorization payload.
///
/// Structural only: the *binding* checks (does it match this header, is the key the leaf's declared
/// authority, does the signature verify) are contextual and live in body-validation clause 7, because
/// they need the block and the resolved leaf. Isolation just rejects malformed or wrong-sized objects
/// cheaply, before any signature work.
fn validate_block_authorization(payload: &[u8]) -> Result<(), PalwTxError> {
    let auth: PalwBlockAuthorizationV1 = decode_palw_payload(payload)?;
    check_palw_version(auth.version)?;
    if auth.authority_public_key.len() != crate::dns_finality::STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(PalwTxError::InvalidField("authorization.authority_public_key"));
    }
    if auth.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(auth.signature.len()));
    }
    // ADR-0040 (AUTH-TXSHAPE) — the payload must be the CANONICAL encoding of the object it decodes to,
    // not merely an encoding that decodes to it. `decode_palw_payload` is already strict about trailing
    // bytes, but the authorization's payload is hashed into the transaction's merkle leaf, and on a lane
    // with no proof-of-work every distinct-but-equivalent encoding would be another free block hash for
    // the same signed authorization. The round-trip is a total comparison, so it cannot miss a field.
    if borsh::to_vec(&auth).map(|canonical| canonical != payload).unwrap_or(true) {
        return Err(PalwTxError::NonCanonical("authorization.payload"));
    }
    Ok(())
}

fn validate_public_leaf(leaf: &PalwPublicLeafV1, batch_id: &Hash64) -> Result<(), PalwTxError> {
    check_palw_version(leaf.version)?;
    if leaf.batch_id != *batch_id {
        return Err(PalwTxError::InvalidField("leaf.batch_id"));
    }
    if PalwProofType::from_u8(leaf.proof_type).is_none() {
        return Err(PalwTxError::InvalidField("leaf.proof_type"));
    }
    if leaf.quantum_count == 0 {
        return Err(PalwTxError::InvalidField("leaf.quantum_count"));
    }
    let object_len = leaf.receipt_da_object_len as usize;
    let expected_chunks = object_len.div_ceil(da::PALW_DA_CHUNK_BYTES);
    if leaf.receipt_da_root == Hash64::default()
        || object_len == 0
        || object_len > da::PALW_DA_MAX_OBJECT_BYTES
        || leaf.receipt_da_chunk_count == 0
        || leaf.receipt_da_chunk_count as usize > da::PALW_DA_MAX_CHUNKS
        || leaf.receipt_da_chunk_count as usize != expected_chunks
    {
        return Err(PalwTxError::InvalidField("leaf.receipt_da_metadata"));
    }
    match leaf.receipt_da_object_version {
        da::PALW_RECEIPT_DA_OBJECT_VERSION_V1 => {
            if leaf.receipt_v3_compute_set_id != Hash64::default()
                || leaf.receipt_v3_job_challenge != Hash64::default()
                || leaf.receipt_v3_issued_epoch != 0
                || leaf.receipt_v3_expires_epoch != 0
            {
                return Err(PalwTxError::InvalidField("leaf.receipt_v3_legacy_sentinel"));
            }
        }
        da::PALW_RECEIPT_DA_OBJECT_VERSION_V2 => {
            if leaf.receipt_v3_compute_set_id == Hash64::default()
                || leaf.receipt_v3_job_challenge == Hash64::default()
                || leaf.job_nullifier != leaf.receipt_v3_job_challenge
                || leaf.receipt_v3_issued_epoch > leaf.registered_epoch
                || leaf.receipt_v3_expires_epoch != leaf.expiry_epoch
            {
                return Err(PalwTxError::InvalidField("leaf.receipt_v3_expectations"));
            }
        }
        _ => return Err(PalwTxError::InvalidField("leaf.receipt_da_object_version")),
    }
    // ADR-0045 D3-b (design memo §1) — PCPB field coherence, context-free half. The branch tag must be
    // a known kind, and the self-arm anchor fields must be present exactly on the self branch: an
    // external leaf carrying an a_commit (or vice versa) is a category error that would otherwise let
    // one branch wear the other's clothes at the acceptance gate. Zero-ness of the snapshot roots is
    // NOT checked here — a zero root simply can never equal a store-resolved commitment at clause 12,
    // and legacy Object-V1 leaves (whose PCPB fields are all sentinels) die at clause 11's
    // re-derivation, both fail-closed at the coordinate that owns the context.
    match leaf.dispatch_kind {
        PALW_DISPATCH_KIND_BEACON_ASSIGNED => {
            if leaf.a_commit != Hash64::default() || leaf.a_commit_epoch != 0 {
                return Err(PalwTxError::InvalidField("leaf.pcpb_external_sentinel"));
            }
        }
        PALW_DISPATCH_KIND_SELF_SERIAL => {
            if leaf.a_commit == Hash64::default() {
                return Err(PalwTxError::InvalidField("leaf.pcpb_a_commit"));
            }
        }
        _ => return Err(PalwTxError::InvalidField("leaf.dispatch_kind")),
    }
    // ADR-0040 ECON-03 shape rule: a leaf must not name the same bond for both halves of a replica pair, which
    // would let one provider's collateral back both replicas and defeat the point of running k = 2.
    //
    // It is NOT the collateral check, and it never could be here. Whether an outpoint resolves to
    // ACTIVE collateral is a question about a DAA score along a particular chain, and this validator is
    // context-free by construction — BIND-03 settled that the batch view stays at the body/mergeset
    // coordinate and that body validity must not read point-of-view state. Resolving here would also
    // make an already-accepted batch retroactively invalid whenever a bond it names is unbonded by a
    // third party.
    //
    // The resolution lives at the reward/virtual coordinate instead: `palw_work_reward_class`
    // (virtual_processor/utxo_validation.rs) looks BOTH outpoints up in the selected-parent
    // `ProviderBondView` and classifies the source `ReplicaPalwUnbackedCollateral` — paid nothing — if
    // either fails to resolve `Active`. Until that rule existed, this line was the ONLY thing standing
    // between the 77 % provider base and two arbitrary numbers.
    if leaf.provider_a_bond == leaf.provider_b_bond {
        return Err(PalwTxError::InvalidField("leaf.provider_bonds"));
    }
    // ADR-0040 P0-4 (ECON-01) — both reward scripts must be payable AS COINBASE OUTPUTS, because that is
    // exactly what `expected_coinbase_transaction` emits them as. A script that passes here but not the
    // coinbase rules would brick every descendant that merges the algo-4 source. The old check was a
    // 1024-byte length bound only, which is neither the PQ-class rule nor the 150-byte coinbase bound.
    if !palw_reward_script_is_coinbase_representable(&leaf.provider_a_reward_script)
        || !palw_reward_script_is_coinbase_representable(&leaf.provider_b_reward_script)
    {
        return Err(PalwTxError::InvalidField("leaf.reward_script"));
    }
    if !(leaf.registered_epoch < leaf.activation_epoch && leaf.activation_epoch < leaf.expiry_epoch) {
        return Err(PalwTxError::InvalidField("leaf.epoch_range"));
    }
    Ok(())
}

/// Context-free ADR-0040 §5.15 leaf-chunk validator.
///
/// # Proof-length validation
///
/// This stage enforces the static bound `proof.len() <= 8` from
/// `PALW_MAX_BATCH_LEAVES_V1 = 256`. The exact bound
/// `proof.len() == palw_leaf_merkle_depth(manifest.leaf_count)` needs the manifest and belongs at
/// the acceptance gate, which has already loaded the manifest.
///
/// The static bound rejects malformed chunks before state access. The exact bound makes the proof for a
/// given `(leaf, index, root)` unique, closing the
///   variable-length-path forgeries that a mere upper bound leaves open.
fn validate_leaf_chunk(payload: &[u8]) -> Result<(), PalwTxError> {
    let chunk: PalwLeafChunkV1 = decode_palw_payload(payload)?;
    // NOT `check_palw_version`: leaf chunks are v3 (membership proofs + PCPB witnesses), every other
    // payload kind is still v1. v1/v2 leaf chunks are rejected — a lenient parse defaulting `proofs`
    // or `witnesses` to empty would reopen the CHUNK-INDEX SQUAT hole (v1) or ship the partial PCPB
    // gate D3-b forbids (v2).
    if chunk.version != PALW_LEAF_CHUNK_VERSION_V3 {
        return Err(PalwTxError::UnsupportedVersion(chunk.version));
    }
    check_count("leaf_chunk.leaves", chunk.leaves.len(), 1, PALW_MAX_LEAVES_PER_CHUNK)?;
    if chunk.proofs.len() != chunk.leaves.len() {
        return Err(PalwTxError::InvalidCount {
            field: "leaf_chunk.proofs",
            count: chunk.proofs.len(),
            min: chunk.leaves.len(),
            max: chunk.leaves.len(),
        });
    }
    if chunk.witnesses.len() != chunk.leaves.len() {
        return Err(PalwTxError::InvalidCount {
            field: "leaf_chunk.witnesses",
            count: chunk.witnesses.len(),
            min: chunk.leaves.len(),
            max: chunk.leaves.len(),
        });
    }
    for proof in &chunk.proofs {
        if proof.len() > PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN {
            return Err(PalwTxError::InvalidCount {
                field: "leaf_chunk.proof_len",
                count: proof.len(),
                min: 0,
                max: PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN,
            });
        }
    }
    for witness in &chunk.witnesses {
        validate_pcpb_witness_shape(witness)?;
    }
    let mut ticket_nullifiers = HashSet::with_capacity(chunk.leaves.len());
    for leaf in &chunk.leaves {
        validate_public_leaf(leaf, &chunk.batch_id)?;
        // I-13: leaves publish nullifier COMMITMENTS; distinct nullifiers ⇒ distinct commitments, so the
        // per-batch uniqueness check is over the commitments.
        if !ticket_nullifiers.insert(leaf.ticket_nullifier_commitment) {
            return Err(PalwTxError::NonCanonical("leaf_chunk.ticket_nullifiers"));
        }
    }
    if !chunk.leaves.windows(2).all(|w| w[0].leaf_index < w[1].leaf_index) {
        return Err(PalwTxError::NonCanonical("leaf_chunk.leaf_indices"));
    }
    Ok(())
}

/// ADR-0045 D3-b — context-free SHAPE bounds of one PCPB witness: vector lengths and fixed ML-DSA-87
/// widths only, all checked BEFORE any hashing or state access (the cheap-rejection half of the split
/// `validate_leaf_chunk` documents for membership proofs). The semantic half — challenge re-derivation
/// and dispatch-evidence validity against the resolved snapshot/beacon — is the acceptance arm's
/// clauses 11/12.
fn validate_pcpb_witness_shape(witness: &PalwLeafPcpbWitnessV1) -> Result<(), PalwTxError> {
    let membership_len = |field: &'static str, m: &PalwMerkleMembership| {
        if m.siblings.len() > PALW_MAX_PCPB_MEMBERSHIP_SIBLINGS {
            Err(PalwTxError::InvalidCount { field, count: m.siblings.len(), min: 0, max: PALW_MAX_PCPB_MEMBERSHIP_SIBLINGS })
        } else {
            Ok(())
        }
    };
    match &witness.dispatch {
        PalwDispatchEvidence::BeaconAssigned(p) => {
            for (field, m) in [
                ("pcpb.slot_a.snapshot_membership", &p.slot_a.snapshot_membership),
                ("pcpb.slot_a.assignment_membership", &p.slot_a.assignment_membership),
                ("pcpb.slot_b.snapshot_membership", &p.slot_b.snapshot_membership),
                ("pcpb.slot_b.assignment_membership", &p.slot_b.assignment_membership),
            ] {
                membership_len(field, m)?;
            }
        }
        PalwDispatchEvidence::SelfSerial(p) => {
            for (field, m) in [
                ("pcpb.a_snapshot_membership", &p.a_snapshot_membership),
                ("pcpb.b_snapshot_membership", &p.b_snapshot_membership),
                ("pcpb.b_assignment_membership", &p.b_assignment_membership),
            ] {
                membership_len(field, m)?;
            }
            if p.b_ml_dsa_pk.len() != STAKE_VALIDATOR_PUBKEY_LEN {
                return Err(PalwTxError::InvalidPublicKeyLen(p.b_ml_dsa_pk.len()));
            }
            if p.b_signature.len() != STAKE_ATTESTATION_SIG_LEN {
                return Err(PalwTxError::InvalidSignatureLen(p.b_signature.len()));
            }
            if p.b_receipt_preimage.len() > PALW_MAX_PCPB_RECEIPT_PREIMAGE_BYTES {
                return Err(PalwTxError::InvalidCount {
                    field: "pcpb.b_receipt_preimage",
                    count: p.b_receipt_preimage.len(),
                    min: 0,
                    max: PALW_MAX_PCPB_RECEIPT_PREIMAGE_BYTES,
                });
            }
        }
    }
    Ok(())
}

/// kaspa-pq **ADR-0040 §5.17.3 (§CERT-REDERIVE, bounded inclusion rule)** — the maximum distance, in
/// PALW epochs, that a batch certificate may be included PAST its own `audit_beacon_epoch`.
///
/// # Inclusion bound
///
/// The audit-epoch beacon seed `R_{audit_beacon_epoch − 1}` is resolved by a buried selected-parent walk
/// ([`resolve_palw_audit_epoch_seed`], `consensus/src/processes/palw.rs`). That walk is FAIL-OPEN by
/// nature — if the audit epoch's header is pruned, the walk simply cannot find it. Making certificate
/// verification treat "seed unresolvable" as **fail-CLOSED (reject)** is only sound if a *valid* seed is
/// guaranteed to still be reachable; otherwise a censor could sit on an honest certificate until its
/// audit epoch falls off the pruning window and it becomes unverifiable-hence-rejected. This rule closes
/// that gap: a certificate must be included within `N` epochs of its audit epoch, and `N` is chosen so
/// the audit-epoch header is always still buried (unpruned) — far smaller than the pruning depth.
///
/// # Anchor for `N` (from [`PalwBatchManifestV1::admission_valid`])
///
/// A batch registered at epoch `r` has, by admission, `expiry_epoch <= r + 2·registration_lead_epochs +
/// audit_window_epochs + active_window_epochs` (`min_activation = r + lead + audit`; `max_activation =
/// min_activation + lead`; `expiry <= activation + active`). A certificate is an audit OF a registered
/// batch, so `audit_beacon_epoch >= r`, and it is only referenceable while the including block's epoch is
/// `< expiry_epoch`. Hence for ANY valid inclusion `block_epoch − audit_beacon_epoch < expiry − r <=
/// 2·lead + audit + active`. Using that sum as `N` therefore never strands a timely certificate, while
/// any inclusion beyond it is already past the batch's expiry (a dead reference) — exactly the region
/// where the seed walk's fail-open may soundly become fail-closed. Saturating throughout so a params
/// overflow cannot wrap `N` down to a tiny value that WOULD strand valid certificates.
///
/// `verify_certificate_attestation` requires a certificate to
/// be included within this window of its audit epoch; that is what turns the seed resolver's fail-open
/// (a pruned / unreachable audit-epoch header) into a sound fail-CLOSED rejection — a timely certificate
/// is never stranded, and anything past the window is already past expiry.
pub fn palw_audit_epoch_inclusion_window_epochs(admission: &PalwBatchAdmissionParams) -> u64 {
    admission
        .registration_lead_epochs
        .saturating_mul(2)
        .saturating_add(admission.audit_window_epochs)
        .saturating_add(admission.active_window_epochs)
}

/// **ADR-0045 D3-a — how many past epochs of `R_E` the seed history must retain.**
///
/// `ceil(paid_work_walk_bound_daa / epoch_len) + audit inclusion window + 2`: the batch lifecycle
/// expressed in epochs, plus the certificate-inclusion window, plus boundary slack. This is the
/// epoch-space form of the same discipline the paid-work walk already obeys — a verification
/// lookback must land inside RETAINED territory — which is what keeps the store bounded by
/// parameters rather than by chain length.
///
/// PCPB re-derives `derive_b(R_e, …)` for the epoch a ticket was bound to, and that epoch can be
/// older than anything the three existing seed sources can answer for: the header walk fails
/// closed below the pruning point, per-block state rows are deleted by the pruning pass, and the
/// accumulator's `retain_future_of` drops past epochs BY DESIGN (it carries the seed's material,
/// not the seed). Precondition (a) of the wiring decision is exactly this gap.
/// D3-b amendment to the D3-a arithmetic: `+ freshness_window_epochs (w)`. Clause 11 re-derives the
/// job challenge under `R_{issued_epoch}`, and `issued_epoch` may trail `registered_epoch` by up to
/// `w`; the original window covered only the audit lookback and would have made the OLDEST honest
/// challenge epoch unresolvable — a fail-closed rejection of an honest leaf, the P1-7 failure shape.
pub fn palw_beacon_seed_history_window_epochs(
    admission: &PalwBatchAdmissionParams,
    epoch_length_daa: u64,
    freshness_window_epochs: u64,
) -> u64 {
    let epoch_len = epoch_length_daa.max(1);
    admission
        .paid_work_walk_bound_daa(epoch_len)
        .div_ceil(epoch_len)
        .saturating_add(palw_audit_epoch_inclusion_window_epochs(admission))
        .saturating_add(freshness_window_epochs)
        .saturating_add(2)
}

/// ADR-0045 D3-b — how many past epochs of provider-snapshot commitments the history must retain:
/// the beacon window plus the snapshot lag `k` (clause 12 resolves the snapshot at `anchor − k`,
/// one lag deeper than the oldest beacon read).
pub fn palw_provider_snapshot_history_window_epochs(
    admission: &PalwBatchAdmissionParams,
    epoch_length_daa: u64,
    freshness_window_epochs: u64,
    snapshot_lag_epochs: u64,
) -> u64 {
    palw_beacon_seed_history_window_epochs(admission, epoch_length_daa, freshness_window_epochs).saturating_add(snapshot_lag_epochs)
}

/// The oldest epoch a history window anchored at `current_epoch` must still answer for. Epochs
/// strictly below this are sweepable; a read below it is `None`, i.e. PCPB fails closed.
pub fn palw_beacon_seed_history_floor(current_epoch: u64, window_epochs: u64) -> u64 {
    current_epoch.saturating_sub(window_epochs)
}

/// kaspa-pq **ADR-0040 §5.17.3 (§CERT-REDERIVE, bounded inclusion rule)** — the pure predicate half of
/// [`palw_audit_epoch_inclusion_window_epochs`]. `true` iff a certificate declaring `audit_beacon_epoch`
/// may be included at a block sitting in `including_block_epoch`:
///
/// 1. the audit epoch is not in the FUTURE of the including block (`audit_beacon_epoch <=
///    including_block_epoch`) — a certificate cannot commit to an audit round its including block has not
///    yet reached; and
/// 2. the including block is at most `inclusion_window_epochs` past that audit epoch, so
///    `R_{audit_beacon_epoch − 1}` is still buried-resolvable (see the window function's anchor).
///
/// Saturating subtraction so clause (1) is what rejects a future audit epoch, never an underflow.
pub fn palw_certificate_included_within_audit_window(
    audit_beacon_epoch: u64,
    including_block_epoch: u64,
    inclusion_window_epochs: u64,
) -> bool {
    audit_beacon_epoch <= including_block_epoch && including_block_epoch.saturating_sub(audit_beacon_epoch) <= inclusion_window_epochs
}

/// kaspa-pq **ADR-0040 §5.17.3 (§CERT-REDERIVE)** — the PURE selection core of the audit-epoch seed
/// resolver: given `audit_beacon_epoch` and an ALREADY-ORDERED sequence of buried header facts
/// `(version, daa_score, palw_beacon_seed)` NEWEST→OLDEST along the selected-parent chain, return
/// `R_{audit_beacon_epoch − 1}` — the `palw_beacon_seed` of the newest header at epoch
/// `audit_beacon_epoch − 1` — or `None` on any fail-closed condition.
///
/// Split out from the store-backed walk `resolve_palw_audit_epoch_seed`
/// (`consensus/src/processes/palw.rs`) so the epoch-keying / newest-in-epoch / target = audit − 1 /
/// fail-closed logic is unit-testable with hand-built sequences (including the non-zero-seed `Some`
/// path, which a real empty chain cannot produce — beacon seeds stay zero in DegradedGrace/Halted
/// without bonded reveals). The adapter supplies the ordering; ORDER-INDEPENDENCE is that adapter's
/// property (it reads only the deterministic selected-parent chain — see the resolver's proof), NOT this
/// function's, which is order-SENSITIVE by contract and trusts its caller to pass selected-parent order.
///
/// Fail-closed (`None`): `audit_beacon_epoch == 0` (no previous epoch); the first header encountered is
/// pre-v3 / pre-activation / zero-seed (the activation boundary — the predecessor seed is underivable);
/// the walk descends BELOW the target epoch without a hit (the audit epoch is in the block's future or a
/// daa gap); or the sequence ends (pruned history / genesis reached) before the target epoch is found.
/// ADR-0040 §5.14.7.3 (item 4) — resolve the beacon seed `R_target` at an ARBITRARY target epoch by the
/// same fail-closed buried-header descent [`palw_audit_epoch_seed_select`] uses (that function is now the
/// `target = audit_beacon_epoch − 1` specialization — a single, non-divergent walk). PURE, INERT: the PCPB
/// self arm resolves the post-commit beacon `R_{registered+Δ}` through this at VALIDATION time, when that
/// epoch is already in the validating block's past, so it is the same backward walk. **Zero production
/// callers until the wiring slice** — the caller that reads the widened `retain_future_of` window is part
/// of the atomic cutover; this pure selector is not.
///
/// `buried_headers_newest_first` yields `(palw_header_version, daa_score, epoch_seed)` newest-first along
/// the selected-parent chain. Fail-closed (`None`) on: a pre-activation / zero-seed header (the seed is
/// underivable at the activation boundary), descent below `target_epoch` without a hit (the target is in
/// the block's future or a DAA gap), or exhaustion (pruned / genesis) before the target epoch.
pub fn palw_epoch_seed_at(
    target_epoch: u64,
    activation_daa_score: u64,
    epoch_length_daa: u64,
    buried_headers_newest_first: impl Iterator<Item = (u16, u64, Hash64)>,
) -> Option<Hash64> {
    let epoch_len = epoch_length_daa.max(1);
    for (version, daa_score, seed) in buried_headers_newest_first {
        if version < crate::constants::PALW_HEADER_VERSION || daa_score < activation_daa_score || seed == Hash64::default() {
            return None; // activation boundary / underivable seed ⇒ fail CLOSED
        }
        let epoch = daa_score / epoch_len;
        if epoch == target_epoch {
            // First header seen for the target epoch while descending is the NEWEST buried header of that
            // epoch; every header in one epoch carries the same seed, so any representative is equivalent.
            return Some(seed);
        }
        if epoch < target_epoch {
            return None; // descended past the target epoch without a hit ⇒ not in the block's past
        }
    }
    None // sequence exhausted (pruned / genesis) before the target epoch ⇒ fail CLOSED
}

pub fn palw_audit_epoch_seed_select(
    audit_beacon_epoch: u64,
    activation_daa_score: u64,
    epoch_length_daa: u64,
    buried_headers_newest_first: impl Iterator<Item = (u16, u64, Hash64)>,
) -> Option<Hash64> {
    // R_{E-1}: audit_beacon_epoch == 0 has no previous-epoch seed. Fail closed. Delegates to the general
    // `palw_epoch_seed_at` so the fail-closed descent has a single, non-divergent implementation.
    let target_epoch = audit_beacon_epoch.checked_sub(1)?;
    palw_epoch_seed_at(target_epoch, activation_daa_score, epoch_length_daa, buried_headers_newest_first)
}

fn validate_certificate(payload: &[u8]) -> Result<(), PalwTxError> {
    let cert: PalwBatchCertificateV2 = decode_palw_payload(payload)?;
    if cert.version != PALW_BATCH_CERTIFICATE_VERSION_V2 {
        return Err(PalwTxError::UnsupportedVersion(cert.version));
    }
    check_count("certificate.votes", cert.votes.len(), 1, PALW_MAX_AUDITOR_VOTES_V2)?;
    if cert.passed_leaf_count == 0 {
        return Err(PalwTxError::InvalidField("certificate.passed_leaf_count"));
    }
    if !(cert.audit_beacon_epoch <= cert.certificate_epoch
        && cert.certificate_epoch < cert.activation_epoch
        && cert.activation_epoch < cert.expiry_epoch)
    {
        return Err(PalwTxError::InvalidField("certificate.epoch_range"));
    }
    for vote in &cert.votes {
        if vote.vote > 1 {
            return Err(PalwTxError::InvalidField("certificate.vote"));
        }
        if vote.signature.len() != STAKE_ATTESTATION_SIG_LEN {
            return Err(PalwTxError::InvalidSignatureLen(vote.signature.len()));
        }
        if vote.passed_leaf_count != cert.passed_leaf_count {
            return Err(PalwTxError::InvalidField("certificate.vote.passed_leaf_count"));
        }
        if vote.rejected_leaf_bitmap_root != cert.rejected_leaf_bitmap_root {
            return Err(PalwTxError::InvalidField("certificate.vote.rejected_leaf_bitmap_root"));
        }
    }
    if !cert.votes.windows(2).all(|w| cmp_outpoint(&w[0].bond_outpoint, &w[1].bond_outpoint) == Ordering::Less) {
        return Err(PalwTxError::NonCanonical("certificate.votes"));
    }
    Ok(())
}

fn validate_revocation(payload: &[u8]) -> Result<(), PalwTxError> {
    let revocation: PalwRevocationV1 = decode_palw_payload(payload)?;
    check_palw_version(revocation.version)?;
    if revocation.evidence_hash == Hash64::default() {
        return Err(PalwTxError::InvalidField("revocation.evidence_hash"));
    }
    Ok(())
}

fn validate_beacon_commit(payload: &[u8]) -> Result<(), PalwTxError> {
    let commit: PalwBeaconCommitV1 = decode_palw_payload(payload)?;
    check_palw_version(commit.version)?;
    if commit.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(commit.signature.len()));
    }
    Ok(())
}

fn validate_beacon_reveal(payload: &[u8]) -> Result<(), PalwTxError> {
    let reveal: PalwBeaconRevealV1 = decode_palw_payload(payload)?;
    check_palw_version(reveal.version)?;
    if reveal.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(reveal.signature.len()));
    }
    Ok(())
}

fn validate_da_challenge(payload: &[u8]) -> Result<(), PalwTxError> {
    use da::{PALW_DA_CHALLENGE_VERSION_V1, PALW_DA_MAX_ONCHAIN_CHALLENGE_BYTES, PalwDaChallengeV1};
    if payload.len() > PALW_DA_MAX_ONCHAIN_CHALLENGE_BYTES {
        return Err(PalwTxError::PayloadTooLarge { len: payload.len(), max: PALW_DA_MAX_ONCHAIN_CHALLENGE_BYTES });
    }
    let challenge: PalwDaChallengeV1 = decode_palw_payload(payload)?;
    if challenge.version != PALW_DA_CHALLENGE_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(challenge.version));
    }
    if challenge.obligation_id == Hash64::default()
        || challenge.challenge_nonce == Hash64::default()
        || challenge.response_deadline_daa_score <= challenge.opened_daa_score
    {
        return Err(PalwTxError::InvalidField("da_challenge.binding_or_deadline"));
    }
    if challenge.challenger_owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(PalwTxError::InvalidPublicKeyLen(challenge.challenger_owner_public_key.len()));
    }
    if challenge.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(challenge.signature.len()));
    }
    Ok(())
}

fn validate_da_response(payload: &[u8]) -> Result<(), PalwTxError> {
    use da::{
        PALW_DA_CHUNK_BYTES, PALW_DA_MAX_CHUNKS, PALW_DA_MAX_OBJECT_BYTES, PALW_DA_MAX_ONCHAIN_RESPONSE_BYTES,
        PALW_DA_MAX_PROOF_DEPTH, PALW_DA_RESPONSE_VERSION_V1, PALW_RECEIPT_DA_OBJECT_VERSION_V1, PALW_RECEIPT_DA_OBJECT_VERSION_V2,
        PALW_RECEIPT_DA_PROOF_VERSION_V1, PalwDaResponseV1,
    };
    if payload.len() > PALW_DA_MAX_ONCHAIN_RESPONSE_BYTES {
        return Err(PalwTxError::PayloadTooLarge { len: payload.len(), max: PALW_DA_MAX_ONCHAIN_RESPONSE_BYTES });
    }
    let response: PalwDaResponseV1 = decode_palw_payload(payload)?;
    if response.version != PALW_DA_RESPONSE_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(response.version));
    }
    if response.challenge_id == Hash64::default() {
        return Err(PalwTxError::InvalidField("da_response.challenge_id"));
    }
    if response.provider_owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(PalwTxError::InvalidPublicKeyLen(response.provider_owner_public_key.len()));
    }
    if response.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(response.signature.len()));
    }
    let proof = &response.chunk_proof;
    if proof.version != PALW_RECEIPT_DA_PROOF_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(proof.version));
    }
    if !matches!(proof.object_version, PALW_RECEIPT_DA_OBJECT_VERSION_V1 | PALW_RECEIPT_DA_OBJECT_VERSION_V2) {
        return Err(PalwTxError::UnsupportedVersion(proof.object_version));
    }
    let object_len = proof.object_len as usize;
    if object_len == 0 || object_len > PALW_DA_MAX_OBJECT_BYTES {
        return Err(PalwTxError::InvalidField("da_response.object_len"));
    }
    let expected_count = object_len.div_ceil(PALW_DA_CHUNK_BYTES);
    if proof.chunk_count as usize != expected_count || expected_count == 0 || expected_count > PALW_DA_MAX_CHUNKS {
        return Err(PalwTxError::InvalidField("da_response.chunk_count"));
    }
    if proof.chunk_index >= proof.chunk_count {
        return Err(PalwTxError::InvalidField("da_response.chunk_index"));
    }
    let expected_len = if proof.chunk_index + 1 == proof.chunk_count {
        object_len - proof.chunk_index as usize * PALW_DA_CHUNK_BYTES
    } else {
        PALW_DA_CHUNK_BYTES
    };
    if proof.chunk.len() != expected_len {
        return Err(PalwTxError::InvalidField("da_response.chunk_len"));
    }
    let expected_depth = expected_count.next_power_of_two().ilog2() as usize;
    if proof.siblings.len() != expected_depth || proof.siblings.len() > PALW_DA_MAX_PROOF_DEPTH {
        return Err(PalwTxError::InvalidField("da_response.proof_depth"));
    }
    Ok(())
}

fn validate_da_timeout_evidence(payload: &[u8]) -> Result<(), PalwTxError> {
    use da::{PALW_DA_MAX_ONCHAIN_TIMEOUT_BYTES, PALW_DA_TIMEOUT_EVIDENCE_VERSION_V1, PalwDaTimeoutEvidenceV1};
    if payload.len() > PALW_DA_MAX_ONCHAIN_TIMEOUT_BYTES {
        return Err(PalwTxError::PayloadTooLarge { len: payload.len(), max: PALW_DA_MAX_ONCHAIN_TIMEOUT_BYTES });
    }
    let evidence: PalwDaTimeoutEvidenceV1 = decode_palw_payload(payload)?;
    if evidence.version != PALW_DA_TIMEOUT_EVIDENCE_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(evidence.version));
    }
    if evidence.challenge_id == Hash64::default() {
        return Err(PalwTxError::InvalidField("da_timeout.challenge_id"));
    }
    Ok(())
}

/// Search-availability txs use the canonical length-prefixed codec (`decode_strict`) of the
/// search-snapshot module rather than borsh: unknown version, bound violation, malformed prefix and
/// trailing bytes are all decode failures, so isolation admission == exact wire canonicality.
fn validate_search_challenge(payload: &[u8]) -> Result<(), PalwTxError> {
    use search_snapshot::{PALW_SEARCH_MAX_ONCHAIN_CHALLENGE_BYTES, PalwSearchChallengeTxV1};
    if payload.len() > PALW_SEARCH_MAX_ONCHAIN_CHALLENGE_BYTES {
        return Err(PalwTxError::PayloadTooLarge { len: payload.len(), max: PALW_SEARCH_MAX_ONCHAIN_CHALLENGE_BYTES });
    }
    let challenge = PalwSearchChallengeTxV1::decode_strict(payload).map_err(|_| PalwTxError::Decode)?;
    if challenge.object_root == Hash64::default() {
        return Err(PalwTxError::InvalidField("search_challenge.object_root"));
    }
    if let Some(registration) = &challenge.registration {
        // Shape-only here (context-free): both signatures, the same-key rule, the anchor→assignment
        // link and the bonded-registry lookup are contextual and run at dispatch.
        if registration.signed_anchor.anchor.object_root != challenge.object_root {
            return Err(PalwTxError::InvalidField("search_challenge.registration_object_root"));
        }
    }
    Ok(())
}

fn validate_search_response(payload: &[u8]) -> Result<(), PalwTxError> {
    use search_snapshot::{PALW_SEARCH_MAX_ONCHAIN_RESPONSE_BYTES, PalwSearchResponseTxV1};
    if payload.len() > PALW_SEARCH_MAX_ONCHAIN_RESPONSE_BYTES {
        return Err(PalwTxError::PayloadTooLarge { len: payload.len(), max: PALW_SEARCH_MAX_ONCHAIN_RESPONSE_BYTES });
    }
    let response = PalwSearchResponseTxV1::decode_strict(payload).map_err(|_| PalwTxError::Decode)?;
    if response.object_root == Hash64::default() {
        return Err(PalwTxError::InvalidField("search_response.object_root"));
    }
    Ok(())
}

fn validate_search_timeout(payload: &[u8]) -> Result<(), PalwTxError> {
    use search_snapshot::{PALW_SEARCH_MAX_ONCHAIN_TIMEOUT_BYTES, PalwSearchTimeoutTxV1};
    if payload.len() > PALW_SEARCH_MAX_ONCHAIN_TIMEOUT_BYTES {
        return Err(PalwTxError::PayloadTooLarge { len: payload.len(), max: PALW_SEARCH_MAX_ONCHAIN_TIMEOUT_BYTES });
    }
    let timeout = PalwSearchTimeoutTxV1::decode_strict(payload).map_err(|_| PalwTxError::Decode)?;
    if timeout.object_root == Hash64::default() {
        return Err(PalwTxError::InvalidField("search_timeout.object_root"));
    }
    Ok(())
}

/// Strict context-free PALW payload admission by subnetwork byte. This is safe for transaction
/// isolation because it never reads an activation score or chain state. Contextual validation must
/// subsequently enforce the PALW activation fence, [`PalwBeaconCommitV1::is_in_phase`] /
/// [`PalwBeaconRevealV1::is_in_phase`], active bond/key binding, and ML-DSA verification.
///
/// `0x37` (provider unbond) is accepted as of ADR-0040 ECON-03 leg 5, and its acceptance here means
/// SHAPE ONLY. The binding to [`PalwProviderBondPayloadV1::owner_public_key`], the ML-DSA-87 signature
/// and the `Pending`/`Active` precondition are contextual, and live in
/// `palw_provider_unbond_authorized` (consensus/src/processes/palw.rs), which `verify_expected_utxo_state`
/// runs over each block's accepted transactions against the selected-parent [`ProviderBondView`]. So an
/// accepted `0x37` in a VALID block is an AUTHORIZED exit, and stamps `unbond_request_daa_score` through
/// [`palw_provider_bond_mutations_from_accepted_txs`].
pub fn validate_palw_overlay_payload(subnetwork_byte: u8, payload: &[u8]) -> Result<(), PalwTxError> {
    let kind = PalwTxKind::from_subnetwork_byte(subnetwork_byte).ok_or(PalwTxError::UnsupportedKind(subnetwork_byte))?;
    if payload.len() > PALW_MAX_OVERLAY_PAYLOAD_BYTES {
        return Err(PalwTxError::PayloadTooLarge { len: payload.len(), max: PALW_MAX_OVERLAY_PAYLOAD_BYTES });
    }
    match kind {
        PalwTxKind::ProviderBond => validate_provider_bond(payload),
        PalwTxKind::BatchManifest => validate_manifest(payload),
        PalwTxKind::LeafChunk => validate_leaf_chunk(payload),
        PalwTxKind::BatchCertificate => validate_certificate(payload),
        PalwTxKind::Revocation => validate_revocation(payload),
        PalwTxKind::BeaconCommit => validate_beacon_commit(payload),
        PalwTxKind::BeaconReveal => validate_beacon_reveal(payload),
        // ADR-0040 ECON-03 leg 5 — 0x37 acceptance is OPEN as of this slice, and the precondition the
        // previous fail-closed arm named is what changed: `palw_provider_unbond_authorized`
        // (consensus/src/processes/palw.rs) now exists. This stateless check is the SHAPE half only —
        // decodability, version, key and signature LENGTHS. It proves nothing about authorization.
        //
        // The ordering rule this obeys: the exit must be frozen BEFORE the leg-4 spend gate locks
        // output-0 for the bond's lifetime, because a lock with no release is confiscation rather
        // than collateral. Landing the gate first would strand every bonded provider's coins.
        //
        // The authorization half IS wired: `palw_provider_unbond_authorized` is called from
        // `verify_expected_utxo_state`, and the registry writer (`stage_palw_provider_bond_mutations`)
        // applies the resulting `Unbond` mutation at prefix 241. An accepted 0x37 inside a valid block
        // therefore stamps `unbond_request_daa_score` on a bond whose owner really signed for it.
        // `econ03_unbond_acceptance_is_shape_only` still holds and still matters: it pins that THIS
        // function proves nothing about authorization, so the contextual rule can never be deleted on
        // the theory that isolation already covered it.
        //
        // Leg 4 is enforced by `ProviderBondSpendFilter::locks` in `utxo_validation.rs`: until a
        // valid unbond request has aged through the configured delay, mergeset acceptance skips any
        // transaction that spends the registered provider bond's output-0. Together with this
        // contextual authorization arm, the exit is therefore both owner-authorized and delayed.
        PalwTxKind::ProviderUnbond => validate_provider_unbond(payload),
        PalwTxKind::BlockAuthorization => validate_block_authorization(payload),
        PalwTxKind::DaChallenge => validate_da_challenge(payload),
        PalwTxKind::DaResponse => validate_da_response(payload),
        PalwTxKind::DaTimeoutEvidence => validate_da_timeout_evidence(payload),
        PalwTxKind::SearchChallenge => validate_search_challenge(payload),
        PalwTxKind::SearchResponse => validate_search_response(payload),
        PalwTxKind::SearchTimeout => validate_search_timeout(payload),
    }
}

/// kaspa-pq **ADR-0040 (ECON-03)** — stateless validation of a whole PALW overlay **transaction**,
/// mirroring the DNS split ([`crate::dns_finality::validate_stake_bond_payload`] /
/// [`crate::dns_finality::validate_stake_bond_tx`]).
///
/// It runs [`validate_palw_overlay_payload`] for every kind and, for `ProviderBond`, ADDITIONALLY
/// enforces the value lock against the transaction's own outputs. This exists because the
/// payload-only entry point cannot see outputs, and the lock rule must live at the coordinate where
/// a rejection actually rejects the transaction — the isolation validator — rather than in the
/// overlay-effect arm whose error is discarded.
///
/// [`validate_palw_overlay_payload`] stays public and unchanged for mempool / producer callers that
/// legitimately hold only a payload.
pub fn validate_palw_overlay_tx(subnetwork_byte: u8, payload: &[u8], outputs: &[TransactionOutput]) -> Result<(), PalwTxError> {
    validate_palw_overlay_payload(subnetwork_byte, payload)?;
    if PalwTxKind::from_subnetwork_byte(subnetwork_byte) == Some(PalwTxKind::ProviderBond) {
        validate_provider_bond_tx(payload, outputs)?;
    }
    Ok(())
}

// =============================================================================================
// Batch state machine (design §9.5). Pure transition function; the caller supplies the events
// (chunk/bond completion, beacon reached, quorum, timeouts, activation/expiry, fraud) from consensus
// state. Terminal states (Slashed / Expired / Revoked) have no outgoing edges. Only `Active` is
// block-eligible, and an `Incomplete` batch (stuck in `Registering` past its lead) expires and is
// never usable (I-2 / §9.5).
// =============================================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub enum PalwBatchStatus {
    /// No manifest seen yet.
    Missing,
    /// Manifest accepted; awaiting all leaf chunks + bonds.
    Registering,
    /// All chunks + bonds present; awaiting the audit beacon.
    Committed,
    /// Audit beacon reached; canary audit in progress.
    Auditing,
    /// Certificate quorum reached; awaiting the ≥ 1-epoch activation delay.
    Certified,
    /// Live and block-eligible.
    Active,
    /// Failed audit → bonds slashed (terminal).
    Slashed,
    /// Timed out (incomplete / not-activated / expired) (terminal).
    Expired,
    /// Post-activation fraud evidence → revoked (terminal, non-retroactive; §9.5).
    Revoked,
}

/// Events that drive [`PalwBatchStatus`] transitions (design §9.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBatchEvent {
    ManifestAccepted,
    ChunksAndBondsComplete,
    AuditBeaconReached,
    CertificateQuorum,
    AuditFailed,
    /// Registration / audit / certified-not-activated timeout.
    Timeout,
    /// The activation epoch was reached (caller enforces the ≥ 1-epoch delay from `Certified`).
    ActivationReached,
    ExpiryReached,
    FraudEvidence,
}

impl PalwBatchStatus {
    /// The next status for `(self, event)`, or `None` if the transition is invalid (rejected). Pure.
    pub fn next(self, event: PalwBatchEvent) -> Option<PalwBatchStatus> {
        use PalwBatchEvent::*;
        use PalwBatchStatus::*;
        Some(match (self, event) {
            (Missing, ManifestAccepted) => Registering,
            (Registering, ChunksAndBondsComplete) => Committed,
            (Registering, Timeout) => Expired, // incomplete batch → never block-eligible
            (Committed, AuditBeaconReached) => Auditing,
            (Auditing, CertificateQuorum) => Certified,
            (Auditing, AuditFailed) => Slashed,
            (Auditing, Timeout) => Expired,
            (Certified, ActivationReached) => Active,
            (Certified, ExpiryReached) => Expired, // certified but never activated in time
            (Certified, FraudEvidence) => Revoked,
            (Active, ExpiryReached) => Expired,
            (Active, FraudEvidence) => Revoked,
            _ => return None,
        })
    }

    /// Only an `Active` batch can back a block (design §9.5 / §14.2).
    #[inline]
    pub fn is_block_eligible(self) -> bool {
        self == PalwBatchStatus::Active
    }

    /// Terminal states have no outgoing transitions.
    #[inline]
    pub fn is_terminal(self) -> bool {
        matches!(self, PalwBatchStatus::Slashed | PalwBatchStatus::Expired | PalwBatchStatus::Revoked)
    }
}

/// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2) — the uniform-depth **Merkle** depth for `leaf_count`
/// leaves: `d = ceil(log2(max(leaf_count, 1)))`. A batch holds at most [`PALW_MAX_BATCH_LEAVES_V1`]
/// = 256 leaves, so `d <= 8` and every membership proof is exactly `d` siblings long.
#[inline]
pub fn palw_leaf_merkle_depth(leaf_count: u32) -> u32 {
    let n = leaf_count.max(1) as u64;
    if n <= 1 { 0 } else { u64::BITS - (n - 1).leading_zeros() }
}

/// The padding constant for the uniform-depth leaf tree: `Hash64_k(leaf-merkle-empty, "")`.
///
/// Absent leaves are padded to `2^d` with THIS CONSTANT — not by duplicating the tail. Tail duplication
/// makes the tree odd-arity-dependent and admits the classic second-preimage family in which a shorter
/// leaf vector reproduces a longer vector's apex; uniform padding kills it outright and, as a bonus,
/// makes every proof exactly `d` siblings so a proof length is itself a checkable invariant.
#[inline]
pub fn palw_leaf_merkle_empty() -> Hash64 {
    blake2b_512_keyed(PALW_LEAF_MERKLE_EMPTY_DOMAIN, &[])
}

/// The level-0 node for `leaf_hash` sitting at `leaf_index`: `Hash64_k(leaf-merkle-leaf, i_le32 ‖ h)`.
///
/// The index is bound INSIDE the leaf node. That is what buys second-preimage resistance across
/// positions: a leaf that is legitimately a member at index `i` cannot be replayed as a member at any
/// `j != i`, because the node itself — not merely the path — differs.
#[inline]
fn palw_leaf_merkle_leaf_node(leaf_index: u32, leaf_hash: &Hash64) -> Hash64 {
    let mut p = Vec::with_capacity(4 + HASH64_SIZE);
    p.extend_from_slice(&leaf_index.to_le_bytes());
    push_hash(&mut p, leaf_hash);
    blake2b_512_keyed(PALW_LEAF_MERKLE_LEAF_DOMAIN, &p)
}

/// An internal node: `Hash64_k(leaf-merkle-node, left ‖ right)`. The domain is DISJOINT from the leaf
/// node domain, so an internal digest can never be presented as a leaf digest (and vice versa) — the
/// classic leaf/internal confusion defence.
#[inline]
fn palw_leaf_merkle_internal_node(left: &Hash64, right: &Hash64) -> Hash64 {
    let mut p = Vec::with_capacity(2 * HASH64_SIZE);
    push_hash(&mut p, left);
    push_hash(&mut p, right);
    blake2b_512_keyed(PALW_LEAF_MERKLE_NODE_DOMAIN, &p)
}

/// The final root: `Hash64_k(leaf-root, leaf_count_le64 ‖ apex)`.
///
/// Re-using [`PALW_LEAF_ROOT_DOMAIN`] keeps a `leaf_root` value disjoint from every other PALW digest
/// (including the tree's own internal nodes), and the `u64`-LE count prefix preserves the two properties
/// the flat construction asserted and this one must not lose: COUNT sensitivity and — via the
/// index-bound leaf nodes — ORDER sensitivity.
#[inline]
fn palw_leaf_merkle_finalize(leaf_count: u32, apex: &Hash64) -> Hash64 {
    let mut p = Vec::with_capacity(8 + HASH64_SIZE);
    p.extend_from_slice(&(leaf_count as u64).to_le_bytes());
    push_hash(&mut p, apex);
    blake2b_512_keyed(PALW_LEAF_ROOT_DOMAIN, &p)
}

/// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2) — the manifest's `leaf_root`, as a **uniform-depth Merkle
/// root** over the ORDERED per-leaf [`PalwPublicLeafV1::leaf_hash`] digests.
///
/// SUPERSEDES the flat `palw_leaf_root` (ADR-0039 §9.3), whose doc claimed "leaf presence is verified
/// once the batch is chunk-complete (§9.3), not per-leaf-with-a-Merkle-proof". **That completeness gate
/// never existed** — the flat root had ZERO consensus callers, so nothing ever checked stored leaves
/// against `manifest.leaf_root` (ADR-0040 §5.15.2/§5.15.3). It is now a per-leaf Merkle proof, verified
/// at the ACCEPTANCE coordinate before a leaf is stored.
///
/// Construction (pinned by `palw_leaf_merkle_root_construction_golden`):
/// * `d = ceil(log2(max(n, 1)))`, UNIFORM; level 0 is padded to `2^d` with the constant
///   [`palw_leaf_merkle_empty`];
/// * leaf node binds the index: `Hash64_k(leaf-merkle-leaf, i_le32 ‖ leaf_hash)`;
/// * internal node: `Hash64_k(leaf-merkle-node, left ‖ right)`, a domain disjoint from the leaf domain;
/// * root: `Hash64_k(leaf-root, n_le64 ‖ apex)`.
///
/// C4 content-addressing still holds and is now ENFORCED rather than merely documented: `leaf_root` is
/// inside [`PalwBatchManifestV1::content_id`], `batch_id == content_id()`, and a leaf may only be stored
/// under `(batch_id, leaf_index)` if it proves membership in that batch's root. Writing someone else's
/// `batch_id` with your own leaf therefore costs a BLAKE2b-512 second preimage.
pub fn palw_leaf_merkle_root(ordered_leaf_hashes: &[Hash64]) -> Hash64 {
    let n = ordered_leaf_hashes.len() as u32;
    let d = palw_leaf_merkle_depth(n);
    let width = 1usize << d;
    let empty = palw_leaf_merkle_empty();
    let mut level: Vec<Hash64> = Vec::with_capacity(width);
    for (i, h) in ordered_leaf_hashes.iter().enumerate() {
        level.push(palw_leaf_merkle_leaf_node(i as u32, h));
    }
    level.resize(width, empty);
    for _ in 0..d {
        level = level.chunks_exact(2).map(|p| palw_leaf_merkle_internal_node(&p[0], &p[1])).collect();
    }
    debug_assert_eq!(level.len(), 1);
    palw_leaf_merkle_finalize(n, &level[0])
}

/// Produce the membership proof for `leaf_index` — the `d` sibling digests, bottom-up. Returns `None`
/// if `leaf_index` is out of range. Producers (miner / auditor / the reference mint) MUST derive proofs
/// through this function rather than reimplementing the fold, so a construction change cannot drift.
pub fn palw_leaf_merkle_proof(ordered_leaf_hashes: &[Hash64], leaf_index: u32) -> Option<PalwLeafMembershipProofV1> {
    let n = ordered_leaf_hashes.len() as u32;
    if leaf_index >= n {
        return None;
    }
    let d = palw_leaf_merkle_depth(n);
    let width = 1usize << d;
    let empty = palw_leaf_merkle_empty();
    let mut level: Vec<Hash64> = Vec::with_capacity(width);
    for (i, h) in ordered_leaf_hashes.iter().enumerate() {
        level.push(palw_leaf_merkle_leaf_node(i as u32, h));
    }
    level.resize(width, empty);
    let mut siblings = Vec::with_capacity(d as usize);
    let mut idx = leaf_index as usize;
    for _ in 0..d {
        siblings.push(level[idx ^ 1]);
        level = level.chunks_exact(2).map(|p| palw_leaf_merkle_internal_node(&p[0], &p[1])).collect();
        idx >>= 1;
    }
    Some(PalwLeafMembershipProofV1 { siblings })
}

/// Verify that `leaf_hash` is the member at `leaf_index` of a `leaf_count`-leaf tree rooted at
/// `expected_root`.
///
/// The direction bits are DERIVED from `leaf_index` — never carried in the payload, so an attacker has
/// no free bits to grind. The proof length is required to be exactly `palw_leaf_merkle_depth(leaf_count)`
/// (both too-short and too-long are rejected, BEFORE any hashing), which makes the proof for a given
/// `(leaf, index, root)` unique.
pub fn palw_verify_leaf_membership(
    leaf_hash: &Hash64,
    leaf_index: u32,
    leaf_count: u32,
    proof: &PalwLeafMembershipProofV1,
    expected_root: &Hash64,
) -> bool {
    if leaf_index >= leaf_count {
        return false;
    }
    let d = palw_leaf_merkle_depth(leaf_count);
    if proof.siblings.len() as u32 != d {
        return false;
    }
    let mut node = palw_leaf_merkle_leaf_node(leaf_index, leaf_hash);
    for (level, sibling) in proof.siblings.iter().enumerate() {
        node = if (leaf_index >> level) & 1 == 0 {
            palw_leaf_merkle_internal_node(&node, sibling)
        } else {
            palw_leaf_merkle_internal_node(sibling, &node)
        };
    }
    palw_leaf_merkle_finalize(leaf_count, &node) == *expected_root
}

/// ADR-0039 §12.1 clause-6-style referenceability: whether an algo-4 header targeting `epoch` could ever
/// resolve against a batch in `status` with the given windows — i.e. whether a past-relative overlay view
/// must RETAIN the batch. Panel-frozen (see design §18.2):
/// * terminal states (`Slashed`/`Expired`/`Revoked`) or any revoked batch → never referenceable → drop;
/// * `Active`/`Certified` → retain while `epoch < expiry_epoch` (a leaf/cert is block-eligible only
///   inside its active window, §14.2);
/// * pre-certification (`Registering`/`Committed`/`Auditing`) → retain until the registration + lead +
///   audit budget elapses, after which it can no longer certify. Deliberately does NOT key on the
///   evidence window (fraud on an already-expired batch changes no header verdict — that window bounds
///   the provider BOND record, not the batch view). Monotone: a child's epoch ≥ its parent's, so
///   `child_epoch < expiry ⟹ parent_epoch < expiry`, mirroring the beacon `retain_future_of` argument.
pub fn palw_batch_referenceable(
    status: PalwBatchStatus,
    revoked: bool,
    registration_epoch: u64,
    expiry_epoch: u64,
    epoch: u64,
    registration_lead_epochs: u64,
    audit_window_epochs: u64,
) -> bool {
    use PalwBatchStatus::*;
    // ADR-0045 D1 (SS-04 RESOLVED): revocation is uniformly NON-RETROACTIVE, governed solely by the
    // `revoked` argument (callers derive it from `revoked_from_daa` as `daa >= from`). `Revoked` status
    // is a non-retroactive LABEL, never a retroactive terminal here: a batch reaches `Revoked` only from
    // `Active`/`Certified` (it DID back blocks), so every block with `daa < effective_daa` must still
    // resolve it (§9.5). `Slashed`/`Expired` stay retroactively unreferenceable — those never validly
    // backed a block. `is_terminal()` (which still includes `Revoked`) is intentionally NOT used here as
    // the drop gate, because it conflates "no outgoing transition" with "never referenceable".
    if revoked {
        return false;
    }
    match status {
        // Revoked-before-its-effective-DAA behaves like its pre-revocation state (Active/Certified).
        Active | Certified | Revoked => epoch < expiry_epoch,
        Registering | Committed | Auditing => {
            epoch <= registration_epoch.saturating_add(registration_lead_epochs).saturating_add(audit_window_epochs)
        }
        Missing | Slashed | Expired => false,
    }
}

// ADR-0040 SS-04 — RESOLVED by ADR-0045 D1 (non-retroactive). There is now ONE time-carrying
// representation of "this batch was revoked": `revoked_from_daa` (the `revoked` argument above), which
// every consumer compares NON-RETROACTIVELY (`daa < from` ⇒ still eligible/referenceable):
//   * `PalwBatchLifecycleV1::is_block_eligible_at` — `revoked_from_daa.is_none_or(|from| daa < from)`.
//   * `PalwBatchViewV1::retain` — passes `revoked_from_daa.is_some_and(|from| daa >= from)`.
//   * `palw_batch_referenceable` — as of ADR-0045 D1, honors only that `revoked` bool and treats the
//     `Revoked` STATUS as a non-retroactive label (governed by the same field), no longer as a
//     retroactive terminal. So a block with `daa < effective_daa` still resolves the batch, matching
//     §9.5, and there is no second door.
//
// Why non-retroactive (ADR-0045 D1): a provider reward is settled in the merging block's coinbase and
// is immediately final in the UTXO model — it cannot be unwound after the fact. Retroactive revocation
// would require escrow/maturity, contradicting the §17.1 immediate-distribution design. Fraud deterrence
// therefore lives in BOND SLASH (a retroactive-on-collateral penalty), while revocation only stops
// FUTURE credit from `effective_daa` onward.
//
// STILL LATENT (no live change): `FraudEvidence` has zero production call sites; no batch can currently
// reach `Revoked`. The contract for WHOEVER WIRES `FraudEvidence`: set `revoked_from_daa` to the
// effective DAA alongside (or instead of) moving the status — the `Revoked` status alone carries NO
// time semantics anymore, so leaving `revoked_from_daa` unset would make the batch look never-revoked.

/// ADR-0039 §9.2/§9.3 — the batch-admission bounds the mergeset-delta builder enforces (the subset of
/// `PalwParams` the fork-local batch view needs). A `const` so it lives in the `const Params` presets.
/// Inert placeholder values while PALW is inactive; recalibrated at re-genesis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBatchAdmissionParams {
    pub max_batch_leaves: u32,
    pub max_leaf_chunk_leaves: u16,
    pub registration_lead_epochs: u64,
    pub active_window_epochs: u64,
    pub audit_window_epochs: u64,
    /// §7 economic floor per leaf, in sompi. A batch's `total_leaf_bond_sompi` must cover
    /// `leaf_count · min_leaf_bond_sompi`. Calibrated at re-genesis to each tier's measured `c_saved`
    /// (the GPU-execution cost a forger avoids by NOT running the inference) — the missing term in the
    /// forgery-EV inequality: a forger's gain is `R + c_saved`, and `q·slash ≈ R` only offsets the
    /// reward, so `leaf_bond + credential_loss` must cover `c_saved`. Inert placeholder `0`.
    pub min_leaf_bond_sompi: u64,
    /// Static-audit finding H-01 — concurrent view slots one SPONSOR may hold. `0` = no quota.
    ///
    /// This is the parameter that turns bond sponsorship into an actual cost. Sponsorship alone does
    /// not close H-01: one bond can sponsor all `max_view_batches` slots as cheaply as one. With a
    /// quota of `q`, filling the view costs `ceil(max_view_batches / q)` distinct bonded owner keys,
    /// i.e. that many times the provider-bond floor in locked collateral.
    ///
    /// Sized against `max_view_batches`, not in isolation: raising this cheapens the flood, lowering
    /// it prices out an operator running several honest batches at once. Both move together.
    pub max_view_batches_per_sponsor: u32,
    /// kaspa-pq **ADR-0040 CERT-TRUST — INERT (dead parameter).**
    ///
    /// Formerly the §12′ / CERT-UNIQ minimum supersession window. Supersession itself is REMOVED from
    /// the body/mergeset coordinate (it ranked certificates by an unverifiable self-declared stake
    /// figure), so this parameter has no reader. It is left in the struct rather than removed because
    /// [`PalwBatchAdmissionParams`] is part of the consensus params surface and dropping a field is a
    /// larger, riskier diff than documenting it as inert. Do not give it a reader without re-opening
    /// the CERT-TRUST argument.
    pub supersession_window_daa: u64,
    /// ADR-0040 P1-11 (DOS-03) — the maximum number of batches a fork-relative view may carry.
    ///
    /// `PalwBatchViewV1` is CLONED and re-persisted on every block, so an unbounded batch count is
    /// per-block amplified state whose only rate limit is transaction fees. `0` means unbounded (the
    /// pre-cap behaviour), which [`PalwBatchAdmissionParams::is_consistent_for_activation`] rejects —
    /// and, per ADR-0040 P1-5, that predicate is asserted over every activated preset by
    /// `palw_activated_presets_bound_the_view` in `config::params`. Until that check existed this doc
    /// claimed an enforcement that did not exist anywhere in the tree.
    pub max_view_batches: u32,
    /// kaspa-pq **ADR-0040 ECON-03 / SEL-01 — the per-provider-bond minimum, in sompi.**
    ///
    /// A provider bond declaring less than this is DROPPED at acceptance
    /// ([`palw_provider_bond_mutations_from_accepted_txs`]) and therefore never enters the registry,
    /// never becomes `Active`, and never backs a reward.
    ///
    /// This is the half of SEL-01 that actually stops free splitting. Selection is not yet
    /// bond-weighted, but admission checking only `amount_sompi != 0` meant one bond could be split
    /// into a hundred at zero cost. A capital floor bounds that Sybil hole even before weighting
    /// lands. Like `max_view_batches`, it is non-vacuous only because
    /// [`PalwBatchAdmissionParams::is_consistent_for_activation`] rejects `0` and that predicate is
    /// asserted over every activated preset — a floor that only a comment enforces is not a floor.
    /// Inert placeholder `0` on non-activating presets.
    pub min_provider_bond_sompi: u64,
    /// kaspa-pq **ADR-0040 ECON-03 (leg 5 / THE WIRE) — the network minimum exit delay, in PALW
    /// epochs.**
    ///
    /// [`PalwProviderBondPayloadV1::unbond_delay_epochs`] is operator-declared, and
    /// [`palw_provider_bond_mutations_from_accepted_txs`] clamps it UP to this floor before the record
    /// enters the registry. The clamp is the point: the exit delay IS the slashable window, so an
    /// operator that declares a one-epoch delay would be granting itself near-immunity. Mirrors the
    /// DNS `DnsParams::unbonding_period_blocks` floor, differing only in unit (epochs, because the
    /// payload's field is in epochs — [`provider_bond_release_daa_score`] converts).
    ///
    /// Like [`Self::min_provider_bond_sompi`], non-zero-ness is ENFORCED by
    /// [`Self::is_consistent_for_activation`] over every activated preset, not merely documented: a
    /// zero floor is an instant-exit bond, which is not collateral. The MAGNITUDE is a re-genesis
    /// calibration.
    pub provider_unbond_floor_epochs: u64,
}

impl PalwBatchAdmissionParams {
    /// kaspa-pq **ADR-0040 P1-5 — the batch-admission re-genesis preflight.**
    ///
    /// After the P1-9 removal the persisted view carries exactly one unbounded-in-principle dimension,
    /// `batches`, and `max_view_batches` is the ONLY thing that bounds it ([`PalwBatchViewV1::
    /// apply_manifest`] refuses at the cap). That parameter had two readers and nothing at all asserted
    /// it was non-zero, so a one-word params edit could silently restore the unbounded pre-cap
    /// behaviour on an activated preset while every test still passed. A bound that only a comment
    /// enforces is not a bound.
    ///
    /// Evaluated only for presets that actually activate PALW (`palw_activation_daa_score != u64::MAX`);
    /// the inert placeholder values are not required to satisfy it.
    pub fn is_consistent_for_activation(&self) -> bool {
        self.max_view_batches != 0
            && self.max_batch_leaves != 0
            && self.max_leaf_chunk_leaves != 0
            && self.max_leaf_chunk_leaves as usize <= PALW_MAX_LEAVES_PER_CHUNK
            // ADR-0040 ECON-03: a zero provider floor makes bond-splitting free, so the anti-Sybil
            // property would rest on nothing. This is the clause that makes
            // `min_provider_bond_sompi`'s doc true rather than paper.
            && self.min_provider_bond_sompi != 0
            // ADR-0040 ECON-03 leg 5: a zero exit floor means a bond can be requested out on the
            // block after it is created, so the slashable window collapses to nothing and the
            // "collateral" is unslashable in practice. Same enforcement shape as the amount floor.
            && self.provider_unbond_floor_epochs != 0
    }

    // DELIBERATE OMISSION — `min_leaf_bond_sompi != 0` is NOT asserted here, though the surrounding
    // argument would seem to demand it. Stating the reason so the gap stays visible rather than
    // becoming a silent inconsistency:
    //
    // 1. Adding it would make this predicate fail on all six shipped presets, since every one of them
    //    carries `min_leaf_bond_sompi: 0`. The only way to make it pass is to write a number — and
    //    body_processor/processor.rs:427-433 records that pricing this parameter is a CALIBRATION
    //    decision reserved to the re-genesis that activates PALW, explicitly "not to a remediation
    //    patch", because it trades off against `max_view_batches` and against pricing out small
    //    honest providers.
    // 2. Satisfying a new assertion with an arbitrary placeholder is strictly worse than leaving the
    //    gap open: it would flip the ADR-0040 §5.12 gate row to green while the calibration remains
    //    undone, converting a documented, visible activation blocker into an invisible one. This ADR
    //    has already shipped bounds that only a comment kept; a bound kept only by a meaningless
    //    number is the same defect wearing an assertion.
    // 3. Unlike `min_provider_bond_sompi` — a NEW parameter with a new reader, where a non-zero value
    //    changes no pre-existing behaviour — raising `min_leaf_bond_sompi` above zero changes
    //    `PalwBatchManifestV1::admission_valid` on the two ACTIVATED presets, i.e. a consensus
    //    validity change outside this slice's scope.
    //
    // The leaf-bond vacuity therefore remains an open, documented ECON finding, unchanged by ECON-03.

    /// kaspa-pq **ADR-0040 §5.15.13 (G16 / P1-9-RELAND)** — the maximum number of epochs that can
    /// separate two blocks which both reference leaves of the SAME batch.
    ///
    /// This is the whole reason the reward-coordinate duplicate-work rule can use a BOUNDED
    /// selected-chain walk instead of unbounded carried state. Every term below is ENFORCED by
    /// [`PalwBatchManifestV1::admission_valid`], not asserted here:
    ///
    /// * `registration_epoch == accept_epoch` — the phase freeze. `registration_epoch` is a manifest
    ///   field and the manifest is content-addressed (`batch_id == content_id()`), so a given
    ///   `batch_id` can be admitted into a fork's view at exactly ONE epoch, ever. Re-broadcasting a
    ///   dropped batch later cannot re-admit it.
    /// * `activation_not_before_epoch <= registration_epoch + lead + audit + lead` — the DOS-04 bound
    ///   from ABOVE (`min_activation` plus one lead window of scheduling slack).
    /// * `expiry_epoch <= activation_not_before_epoch + active_window` — the active-window bound.
    ///
    /// and block eligibility itself requires `epoch < expiry_epoch`
    /// ([`PalwBatchLifecycleV1::is_block_eligible_at`]). Composing them:
    ///
    /// ```text
    ///   expiry_epoch <= registration_epoch + 2·lead + audit + active_window
    /// ```
    ///
    /// so the epochs at which ANY block may claim a leaf of the batch all lie in
    /// `[registration_epoch, expiry_epoch)`, an interval of at most this width.
    ///
    /// A `job_nullifier` is batch-bound (§5.15 / M2: it sits inside `leaf_hash`, which opens to
    /// `manifest.leaf_root`, which is inside `content_id() == batch_id`), so two claims of the SAME
    /// nullifier from the SAME batch are separated by at most this many epochs.
    ///
    /// # What this bound does NOT reach — read before calling G16 closed
    ///
    /// Two claims from DIFFERENT batches are separated by at most this many epochs only if the two
    /// batches' life windows OVERLAP. Nothing stops a producer from registering the same
    /// `job_nullifier` into a batch now and into another batch a year from now; those two claims are
    /// arbitrarily far apart in DAA and NO bounded walk sees both.
    ///
    /// Closing that case needs one of exactly two things, and neither is available here:
    ///
    /// * a PERMANENT nullifier set — unbounded state, growing forever with no eviction rule that is
    ///   not itself an attack surface. That is the shape ADR-0040 P1-5 deleted, and re-adding it at a
    ///   different coordinate does not make it bounded;
    /// * a FRESHNESS binding inside the leaf — `job_nullifier` committing to a recent beacon/epoch, so
    ///   that reusing an old computation forces a different nullifier. That is a `PalwPublicLeafV1`
    ///   format change (`LEAF_LEN`/`LEAF_FNV` move), i.e. a different slice entirely.
    ///
    /// So the rule this bound supports is the BOUNDED-WINDOW duplicate-work rule, which is what the
    /// reward coordinate can soundly enforce today. It is not the global rule the G16 row describes.
    /// Do not let the two be conflated by a later edit.
    pub fn max_batch_life_epochs(&self) -> u64 {
        self.registration_lead_epochs
            .saturating_mul(2)
            .saturating_add(self.audit_window_epochs)
            .saturating_add(self.active_window_epochs)
    }

    /// kaspa-pq **ADR-0040 §5.15.13 (G16)** — [`Self::max_batch_life_epochs`] converted to the DAA
    /// units the selected-chain walk actually measures in, plus one epoch of slack because
    /// `epoch = daa_score / epoch_length_daa` truncates (two blocks in the same epoch can be up to
    /// `epoch_length_daa - 1` DAA apart, and the interval is half-open at both ends).
    ///
    /// This is the analogue of `reward_uniqueness_window_blocks` for the validator-attestation dedup,
    /// and it is sound for the same reason: it is paired with an ADMISSION-side filter that makes a
    /// claim outside the window impossible rather than merely unrewarded. There, the filter is the
    /// recency check on `att.target_daa_score`; here it is `resolvable_batch`'s `epoch <
    /// expiry_epoch` plus the content-addressed registration freeze. Without that pairing the number
    /// below would be a comment, not a bound.
    pub fn paid_work_walk_bound_daa(&self, epoch_length_daa: u64) -> u64 {
        self.max_batch_life_epochs().saturating_add(1).saturating_mul(epoch_length_daa.max(1))
    }

    /// §16.3 testnet defaults (mirrors `PalwParams::testnet_inert_default`). Inert.
    pub const INERT: PalwBatchAdmissionParams = PalwBatchAdmissionParams {
        max_batch_leaves: 256,
        max_leaf_chunk_leaves: PALW_MAX_LEAVES_PER_CHUNK as u16,
        registration_lead_epochs: 2,
        active_window_epochs: 6,
        audit_window_epochs: 6,
        min_leaf_bond_sompi: 0,
        // Static-audit finding H-01 — the per-sponsor quota. `4` against `max_view_batches = 1_024`
        // means filling the view needs 256 distinct bonded owner keys = 2,560 MSK locked at the
        // 10-MSK provider floor, versus the transaction fees it cost before. An honest operator can
        // still hold four concurrent batches, which is more than the shipped harness ever registers.
        // Re-price this WITH `max_view_batches` and `min_provider_bond_sompi`, never alone.
        max_view_batches_per_sponsor: 4,
        supersession_window_daa: 0,
        max_view_batches: 1_024,
        // ADR-0040 ECON-03 — the anti-split floor. NON-ZERO is the enforced property
        // (`is_consistent_for_activation`); the MAGNITUDE is a re-genesis calibration, and this
        // testnet-scale value deliberately mirrors the DNS testnet `min_bond_amount_sompi`
        // (`10 * SOMPI_PER_KASPA`) because the only two presets that activate PALW are testnets.
        // A mainnet activation must re-price this alongside `max_view_batches`.
        min_provider_bond_sompi: 10 * crate::constants::SOMPI_PER_KASPA,
        // ADR-0040 ECON-03 leg 5 — the exit-delay floor. NON-ZERO is the enforced property; the
        // magnitude is a re-genesis calibration. 6 epochs at `palw_epoch_length_daa = 100` and 10 BPS
        // is ~60 seconds, chosen to match `audit_window_epochs` so a bond cannot exit before the audit
        // window that could slash it has closed. A public/value activation must re-price it together
        // with the longer DA retention horizon; the DA exit gate independently prevents an early
        // collateral spend while any live obligation remains.
        provider_unbond_floor_epochs: 6,
    };
}

/// ADR-0039 §18.2 — the COMPACT, fork-relative lifecycle facts of one batch carried in a
/// [`PalwBatchViewV1`]. Only the genuinely fork-dependent bits (status + presence + windows +
/// content-address roots); the ~840 B/leaf immutable CONTENT stays in the content-addressed blob store
/// (a full-carry entry is ~292 KB — a per-block clone+persist DoS, C4 panel Q1). `leaf_root` +
/// `cert_hash` bind the blobs a resolver reads back; `chunks_present_count` drives the §9.3 completeness
/// gate (Registering → Committed once it equals `chunk_count`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBatchLifecycleV1 {
    pub status: PalwBatchStatus,
    pub registration_epoch: u64,
    pub activation_not_before_epoch: u64,
    pub expiry_epoch: u64,
    pub leaf_count: u32,
    pub chunk_count: u16,
    /// Bitmap of the received chunk indices (idempotent per index; `chunk_count <= 256` for
    /// `max_batch_leaves 256 / max_leaf_chunk_leaves >= 1`, so a 256-bit `[u64; 4]` covers every case).
    /// The §9.3 completeness gate fires when its popcount reaches `chunk_count`.
    pub chunks_present: [u64; 4],
    pub leaf_root: Hash64,
    /// The FIRST certificate hash observed for this batch on this fork (write-once — see
    /// [`PalwBatchViewV1::apply_certificate`]). Read only as a boolean "has been certified at all";
    /// the AUTHORITATIVE certificate a block uses is the attested blob its header names, resolved from
    /// `palw_store`, not this field.
    pub cert_hash: Option<Hash64>,
    /// Static-audit finding H-01 — the high half of this batch's SPONSOR tag: the first 128 bits of
    /// the address payload of the provider-bond owner key whose script the manifest carrier spent.
    /// [`PALW_SPONSOR_UNATTRIBUTED`] (`0`) means "no sponsor recorded".
    ///
    /// **This field was `cert_activation_epoch`**, one of the CERT-TRUST inert pair: never written by
    /// the fold, never read by `is_block_eligible_at`, retained only so the block-keyed overlay
    /// view's borsh encoding stayed stable. Reusing it — same type, same position — is what lets the
    /// sponsor quota ship WITHOUT a `LATEST_DB_VERSION` bump (a forced re-sync of every node) and
    /// without touching the four hand-maintained legacy mirrors of this struct, whose divergence
    /// failure mode is a total chain break rather than a partition.
    ///
    /// The safety argument is that below `palw_manifest_sponsorship_daa_score` both halves are
    /// PROVABLY always zero: the fold wrote `0` unconditionally for the entire life of the inert
    /// pair, and `apply_manifest` still writes `PALW_SPONSOR_UNATTRIBUTED` below the fence. So every
    /// row already on disk decodes to "unattributed" under the new meaning, which is exactly what it
    /// meant before.
    ///
    /// The serde alias keeps `PalwAuditRoundFacts` JSON (this struct is embedded in it and shipped
    /// over `getPalwAuditFacts` as `facts_json`) readable for artifacts produced before the rename —
    /// serde JSON is field-NAME keyed, unlike borsh and bincode which are positional.
    #[serde(alias = "cert_activation_epoch")]
    pub sponsor_tag_hi: u64,
    /// Static-audit finding H-01 — the low half of the sponsor tag. See [`Self::sponsor_tag_hi`];
    /// this field was `cert_expiry_epoch`.
    #[serde(alias = "cert_expiry_epoch")]
    pub sponsor_tag_lo: u64,
    /// kaspa-pq **ADR-0040 CERT-TRUST — INERT.** Formerly the §12′ supersession comparator. It ranked
    /// certificates by a SELF-DECLARED integer at a coordinate with no bond view, so `u128::MAX` won
    /// every comparison and permanently evicted honest certificates. The comparator is removed; this
    /// field is never written and never read. `approving_stake` remains a real, verified commitment at
    /// the VIRTUAL coordinate (`verify_certificate_attestation` check 4), which is the only place the
    /// tally can be recomputed. Retained for borsh-encoding stability; treat as always `0`.
    pub cert_approving_stake: u128,
    /// `d₀`: the DAA of the first certificate in this fork's view. This is a timestamp rather than a
    /// supersession window and is re-derived per view. `None` until the first certificate.
    pub first_cert_daa: Option<u64>,
    pub revoked_from_daa: Option<u64>,
}

impl PalwBatchLifecycleV1 {
    /// Static-audit finding H-01 — this batch's sponsor, or [`PALW_SPONSOR_UNATTRIBUTED`].
    #[inline]
    pub fn sponsor_tag(&self) -> (u64, u64) {
        (self.sponsor_tag_hi, self.sponsor_tag_lo)
    }

    /// Whether an algo-4 header targeting `epoch` may resolve against this batch (present, Active, not
    /// revoked, certified at all, and inside the batch's own declared window). The per-leaf facts
    /// (nullifier / proof-type / leaf window) come from the content-verified leaf blob; this is the
    /// batch-level gate.
    ///
    /// # kaspa-pq ADR-0040 CERT-TRUST — why the CERTIFICATE window is no longer read here
    ///
    /// `cert_activation_epoch` / `cert_expiry_epoch` used to be copied into the entry by the
    /// body-coordinate certificate fold, which has no bond view and therefore cannot verify anything a
    /// certificate declares. Reading attacker-declarable epochs in a *rejection* gate made this the DoS
    /// surface: one unverified (indeed never-accepted) overlay tx declaring `expiry_epoch = 0` bricked
    /// an honest provider's whole batch.
    ///
    /// They are redundant, not merely dangerous: the authoritative certificate window is re-derived
    /// from the ATTESTED blob at
    /// `body_validation_in_context` (`resolve_palw_binding` → `cert_active`), which reads `palw_store`
    /// — populated only behind `verify_certificate_attestation`. So the window is still enforced, at
    /// the only coordinate that can verify it. What survives here is the un-forgeable-in-the-harmful-
    /// direction part: `cert_hash.is_some()` is monotone (a forged fold can only make a batch MORE
    /// eligible in the view, and that buys nothing because the store gate is independent), and
    /// `expiry_epoch` comes from the content-addressed manifest, not from a certificate.
    /// # kaspa-pq ADR-0040 SS-04 — why this takes a DAA as well as an epoch
    ///
    /// `revoked_from_daa` is, as its name says, a DAA score: §9.5 specifies revocation as
    /// **non-retroactive from `effective_daa`**, so a leaf already drawn for an interval below that
    /// point stays spendable and only future intervals are killed. The gate used to read the field as
    /// `revoked_from_daa.is_none()` — i.e. *any* revocation rejected the batch at *every* epoch, which
    /// is total retroactivity and the exact opposite of what [`PalwBatchViewV1::mark_revoked`]'s doc
    /// claimed. The field was written and never compared to anything.
    ///
    /// Enforcing the specified rule needs a DAA at the gate, which is why the epoch alone was not
    /// enough: epochs and DAA scores are different clocks and neither is derivable from the other here.
    /// The caller passes the candidate block's own `header.daa_score`, which is consensus-validated
    /// post-GHOSTDAG as a function of the block's past — so a miner cannot understate it to slip under
    /// a revocation, for the same reason it cannot understate it for clause 5.
    pub fn is_block_eligible_at(&self, epoch: u64, daa: u64) -> bool {
        self.revoked_from_daa.is_none_or(|from| daa < from)
            && self.status.is_block_eligible()
            && self.cert_hash.is_some()
            && epoch < self.expiry_epoch
    }
}

/// ADR-0039 §18.2 — the fork-relative PALW batch-lifecycle view: a compact `batch_id → lifecycle` map
/// carried per block (clone the selected parent's, apply this block's deltas, `retain` the still-
/// referenceable set). This is the past-relative overlay `check_palw_ticket` must resolve against
/// instead of the global virtual-tip store. The type and its retain/resolve operations are independent
/// of the pipeline stage that constructs it.
///
/// # Job-nullifier state
///
/// This view intentionally has no first-claim-wins `job_nullifiers` registry. The body/mergeset
/// fold has no `ActiveBondView`, cannot resolve a bond outpoint to a signing key, and performs no
/// ML-DSA verification. A first-claim-wins registry at this coordinate would accept unauthenticated
/// identifiers and add cloned, payload-driven state. Duplicate-work accounting is instead performed at
/// the reward/virtual coordinate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBatchViewV1 {
    pub version: u16,
    pub batches: BTreeMap<Hash64, PalwBatchLifecycleV1>,
}

impl PalwBatchViewV1 {
    pub fn new() -> Self {
        Self { version: 1, batches: BTreeMap::new() }
    }

    /// The batch's lifecycle facts, if present in this fork's past.
    pub fn entry(&self, batch_id: &Hash64) -> Option<&PalwBatchLifecycleV1> {
        self.batches.get(batch_id)
    }

    /// The batch a header may resolve against at `epoch`, or `None` (absent / not block-eligible).
    /// `daa` is the candidate block's own DAA score; see [`PalwBatchLifecycleV1::is_block_eligible_at`]
    /// for why the §9.5 non-retroactive revocation rule needs it (ADR-0040 SS-04).
    pub fn resolvable_batch(&self, batch_id: &Hash64, epoch: u64, daa: u64) -> Option<&PalwBatchLifecycleV1> {
        self.batches.get(batch_id).filter(|e| e.is_block_eligible_at(epoch, daa))
    }

    /// Drop batches no longer referenceable by any future algo-4 header (design §18.2), bounding the
    /// carried view independently of chain length. Uses [`palw_batch_referenceable`]; monotone in epoch.
    ///
    /// # kaspa-pq ADR-0040 SS-04 — why eviction is DAA-aware too
    ///
    /// Fixing only [`PalwBatchLifecycleV1::is_block_eligible_at`] would have left revocation retroactive
    /// through a second door. `palw_batch_referenceable` takes a `revoked: bool` and drops the entry
    /// outright, so a **future-dated** revocation (`effective_daa` above the current tip) would evict the
    /// batch immediately; every descendant block with `daa < effective_daa` — which §9.5 says must still
    /// resolve — would then fail with "batch not in this block's past" instead. Eviction is therefore
    /// gated on the revocation having actually taken effect at `daa`, matching the eligibility gate
    /// exactly. Once `daa >= effective_daa` no future header can reference the batch, so dropping it is
    /// still sound and the view stays bounded.
    pub fn retain(&mut self, epoch: u64, daa: u64, registration_lead_epochs: u64, audit_window_epochs: u64) {
        self.batches.retain(|_, e| {
            palw_batch_referenceable(
                e.status,
                e.revoked_from_daa.is_some_and(|from| daa >= from),
                e.registration_epoch,
                e.expiry_epoch,
                epoch,
                registration_lead_epochs,
                audit_window_epochs,
            )
        });
    }

    // ---- §9.5 tx-driven deltas (the B-way `Δ(mergeset(B))` applied to `view(SP(B))`). All pure; each
    // returns whether it mutated the view. The immutable leaf/cert CONTENT lives in the content-addressed
    // blob store — the view only tracks the fork-dependent lifecycle. ----

    /// Missing → Registering. Admission-gated ([`PalwBatchManifestV1::admission_valid`], which pins the
    /// content-addressed `batch_id`); idempotent on the content id (a re-registration of the exact same
    /// batch is a no-op — the first mergeset occurrence wins).
    #[allow(clippy::too_many_arguments)]
    /// Static-audit finding H-01 — concurrent view slots currently held by one sponsor.
    ///
    /// Derived, never cached. See the call site in [`Self::apply_manifest`] for why that is the
    /// reorg-safety property rather than a performance compromise.
    pub fn sponsor_slots(&self, tag: (u64, u64)) -> u32 {
        if tag == PALW_SPONSOR_UNATTRIBUTED {
            return 0;
        }
        self.batches.values().filter(|entry| entry.sponsor_tag() == tag).count() as u32
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_manifest(
        &mut self,
        m: &PalwBatchManifestV1,
        accept_epoch: u64,
        max_batch_leaves: u32,
        max_leaf_chunk_leaves: u16,
        registration_lead_epochs: u64,
        active_window_epochs: u64,
        audit_window_epochs: u64,
        min_leaf_bond_sompi: u64,
        max_view_batches: u32,
        sponsorship: PalwManifestSponsorshipV1,
    ) -> bool {
        // ADR-0040 P1-11 (DOS-03): the view is cloned and re-persisted per block, so an unbounded batch
        // count is per-block amplified state rate-limited only by fees. Refuse admission at the cap
        // rather than evicting an existing batch — eviction would let a flood displace honest batches,
        // turning a resource bound into a censorship tool.
        if max_view_batches != 0 && self.batches.len() >= max_view_batches as usize {
            return false;
        }
        // Static-audit finding H-01, FAIL-CLOSED half. Past the fence a manifest with no resolved
        // sponsor is refused here, before `admission_valid`. Checking it at the top means a caller
        // that forgets to resolve the sponsor gets refusals, not free slots — the failure direction
        // that costs an operator a retry instead of costing the network its censorship resistance.
        if sponsorship.required && sponsorship.tag == PALW_SPONSOR_UNATTRIBUTED {
            return false;
        }
        if !m.admission_valid(
            accept_epoch,
            max_batch_leaves,
            max_leaf_chunk_leaves,
            registration_lead_epochs,
            active_window_epochs,
            audit_window_epochs,
            min_leaf_bond_sompi,
        ) {
            return false;
        }
        if self.batches.contains_key(&m.batch_id) {
            return false; // already registered on this fork (idempotent; content-addressed ⇒ same batch)
        }
        // Static-audit finding H-01, QUOTA half. Checked AFTER the idempotence return so a re-broadcast
        // of an already-registered manifest cannot burn a sponsor's slot.
        //
        // The count is DERIVED by scanning the view, never stored in a counter. That is the whole
        // reorg-safety argument: `PalwBatchViewV1` is cloned from the selected parent and persisted
        // per chain block, so the count is a pure function of the view it is taken from and cannot
        // desynchronise across attach/detach the way a maintained counter would. The scan is over at
        // most `max_view_batches` (1024) entries, on a path that already clones the whole map.
        if sponsorship.tag != PALW_SPONSOR_UNATTRIBUTED
            && sponsorship.max_view_batches_per_sponsor != 0
            && self.sponsor_slots(sponsorship.tag) >= sponsorship.max_view_batches_per_sponsor
        {
            return false;
        }
        self.batches.insert(
            m.batch_id,
            PalwBatchLifecycleV1 {
                status: PalwBatchStatus::Registering,
                registration_epoch: m.registration_epoch,
                activation_not_before_epoch: m.activation_not_before_epoch,
                expiry_epoch: m.expiry_epoch,
                leaf_count: m.leaf_count,
                chunk_count: m.chunk_count,
                chunks_present: [0u64; 4],
                leaf_root: m.leaf_root,
                cert_hash: None,
                sponsor_tag_hi: sponsorship.tag.0,
                sponsor_tag_lo: sponsorship.tag.1,
                cert_approving_stake: 0,
                first_cert_daa: None,
                revoked_from_daa: None,
            },
        );
        true
    }

    /// Record leaf chunk `chunk_index` for a Registering batch (idempotent per index via the bitmap; a
    /// re-sent chunk is a no-op, so duplicates cannot spoof completeness). When the bitmap's popcount
    /// reaches `chunk_count`, advances Registering → Committed.
    ///
    /// Merkle membership is enforced separately by `apply_palw_overlay_effect` before leaf insertion.
    /// This method only tracks `(batch_id, chunk_index)`, so its bitmap is a completeness hint rather
    /// than a content binding.
    pub fn apply_leaf_chunk(&mut self, batch_id: &Hash64, chunk_index: u16) -> bool {
        let Some(e) = self.batches.get_mut(batch_id) else { return false };
        if e.status != PalwBatchStatus::Registering || chunk_index >= e.chunk_count {
            return false;
        }
        let (word, bit) = ((chunk_index / 64) as usize, chunk_index % 64);
        let mask = 1u64 << bit;
        if e.chunks_present[word] & mask != 0 {
            return false; // already present (idempotent)
        }
        e.chunks_present[word] |= mask;
        let present: u32 = e.chunks_present.iter().map(|w| w.count_ones()).sum();
        if present == e.chunk_count as u32
            && let Some(next) = e.status.next(PalwBatchEvent::ChunksAndBondsComplete)
        {
            e.status = next;
        }
        true
    }

    /// A certificate (§10) advances Committed/Auditing → Certified and records the cert hash.
    /// (Committed → Auditing on the audit beacon is driven by [`Self::advance_epoch`].)
    ///
    /// # kaspa-pq ADR-0040 CERT-TRUST — WRITE-ONCE, NON-DESTRUCTIVE, AND DELIBERATELY BLIND
    ///
    /// This runs at the BODY/mergeset coordinate (`commit_palw_overlay_view`). That coordinate has no
    /// `ActiveBondView` — the bond view is accumulated during virtual chain traversal — so it cannot
    /// verify ANYTHING a certificate declares: not the votes, not the quorum, not the approving stake.
    /// It cannot even tell whether the carrying transaction was ever accepted (the fold reads raw
    /// mergeset bodies by necessity; moving it to the acceptance coordinate is a consensus split, see
    /// the argument at `commit_palw_overlay_view`).
    ///
    /// The previous rule ranked competing certificates by `cert.approving_stake` — a self-declared
    /// integer — *here*. That was the CERT-TRUST hole: a certificate declaring `u128::MAX` won every
    /// comparison, permanently evicted the honest certificate from the view, and installed an
    /// attacker-chosen `[activation, expiry)` window that `is_block_eligible_at` then enforced. One
    /// broadcast transaction, no stake, no bond, no valid signature, not even acceptance, bricked an
    /// honest provider's entire batch. The remedy is not to relocate the comparison but to remove the
    /// trust: **a coordinate that cannot verify a value must not rank by it.**
    ///
    /// So the fold is now purely MONOTONE:
    ///
    /// * `Committed | Auditing → Certified` promotion only;
    /// * `cert_hash` is set once, on first arrival, and never overwritten;
    /// * no window and no stake figure is copied out of the certificate at all;
    /// * an already-`Certified` batch is untouched — the function returns `false`.
    ///
    /// Nothing an unverified fold can do is therefore DESTRUCTIVE. The worst case is that a junk
    /// transaction promotes a batch to `Certified` with a `cert_hash` that names no attested blob,
    /// which buys nothing: to mine, a header must name a certificate that resolves out of `palw_store`,
    /// and that store is written only behind `verify_certificate_attestation` at the virtual
    /// coordinate.
    ///
    /// # Certificate supersession
    ///
    /// This coordinate does not replace an existing certificate.
    /// `supersession_window_daa`, `cert_approving_stake`, `cert_activation_epoch` and
    /// `cert_expiry_epoch` are inert. Certificates are content-addressed and coexist in `palw_store`,
    /// so a miner references whichever attested
    /// certificate it prefers via `palw_epoch_certificate_hash`, and including a minority assembly does
    /// not suppress a fuller one. If a stake-ordered canonical winner is wanted, it belongs at the
    /// VIRTUAL coordinate, where the bond view exists and the tally is actually recomputed.
    pub fn apply_certificate(&mut self, batch_id: &Hash64, cert_hash: Hash64, current_daa: u64) -> bool {
        let Some(e) = self.batches.get_mut(batch_id) else { return false };
        let next = match e.status {
            PalwBatchStatus::Auditing => e.status.next(PalwBatchEvent::CertificateQuorum),
            // tolerate a certificate that arrives before the audit-beacon epoch tick has advanced the
            // status to Auditing (mergeset ordering) — Committed also accepts the quorum.
            PalwBatchStatus::Committed => Some(PalwBatchStatus::Certified),
            // Already certified (or expired/revoked/registering): NOT a supersession candidate. See the
            // CERT-TRUST note above — first arrival wins and is never displaced at this coordinate.
            _ => None,
        };
        let Some(next) = next else { return false };
        e.status = next;
        // Write-once. The promotion arms above are only reachable from a state in which `cert_hash` is
        // still `None`, but the guard is explicit so a future status-machine edit cannot silently turn
        // this back into an overwrite.
        if e.cert_hash.is_none() {
            e.cert_hash = Some(cert_hash);
        }
        e.first_cert_daa.get_or_insert(current_daa);
        true
    }

    /// Header-v4 accepted-provenance certificate transition. Unlike the legacy raw-body fold above,
    /// the caller has already verified ML-DSA votes, recomputed quorum stake and passed the selected-
    /// parent DA gate. The canonical winner is therefore the greatest verified approving stake, with
    /// the lexicographically smallest content hash as a total-order tie break. This makes replay,
    /// sibling arrival order and `junk-first / honest-later` delivery irrelevant.
    pub fn apply_verified_certificate(
        &mut self,
        batch_id: &Hash64,
        cert_hash: Hash64,
        approving_stake: u128,
        current_daa: u64,
    ) -> bool {
        let Some(entry) = self.batches.get_mut(batch_id) else { return false };
        if !matches!(
            entry.status,
            PalwBatchStatus::Committed | PalwBatchStatus::Auditing | PalwBatchStatus::Certified | PalwBatchStatus::Active
        ) {
            return false;
        }
        let replace = match entry.cert_hash {
            None => true,
            Some(existing) => {
                approving_stake > entry.cert_approving_stake
                    || (approving_stake == entry.cert_approving_stake && cert_hash.as_bytes() < existing.as_bytes())
            }
        };
        if !replace {
            return false;
        }
        if matches!(entry.status, PalwBatchStatus::Committed | PalwBatchStatus::Auditing) {
            entry.status = PalwBatchStatus::Certified;
        }
        entry.cert_hash = Some(cert_hash);
        entry.cert_approving_stake = approving_stake;
        entry.first_cert_daa.get_or_insert(current_daa);
        true
    }

    /// §9.5 non-retroactive revocation from `effective_daa`. Only future unused leaves are invalidated;
    /// the entry is kept (still referenceable for already-drawn intervals below `effective_daa`) but the
    /// resolver treats a revoked batch as non-block-eligible from `effective_daa` on.
    pub fn mark_revoked(&mut self, batch_id: &Hash64, effective_daa: u64) -> bool {
        let Some(e) = self.batches.get_mut(batch_id) else { return false };
        if e.revoked_from_daa.is_some() {
            return false;
        }
        e.revoked_from_daa = Some(effective_daa);
        true
    }

    /// The epoch-driven transitions (design §9.5): Committed → Auditing at the audit-beacon epoch;
    /// Certified → Active at `activation_not_before_epoch`; and the incomplete/expiry timeouts. Applied
    /// once per block at the block's epoch (before [`Self::retain`]). Pure + monotone in epoch.
    /// Delegates to [`Self::advance_epoch_gated`] with the activation gate OPEN (the pre-K5 behavior;
    /// every existing caller/test is byte-identical).
    pub fn advance_epoch(&mut self, epoch: u64, registration_lead_epochs: u64, audit_window_epochs: u64) {
        self.advance_epoch_gated(epoch, registration_lead_epochs, audit_window_epochs, true)
    }

    /// [`Self::advance_epoch`] with the ADR-0039 §11.3 (K5) activation gate: while `activation_open` is
    /// `false` — the lagged buried beacon-health signal certifies a non-Healthy window
    /// ([`palw_lagged_activation_open`]) — the `Certified → Active` transition is FROZEN (no NEW batch
    /// activates during degradation). The gate DELAYS, never cancels: a frozen `Certified` batch flips
    /// `Active` on a later gated call with `true` (if still inside its windows), and its expiry timeout
    /// still fires while frozen (the `Active | Certified` expiry arm below is not gated). All other
    /// transitions (Registering/Committed/expiries) are gate-independent.
    pub fn advance_epoch_gated(&mut self, epoch: u64, registration_lead_epochs: u64, audit_window_epochs: u64, activation_open: bool) {
        use PalwBatchStatus::*;
        for e in self.batches.values_mut() {
            let deadline = e.registration_epoch.saturating_add(registration_lead_epochs).saturating_add(audit_window_epochs);
            match e.status {
                // an incomplete batch that missed its chunk/audit budget expires (I-2).
                Registering if epoch > deadline => e.status = Expired,
                // all chunks present; the audit beacon for this epoch opens the audit window.
                Committed if epoch >= deadline => e.status = Auditing,
                // certified and its activation epoch reached ⇒ live (only while the beacon admits new
                // activations — K5; a frozen batch falls through to the expiry arm below).
                Certified if activation_open && epoch >= e.activation_not_before_epoch => {
                    e.status = if epoch < e.expiry_epoch { Active } else { Expired };
                }
                Active | Certified if epoch >= e.expiry_epoch => e.status = Expired,
                _ => {}
            }
        }
    }
}

impl Default for PalwBatchViewV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwBatchViewV1 {
    fn estimate_mem_units(&self) -> usize {
        self.batches.len().max(1)
    }
}

// =============================================================================================
// Component-work cap — the D4 security bound (design §5.3, §15.5). Pure big-int arithmetic over
// `BlueWorkType`; the GHOSTDAG finalization slice calls these. Even a total forgery of the PALW /
// DNS-certificate stack amplifies an attacker's own hash work by at most `(cap+1)×` (5× at cap 4):
// `E = H + min(C, cap·H)`, so `H ≤ E ≤ (cap+1)·H` (invariant I-1).
// =============================================================================================

/// The canonical compute-to-hash cap ratio (design §5.3, invariant I-1): certified compute work is
/// credited into effective GHOSTDAG work at most `COMPUTE_TO_HASH_CAP ×` the cumulative hash work, so
/// `H ≤ E ≤ (COMPUTE_TO_HASH_CAP + 1)·H`. Single source of truth shared by [`PalwParams`] and the
/// GHOSTDAG finalization path so the two can never drift.
pub const COMPUTE_TO_HASH_CAP: u64 = 4;

#[inline]
fn compute_work_cap(hash_work: BlueWorkType, compute_to_hash_cap: u64) -> BlueWorkType {
    let (cap_h, overflow) = hash_work.overflowing_mul_u64(compute_to_hash_cap);
    if overflow { BlueWorkType::MAX } else { cap_h }
}

/// The capped compute-work term: `min(compute_work, cap · hash_work)` (design §15.5). Saturating on
/// the `cap · hash_work` multiply so a pathological hash work near `BlueWorkType::MAX` cannot wrap.
#[inline]
pub fn capped_compute_work(compute_work: BlueWorkType, hash_work: BlueWorkType, compute_to_hash_cap: u64) -> BlueWorkType {
    core::cmp::min(compute_work, compute_work_cap(hash_work, compute_to_hash_cap))
}

/// Remaining compute-credit capacity `max(cap·H − C, 0)` (design §5.4 / clause 8). The multiply
/// saturates consistently with [`capped_compute_work`], and an over-cap `C` produces zero rather
/// than wrapping. An algo-4 block is admissible only while this value is non-zero; Stage-A weight
/// zero still has positive headroom once the permanent hash floor has accumulated work.
#[inline]
pub fn compute_headroom(hash_work: BlueWorkType, compute_work: BlueWorkType, compute_to_hash_cap: u64) -> BlueWorkType {
    compute_work_cap(hash_work, compute_to_hash_cap).saturating_sub(compute_work)
}

/// Effective blue work `E = H + min(C, cap·H)` (design §5.3). This is the single value that enters
/// fork choice as the legacy `blue_work`; the components `H`/`C` are carried separately in Header v3
/// but are never fork-choice tie-breakers. Saturating add so `E` cannot wrap.
#[inline]
pub fn effective_blue_work(hash_work: BlueWorkType, compute_work: BlueWorkType, compute_to_hash_cap: u64) -> BlueWorkType {
    hash_work.saturating_add(capped_compute_work(compute_work, hash_work, compute_to_hash_cap))
}

// =============================================================================================
// Deterministic selection (provider pair §8.1, auditors §10.2) + coinbase pair split (§17.3).
// All pure and beacon-seeded — the requester cannot choose the pair, and every node derives the
// same auditors / split.
// =============================================================================================

/// Beacon-seeded provider index (design §8.1): `H(seed ‖ job_capability ‖ which ‖ attempt) mod
/// count`. `which` is 0 (provider A) or 1 (provider B); `attempt` salts rejection re-sampling when a
/// derived pair fails the distinctness / operator-group / region constraints. `count` must be > 0.
///
/// # SUPERSEDED by the SEL-01 weighted sampler (ADR-0040 §5.17.5)
///
/// This draw is UNIFORM over an index space, so it is blind to bond stake: a bonded operator that
/// splits one bond of `N` into `k` outpoints appears as `k` separate indices and buys `k` chances for
/// the same capital. The replacement is [`palw_weighted_sample_without_replacement`] over
/// [`aggregate_provider_credentials_at`] — a draw weighted by CREDENTIAL-aggregated, consensus-verified
/// collateral, where splitting is neutral because all of a credential's outpoints collapse into one
/// weight. This function is retained only as the negative reference and for its pinned test; it has no
/// consensus caller and must not acquire one (the provider-pair wiring adopts the weighted primitive in
/// the atomic activation slice, §5.17.7).
pub fn provider_index(seed: &Hash64, job_capability: &Hash64, which: u8, attempt: u32, count: u64) -> u64 {
    let mut p = Vec::with_capacity(2 * HASH64_SIZE + 1 + 4);
    push_hash(&mut p, seed);
    push_hash(&mut p, job_capability);
    p.push(which);
    p.extend_from_slice(&attempt.to_le_bytes());
    let d = blake2b_512_keyed(PALW_PROVIDER_SELECT_DOMAIN, &p);
    digest_low_u64(&d) % count.max(1)
}

/// Rejection-sample a distinct, acceptable provider pair (design §8.1). `accept(a, b)` encodes the
/// richer constraints the caller can check (distinct bond outpoint / operator group / region / relay
/// session) against its bond view; distinctness `a != b` is always enforced here. Returns `None` if
/// no acceptable pair is found within `max_attempts` (or `count < 2`).
///
/// # SUPERSEDED by the SEL-01 weighted sampler (ADR-0040 §5.17.5)
///
/// This composes the UNIFORM [`provider_index`], so it inherits the same Sybil-split hole. The weighted
/// replacement draws two DISTINCT credentials via [`palw_weighted_sample_without_replacement`] (`k = 2`)
/// over [`aggregate_provider_credentials_at`]: non-replacement makes a self-pair impossible without an
/// accept closure, and the weight is credential-aggregated resolved collateral. Retained only for its
/// pinned test; no consensus caller.
pub fn select_provider_pair(
    seed: &Hash64,
    job_capability: &Hash64,
    count: u64,
    max_attempts: u32,
    accept: impl Fn(u64, u64) -> bool,
) -> Option<(u64, u64)> {
    if count < 2 {
        return None;
    }
    for attempt in 0..max_attempts {
        let a = provider_index(seed, job_capability, 0, attempt, count);
        let b = provider_index(seed, job_capability, 1, attempt, count);
        if a != b && accept(a, b) {
            return Some((a, b));
        }
    }
    None
}

/// Auditor selection weight (design §10.2): `H(R_{E-1} ‖ batch_id ‖ bond_outpoint)`. Auditors are the
/// bonds with the smallest scores; the registering provider and related bonds are excluded by the
/// caller *before* scoring.
///
/// # SUPERSEDED by the SEL-01 weighted sampler (ADR-0040 §5.17.5)
///
/// This score ranks per BOND OUTPOINT and is blind to stake, so it is an UNWEIGHTED lottery in which
/// splitting a bond into `k` outpoints buys `k` entries. The replacement is
/// [`select_auditor_committee`] (built on [`palw_weighted_sample_without_replacement`] over
/// [`aggregate_provider_credentials_at`]), which draws credentials weighted by aggregated collateral so
/// splitting is neutral. Retained only as the negative reference and for its pinned test.
pub fn auditor_score(prev_seed: &Hash64, batch_id: &Hash64, bond: &TransactionOutpoint) -> Hash64 {
    let mut p = Vec::with_capacity(2 * HASH64_SIZE + HASH64_SIZE + 4);
    push_hash(&mut p, prev_seed);
    push_hash(&mut p, batch_id);
    push_hash(&mut p, &bond.transaction_id);
    p.extend_from_slice(&bond.index.to_le_bytes());
    blake2b_512_keyed(PALW_AUDITOR_SELECT_DOMAIN, &p)
}

/// Deterministically sample `count` auditors by lowest [`auditor_score`] (ties broken by the bond
/// outpoint) from an already-filtered candidate set (design §10.2). Returns them in canonical
/// (score, outpoint) order so every node agrees.
///
/// # Renamed from `sample_auditors_by_score` (ADR-0040)
///
/// "top" read as *highest stake*, which would be a **standing whale committee** — a pre-identifiable,
/// bribable, DoS-able fixed set, and a completely different mechanism from the design's beacon-seeded
/// weighted sampling. The function never did that (it ranks by hash score, so it is an UNWEIGHTED
/// lottery), but the name invited an implementer to make it so. This is the same failure mode the
/// `runtime_class_id` review flagged: a name that keeps its shape while its meaning is swapped out is
/// how the next person gets hurt.
///
/// # SUPERSEDED — the target mechanism has LANDED (ADR-0040 SEL-01 / §5.17.5)
///
/// Sampling here is **per-outpoint and unweighted**, so splitting one bond into `n` outpoints buys `n`
/// lottery tickets. The replacement — bond-weighted sampling **without replacement over
/// CREDENTIAL-aggregated stake** — now exists as [`select_auditor_committee`], composed from
/// [`aggregate_provider_credentials_at`] (which collapses a credential's split outpoints into one
/// weight) and [`palw_weighted_sample_without_replacement`]. AUTHSET-01's certificate re-derivation
/// will bind the WEIGHTED selector, never this one. This function is kept only as the negative
/// reference and for its pinned test; it still has NO consensus caller and must not acquire one. (The
/// off-protocol slate producer `mil::miner::audit::select_audit_slate` also still calls it; both it and
/// the AUTHSET verifier move onto the weighted selector together in the atomic activation slice
/// §5.17.7, so a producer/verifier mismatch cannot arise — there is no verifier yet.)
pub fn sample_auditors_by_score(
    prev_seed: &Hash64,
    batch_id: &Hash64,
    candidates: &[TransactionOutpoint],
    count: usize,
) -> Vec<TransactionOutpoint> {
    let mut scored: Vec<(Hash64, TransactionOutpoint)> =
        candidates.iter().map(|b| (auditor_score(prev_seed, batch_id, b), *b)).collect();
    scored.sort_by(|x, y| {
        x.0.as_byte_slice()
            .cmp(y.0.as_byte_slice())
            .then_with(|| x.1.transaction_id.as_byte_slice().cmp(y.1.transaction_id.as_byte_slice()))
            .then(x.1.index.cmp(&y.1.index))
    });
    scored.into_iter().take(count).map(|(_, b)| b).collect()
}

/// ADR-0039 §10.2 — the canonical commitment over a certificate's selected auditor set: a keyed hash
/// of the auditor bond outpoints in canonical (outpoint) order, length-prefixed. This is the value a
/// [`PalwBatchCertificateV2::auditor_set_commitment`] holds; recomputing it from the beacon-selected
/// set (via [`select_auditor_committee`]) and comparing binds the certificate to the audit round's
/// auditor slate. The bonds are sorted here, so the commitment is independent of the caller's input order.
/// Inert: referenced only by the (off-protocol) certificate producer and its tests until the audit
/// slice enforces the binding. The bond-weighted, credential-aggregated selector that binding requires
/// (ADR-0040 SEL-01) now exists — [`select_auditor_committee`] — but AUTHSET-01 still has no consensus
/// caller; see §5.17.7 for why the commitment check, the SAMPLE-01 root re-derivation, and the producer
/// rebuild land as ONE atomic activation slice, not as a lone verifier tweak.
pub fn auditor_set_commitment(bonds: &[TransactionOutpoint]) -> Hash64 {
    let mut sorted = bonds.to_vec();
    sorted.sort_by(cmp_outpoint);
    let mut p = Vec::with_capacity(8 + sorted.len() * (HASH64_SIZE + 4));
    p.extend_from_slice(&(sorted.len() as u64).to_le_bytes());
    for b in &sorted {
        push_hash(&mut p, &b.transaction_id);
        p.extend_from_slice(&b.index.to_le_bytes());
    }
    blake2b_512_keyed(PALW_AUDITOR_SET_DOMAIN, &p)
}

// =============================================================================================
// ADR-0040 SEL-01 (§5.17.5) — credential-aggregated, bond-weighted, NON-REPLACEMENT selection.
//
// The defect SEL-01 closes: `auditor_score` ranks per BOND OUTPOINT and `provider_index` draws
// UNIFORMLY, so a bonded operator that splits one bond of `N` sompi into `k` outpoints of `N/k` buys
// `k` lottery entries for the same capital (§4.1: "outpoint 単位 = 100 分割で抽選券 100 枚"). The fix
// is to (a) AGGREGATE stake by CREDENTIAL before drawing — so splitting leaves total stake unchanged —
// and (b) WEIGHT the draw by that aggregated, CONSENSUS-VERIFIED collateral (the ECON-03 resolved bond
// amount at a frozen point of view), never a self-declared number.
//
// PURE and order-independent (§5.17.2 / §5.17.10). The only stateful inputs are the frozen
// `ProviderBondView` snapshot — whose `apply`/`revert` are exact inverses, so every reorg path to a
// block yields the same view — and the buried-walk beacon seed `prev_seed` (§5.17.3). Given those, the
// candidate multiset and every draw are a deterministic function of the block's past alone; nothing
// here reads a tip-relative store, a mutable status flag, or the epoch-keyed accum.
//
// The selection primitives are pure and are used by `verify_certificate_attestation` to re-derive the
// AUTHSET-01 commitment.
// =============================================================================================

/// One credential's aggregated, consensus-verified stake for SEL-01 selection.
///
/// `credential` is the aggregation key: the bond owner's identity (`owner_pubkey_hash`). In v1 the
/// signing key IS the registration credential and there is no delegation layer (§5.16.1), so the
/// credential doubles as the "delegation root" the §4.1 exclusions name. Splitting a bond keeps
/// `owner_public_key` — hence `owner_pubkey_hash` — constant, so every split share lands in the SAME
/// credential and the summed `weight` is unchanged: that is the whole anti-split property.
///
/// `weight` is the sum of the RESOLVED collateral (`PalwProviderBondRecord::amount_sompi`, which ECON-03
/// cross-checked against locked coins) of every bond of this credential that is `Active` at the point of
/// view. `representative` is the credential's canonical bond outpoint (the smallest by [`cmp_outpoint`])
/// — a stable, order-independent stand-in for the credential in the selected slate and its
/// [`auditor_set_commitment`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwCredentialStake {
    pub credential: Hash64,
    pub weight: u128,
    pub representative: TransactionOutpoint,
}

/// Aggregate a [`ProviderBondView`] into per-credential stake at `pov_daa_score` (SEL-01, §5.17.5).
///
/// Sums the resolved collateral of every `Active` bond by `owner_pubkey_hash`, dropping:
/// - bonds that do not resolve to `Active` collateral at the point of view — pending (immature),
///   unbonding, or slashed — since [`is_provider_bond_active_at`] is false and they back nothing; and
/// - any credential in `excluded_credentials` (the registering provider's own credential / same
///   delegation root / a previously-selected partner) or whose `operator_group_id` is in
///   `excluded_operator_groups` (operator-group siblings, §5.17.4 step 2).
///
/// A credential whose aggregated `Active` weight is `0` is not a candidate. The result is sorted by
/// `credential` (a total order), so it is a pure function of the view's CONTENTS, independent of the
/// order bonds were inserted — the order-independence SEL-01 requires (§5.17.10).
pub fn aggregate_provider_credentials_at(
    view: &ProviderBondView,
    pov_daa_score: u64,
    excluded_credentials: &HashSet<Hash64>,
    excluded_operator_groups: &HashSet<Hash64>,
) -> Vec<PalwCredentialStake> {
    let mut by_credential: std::collections::HashMap<Hash64, (u128, TransactionOutpoint)> = std::collections::HashMap::new();
    for record in view.records() {
        // Non-`Active` bonds contribute nothing — pending/immature, unbonding, or slashed. This is the
        // eligibility floor: a bond only weighs a credential once its ECON-03 collateral is live.
        if !is_provider_bond_active_at(&record, pov_daa_score) {
            continue;
        }
        let credential = record.owner_pubkey_hash;
        if excluded_credentials.contains(&credential) || excluded_operator_groups.contains(&record.operator_group_id) {
            continue;
        }
        let entry = by_credential.entry(credential).or_insert((0u128, record.bond_outpoint));
        entry.0 = entry.0.saturating_add(record.amount_sompi as u128);
        // Canonical representative = smallest outpoint, so it does not depend on iteration order.
        if cmp_outpoint(&record.bond_outpoint, &entry.1) == Ordering::Less {
            entry.1 = record.bond_outpoint;
        }
    }
    let mut out: Vec<PalwCredentialStake> = by_credential
        .into_iter()
        .filter(|(_, (w, _))| *w > 0)
        .map(|(credential, (weight, representative))| PalwCredentialStake { credential, weight, representative })
        .collect();
    out.sort_by(|a, b| a.credential.as_byte_slice().cmp(b.credential.as_byte_slice()));
    out
}

/// **REG-CAP-01 — the MEASURED audit capacity behind one Compute Set at one point of view.**
///
/// Every field is derived from the SAME population [`aggregate_provider_credentials_at`] hands the
/// committee sampler, because a gate that measures a different population than the selector draws
/// from is not a gate. Shares are basis points of total matured stake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAuditorCapacityV1 {
    pub eligible_credentials: u32,
    pub operator_groups: u32,
    pub total_matured_bond: u128,
    pub largest_credential_share_bps: u16,
    pub largest_group_share_bps: u16,
}

/// **REG-CAP-01 — the capacity a Compute Set must have before it may earn.**
///
/// # What was wrong
///
/// `PalwComputeSetPolicyV1::auditor_capacity_threshold` and
/// `PalwConformanceVectorSetV1::auditor_capacity_threshold` were compared against a
/// `measured_auditor_capacity` argument that NO production caller ever supplied. The number was
/// governed, stored, and checked against nothing.
///
/// Worse, a bare count is the wrong shape: five credentials behind one operator, or five
/// credentials where one holds 96 % of the stake, is one auditor wearing hats. The committee
/// sampler already aggregates by credential and excludes operator-group siblings, so the gate has
/// to speak the same language — counts AND concentration.
///
/// # The rule this encodes
///
/// When capacity is short, the network lowers ISSUANCE, never the safety requirement. There is
/// deliberately no path here that shrinks `min_selected_committee_members` to fit the auditors
/// available: a set that cannot be audited independently goes to Shadow with zero share and zero
/// weight and keeps serving traffic without minting DAG credit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAuditorCapacityGateV1 {
    pub min_eligible_credentials: u16,
    pub min_operator_groups: u16,
    pub min_selected_committee_members: u16,
    pub max_largest_credential_share_bps: u16,
    pub max_largest_group_share_bps: u16,
    pub min_total_matured_bond: u128,
}

impl PalwAuditorCapacityGateV1 {
    /// Initial permissioned-testnet calibration. These are a starting point tied to a threat model
    /// and a bond price, not derived constants — a value-bearing network wants materially more
    /// (the audit's own guidance: ≥8 credentials, ≥5 groups, ≥5 committee, ≤25–33 % concentration).
    pub const TESTNET: Self = Self {
        min_eligible_credentials: 4,
        min_operator_groups: 3,
        min_selected_committee_members: 3,
        max_largest_credential_share_bps: 4_000,
        max_largest_group_share_bps: 5_000,
        min_total_matured_bond: 0,
    };

    /// `Ok(())` when `capacity` supports independent auditing; otherwise WHY not, so an operator
    /// can see which dimension is short rather than only that the set stopped earning.
    pub fn admits(&self, capacity: &PalwAuditorCapacityV1) -> Result<(), PalwAuditorCapacityShortfall> {
        use PalwAuditorCapacityShortfall::*;
        if capacity.eligible_credentials < self.min_eligible_credentials as u32 {
            return Err(TooFewCredentials { have: capacity.eligible_credentials, need: self.min_eligible_credentials });
        }
        if capacity.operator_groups < self.min_operator_groups as u32 {
            return Err(TooFewOperatorGroups { have: capacity.operator_groups, need: self.min_operator_groups });
        }
        // A committee cannot exceed the credentials it is drawn from (sampling is WITHOUT
        // replacement), so this is the "we cannot seat a real committee" case.
        if capacity.eligible_credentials < self.min_selected_committee_members as u32 {
            return Err(CommitteeUnseatable { have: capacity.eligible_credentials, need: self.min_selected_committee_members });
        }
        if capacity.total_matured_bond < self.min_total_matured_bond {
            return Err(TooLittleBond { have: capacity.total_matured_bond, need: self.min_total_matured_bond });
        }
        if capacity.largest_credential_share_bps > self.max_largest_credential_share_bps {
            return Err(CredentialTooConcentrated {
                share_bps: capacity.largest_credential_share_bps,
                cap_bps: self.max_largest_credential_share_bps,
            });
        }
        if capacity.largest_group_share_bps > self.max_largest_group_share_bps {
            return Err(OperatorGroupTooConcentrated {
                share_bps: capacity.largest_group_share_bps,
                cap_bps: self.max_largest_group_share_bps,
            });
        }
        Ok(())
    }
}

/// Which dimension of [`PalwAuditorCapacityGateV1`] a set fell short on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwAuditorCapacityShortfall {
    #[error("{have} eligible auditor credentials, need {need}")]
    TooFewCredentials { have: u32, need: u16 },
    #[error("{have} independent operator groups, need {need}")]
    TooFewOperatorGroups { have: u32, need: u16 },
    #[error("{have} credentials cannot seat a committee of {need} (sampling is without replacement)")]
    CommitteeUnseatable { have: u32, need: u16 },
    #[error("{have} sompi of matured auditor bond, need {need}")]
    TooLittleBond { have: u128, need: u128 },
    #[error("one credential holds {share_bps} bps of auditor stake, cap is {cap_bps}")]
    CredentialTooConcentrated { share_bps: u16, cap_bps: u16 },
    #[error("one operator group holds {share_bps} bps of auditor stake, cap is {cap_bps}")]
    OperatorGroupTooConcentrated { share_bps: u16, cap_bps: u16 },
}

/// **REG-CAP-01 — measure the audit capacity actually present at `pov_daa_score`.**
///
/// Credential aggregation is [`aggregate_provider_credentials_at`] verbatim (so the numbers
/// describe the exact draw population); operator groups are then rolled up over that same set, so
/// a credential split across ten bonds counts once and ten credentials behind one operator count
/// as one group.
pub fn measure_auditor_capacity(view: &ProviderBondView, pov_daa_score: u64) -> PalwAuditorCapacityV1 {
    let empty: HashSet<Hash64> = HashSet::new();
    let credentials = aggregate_provider_credentials_at(view, pov_daa_score, &empty, &empty);
    // Map each eligible credential to its operator group. A credential's bonds all carry the same
    // group in practice; take the smallest so the rollup is order-independent either way.
    let mut group_of: std::collections::HashMap<Hash64, Hash64> = std::collections::HashMap::new();
    for record in view.records() {
        if !is_provider_bond_active_at(&record, pov_daa_score) {
            continue;
        }
        let entry = group_of.entry(record.owner_pubkey_hash).or_insert(record.operator_group_id);
        if record.operator_group_id.as_byte_slice() < entry.as_byte_slice() {
            *entry = record.operator_group_id;
        }
    }
    let total: u128 = credentials.iter().fold(0u128, |sum, c| sum.saturating_add(c.weight));
    let mut by_group: std::collections::HashMap<Hash64, u128> = std::collections::HashMap::new();
    for credential in &credentials {
        let group = group_of.get(&credential.credential).copied().unwrap_or(credential.credential);
        let slot = by_group.entry(group).or_insert(0);
        *slot = slot.saturating_add(credential.weight);
    }
    let share_bps = |part: u128| -> u16 {
        if total == 0 {
            return 0;
        }
        // Round UP: a concentration cap must not be cleared by truncation.
        (part.saturating_mul(BPS_DENOMINATOR_U128).div_ceil(total)).min(BPS_DENOMINATOR_U128) as u16
    };
    PalwAuditorCapacityV1 {
        eligible_credentials: credentials.len() as u32,
        operator_groups: by_group.len() as u32,
        total_matured_bond: total,
        largest_credential_share_bps: share_bps(credentials.iter().map(|c| c.weight).max().unwrap_or(0)),
        largest_group_share_bps: share_bps(by_group.values().copied().max().unwrap_or(0)),
    }
}

const BPS_DENOMINATOR_U128: u128 = 10_000;

/// Deterministic weighted draw in `[0, total)` for round `round` (SEL-01) — a wide (128-bit) reduction
/// of a keyed digest of `seed ‖ context ‖ round`. The modulo bias is cryptographically negligible for
/// any realistic total (`total ≪ 2^128`) and, being deterministic, is consensus-safe regardless.
fn palw_weighted_draw(seed: &Hash64, context: &Hash64, round: u64, total: u128) -> u128 {
    let mut p = Vec::with_capacity(2 * HASH64_SIZE + 8);
    push_hash(&mut p, seed);
    push_hash(&mut p, context);
    p.extend_from_slice(&round.to_le_bytes());
    let d = blake2b_512_keyed(PALW_WEIGHTED_DRAW_DOMAIN, &p);
    let b = d.as_byte_slice();
    let draw =
        u128::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]);
    draw % total.max(1)
}

/// Credential-aggregated, bond-weighted, NON-REPLACEMENT sample of up to `k` credentials (SEL-01,
/// §5.17.5) — the replacement for the unweighted [`auditor_score`] ordering and the uniform
/// [`provider_index`].
///
/// `candidates` are the per-credential stakes from [`aggregate_provider_credentials_at`]. Each round
/// draws one credential with probability proportional to its remaining `weight`, then REMOVES it, so a
/// credential is never selected twice (this is also what makes a provider self-pair impossible at
/// `k = 2`). Returns the selected credentials in SELECTION order (first draw first); fewer than `k` are
/// returned only if the pool is exhausted.
///
/// # Order independence (the SEL-01 disqualifier, proved)
///
/// The pool is sorted by `credential` up front (a total order), and each draw walks that canonical order
/// accumulating weight until it passes the draw value. The draw value depends only on
/// `(seed, context, round)`. Removal preserves canonical order. Therefore the output is a pure function
/// of the candidate SET and `(seed, context)` — identical no matter what order the caller listed the
/// candidates or (upstream) what reorg path produced the `bond_view`.
///
/// # Anti-split
///
/// Because [`aggregate_provider_credentials_at`] collapses all of one credential's outpoints into a
/// single `(credential, weight)` BEFORE this runs, splitting a bond of `N` into `k` outpoints of `N/k`
/// produces the IDENTICAL candidate here (same credential, same summed weight `N`), hence the identical
/// draw — splitting buys nothing. Splitting into distinct CREDENTIALS (fresh owner keys) is a different,
/// out-of-scope Sybil the design accepts and bounds economically (§3.4.1 / §5 β), not here.
pub fn palw_weighted_sample_without_replacement(
    seed: &Hash64,
    context: &Hash64,
    candidates: &[PalwCredentialStake],
    k: usize,
) -> Vec<PalwCredentialStake> {
    let mut pool: Vec<PalwCredentialStake> = candidates.to_vec();
    pool.sort_by(|a, b| a.credential.as_byte_slice().cmp(b.credential.as_byte_slice()));
    let mut total: u128 = pool.iter().fold(0u128, |acc, c| acc.saturating_add(c.weight));
    let mut selected: Vec<PalwCredentialStake> = Vec::with_capacity(k.min(pool.len()));
    let mut round = 0u64;
    while selected.len() < k && !pool.is_empty() && total > 0 {
        let draw = palw_weighted_draw(seed, context, round, total);
        // Walk the canonical pool accumulating weight; the first candidate whose cumulative weight
        // passes `draw` is selected. Since `draw < total` and the final cumulative weight == total, some
        // candidate always triggers; the last index is a belt-and-suspenders fallthrough only.
        let mut acc = 0u128;
        let mut idx = pool.len() - 1;
        for (i, c) in pool.iter().enumerate() {
            acc = acc.saturating_add(c.weight);
            if draw < acc {
                idx = i;
                break;
            }
        }
        total = total.saturating_sub(pool[idx].weight);
        selected.push(pool.remove(idx));
        round += 1;
    }
    selected
}

/// Beacon-select the eligible auditor committee for a batch while PRESERVING each selected credential's
/// aggregate weight (SEL-01 + AUTHSET-01, §5.17.4 / §5.17.5).
///
/// The weight is load-bearing after selection too: certificate PASS stake and the full-slate quorum
/// denominator must use the same credential-aggregated collateral that drove the draw. Reducing a
/// selected credential back to its representative bond's individual amount would make bond splitting
/// change quorum weight even though it cannot change selection weight. Members are returned in canonical
/// representative-outpoint order; the commitment is over those representatives, as before.
pub fn select_weighted_auditor_committee(
    prev_seed: &Hash64,
    batch_id: &Hash64,
    bond_view: &ProviderBondView,
    pov_daa_score: u64,
    excluded_credentials: &HashSet<Hash64>,
    excluded_operator_groups: &HashSet<Hash64>,
    committee_size: usize,
) -> (Vec<PalwCredentialStake>, Hash64) {
    let candidates = aggregate_provider_credentials_at(bond_view, pov_daa_score, excluded_credentials, excluded_operator_groups);
    let mut slate = palw_weighted_sample_without_replacement(prev_seed, batch_id, &candidates, committee_size);
    slate.sort_by(|a, b| cmp_outpoint(&a.representative, &b.representative));
    let bonds: Vec<TransactionOutpoint> = slate.iter().map(|member| member.representative).collect();
    let commitment = auditor_set_commitment(&bonds);
    (slate, commitment)
}

/// Compatibility projection of [`select_weighted_auditor_committee`] for consumers that need only the
/// canonical representative outpoints and set commitment. Consensus quorum accounting must use the
/// weighted form; this wrapper intentionally discards the aggregate weights.
///
/// `prev_seed` is `R_{audit_beacon_epoch-1}` from the buried-walk resolver (§5.17.3);
/// `pov_daa_score = audit_beacon_epoch * epoch_len` is the frozen snapshot (§5.17.2). `excluded_*` are
/// the §5.17.4-step-2 exclusions the caller derives from the batch's on-chain leaves (registering
/// provider credential / same delegation root / operator-group siblings / previously-selected).
///
/// This is the value AUTHSET-01 will re-derive at `verify_certificate_attestation` and match against a
/// certificate's declared `auditor_set_commitment`; it has NO consensus caller yet (the wiring, together
/// with the SAMPLE-01 root re-derivation and the producer rebuild, is the atomic activation slice
/// §5.17.7). The returned outpoints are sorted, so the commitment is independent of selection order.
pub fn select_auditor_committee(
    prev_seed: &Hash64,
    batch_id: &Hash64,
    bond_view: &ProviderBondView,
    pov_daa_score: u64,
    excluded_credentials: &HashSet<Hash64>,
    excluded_operator_groups: &HashSet<Hash64>,
    committee_size: usize,
) -> (Vec<TransactionOutpoint>, Hash64) {
    let (slate, commitment) = select_weighted_auditor_committee(
        prev_seed,
        batch_id,
        bond_view,
        pov_daa_score,
        excluded_credentials,
        excluded_operator_groups,
        committee_size,
    );
    let bonds = slate.into_iter().map(|member| member.representative).collect();
    (bonds, commitment)
}

// =============================================================================================
// ADR-0040 SAMPLE-01 (§5.17.6) — the on-chain `audit_sample_root` REDEFINITION.
//
// The defect SAMPLE-01 closes: `audit_sample_root` was a producer-supplied field with ZERO consensus
// re-derivation, so a producer could sign any value. The intent (I-14) was that the value commit to the
// beacon-selected RECEIPT CHUNKS an auditor fetched — but those chunks live in off-chain DA, so
// consensus cannot recompute a root over their contents and cannot prove possession on-chain.
//
// The soundest enforceable substitute (§5.17.6) is a REDEFINITION: sample the batch's ON-CHAIN leaves
// by the audit-epoch beacon seed, and commit to exactly those leaves' `receipt_da_root` values (each a
// per-leaf DA commitment already stored on chain). This binds "the certificate covers exactly the
// beacon-selected leaves' DA roots" — strictly weaker than off-chain possession, but re-derivable by
// every node from `(prev_seed, batch_id, leaf_count, sample_size, the on-chain leaves)`.
//
// PURE and order-independent: both functions below are deterministic functions of their arguments
// alone. The sampler reads no state; the root is a fold over the ordered `receipt_da_root` values the
// caller passes in ascending sampled-index order. The order-independence of the INPUTS (the resolved
// seed and the on-chain leaves) is the caller's property, proved at the coordinate.
// =============================================================================================

/// One weighted-style draw for the leaf sampler (SAMPLE-01) — a wide reduction of a keyed digest of
/// `seed ‖ batch_id ‖ round`, domain-separated by [`PALW_AUDIT_SAMPLE_DOMAIN`]. Deterministic, so any
/// modulo bias is consensus-safe; the values here (`remaining ≤ u32::MAX`) make it negligible anyway.
fn palw_audit_sample_draw(seed: &Hash64, batch_id: &Hash64, round: u64) -> u128 {
    let mut p = Vec::with_capacity(2 * HASH64_SIZE + 8);
    push_hash(&mut p, seed);
    push_hash(&mut p, batch_id);
    p.extend_from_slice(&round.to_le_bytes());
    let d = blake2b_512_keyed(PALW_AUDIT_SAMPLE_DOMAIN, &p);
    let b = d.as_byte_slice();
    u128::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]])
}

/// Deterministically choose up to `sample_size` DISTINCT leaf indices in `[0, leaf_count)` for a batch's
/// audit round, keyed by the prior-epoch beacon seed `prev_seed` and the batch id (SAMPLE-01, §5.17.6).
///
/// Realized as a partial Fisher–Yates shuffle over `0..leaf_count`, taking the first
/// `min(sample_size, leaf_count)` positions; the swap targets come from [`palw_audit_sample_draw`]. The
/// returned indices are sorted ascending, so the downstream `receipt_da_root` list — and hence the
/// re-derived [`palw_audit_sample_root`] — is canonical and independent of draw order.
///
/// PURE and order-independent: the result is a function of `(prev_seed, batch_id, leaf_count,
/// sample_size)` alone — no state read, no dependence on caller order. Two nodes with the same seed
/// (resolved order-independently by the buried walk, §5.17.3) sample the identical index set. When
/// `sample_size >= leaf_count` every leaf is selected; when `leaf_count == 0` the set is empty.
pub fn palw_deterministic_sample(prev_seed: &Hash64, batch_id: &Hash64, leaf_count: u32, sample_size: u32) -> Vec<u32> {
    if leaf_count == 0 || sample_size == 0 {
        return Vec::new();
    }
    let take = sample_size.min(leaf_count) as usize;
    let mut pool: Vec<u32> = (0..leaf_count).collect();
    for i in 0..take {
        // `remaining` positions still to draw from: [i, leaf_count). Draw an offset in [0, remaining)
        // and swap the chosen element into position `i`. `remaining ≥ 1` for every `i < take`.
        let remaining = (leaf_count as usize) - i;
        let offset = (palw_audit_sample_draw(prev_seed, batch_id, i as u64) % remaining as u128) as usize;
        pool.swap(i, i + offset);
    }
    let mut chosen: Vec<u32> = pool[..take].to_vec();
    chosen.sort_unstable();
    chosen
}

/// The re-derived `audit_sample_root` (SAMPLE-01, §5.17.6): a canonical, domain-separated vector
/// commitment over the beacon-selected leaves' `receipt_da_root` values, given in ASCENDING sampled-index
/// order (as [`palw_deterministic_sample`] returns them). Length-prefixed so a shorter or longer list
/// can never collide, and keyed by [`PALW_AUDIT_SAMPLE_ROOT_DOMAIN`] so a sample root can never equal any
/// other PALW commitment (e.g. a `leaf_root`). This is the value `verify_certificate_attestation`
/// re-derives and matches against `cert.audit_sample_root`, and that every auditor vote signs over.
///
/// The ADR states the redefinition as `merkle_root({leaf[i].receipt_da_root : i ∈ indices})`; a
/// length-prefixed keyed fold over the same ordered set is an equivalent binding vector commitment and
/// is what the producer and verifier BOTH compute, so there is no producer/verifier form mismatch.
pub fn palw_audit_sample_root(receipt_da_roots: &[Hash64]) -> Hash64 {
    let mut p = Vec::with_capacity(8 + receipt_da_roots.len() * HASH64_SIZE);
    p.extend_from_slice(&(receipt_da_roots.len() as u64).to_le_bytes());
    for r in receipt_da_roots {
        push_hash(&mut p, r);
    }
    blake2b_512_keyed(PALW_AUDIT_SAMPLE_ROOT_DOMAIN, &p)
}

/// PALW **algo-4** lane coinbase split (basis points, sums to 10 000), **asymmetric to the algo-3
/// hash lane** which keeps its 62 / 8 / 30. ADR-0039 §17.1 (amended 2026-07-13): the compute lane
/// routes a larger base to the LLM providers by HALVING the validator share 30 % → 15 %; the freed
/// 15 % goes to the GPU compute source, so the provider base is 62 % + 15 % = **77 %**. This is an
/// intentional trade of DNS-finality validator subsidy for compute incentive (§17, user decision), and
/// only on PALW blocks — hash-lane blocks are unchanged. At the 1 : 4 lane split (10-BPS PALW genesis:
/// hash 2 + replica 8; proportion-identical to the 40-BPS 8 + 32) the effective validator subsidy
/// across ALL blocks is ≈ 0.2·30 % + 0.8·15 % = **18 %** (down from 30 %) — independent of total BPS.
pub const PALW_PROVIDER_BASE_BPS: u16 = 7700; // 77 % → provider pair (was 62 %; +15 pt taken from validator)
/// PALW algo-4 lane includer/assembler share — 8 %, unchanged from the hash lane.
pub const PALW_INCLUSION_BPS: u16 = 800;
/// PALW algo-4 lane validator share — 15 % (hash lane keeps 30 %). The halved compute-lane validator
/// subsidy is the source of the extra 15 % routed to providers via [`PALW_PROVIDER_BASE_BPS`].
pub const PALW_VALIDATOR_BPS: u16 = 1500;

// Provider-pair rewards are derived through the production composition:
// `split_block_subsidy(...).worker_base_sompi` followed by
// `premium_split(base, replica_count, π)`. Keeping this as the only implementation avoids arithmetic
// drift between helpers and coinbase construction.

/// ADR-0039 §15.3 / I-5 — the deterministic PALW **duplicate-ticket** rule (the nullifier dedup that
/// makes double-use detectable from the header DAG alone). Given the child's mergeset in **consensus
/// order** (ascending blue-work, hash tie-break) as one `Option<ticket_nullifier>` per block (`None`
/// for a non-PALW / algo-3 block) and `seed_active` = the nullifiers already active in the **selected
/// parent's past** (produced by the §15.2 active-nullifier window store), returns the mergeset
/// positions that are DUPLICATE algo-4 tickets and must be recolored red (`PalwDuplicateTicket`): a
/// nullifier already active before this block, or already first-seen earlier in this very mergeset.
/// First-seen tickets are kept blue-eligible.
///
/// Pure and order-deterministic — every node derives the same set from the header DAG, no Bloom filter
/// or probabilistic structure in the consensus decision (I-5). algo-3 blocks pass `None` and are never
/// duplicates. Inert: with no algo-4 header minted, every entry is `None`, so the result is always
/// empty and GHOSTDAG coloring is unchanged.
///
/// NOTE: this is the frozen dedup **semantics**. Wiring it INTO the GHOSTDAG coloring loop (not as a
/// naive post-pass vector move — forbidden by §15.3) and building the persistent selected-parent-past
/// `seed_active` window store (§15.2) are the activation steps, gated on the PALW fence.
pub fn palw_duplicate_ticket_positions(ordered: &[Option<Hash64>], seed_active: &HashSet<Hash64>) -> Vec<usize> {
    let mut seen: HashSet<Hash64> = seed_active.clone();
    let mut dups = Vec::new();
    for (i, nf) in ordered.iter().enumerate() {
        if let Some(nf) = nf {
            // `insert` returns false when the nullifier was already active (seed) or already first-seen
            // earlier in this mergeset — either way a double-use ⇒ recolor red.
            if !seen.insert(*nf) {
                dups.push(i);
            }
        }
    }
    dups
}

/// ADR-0039 §15.2 — the **active-nullifier window**: the ticket nullifiers still inside the retention
/// window, each keyed by the DAA score it was first seen at so the window prunes forward and stays
/// bounded to `nullifier_retention_daa`. Deterministically reconstructable from the header DAG (no
/// Bloom filter in the consensus decision, I-5). A sorted (`BTreeMap`) structure so a copy-on-write
/// fork view and any canonical active-set commitment are stable across nodes.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwActiveNullifierSet {
    seen: BTreeMap<Hash64, u64>,
}

impl PalwActiveNullifierSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// First-seen insert. Returns `false` (a duplicate — the §15.3 recolor trigger) if the nullifier is
    /// already active; keeps the earliest first-seen DAA otherwise.
    pub fn insert(&mut self, nullifier: Hash64, first_seen_daa: u64) -> bool {
        use std::collections::btree_map::Entry;
        match self.seen.entry(nullifier) {
            Entry::Occupied(_) => false,
            Entry::Vacant(v) => {
                v.insert(first_seen_daa);
                true
            }
        }
    }

    pub fn contains(&self, nullifier: &Hash64) -> bool {
        self.seen.contains_key(nullifier)
    }

    /// Prune every nullifier first seen strictly before `retention_floor_daa` (= `current_daa −
    /// nullifier_retention_daa`), bounding the window. Returns the count pruned.
    pub fn prune_below(&mut self, retention_floor_daa: u64) -> usize {
        let before = self.seen.len();
        self.seen.retain(|_, daa| *daa >= retention_floor_daa);
        before - self.seen.len()
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// The active nullifiers in canonical (sorted) order — the seed a child derives its dedup from and
    /// the preimage of any active-set commitment.
    pub fn iter_sorted(&self) -> impl Iterator<Item = (&Hash64, &u64)> {
        self.seen.iter()
    }

    /// Seed this set from another window (first-seen semantics — keeps the earliest DAA per nullifier).
    /// Used to seed a child's GHOSTDAG dedup from its selected parent's persisted window (§15.2), so a
    /// block reusing a buried ANCESTOR's ticket — not just one in the current mergeset — is detected.
    pub fn merge_from(&mut self, other: &PalwActiveNullifierSet) {
        for (nf, daa) in other.iter_sorted() {
            self.insert(*nf, *daa);
        }
    }

    /// ADR-0039 §15.3 integrated with the §15.2 window: apply a child's mergeset (per-block
    /// `Option<(ticket_nullifier, daa_score)>` in consensus order; `None` = non-PALW) to this set
    /// **seeded from the selected parent's past**, returning the mergeset positions that are DUPLICATE
    /// algo-4 tickets to recolor red, and inserting the first-seen ones into the window. Deterministic;
    /// mirrors the pure [`palw_duplicate_ticket_positions`] while advancing the persistent window.
    pub fn apply_mergeset(&mut self, ordered: &[Option<(Hash64, u64)>]) -> Vec<usize> {
        let mut dups = Vec::new();
        for (i, entry) in ordered.iter().enumerate() {
            if let Some((nf, daa)) = entry
                && !self.insert(*nf, *daa)
            {
                dups.push(i);
            }
        }
        dups
    }
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwActiveNullifierSet {
    /// Unit-estimable by the number of active nullifiers (the store uses a `Count` cache policy, like
    /// the other per-block Vec/map stores — no byte estimation).
    fn estimate_mem_units(&self) -> usize {
        self.seen.len().max(1)
    }
}

/// A red / duplicate PALW source pays the provider pair **nothing** (design §17.4); the unminted base
/// is NOT redistributed to the current miner (it is unissued / security-reserve). This helper names
/// that rule at the type level.
#[inline]
pub const fn palw_red_or_duplicate_provider_reward() -> (u64, u64) {
    (0, 0)
}

// =============================================================================================
// Canonical Compute v1 §13 — determinism class as DATA (docs/design/misaka-canonical-compute-v1.md).
// The class boundary is a *verifiable predicate* ("reproduces a committed golden vector set"), not a
// hardware enumeration; adding/retiring a class is a governed DATA commit, not a hard fork. Pure types +
// predicates only — inert until a set is committed (none on any shipped preset), so byte-identical live.
// =============================================================================================

/// A committed conformance vector set: the DATA that *defines* one determinism class. A provider is in the
/// class iff it reproduces this set's golden vectors byte-exact (checked off-chain by the §14 self-
/// conformance gate); on-chain only the set's lifecycle is carried so a registration referencing it is
/// decidable. **Multi-set by construction** (rolling migration): a codegen / driver / OS cliff, or a
/// model/manifest update, is a NEW set that coexists with the old, and k=2 pairs form only *within* one set
/// (soundness invariant across the migration window).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwConformanceVectorSetV1 {
    pub version: u16,
    /// Content id of the committed golden vector set — the class predicate's subject and the k=2 pairing key.
    pub set_id: Hash64,
    /// The tier this set serves. Tier activation is DERIVED from an active set referencing this id (§15);
    /// there is deliberately NO separate `enabled_tiers` flag (single source of truth).
    pub model_profile_id: Hash64,
    /// The set becomes referenceable for NEW registrations at this DAA score (once the auditor-capacity gate
    /// is met). `u64::MAX` = a candidate set not yet activated.
    pub activation_daa_score: u64,
    /// Non-retroactive deprecation (isomorphic to revocation): `Some(d)` refuses only NEW registrations at
    /// `daa >= d`; already-active providers and already-minted leaves are untouched.
    pub deprecated_from_daa: Option<u64>,
    /// Minimum bit-exact-reproducing auditor capacity before this set may certify (§13 activation gate): an
    /// audit rail that cannot reproduce a set cannot certify it.
    pub auditor_capacity_threshold: u32,
}

impl PalwConformanceVectorSetV1 {
    /// (A)-1 registration predicate (§13/§14) — may a NEW provider registration referencing this set be
    /// admitted at `daa_score`, given the currently measured bit-exact auditor capacity for this set?
    /// Requires: past activation, not deprecated (non-retroactive), and the auditor-capacity gate met. Pure.
    #[inline]
    pub fn registerable(&self, daa_score: u64, measured_auditor_capacity: u32) -> bool {
        daa_score >= self.activation_daa_score
            && self.deprecated_from_daa.is_none_or(|d| daa_score < d)
            && measured_auditor_capacity >= self.auditor_capacity_threshold
    }
}

/// §13 — two providers may form a k=2 pair iff they reference the SAME conformance vector set (same
/// determinism class). Distinct sets NEVER pair, even within one tier during a rolling migration — that is
/// what keeps soundness invariant while a new set coexists with the old.
#[inline]
pub fn palw_providers_can_pair(a_set_id: &Hash64, b_set_id: &Hash64) -> bool {
    a_set_id == b_set_id
}

/// §13/§15 — tier activation is DERIVED, never a flag: a tier (`model_profile_id`) is live at `daa_score`
/// iff at least one committed set referencing it is `registerable` there. `capacity(set_id)` supplies the
/// measured bit-exact auditor capacity per set. QW9 is the only tier with an active set at genesis; QW4
/// with no active set is naturally non-live (re-enable = commit a QW4 set — a data commit, not a fork).
pub fn palw_tier_is_live(
    sets: &[PalwConformanceVectorSetV1],
    model_profile_id: &Hash64,
    daa_score: u64,
    capacity: impl Fn(&Hash64) -> u32,
) -> bool {
    sets.iter().any(|s| &s.model_profile_id == model_profile_id && s.registerable(daa_score, capacity(&s.set_id)))
}

// =============================================================================================
// Canonical Compute v1 §17 — MODEL-as-data. The set record is the SOLE surface through which a model /
// MoE / VLM / trace-restructure change reaches the chain; consensus sees only this record + opaque hashes,
// so a new-model migration is a DATA commit, not a fork (docs/design/misaka-canonical-compute-v1.md §17).
// Pure types + predicates only — a consensus island with zero live callers, no serialized instance on any
// preset/header/store/leaf, so byte-identical live.
// =============================================================================================

/// §17.3 — the integer-only economic VALUES a set record carries; the FORMULAS and FLOORS stay in protocol
/// (§17.5 defense 2). Integer-only by design — a float on any consensus-adjacent path is a determinism
/// hazard.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwSetEconParamsV1 {
    /// Per-leaf minimum provider bond (VALUE; the floor is a protocol check, [`PalwComputeSetRecordV1::registerable`]).
    pub min_leaf_bond_sompi: u64,
    /// Job timeout in DAA (VALUE).
    pub job_timeout_daa: u64,
}

pub const PALW_WEIGHT_FACTOR_BPS_MAX: u16 = 10_000;

/// §17.3 — the model-as-data migration record. Extends the §13 set by *referencing* an immutable `set_id`
/// (the content-id / k=2 pairing key) and binding a compute-spec version, an OPAQUE model manifest, the
/// class predicate (`vector_commitment`), integer econ VALUES, and a governed credit ramp to it. A new
/// model / MoE / VLM is a new record; consensus never parses the model, only these fields + opaque hashes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetRecordV1 {
    pub version: u16,
    /// The determinism class this record governs — the immutable content-id / pairing key (§13), NOT
    /// redefined here.
    pub set_id: Hash64,
    /// The op-catalog version this set's providers/auditors run (§17.4); a bump is a spec release + software
    /// rollout, not a consensus rule change.
    pub compute_spec_version: u32,
    /// OPAQUE — weights / tokenizer / quant / shape live in DA; consensus never parses it.
    pub model_manifest_hash: Hash64,
    /// The committed golden vector set = the class predicate (§11/§13); providers reproduce it byte-exact.
    pub vector_commitment: Hash64,
    /// Attested K0 benchmark commitment backing `compute_work_scale` (§17.5 defense 3). Opaque.
    pub quantum_calibration_evidence: Hash64,
    /// The integer quantum→work scale VALUE (the FORMULA `normalize_palw_work` + the cap `E=H+min(C,4H)`
    /// stay protocol). Credited RAMPED via [`Self::effective_compute_work_scale`].
    pub compute_work_scale: u64,
    /// Integer econ VALUES (formulas + floors are protocol).
    pub econ_params: PalwSetEconParamsV1,
    /// Per-set credit ramp in basis points (0..=10000), governed + rate-limited (§17.5 defense 3). A shadow
    /// set commits at 0 and ramps up.
    pub weight_factor_bps: u16,
    /// Non-retroactive lifecycle (§13, frozen): referenceable for NEW registrations in
    /// `[activation_daa_score, deprecated_from_daa)`.
    pub activation_daa_score: u64,
    pub deprecated_from_daa: Option<u64>,
    /// The per-set auditor-capacity gate (§13): may not certify below this bit-exact-reproducing capacity.
    pub auditor_capacity_threshold: u32,
    /// Off-chain evidence backing the measured auditor capacity (opaque).
    pub auditor_capacity_evidence_hash: Hash64,
}

impl PalwComputeSetRecordV1 {
    /// The compute-work scale actually credited = `compute_work_scale · weight_factor_bps / 10000` (integer
    /// floor division, saturating). A shadow set (bps 0) credits ZERO DAG weight — it serves MIL fee
    /// traffic + accumulates canary stats without weight (§17.4).
    #[inline]
    pub fn effective_compute_work_scale(&self) -> u64 {
        (self.compute_work_scale as u128 * self.weight_factor_bps.min(PALW_WEIGHT_FACTOR_BPS_MAX) as u128
            / PALW_WEIGHT_FACTOR_BPS_MAX as u128) as u64
    }

    /// (A)-1 registration predicate, record form (§17.5 fix 4): past activation, not deprecated
    /// (non-retroactive), the auditor-capacity gate met, `weight_factor_bps` in range, and the econ VALUES
    /// respect the protocol floor. Pure.
    pub fn registerable(&self, daa_score: u64, measured_auditor_capacity: u32, econ_floor: &PalwSetEconParamsV1) -> bool {
        daa_score >= self.activation_daa_score
            && self.deprecated_from_daa.is_none_or(|d| daa_score < d)
            && measured_auditor_capacity >= self.auditor_capacity_threshold
            && self.weight_factor_bps <= PALW_WEIGHT_FACTOR_BPS_MAX
            && self.econ_params.min_leaf_bond_sompi >= econ_floor.min_leaf_bond_sompi
            && self.econ_params.job_timeout_daa >= econ_floor.job_timeout_daa
    }
}

/// §17.5 fix 4 — registration = a valid reference to an ACTIVE set record: resolve `set_id` to a
/// registerable record. The record-based analogue of [`PalwConformanceVectorSetV1::registerable`]. Pure.
pub fn palw_registration_references_active_set(
    records: &[PalwComputeSetRecordV1],
    set_id: &Hash64,
    daa_score: u64,
    measured_auditor_capacity: u32,
    econ_floor: &PalwSetEconParamsV1,
) -> bool {
    records.iter().any(|r| &r.set_id == set_id && r.registerable(daa_score, measured_auditor_capacity, econ_floor))
}

/// kaspa-pq **ADR-0040 §OWN / SC-08 — the outcome of resolving a compute set against the registry.**
///
/// Three cases, deliberately distinguished. Collapsing them into "scale or fallback" is what made
/// "demote a set to weight 0" inexpressible: an *omitted* set resolved to the protocol default scale,
/// i.e. an unregistered set was credited exactly like a registered one. For a registry, unregistered
/// must mean **no credit**, never **default credit**.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSetResolution {
    /// A record governs this set at this DAA score; carries its ramped scale (which may itself be 0 for
    /// a shadow set at `weight_factor_bps = 0` — a *deliberate* zero, distinct from the two below).
    Active { compute_work_scale: u64 },
    /// A record exists but does not govern here — before `activation_daa_score`, or at/after
    /// `deprecated_from_daa`. Credit is zero, and the caller can say WHY.
    NotGoverning,
    /// No record at all. Credit is zero — fail-closed.
    Unregistered,
}

impl PalwSetResolution {
    /// The credited scale. Both non-active cases are zero; only a governing record can credit.
    pub fn compute_work_scale(self) -> u64 {
        match self {
            Self::Active { compute_work_scale } => compute_work_scale,
            Self::NotGoverning | Self::Unregistered => 0,
        }
    }

    /// Is this set creditable at all right now?
    pub fn is_creditable(self) -> bool {
        matches!(self, Self::Active { .. })
    }
}

/// kaspa-pq **ADR-0040 §OWN — the class registry lookup.**
///
/// Under the single-pool decision (§1) the consensus-binding identity is `compute_set_id`, NOT
/// `runtime_class_id`. The latter is `implementation_id` — non-consensus telemetry — so it must never
/// appear in a credit decision. This function is the only place a set becomes creditable.
///
/// Fail-closed by construction: an unknown or lapsed set yields zero, not a default.
pub fn resolve_compute_set(records: &[PalwComputeSetRecordV1], set_id: &Hash64, daa_score: u64) -> PalwSetResolution {
    let Some(r) = records.iter().find(|r| &r.set_id == set_id) else {
        return PalwSetResolution::Unregistered;
    };
    if daa_score < r.activation_daa_score || r.deprecated_from_daa.is_some_and(|d| daa_score >= d) {
        return PalwSetResolution::NotGoverning;
    }
    PalwSetResolution::Active { compute_work_scale: r.effective_compute_work_scale() }
}

/// §17.5 fix 2 — **LEGACY** scale resolution, kept only for the pre-registry activation seam.
///
/// Prefer [`resolve_compute_set`]. This variant credits `fallback` when no record governs, which is
/// exactly the SC-08 hazard: an unregistered set is credited like a registered one, so "demote to
/// weight 0 by removing the record" silently becomes "credit at the protocol default". Any new caller
/// must use the registry lookup instead.
#[deprecated(note = "ADR-0040 SC-08: use `resolve_compute_set`, which is fail-closed for unregistered sets")]
pub fn resolve_compute_work_scale(records: &[PalwComputeSetRecordV1], set_id: &Hash64, daa_score: u64, fallback: u64) -> u64 {
    records
        .iter()
        .find(|r| &r.set_id == set_id && daa_score >= r.activation_daa_score && r.deprecated_from_daa.is_none_or(|d| daa_score < d))
        .map(|r| r.effective_compute_work_scale())
        .unwrap_or(fallback)
}

/// §17.5 defense 3 — a governed `weight_factor_bps` change is valid only if it moves by at most
/// `max_step_bps` per commit (rate-limit) and stays in `[0, 10000]`, so a captured governance cannot jump a
/// shadow set straight to full DAG weight. Pure; enforced by protocol.
pub fn palw_weight_ramp_step_valid(prev_bps: u16, next_bps: u16, max_step_bps: u16) -> bool {
    next_bps <= PALW_WEIGHT_FACTOR_BPS_MAX && prev_bps.abs_diff(next_bps) <= max_step_bps
}

// =============================================================================================
// Consensus parameters (design §24.5, §26). Testnet defaults; hard-fork-only knobs.
// =============================================================================================

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwParams {
    /// DAA score at which the PALW lane activates. `u64::MAX` = **inert** (never active).
    pub activation_daa_score: u64,
    pub total_bps: u64,
    pub hash_lane_bps: u64,
    pub replica_lane_bps: u64,
    /// `E = H + min(C, compute_to_hash_cap · H)` (design §5.3, invariant I-1).
    pub compute_to_hash_cap: u64,
    pub epoch_length_daa: u64,
    pub registration_lead_epochs: u64,
    pub audit_window_epochs: u64,
    pub active_window_epochs: u64,
    pub nullifier_retention_daa: u64,
    pub evidence_window_epochs: u64,
    /// ADR-0040 §5.14.7.5 (item 6) — PCPB windows, in epochs. **INERT**: no production reader yet (the
    /// wiring slice consumes them via `palw_challenge_fresh` + the snapshot/post-commit resolvers); their
    /// invariants are enforced now by [`Self::is_structurally_valid`]. Re-genesis knobs. `w` = challenge
    /// freshness window; `k` = snapshot lag (the bond-weighted snapshot is fixed at `registered − k`, before
    /// A_commit); `Δ` = post-commit offset (B is drawn from `R_{registered+Δ}`, after A's commit, so A
    /// cannot pre-pick a sybil B). Timeline: `challenge ≤ registered−k < registered ≤ registered+Δ`.
    pub freshness_window_epochs: u64,
    pub snapshot_lag_epochs: u64,
    pub post_commit_delta_epochs: u64,
    pub max_batch_leaves: u32,
    pub max_leaf_chunk_leaves: u16,
    pub min_leaf_bond_sompi: u64,
    pub canary_sample_bps: u16,
    pub min_canaries_per_batch: u16,
    pub auditor_count: u16,
    pub auditor_quorum_num: u16,
    pub auditor_quorum_den: u16,
    pub dns_degraded_grace_epochs: u64,
    pub supported_profiles: Vec<Hash64>,
}

impl PalwParams {
    /// The design §26 testnet start values, at the committed **10-BPS PALW genesis** (hash 2 + replica
    /// 8, cap 4 = a permanent 20 % hash floor; 100 ms block interval; epoch 100 DAA ≈ 10 s at 10 BPS;
    /// nullifier retention 1 200 DAA ≈ 120 s). **Inert by construction** (`activation_daa_score =
    /// u64::MAX`) — flipping a real activation score is a re-genesis / hard-fork decision, not a
    /// default.
    ///
    /// RATIONALE (ADR-0039 §"10 vs 40 BPS", decided 2026-07-14): PALW launches on a dedicated 10-BPS
    /// network so the new consensus hot-path (component work, nullifier dedup, lane DAA, overlay
    /// lookups, ML-DSA authorization) has validation-time headroom (100 ms interval vs 25 ms) and
    /// gentler GHOSTDAG pressure (K≈124 / mergeset 248 vs K≈447 / 512) for its first production run.
    /// The lane proportion (1 : 4) and hash-floor fraction (20 %) are IDENTICAL at 10 BPS (2 + 8), so
    /// the coinbase split and all epoch-denominated windows are unchanged — only `total_bps`, the two
    /// lane rates, and the wall-clock-preserving `*_daa` windows scale by 1/4. PALW's LLM throughput is
    /// asynchronous (GPUs fill a ticket inventory; blocks only draw from it), so 10 BPS does NOT
    /// throttle inference. The 40-BPS split (hash 8 + replica 32, epoch 400 / retention 4 800) is
    /// retained as the later `testnet-palw-40` Stage-B stressnet profile, promoted only after the
    /// 10-BPS soak + weight ladder gates.
    pub fn testnet_inert_default() -> Self {
        Self {
            activation_daa_score: u64::MAX,
            total_bps: 10,
            hash_lane_bps: 2,
            replica_lane_bps: 8,
            compute_to_hash_cap: COMPUTE_TO_HASH_CAP,
            epoch_length_daa: 100,
            registration_lead_epochs: 2,
            audit_window_epochs: 6,
            active_window_epochs: 6,
            nullifier_retention_daa: 1_200,
            evidence_window_epochs: 60,
            // §5.14.7.5 PCPB windows (inert placeholders satisfying the invariants; k=Δ=2, w=6). A
            // re-genesis sets real values — nothing reads these until the wiring slice.
            freshness_window_epochs: 6,
            snapshot_lag_epochs: 2,
            post_commit_delta_epochs: 2,
            max_batch_leaves: 256,
            max_leaf_chunk_leaves: PALW_MAX_LEAVES_PER_CHUNK as u16,
            min_leaf_bond_sompi: 0,
            canary_sample_bps: 100, // 1 %
            min_canaries_per_batch: 1,
            auditor_count: 16,
            auditor_quorum_num: 2,
            auditor_quorum_den: 3,
            dns_degraded_grace_epochs: 1,
            supported_profiles: Vec::new(),
        }
    }

    /// Never true for the inert default (`activation_daa_score = u64::MAX`).
    #[inline]
    pub fn is_active_at(&self, daa_score: u64) -> bool {
        daa_score >= self.activation_daa_score
    }

    /// Structural invariants the params must satisfy (design §5.2/§5.3/§16.3). Cheap; called at
    /// config-build time.
    pub fn is_structurally_valid(&self) -> bool {
        self.total_bps == self.hash_lane_bps + self.replica_lane_bps
            && self.hash_lane_bps > 0
            && self.replica_lane_bps > 0
            && self.compute_to_hash_cap > 0
            && self.epoch_length_daa > 0
            && self.active_window_epochs > 0
            && self.max_leaf_chunk_leaves as usize <= PALW_MAX_LEAVES_PER_CHUNK
            && self.auditor_quorum_den > 0
            && self.auditor_quorum_num <= self.auditor_quorum_den
            // the hash floor must be a positive fraction that the cap actually binds:
            // replica ≤ cap · hash keeps compute work ≤ cap× hash work at steady state.
            && self.replica_lane_bps <= self.compute_to_hash_cap * self.hash_lane_bps
            // ADR-0040 §5.14.7.5 (item 6) — PCPB window invariants (INERT until wired). `k, Δ ≥ 1` (a lag /
            // offset of 0 collapses the pre-/post-commit ordering); `w ≥ Δ` (freshness must at least cover
            // the post-commit draw); and the span `registered−k … registered+Δ` must fit the retained
            // evidence window so both the snapshot epoch and the post-commit beacon are co-resolvable.
            && self.snapshot_lag_epochs >= 1
            && self.post_commit_delta_epochs >= 1
            && self.freshness_window_epochs >= self.post_commit_delta_epochs
            && self.snapshot_lag_epochs + self.post_commit_delta_epochs <= self.evidence_window_epochs
    }
}

// =============================================================================================
// Lane-aware DAA (design §16). The two lanes retarget difficulty INDEPENDENTLY so ticket supply and
// hash rate cannot manipulate each other's difficulty (§16.1); `daa_score` stays the total DAG
// progression. Params + pure per-lane target-time here; the lane-aware `WindowManager` sampling
// (§16.2, only credited-unique-blue sources per lane) and the retarget itself are the activation
// wiring in the `consensus` engine. Inert by construction (no algo-4 header is minted / sampled).
// =============================================================================================

/// Per-second-milliseconds target for one lane: `1000 / lane_bps`, rounded to nearest (min 1). At the
/// frozen 40 BPS split this is 125 ms for the hash lane (8 BPS) and 31 ms for the replica lane (32 BPS,
/// = 31.25 rounded). `lane_bps` must be > 0.
#[inline]
pub fn lane_target_time_ms(lane_bps: u64) -> u64 {
    let bps = lane_bps.max(1);
    ((1000 + bps / 2) / bps).max(1)
}

/// ADR-0039 §16.3 — the per-lane difficulty bits carried by each block (the HOLD sources for the
/// per-lane retarget). Both lanes are carried on EVERY block because the structural blocker is
/// symmetric: a block's selected parent may be on the OTHER lane, so `header.bits` alone cannot supply
/// a lane's "last bits" (that would cross-contaminate). Fixed-width (two `u32`) → borsh + serde +
/// `MemSizeEstimator` storable. **Never written on any preset — but because `DbPalwLaneBitsStore` has
/// no producer wired yet, not because of the PALW fence** (`testnet-palw-110` / `devnet-palw-111` ship
/// `palw_activation_daa_score = 0`). Wiring that writer is an open clause-7 activation blocker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwLaneBitsV1 {
    pub hash_bits: u32,
    pub replica_bits: u32,
}

impl PalwLaneBitsV1 {
    pub fn lane_bits(&self, lane: WorkLane) -> u32 {
        match lane {
            WorkLane::HashFloor => self.hash_bits,
            WorkLane::ReplicaPalw => self.replica_bits,
        }
    }

    pub fn with_lane_bits(mut self, lane: WorkLane, bits: u32) -> Self {
        match lane {
            WorkLane::HashFloor => self.hash_bits = bits,
            WorkLane::ReplicaPalw => self.replica_bits = bits,
        }
        self
    }
}

/// ADR-0039 §16.3 lane-difficulty parameters. Each lane keeps its own retarget window and genesis
/// difficulty; a lane with too few samples holds its last bits rather than collapsing to min difficulty
/// (§16.3). All hard-fork / re-genesis knobs.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LaneDifficultyParams {
    pub hash_target_time_ms: u64,
    pub replica_target_time_ms: u64,
    pub hash_window_size: u64,
    pub replica_window_size: u64,
    /// Below this many in-lane samples, hold the last lane bits (no min-difficulty collapse).
    pub min_samples: u64,
    /// Fixed per-leaf compute-work scale (§5.3 `normalize_palw_work`): `ΔC = scale · calc_work(bits)`.
    /// `1` = one leaf is worth exactly its eligibility-target hash-equivalent work (same unit as the
    /// hash lane, never mixed with `calc_work_512`; see `pow_layer0::calc_work_512` audit-L note).
    pub compute_work_scale: u64,
    /// Per-retarget-step clamp: the measured duration ratio is bounded to `[1/f, f]` so a sparse lane
    /// (e.g. a few-GPU launch reaching `min_samples` at ~10× wall-clock) cannot collapse difficulty in
    /// one step (panel FS-6). `lane_retarget_decision`'s `max_adjust_factor`; must be `>= 1`.
    pub max_adjust_factor: u64,
    /// Per-lane sampling rate. The lane window's sample index increments over the LANE-FILTERED
    /// sequence (only in-lane credited blocks), so this is independent of the total-DAG `difficulty_
    /// sample_rate` (panel FS-5). Must be `>= 1`.
    pub hash_sample_rate: u64,
    pub replica_sample_rate: u64,
    /// Genesis difficulty bits per lane (set at re-genesis; inert placeholder `0`).
    pub genesis_hash_bits: u32,
    pub genesis_replica_bits: u32,
}

impl LaneDifficultyParams {
    /// The design §16.3 testnet defaults at the committed **10 BPS** PALW genesis (hash 2 / replica 8)
    /// split, as a `const` so it can live in the `const Params` presets. Windows mirror the single-lane
    /// difficulty window; the genesis bits are re-genesis placeholders (`0` = inert). Inert — nothing
    /// retargets the replica lane until the PALW fence.
    pub const INERT: LaneDifficultyParams = LaneDifficultyParams {
        hash_target_time_ms: 500,    // lane_target_time_ms(2) = 500 ms (2 BPS)
        replica_target_time_ms: 125, // lane_target_time_ms(8) = 125 ms (8 BPS)
        hash_window_size: 2641,
        replica_window_size: 2641,
        min_samples: 60,
        compute_work_scale: 1,
        max_adjust_factor: 2,
        hash_sample_rate: 1,
        replica_sample_rate: 1,
        genesis_hash_bits: 0,
        genesis_replica_bits: 0,
    };

    pub fn testnet_default() -> Self {
        Self::INERT
    }

    /// Structural sanity (positive windows / targets / scale / rates / clamp). Cheap, config-build time.
    pub fn is_structurally_valid(&self) -> bool {
        self.hash_target_time_ms > 0
            && self.replica_target_time_ms > 0
            && self.hash_window_size > 0
            && self.replica_window_size > 0
            && self.compute_work_scale > 0
            && self.max_adjust_factor >= 1
            && self.hash_sample_rate >= 1
            && self.replica_sample_rate >= 1
    }

    /// The genesis bits per lane (the empty-window HOLD source, panel Q6).
    pub fn genesis_bits(&self, lane: WorkLane) -> u32 {
        match lane {
            WorkLane::HashFloor => self.genesis_hash_bits,
            WorkLane::ReplicaPalw => self.genesis_replica_bits,
        }
    }

    /// The lane's window size / min-samples / sample-rate / target-time (per-lane retarget inputs).
    pub fn lane_window_size(&self, lane: WorkLane) -> u64 {
        match lane {
            WorkLane::HashFloor => self.hash_window_size,
            WorkLane::ReplicaPalw => self.replica_window_size,
        }
    }
    pub fn lane_sample_rate(&self, lane: WorkLane) -> u64 {
        match lane {
            WorkLane::HashFloor => self.hash_sample_rate.max(1),
            WorkLane::ReplicaPalw => self.replica_sample_rate.max(1),
        }
    }
    pub fn lane_target_time_ms(&self, lane: WorkLane) -> u64 {
        match lane {
            WorkLane::HashFloor => self.hash_target_time_ms,
            WorkLane::ReplicaPalw => self.replica_target_time_ms,
        }
    }

    /// ADR-0039 §16.3 — the PALW **re-genesis preflight** consistency predicate for the lane-difficulty
    /// params against the genesis header (panel-required). Asserts: structural sanity; the genesis lane
    /// bits are non-zero (`0` is the inert placeholder — an active net MUST set real bits) and the hash
    /// lane's genesis bits EQUAL the genesis header's bits (`genesis_bits`), so the inert single-lane
    /// HOLD (`get_bits(genesis)`) and the active lane-aware HOLD agree at the boundary; `min_samples`
    /// does not exceed either window. NOT evaluated on live nets (inert placeholders would fail it).
    pub fn is_consistent_for_activation(&self, genesis_bits: u32) -> bool {
        self.is_structurally_valid()
            && self.genesis_hash_bits != 0
            && self.genesis_replica_bits != 0
            && self.genesis_hash_bits == genesis_bits
            && self.min_samples <= self.hash_window_size
            && self.min_samples <= self.replica_window_size
            // ADR-0040 P1-11 (AO-03): `lane_expected_bits` / `lane_retarget_bits` carry an UNPROVEN
            // `min_samples >= 1` precondition — at zero they unwrap an empty window and divide a Uint320
            // by zero. Unreachable at the shipped `min_samples = 60`, but the precondition was only ever
            // documented, and a documented precondition is one a future params edit can break silently.
            // Making it part of validity is what turns it into a real precondition.
            && self.min_samples >= 1
    }
}

/// ADR-0039 §16.2 — whether a mergeset source block is sampled into ITS lane's DAA retarget window.
/// Only credited-blue sources count: the hash lane samples algo-3 blues; the compute lane samples
/// **unique, Active, credited** algo-4 blues. Red / duplicate / revoked / zero-headroom PALW blocks
/// carry no lane work and are excluded, and a block never samples the other lane's window (so ticket
/// supply and hash rate cannot manipulate each other's difficulty, §16.1). `daa_score` itself stays
/// total DAG progression, not per-lane. Pure.
pub fn lane_daa_sample_eligible(sample_lane: WorkLane, block_algo_id: u8, credited_blue: bool, unique_active: bool) -> bool {
    if !credited_blue {
        return false;
    }
    match sample_lane {
        WorkLane::HashFloor => block_algo_id == POW_ALGO_ID_BLAKE2B_SHA3,
        WorkLane::ReplicaPalw => block_algo_id == POW_ALGO_ID_PALW_REPLICA && unique_active,
    }
}

/// The per-lane retarget action (design §16.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneRetargetDecision {
    /// Below `min_samples` in-lane samples: keep the current lane bits unchanged — no sudden collapse
    /// to min difficulty on a thin window (§16.3).
    HoldLastBits,
    /// Retarget: the measured mean interval, clamped to `[target/max_factor, target·max_factor]`, that
    /// the difficulty engine multiplies the current target by (`new_target = current · clamped /
    /// target`; faster-than-target ⇒ smaller target ⇒ higher difficulty).
    Adjust { clamped_measured_ms: u64 },
}

/// ADR-0039 §16.3 — decide the lane retarget from the in-lane sample count and the measured mean
/// interval. Holds the last bits below `min_samples`; otherwise clamps the measured interval to
/// `[target/max_adjust_factor, target·max_adjust_factor]` so one burst / stall cannot swing difficulty
/// arbitrarily. Pure; the big-int `current·clamped/target` step is the difficulty engine's, reused
/// per-lane. `target_ms` / `max_adjust_factor` are treated as ≥ 1.
pub fn lane_retarget_decision(
    sample_count: u64,
    min_samples: u64,
    measured_ms: u64,
    target_ms: u64,
    max_adjust_factor: u64,
) -> LaneRetargetDecision {
    if sample_count < min_samples {
        return LaneRetargetDecision::HoldLastBits;
    }
    let f = max_adjust_factor.max(1);
    let target = target_ms.max(1);
    let lo = (target / f).max(1);
    let hi = target.saturating_mul(f);
    LaneRetargetDecision::Adjust { clamped_measured_ms: measured_ms.clamp(lo, hi) }
}

// §18.1 — unit-estimable `MemSizeEstimator` for the PALW overlay-store value types (the store uses a
// `Count` cache policy, no byte estimation), so `DbPalwStore` can cache them like the other per-key
// stores.
impl kaspa_utils::mem_size::MemSizeEstimator for PalwPublicLeafV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwSnapshotCommitment {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwBatchManifestV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwBatchCertificateV2 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwProviderBondPayloadV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwBatchStatus {}
// Beacon: the per-epoch derived state (block-keyed) + the per-epoch commit/reveal accumulator (the
// stored commitments are `Hash64`; the raw reveal is never persisted). Empty estimators — `Count`-cached
// like the batch overlay values.
impl kaspa_utils::mem_size::MemSizeEstimator for PalwBeaconStateV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwPcpbChainStateV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwBeaconEpochAccumV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwLaneBitsV1 {}

// =============================================================================================
// Tests — freeze the wire format + hash test vectors (design §33).
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::ZERO_HASH64;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    #[test]
    fn versioned_palw_defaults_are_canonical_v1() {
        let overlay = PalwBatchViewV1::default();
        assert_eq!(overlay, PalwBatchViewV1::new());
        assert_eq!(overlay.version, 1);

        let beacon = PalwBeaconEpochAccumV1::default();
        assert_eq!(beacon, PalwBeaconEpochAccumV1::new());
        assert_eq!(beacon.version, 1);
    }

    #[test]
    fn palw_proof_type_is_all_open_no_tee_no_hiding() {
        // ADR-0039 (2026-07-19): only the OPEN proof types survive.
        assert_eq!(PalwProofType::from_u8(1), Some(PalwProofType::ReplicaExactV1));
        assert_eq!(PalwProofType::from_u8(3), Some(PalwProofType::TransparentArgumentV1));
        // TEE (2) and witness-hiding (4) are REMOVED — a leaf carrying them is rejected.
        assert_eq!(PalwProofType::from_u8(2), None, "TEE proof type is removed (all-open PALW)");
        assert_eq!(PalwProofType::from_u8(4), None, "witness-hiding proof type is removed (all-open PALW)");
        assert_eq!(PalwProofType::ReplicaExactV1.as_u8(), 1);
        assert_eq!(PalwProofType::TransparentArgumentV1.as_u8(), 3);
    }

    #[test]
    fn palw_challenge_freshness_window() {
        // D3-b clause 11 (pure half): issued ≤ anchor ∧ anchor+Δ ≤ registered ∧ registered−issued ≤ w.
        // Each bound shown load-bearing at its exact boundary.
        assert!(palw_challenge_fresh(10, 10, 12, 4, 2), "anchor+Δ == registered exactly ⇒ fresh");
        assert!(palw_challenge_fresh(7, 8, 10, 4, 2), "anchor between issued and registered−Δ ⇒ fresh");
        assert!(palw_challenge_fresh(6, 6, 10, 4, 2), "registered−issued == w exactly ⇒ fresh");
        assert!(!palw_challenge_fresh(5, 6, 10, 4, 2), "registered−issued == w+1 ⇒ stale (replay blocked)");
        assert!(!palw_challenge_fresh(11, 11, 10, 4, 2), "challenge from the future ⇒ rejected");
        assert!(!palw_challenge_fresh(10, 9, 12, 4, 2), "anchor before issued ⇒ rejected (self anchor cannot pre-date its challenge)");
        assert!(
            !palw_challenge_fresh(10, 10, 11, 4, 2),
            "anchor+Δ > registered ⇒ rejected — the draw beacon would not post-date the anchor (grindable B selection)"
        );
        assert!(!palw_challenge_fresh(10, u64::MAX, u64::MAX, 4, 2), "anchor+Δ overflow ⇒ rejected, never wrapped");
    }
    // The declared-`bool` `palw_dispatch_proof_valid` test suite was removed with the type itself
    // (D3-b): its acceptance surface is now `pcpb_evidence_tests`, which proves each branch by
    // RE-DERIVATION against real ML-DSA-87 / Merkle / weighted-draw material instead.
    fn op(b: u8, i: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(h(b), i)
    }

    /// ADR-0040 P0-4: a leaf reward script must be payable as a coinbase output, i.e. the exact 69-byte
    /// ML-DSA-87 P2PKH template, so leaf fixtures use this helper.
    fn reward_spk(b: u8) -> ScriptPublicKey {
        crate::dns_finality::p2pkh_mldsa87_spk(&[b; 64])
    }

    // ---- Canonical Compute v1 §13: determinism class as data ----
    fn cvset(set: u8, tier: u8, activation: u64, deprecated: Option<u64>, cap_threshold: u32) -> PalwConformanceVectorSetV1 {
        PalwConformanceVectorSetV1 {
            version: 1,
            set_id: h(set),
            model_profile_id: h(tier),
            activation_daa_score: activation,
            deprecated_from_daa: deprecated,
            auditor_capacity_threshold: cap_threshold,
        }
    }

    #[test]
    fn conformance_set_registerable_window_and_gates() {
        let s = cvset(0x51, 0x99, 100, Some(500), 3);
        // Before activation: not registerable regardless of capacity.
        assert!(!s.registerable(99, 100));
        // In window + capacity met: registerable.
        assert!(s.registerable(100, 3));
        assert!(s.registerable(499, 3));
        // Capacity gate: below threshold ⇒ refused (canary verifiability is a per-set precondition).
        assert!(!s.registerable(200, 2));
        // Non-retroactive deprecation: at/after `deprecated_from_daa`, only NEW registration is refused.
        assert!(!s.registerable(500, 100));
        assert!(!s.registerable(10_000, 100));
        // A never-deprecated set has no upper bound.
        assert!(cvset(0x51, 0x99, 0, None, 0).registerable(u64::MAX, 0));
    }

    #[test]
    fn palw_pairing_is_same_set_only() {
        // Same set id pairs; distinct sets never pair — even within one tier during a rolling migration.
        assert!(palw_providers_can_pair(&h(0x51), &h(0x51)));
        assert!(!palw_providers_can_pair(&h(0x51), &h(0x52)));
    }

    #[test]
    fn tier_activation_is_derived_from_an_active_set() {
        let qw9 = h(0x99);
        let qw4 = h(0x44);
        // Genesis: one active QW9 set, no QW4 set.
        let sets = vec![cvset(0x51, 0x99, 0, None, 1)];
        let cap = |_id: &Hash64| 4u32; // ample auditor capacity
        // QW9 is live (an active set references it); QW4 is naturally non-live (no set) — no flag needed.
        assert!(palw_tier_is_live(&sets, &qw9, 0, cap));
        assert!(!palw_tier_is_live(&sets, &qw4, 0, cap));

        // Rolling migration: a second QW9 set (new manifest/driver) coexists; the tier stays live and
        // providers pair only within their own set (checked by palw_providers_can_pair).
        let sets = vec![cvset(0x51, 0x99, 0, Some(1_000), 1), cvset(0x52, 0x99, 900, None, 1)];
        assert!(palw_tier_is_live(&sets, &qw9, 950, cap)); // old (in window) + new both referenceable
        assert!(palw_tier_is_live(&sets, &qw9, 2_000, cap)); // old deprecated, new still live ⇒ tier live
        // If the only set's auditor capacity is below threshold, the tier is NOT live (gate bites).
        let starved = vec![cvset(0x53, 0x99, 0, None, 5)];
        assert!(!palw_tier_is_live(&starved, &qw9, 0, |_| 4));
    }

    // ---- Canonical Compute v1 §17: model-as-data record ----
    fn econ(bond: u64, timeout: u64) -> PalwSetEconParamsV1 {
        PalwSetEconParamsV1 { min_leaf_bond_sompi: bond, job_timeout_daa: timeout }
    }
    fn record(
        set: u8,
        activation: u64,
        deprecated: Option<u64>,
        cap_threshold: u32,
        scale: u64,
        weight_bps: u16,
        bond: u64,
    ) -> PalwComputeSetRecordV1 {
        PalwComputeSetRecordV1 {
            version: 1,
            set_id: h(set),
            compute_spec_version: 1,
            model_manifest_hash: h(0xA0),
            vector_commitment: h(0xB0),
            quantum_calibration_evidence: h(0xC0),
            compute_work_scale: scale,
            econ_params: econ(bond, 100),
            weight_factor_bps: weight_bps,
            activation_daa_score: activation,
            deprecated_from_daa: deprecated,
            auditor_capacity_threshold: cap_threshold,
            auditor_capacity_evidence_hash: h(0xD0),
        }
    }

    #[test]
    fn compute_set_record_ramp_and_registerable() {
        let floor = econ(1_000, 50);
        // weight ramp: effective = scale * bps / 10000 (floor division). Shadow (0 bps) credits 0.
        assert_eq!(record(0x51, 0, None, 1, 8_000, 0, 1_000).effective_compute_work_scale(), 0);
        assert_eq!(record(0x51, 0, None, 1, 8_000, 5_000, 1_000).effective_compute_work_scale(), 4_000);
        assert_eq!(record(0x51, 0, None, 1, 8_000, 10_000, 1_000).effective_compute_work_scale(), 8_000);
        // registerable: gates + the protocol econ FLOOR (a commit under the bond floor is refused).
        let r = record(0x51, 100, Some(500), 3, 8_000, 5_000, 1_000);
        assert!(r.registerable(100, 3, &floor));
        assert!(!r.registerable(99, 3, &floor), "before activation");
        assert!(!r.registerable(500, 3, &floor), "non-retroactive deprecation");
        assert!(!r.registerable(200, 2, &floor), "auditor-capacity gate");
        // Bond below the protocol floor ⇒ refused even though every other gate passes (§17.5 defense 2).
        assert!(!record(0x51, 0, None, 0, 8_000, 5_000, 999).registerable(0, 0, &floor));
    }

    /// kaspa-pq **ADR-0040 §OWN / SC-08** — the registry is fail-closed, and "weight 0" is expressible.
    ///
    /// The bug this pins: the legacy resolver returned `fallback` for an UNREGISTERED set, so removing a
    /// record — the obvious way to demote a set — silently credited it at the protocol default instead
    /// of zero. A registry whose "absent" case means "default credit" is not a registry.
    ///
    /// Three outcomes must stay distinguishable, because they mean different things operationally:
    /// deliberate shadow-set zero, lapsed/not-yet-live, and never-registered.
    #[test]
    fn compute_set_registry_is_fail_closed_and_can_express_weight_zero() {
        let live = record(0x51, 100, None, 1, 7_000, 10_000, 0); // ramped to full weight
        let shadow = record(0x52, 100, None, 1, 7_000, 0, 0); // registered, weight_factor_bps = 0
        let lapsed = record(0x53, 100, Some(200), 1, 7_000, 10_000, 0);
        let recs = vec![live.clone(), shadow.clone(), lapsed.clone()];

        // (1) A governing record credits its RAMPED scale.
        let r = resolve_compute_set(&recs, &live.set_id, 150);
        assert!(r.is_creditable());
        assert_eq!(r.compute_work_scale(), live.effective_compute_work_scale());
        assert!(r.compute_work_scale() > 0);

        // (2) A registered SHADOW set is creditable-but-zero — a DELIBERATE zero. This is the case
        //     SC-08 said was inexpressible; it must be distinguishable from "absent".
        let r = resolve_compute_set(&recs, &shadow.set_id, 150);
        assert!(r.is_creditable(), "a shadow set IS governed — it simply credits nothing yet");
        assert_eq!(r.compute_work_scale(), 0);

        // (3) Not yet active / already deprecated ⇒ zero, and the caller can say why.
        assert_eq!(resolve_compute_set(&recs, &live.set_id, 99), PalwSetResolution::NotGoverning);
        assert_eq!(resolve_compute_set(&recs, &lapsed.set_id, 200), PalwSetResolution::NotGoverning);
        assert_eq!(resolve_compute_set(&recs, &lapsed.set_id, 150).compute_work_scale(), lapsed.effective_compute_work_scale());

        // (4) An unregistered set resolves to zero without applying a default scale.
        assert_eq!(resolve_compute_set(&recs, &h(0xde), 150), PalwSetResolution::Unregistered);
        assert_eq!(resolve_compute_set(&recs, &h(0xde), 150).compute_work_scale(), 0);
        assert_eq!(resolve_compute_set(&[], &live.set_id, 150), PalwSetResolution::Unregistered);

        // (5) The legacy resolver still exhibits the hazard — pinned so the difference is visible and a
        //     silent revert to it is caught.
        #[allow(deprecated)]
        let legacy = resolve_compute_work_scale(&recs, &h(0xde), 150, 12_345);
        assert_eq!(legacy, 12_345, "the legacy resolver credits an UNREGISTERED set at the fallback — the SC-08 hazard");
        assert_ne!(legacy, resolve_compute_set(&recs, &h(0xde), 150).compute_work_scale());
    }

    #[allow(deprecated)]
    #[test]
    fn resolve_compute_work_scale_matches_active_record_else_fallback() {
        let recs = vec![record(0x51, 0, Some(1_000), 1, 8_000, 5_000, 1_000)]; // ramped → 4_000
        // Matching active set ⇒ the record's RAMPED scale; unknown set or no record ⇒ the protocol fallback.
        assert_eq!(resolve_compute_work_scale(&recs, &h(0x51), 500, 42), 4_000);
        assert_eq!(resolve_compute_work_scale(&recs, &h(0x99), 500, 42), 42, "unknown set ⇒ fallback");
        assert_eq!(resolve_compute_work_scale(&recs, &h(0x51), 2_000, 42), 42, "deprecated ⇒ fallback");
        assert_eq!(resolve_compute_work_scale(&[], &h(0x51), 500, 42), 42, "no records ⇒ fallback (today's flat scale)");
    }

    #[test]
    fn registration_references_active_set_and_ramp_step_bounded() {
        let floor = econ(1_000, 50);
        let recs = vec![record(0x51, 100, None, 1, 8_000, 5_000, 1_000)];
        assert!(palw_registration_references_active_set(&recs, &h(0x51), 100, 1, &floor));
        assert!(!palw_registration_references_active_set(&recs, &h(0x52), 100, 1, &floor), "no such set");
        assert!(!palw_registration_references_active_set(&recs, &h(0x51), 99, 1, &floor), "before activation");
        // §17.5 defense 3: a governed weight change is rate-limited and clamped to [0,10000].
        assert!(palw_weight_ramp_step_valid(0, 500, 500));
        assert!(!palw_weight_ramp_step_valid(0, 5_000, 500), "a jump beyond the step is refused (no shadow→full)");
        assert!(!palw_weight_ramp_step_valid(9_800, 10_200, 500), "clamped to 10000");
    }

    #[test]
    fn epoch_proof_bundle_active_set_records_roundtrip() {
        // The I-12 tail field borsh-round-trips (carried-but-unread).
        let bundle = PalwEpochProofBundleV2 {
            version: PALW_EPOCH_PROOF_BUNDLE_VERSION_V2,
            from_epoch: 1,
            to_epoch: 1,
            beacon_chain: vec![],
            batch_manifests: vec![],
            leaf_chunks: vec![],
            certificates: vec![],
            revocations: vec![],
            nullifier_frontier_root: h(0x60),
            active_set_records: vec![record(0x51, 0, None, 1, 8_000, 5_000, 1_000)],
        };
        let bytes = borsh::to_vec(&bundle).unwrap();
        assert_eq!(PalwEpochProofBundleV2::try_from_slice(&bytes).unwrap(), bundle);
    }

    fn sample_leaf() -> PalwPublicLeafV1 {
        PalwPublicLeafV1 {
            version: 1,
            batch_id: h(1),
            leaf_index: 7,
            job_nullifier: h(2),
            ticket_nullifier_commitment: h(3),
            model_profile_id: h(4),
            runtime_class_id: h(5),
            shape_id: 9,
            quantum_count: 3,
            proof_type: PalwProofType::ReplicaExactV1.as_u8(),
            provider_a_bond: op(10, 0),
            provider_b_bond: op(11, 1),
            provider_a_reward_script: reward_spk(0xa0),
            provider_b_reward_script: reward_spk(0xb0),
            ticket_authority_pk_hash: h(6),
            private_match_commitment: h(7),
            receipt_da_object_version: da::PALW_RECEIPT_DA_OBJECT_VERSION_V1,
            receipt_da_root: h(8),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: 100,
            activation_epoch: 102,
            expiry_epoch: 108,
            leaf_bond_sompi: 1_000,
            // External-branch PCPB sentinels (D3-b): a legacy Object-V1 fixture carries the zero
            // anchor; the snapshot roots are opaque non-zero filler (context-free validation does not
            // resolve them — clause 12 does, and this fixture never reaches an acceptance arm).
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: h(0x71),
            assignment_proof_root: h(0x72),
            dispatch_kind: PALW_DISPATCH_KIND_BEACON_ASSIGNED,
        }
    }

    /// D3-b: an arity-filler PCPB witness for chunk fixtures whose test subject is NOT the PCPB
    /// semantics (those live at the acceptance gate and in `pcpb_evidence_tests`). SHAPE-valid only —
    /// it passes `validate_pcpb_witness_shape`'s bounds and nothing more.
    fn dummy_external_witness() -> PalwLeafPcpbWitnessV1 {
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
        PalwLeafPcpbWitnessV1 {
            scheduler_job_id: h(0xe1),
            requester_credential: h(0xe2),
            request_commitment: h(0xe3),
            dispatch: PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof { slot_a: slot(0xe4, 0), slot_b: slot(0xe8, 10) }),
        }
    }

    #[test]
    fn proof_type_roundtrips_and_pins_wire_bytes() {
        // ADR-0039 (2026-07-19): PALW is all-open — only the OPEN discriminants {1, 3} survive.
        for (v, t) in [(1u8, PalwProofType::ReplicaExactV1), (3, PalwProofType::TransparentArgumentV1)] {
            assert_eq!(t.as_u8(), v);
            assert_eq!(PalwProofType::from_u8(v), Some(t));
        }
        // The removed content-hiding proof types are rejected: 2 (TEE) and 4 (witness-hiding), plus 0/5.
        for v in [0u8, 2, 4, 5] {
            assert_eq!(PalwProofType::from_u8(v), None, "discriminant {v} must not be a valid PALW proof type");
        }
    }

    #[test]
    fn leaf_borsh_roundtrip_and_hash_is_deterministic_and_sensitive() {
        let leaf = sample_leaf();
        let bytes = borsh::to_vec(&leaf).unwrap();
        let back = PalwPublicLeafV1::try_from_slice(&bytes).unwrap();
        assert_eq!(leaf, back);
        assert_eq!(leaf.proof_type(), Some(PalwProofType::ReplicaExactV1));

        // hash is deterministic ...
        assert_eq!(leaf.leaf_hash(), sample_leaf().leaf_hash());
        // ... and changes on any field mutation.
        let mut m = sample_leaf();
        m.leaf_index = 8;
        assert_ne!(leaf.leaf_hash(), m.leaf_hash());
        let mut m2 = sample_leaf();
        m2.ticket_nullifier_commitment = h(0x33);
        assert_ne!(leaf.leaf_hash(), m2.leaf_hash());
    }

    #[test]
    fn hash_helpers_are_domain_separated_and_sensitive() {
        // distinct domains → distinct digests over the same-looking inputs.
        let cc = chain_commit(&h(1), &h(2), 5, 7);
        assert_ne!(cc, ZERO_HASH64);

        // chain_commit is sensitive to the target interval (no per-fork re-roll of the same slot).
        assert_ne!(chain_commit(&h(1), &h(2), 5, 7), chain_commit(&h(1), &h(2), 6, 7));

        // eligibility is sensitive to the nullifier and the interval.
        let e1 = eligibility_hash(7, &h(1), &cc, 5, &h(2), 3, &h(4), &h(9));
        let e2 = eligibility_hash(7, &h(1), &cc, 5, &h(2), 3, &h(4), &h(0xA));
        assert_ne!(e1, e2);
    }

    #[test]
    fn beacon_commit_reveal_binds() {
        let bond = op(0x20, 4);
        let r = [0x5Au8; 64];
        let commitment = beacon_commitment(9, &r, &bond);
        let reveal = PalwBeaconRevealV1 { version: 1, epoch: 9, bond_outpoint: bond, random_64: r, signature: vec![] };
        assert!(reveal.matches_commit(&commitment));
        // wrong epoch / wrong randomness / wrong bond all break the binding.
        assert!(!reveal.matches_commit(&beacon_commitment(10, &r, &bond)));
        assert!(!reveal.matches_commit(&beacon_commitment(9, &[0x5Bu8; 64], &bond)));
        assert!(!reveal.matches_commit(&beacon_commitment(9, &r, &op(0x21, 4))));
    }

    #[test]
    fn beacon_signing_hashes_are_operation_network_and_field_bound() {
        let bond = op(0x22, 7);
        let random = [0xA5; 64];
        let mut commit = PalwBeaconCommitV1 {
            version: 1,
            epoch: 12,
            bond_outpoint: bond,
            commitment: beacon_commitment(12, &random, &bond),
            signature: vec![1, 2, 3],
        };
        let reveal = PalwBeaconRevealV1 { version: 1, epoch: 12, bond_outpoint: bond, random_64: random, signature: vec![4, 5, 6] };

        let network = 0x91_00_00_6e;
        let commit_hash = commit.signing_hash(network);
        let reveal_hash = reveal.signing_hash(network);
        assert_ne!(commit_hash, reveal_hash, "commit/reveal signing domains must be disjoint");
        assert_ne!(commit_hash, commit.signing_hash(network ^ 1), "cross-network replay must fail");
        assert_ne!(reveal_hash, reveal.signing_hash(network ^ 1), "cross-network replay must fail");

        // Signature bytes are excluded from their own message, while every semantic field is bound.
        commit.signature = vec![9; 32];
        assert_eq!(commit_hash, commit.signing_hash(network));
        let mut changed = commit.clone();
        changed.epoch += 1;
        assert_ne!(commit_hash, changed.signing_hash(network));
        changed = commit.clone();
        changed.bond_outpoint.index += 1;
        assert_ne!(commit_hash, changed.signing_hash(network));
        changed = commit.clone();
        changed.commitment = h(0x99);
        assert_ne!(commit_hash, changed.signing_hash(network));

        let mut reveal_changed = reveal.clone();
        reveal_changed.random_64[0] ^= 1;
        assert_ne!(reveal_hash, reveal_changed.signing_hash(network));
    }

    #[test]
    fn beacon_phase_helpers_enforce_e_minus_two_and_e_minus_one() {
        assert_eq!(beacon_commit_target_epoch(10), Some(12));
        assert_eq!(beacon_reveal_target_epoch(10), Some(11));
        assert!(beacon_commit_phase_accepts(10, 12));
        assert!(!beacon_commit_phase_accepts(10, 11));
        assert!(!beacon_commit_phase_accepts(10, 13));
        assert!(beacon_reveal_phase_accepts(10, 11));
        assert!(!beacon_reveal_phase_accepts(10, 10));
        assert!(!beacon_reveal_phase_accepts(10, 12));

        let bond = op(0x23, 0);
        let commit = PalwBeaconCommitV1 { version: 1, epoch: 12, bond_outpoint: bond, commitment: h(1), signature: vec![] };
        let reveal = PalwBeaconRevealV1 { version: 1, epoch: 11, bond_outpoint: bond, random_64: [7; 64], signature: vec![] };
        assert!(commit.is_in_phase(10));
        assert!(reveal.is_in_phase(10));

        // Epoch arithmetic never wraps/saturates into an accepted target.
        assert_eq!(beacon_commit_target_epoch(u64::MAX - 1), None);
        assert_eq!(beacon_reveal_target_epoch(u64::MAX), None);
        assert!(!beacon_commit_phase_accepts(u64::MAX - 1, u64::MAX));
        assert!(!beacon_reveal_phase_accepts(u64::MAX, 0));
    }

    #[test]
    fn beacon_valid_reveal_root_commits_opened_entropy_not_public_commitment() {
        let bond = op(0x24, 3);
        let reveal_a = PalwBeaconRevealV1 { version: 1, epoch: 9, bond_outpoint: bond, random_64: [0x11; 64], signature: vec![] };
        let reveal_b = PalwBeaconRevealV1 { random_64: [0x12; 64], ..reveal_a.clone() };
        let entropy_a = reveal_a.entropy_digest();
        let entropy_b = reveal_b.entropy_digest();
        let public_commitment_a = beacon_commitment(reveal_a.epoch, &reveal_a.random_64, &bond);

        assert_ne!(entropy_a, public_commitment_a, "reveal entropy has a distinct domain from the E-2 commitment");
        assert_ne!(entropy_a, entropy_b, "the opened raw random must influence the digest");
        assert_ne!(
            beacon_valid_reveals_root(&[(bond, entropy_a)]),
            beacon_valid_reveals_root(&[(bond, entropy_b)]),
            "different opened secrets must produce different seed roots"
        );
        assert_ne!(
            beacon_valid_reveals_root(&[(bond, entropy_a)]),
            beacon_valid_reveals_root(&[(bond, public_commitment_a)]),
            "the public commitment is not a substitute reveal entropy input"
        );
    }

    #[test]
    fn receipt_hash_vs_signing_hash_differ_and_match_commitment() {
        let base = ReplicaExecutionReceiptV1 {
            version: 1,
            provider_bond: op(0x30, 0),
            session_public_key: vec![1, 2, 3],
            job_nullifier: h(1),
            job_set_commitment: h(2),
            model_profile_id: h(3),
            runtime_class_id: h(4),
            shape_id: 9,
            quantum_count: 3,
            output_commitment: h(5),
            canonical_gemm_trace_root: h(6),
            operation_schedule_commitment: h(7),
            receipt_da_root: h(8),
            completed_at_epoch: 100,
            signature: vec![0xAA; 16],
        };
        // signing_hash excludes the signature; full hash includes it → they differ.
        assert_ne!(base.signing_hash(), base.hash());
        // signing_hash is stable regardless of the signature bytes; full hash is not.
        let mut resigned = base.clone();
        resigned.signature = vec![0xBB; 16];
        assert_eq!(base.signing_hash(), resigned.signing_hash());
        assert_ne!(base.hash(), resigned.hash());

        // private_match_commitment binds both receipt hashes.
        let a = base.hash();
        let b = resigned.hash();
        let cm = private_match_commitment(
            &base.output_commitment,
            &base.canonical_gemm_trace_root,
            &base.operation_schedule_commitment,
            &base.job_set_commitment,
            &a,
            &b,
        );
        assert_ne!(
            cm,
            private_match_commitment(
                &base.output_commitment,
                &base.canonical_gemm_trace_root,
                &base.operation_schedule_commitment,
                &base.job_set_commitment,
                &b,
                &a
            )
        );
    }

    #[test]
    fn chunk_and_certificate_borsh_roundtrip() {
        let leaves = vec![sample_leaf(), sample_leaf()];
        let hashes: Vec<Hash64> = leaves.iter().map(|l| l.leaf_hash()).collect();
        let proofs: Vec<_> = (0..leaves.len() as u32).map(|i| palw_leaf_merkle_proof(&hashes, i).unwrap()).collect();
        let witnesses = vec![dummy_external_witness(), dummy_external_witness()];
        let chunk = PalwLeafChunkV1 { version: PALW_LEAF_CHUNK_VERSION_V3, batch_id: h(1), chunk_index: 0, leaves, proofs, witnesses };
        let back = PalwLeafChunkV1::try_from_slice(&borsh::to_vec(&chunk).unwrap()).unwrap();
        assert_eq!(chunk, back);
        assert!(chunk.leaves.len() <= PALW_MAX_LEAVES_PER_CHUNK);

        let cert = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: h(1),
            manifest_hash: h(2),
            leaf_root: h(3),
            audit_beacon_epoch: 5,
            audit_sample_root: h(4),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: ZERO_HASH64,
            certificate_epoch: 6,
            activation_epoch: 7,
            expiry_epoch: 13,
            auditor_set_commitment: h(5),
            approving_stake: 0,
            votes: vec![PalwAuditorVoteV2 {
                bond_outpoint: op(0x40, 0),
                vote: 1,
                checked_leaf_bitmap_root: h(6),
                passed_leaf_count: 2,
                rejected_leaf_bitmap_root: ZERO_HASH64,
                signature: vec![9; 4],
            }],
        };
        let cback = PalwBatchCertificateV2::try_from_slice(&borsh::to_vec(&cert).unwrap()).unwrap();
        assert_eq!(cert, cback);
        assert!(cert.is_active_at(7) && cert.is_active_at(12));
        assert!(!cert.is_active_at(6) && !cert.is_active_at(13));
    }

    // =========================================================================================
    // ADR-0040 ECON-03 — the provider bond is RESOLVED collateral, not a self-declared number.
    // =========================================================================================

    /// ADR-0040 ECON-03 leg 5 — `0x37` acceptance is OPEN, and this test states exactly how little
    /// that means, so nobody reads "accepted" as "authorized".
    ///
    /// The stateless arm checks SHAPE only: decodability, version, key and signature lengths. A
    /// request signed by nobody, or by an attacker, passes here — authorization is
    /// `palw_provider_unbond_authorized` (consensus/src/processes/palw.rs), which needs a point of
    /// view this crate does not have.
    ///
    /// Contract: an accepted `0x37` mutates the registry
    /// (`stage_palw_provider_bond_mutations` stamps `unbond_request_daa_score`), so this stateless arm
    /// must never be mistaken for the thing that authorizes it. The authorizer runs at the virtual
    /// coordinate, in `verify_expected_utxo_state`; if this test's signature — 0x42 repeated, which no
    /// key produced — ever started passing there, any party could unbond a stranger's bond.
    #[test]
    fn econ03_unbond_acceptance_is_shape_only() {
        let req = PalwProviderUnbondRequestV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            bond_outpoint: TransactionOutpoint::new(h(0x30), 0),
            owner_public_key: vec![0x41; STAKE_VALIDATOR_PUBKEY_LEN],
            signature: vec![0x42; STAKE_ATTESTATION_SIG_LEN],
        };
        let payload = borsh::to_vec(&req).unwrap();

        // The WIRE is frozen: it round-trips and passes its own stateless shape check.
        assert_eq!(PalwProviderUnbondRequestV1::try_from_slice(&payload).unwrap(), req);
        assert_eq!(validate_provider_unbond(&payload), Ok(()));

        // ACCEPTANCE is open — 0x37 no longer fails closed at the overlay boundary.
        assert_eq!(validate_palw_overlay_payload(0x37, &payload), Ok(()));
        assert_eq!(validate_palw_overlay_tx(0x37, &payload, &[]), Ok(()));

        // ...and that acceptance proves NOTHING about authorization: the signature above is 0x42
        // repeated, which no key produced. Shape is all this coordinate can see.
        assert_eq!(req.signature, vec![0x42; STAKE_ATTESTATION_SIG_LEN]);

        // Shape is still enforced, so a malformed request is refused rather than reaching the
        // authorizer as a decode failure.
        assert_eq!(validate_palw_overlay_payload(0x37, &[0xff, 0x00]), Err(PalwTxError::Decode));
        let mut short_sig = req.clone();
        short_sig.signature.pop();
        assert_eq!(
            validate_palw_overlay_payload(0x37, &borsh::to_vec(&short_sig).unwrap()),
            Err(PalwTxError::InvalidSignatureLen(STAKE_ATTESTATION_SIG_LEN - 1))
        );
        let mut short_key = req.clone();
        short_key.owner_public_key.pop();
        assert_eq!(
            validate_palw_overlay_payload(0x37, &borsh::to_vec(&short_key).unwrap()),
            Err(PalwTxError::InvalidPublicKeyLen(STAKE_VALIDATOR_PUBKEY_LEN - 1))
        );
        let mut bad_version = req.clone();
        bad_version.version = PALW_PAYLOAD_VERSION_V1 + 1;
        assert_eq!(
            validate_palw_overlay_payload(0x37, &borsh::to_vec(&bad_version).unwrap()),
            Err(PalwTxError::UnsupportedVersion(PALW_PAYLOAD_VERSION_V1 + 1))
        );

        // The authorization digest is bound to the network AND the bond, so it is replayable across
        // neither. `ProviderUnbondAuthFilter` consumes this signed preimage at selected-chain
        // acceptance before allowing the collateral-spend exception.
        let mut other_bond = req.clone();
        other_bond.bond_outpoint = TransactionOutpoint::new(h(0x31), 0);
        assert_ne!(req.signing_hash(1), other_bond.signing_hash(1), "digest must bind the bond outpoint");
        assert_ne!(req.signing_hash(1), req.signing_hash(2), "digest must bind the network id");
        let mut other_key = req.clone();
        other_key.owner_public_key = vec![0x77; STAKE_VALIDATOR_PUBKEY_LEN];
        assert_ne!(req.signing_hash(1), other_key.signing_hash(1), "digest must bind the owner key");
    }

    /// kaspa-pq **ADR-0040 (AUTH-TXSHAPE)** — [`build_palw_authorization_transaction`] emits exactly
    /// the shape the two verifiers pin, and pins the subnetwork.
    ///
    /// This asserts each field the isolation-stage `check_palw_block_authorization_shape` names in its
    /// per-field `NonCanonicalPalwAuthorizationTx` errors (version / inputs / outputs / lock_time / gas
    /// / mass), plus the `0x38` subnetwork and the payload's exact round-trip. The round-trip is the
    /// trailing-bytes property: body clause 7 re-decodes the payload and compares the re-encoding, so a
    /// builder that appended even one byte would produce blocks its own validator rejects.
    ///
    /// The shape checker itself is private to `kaspa-consensus`, so this test asserts the fields
    /// directly; the cross-crate agreement is proved by the isolation-stage per-field matrix and by the
    /// end-to-end acceptance test in the consensus crate.
    #[test]
    fn build_palw_authorization_transaction_is_canonical_and_pins_the_subnetwork() {
        let auth = PalwBlockAuthorizationV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            batch_id: h(0xA1),
            leaf_index: 7,
            ticket_nullifier: h(0xA2),
            header_preimage_commitment: h(0xA3),
            authority_public_key: vec![0x5c; STAKE_VALIDATOR_PUBKEY_LEN],
            signature: vec![0x6d; STAKE_ATTESTATION_SIG_LEN],
        };
        let tx = build_palw_authorization_transaction(&auth);

        assert_eq!(tx.version, crate::constants::TX_VERSION, "version must be the canonical TX_VERSION");
        assert!(tx.inputs.is_empty(), "the authorization tx must have no inputs");
        assert!(tx.outputs.is_empty(), "the authorization tx must have no outputs");
        assert_eq!(tx.lock_time, 0, "lock_time must be 0");
        assert_eq!(tx.subnetwork_id, crate::subnets::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION, "the carrying subnetwork must be 0x38");
        assert_eq!(tx.gas, 0, "gas must be 0");
        assert_eq!(tx.mass(), 0, "the storage-mass commitment must be left unset");

        // Exact payload, and no trailing bytes: decode-then-re-encode is the identity.
        assert_eq!(tx.payload, borsh::to_vec(&auth).unwrap(), "payload must be the canonical borsh encoding");
        let decoded = PalwBlockAuthorizationV1::try_from_slice(&tx.payload).expect("payload must decode");
        assert_eq!(decoded, auth, "payload must round-trip to the identical authorization");
        assert_eq!(borsh::to_vec(&decoded).unwrap().len(), tx.payload.len(), "payload must carry no trailing bytes");
    }

    use crate::subnets::SUBNETWORK_ID_PALW_PROVIDER_BOND;
    use crate::tx::Transaction;

    /// A `ProviderBond` transaction carrying `payload` — the shape acceptance derives records from.
    fn econ03_bond_tx(payload: Vec<u8>) -> Transaction {
        Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_PROVIDER_BOND, 0, payload)
    }

    fn econ03_bond_payload(amount: u64, pubkey_byte: u8) -> (PalwProviderBondPayloadV1, Vec<u8>) {
        let bond = PalwProviderBondPayloadV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            owner_public_key: vec![pubkey_byte; STAKE_VALIDATOR_PUBKEY_LEN],
            operator_group_id: h(1),
            runtime_classes: vec![h(2), h(3)],
            capacity_by_shape: vec![(1, 10), (2, 20)],
            reward_key_root: h(4),
            amount_sompi: amount,
            unbond_delay_epochs: 10,
        };
        let payload = borsh::to_vec(&bond).unwrap();
        (bond, payload)
    }

    fn econ03_locking_output(bond: &PalwProviderBondPayloadV1) -> TransactionOutput {
        TransactionOutput { value: bond.amount_sompi, script_public_key: provider_bond_lock_spk(&bond.owner_public_key) }
    }

    /// **LEG 1 — the value lock.** Each rejection below dies if the corresponding line in
    /// [`validate_provider_bond_tx`] is deleted, which is the whole point: before ECON-03 the only
    /// economic check in the admission path was `amount_sompi != 0`, so a provider with an empty
    /// wallet could declare `u64::MAX` and be paid the 77 % base against it.
    #[test]
    fn econ03_a_provider_bond_with_no_backing_is_rejected() {
        let (bond, payload) = econ03_bond_payload(1_000, 0x41);

        // ACCEPT: output-0 locks exactly the declared amount to the owner's own key.
        assert_eq!(validate_provider_bond_tx(&payload, &[econ03_locking_output(&bond)]), Ok(()));

        // REJECT: a bond that declares an amount but creates no output at all — the exact shape of
        // the pre-ECON-03 "number in a database" transaction.
        assert_eq!(validate_provider_bond_tx(&payload, &[]), Err(PalwTxError::MissingProviderBondOutput));

        // REJECT: off by ONE sompi in each direction. Under-locking is theft of collateral;
        // over-locking is a different bond than the one declared.
        for got in [999u64, 1_001] {
            let mut out = econ03_locking_output(&bond);
            out.value = got;
            assert_eq!(
                validate_provider_bond_tx(&payload, &[out]),
                Err(PalwTxError::ProviderBondOutputValueMismatch { expected: 1_000, got })
            );
        }

        // REJECT: the right VALUE locked to somebody else's key. Coins the bond owner does not
        // control are not that owner's collateral, and slashing could never consume them.
        let (other, _) = econ03_bond_payload(1_000, 0x77);
        let mut wrong_key = econ03_locking_output(&bond);
        wrong_key.script_public_key = provider_bond_lock_spk(&other.owner_public_key);
        assert_eq!(validate_provider_bond_tx(&payload, &[wrong_key]), Err(PalwTxError::ProviderBondOutputScriptMismatch));

        // REJECT: zero amount is still refused by the payload check, before the lock is consulted.
        let (zero, zero_payload) = econ03_bond_payload(0, 0x41);
        assert_eq!(
            validate_provider_bond_tx(&zero_payload, &[econ03_locking_output(&zero)]),
            Err(PalwTxError::InvalidField("provider.amount_sompi"))
        );
    }

    /// The lock rule reaches the bond through the SAME entry point consensus calls. If
    /// `validate_palw_overlay_tx` stopped delegating to `validate_provider_bond_tx`, the isolation
    /// validator would silently go back to accepting unbacked bonds — so assert the composition, not
    /// just the leaf function.
    #[test]
    fn econ03_the_tx_level_entry_point_enforces_the_lock() {
        let (bond, payload) = econ03_bond_payload(1_000, 0x41);
        assert_eq!(validate_palw_overlay_tx(0x30, &payload, &[econ03_locking_output(&bond)]), Ok(()));
        assert_eq!(validate_palw_overlay_tx(0x30, &payload, &[]), Err(PalwTxError::MissingProviderBondOutput));
        // Every other PALW kind is unaffected: the tx form must equal the payload form for them, or
        // this change would have quietly altered admission for the whole overlay.
        for (byte, p) in valid_palw_overlay_payloads().into_iter().filter(|(b, _)| *b != 0x30) {
            assert_eq!(validate_palw_overlay_tx(byte, &p, &[]), validate_palw_overlay_payload(byte, &p), "kind 0x{byte:02x}");
        }
    }

    /// **LEG 2 — the record, and the anti-split floor.** A sub-minimum bond is DROPPED, so it never
    /// enters the registry and can never resolve to collateral.
    #[test]
    fn econ03_sub_minimum_bonds_are_dropped_from_the_registry() {
        let (_, payload) = econ03_bond_payload(1_000, 0x41);
        let tx = econ03_bond_tx(payload);
        // Below the floor: no mutation at all.
        assert!(palw_provider_bond_mutations_from_accepted_txs(std::slice::from_ref(&tx), 500, 1_001, 4).is_empty());
        // At the floor: admitted.
        let muts = palw_provider_bond_mutations_from_accepted_txs(&[tx], 500, 1_000, 4);
        assert_eq!(muts.len(), 1);
        let PalwProviderBondMutation::Insert(_, record) = &muts[0] else { panic!("expected an insert") };
        assert_eq!(record.amount_sompi, 1_000);
        // Activation is the ACCEPTANCE daa — a bond cannot insert itself into a past epoch.
        assert_eq!(record.activation_daa_score, 500);
        assert_eq!(record.created_daa_score, 500);
        // The declared delay (10) is above the floor (4) so it stands; the clamp is exercised below.
        assert_eq!(record.unbond_delay_epochs, 10);
    }

    /// The unbond-delay clamp goes UP only — an operator must not be able to shorten its own exit
    /// lock, and therefore its slashable window, by declaring a tiny delay.
    #[test]
    fn econ03_unbond_delay_is_clamped_up_to_the_network_floor() {
        let (_, payload) = econ03_bond_payload(1_000, 0x41);
        let tx = econ03_bond_tx(payload);
        let muts = palw_provider_bond_mutations_from_accepted_txs(&[tx], 500, 1_000, 99);
        let PalwProviderBondMutation::Insert(_, record) = &muts[0] else { panic!("expected an insert") };
        assert_eq!(record.unbond_delay_epochs, 99, "declared 10 must be clamped up to the floor 99");
    }

    /// **LEG 5 — an authorized request moves the record, and the release DAA is the CLAMPED delay.**
    ///
    /// The attack this pins: a provider declares `unbond_delay_epochs = 1`, so its collateral becomes
    /// spendable one epoch after it asks — a slashable window it chose for itself, which is no window.
    /// Acceptance clamps the delay UP to the network floor, and the release DAA is computed from the
    /// clamped field, so the declared figure never reaches the release clock.
    #[test]
    fn econ03_release_daa_uses_the_clamped_delay_not_the_declared_one() {
        const EPOCH_LEN: u64 = 100;
        // The payload declares 10 epochs; the network floor is 99.
        let (declared, payload) = econ03_bond_payload(1_000, 0x41);
        assert_eq!(declared.unbond_delay_epochs, 10);
        let tx = econ03_bond_tx(payload);
        let outpoint = TransactionOutpoint::new(tx.id(), 0);
        let mut view = ProviderBondView::new();
        view.apply(&palw_provider_bond_mutations_from_accepted_txs(&[tx], 500, 1_000, 99));

        // No request yet: there is no release at all, and the bond is Active collateral.
        let record = view.get(&outpoint).unwrap();
        assert_eq!(provider_bond_release_daa_score(record, EPOCH_LEN), None);
        assert!(!is_provider_bond_releasable_at(record, u64::MAX, EPOCH_LEN));

        // An authorized request at DAA 600 moves the record: the stamp is set and status DERIVES to
        // Unbonding from that DAA onward.
        view.apply(&[PalwProviderBondMutation::Unbond(outpoint, 600)]);
        let record = view.get(&outpoint).unwrap();
        assert_eq!(record.unbond_request_daa_score, Some(600));
        assert_eq!(effective_provider_bond_status(record, 599), PalwProviderBondStatus::Active);
        assert_eq!(effective_provider_bond_status(record, 600), PalwProviderBondStatus::Unbonding);

        // The release is 600 + 99*100 — the CLAMPED delay. Had the declared 10 been used it would be
        // 600 + 10*100 = 1_600, which this asserts is NOT releasable.
        assert_eq!(provider_bond_release_daa_score(record, EPOCH_LEN), Some(10_500));
        assert!(!is_provider_bond_releasable_at(record, 1_600, EPOCH_LEN), "the declared delay must not release the bond");
        assert!(!is_provider_bond_releasable_at(record, 10_499, EPOCH_LEN));
        assert!(is_provider_bond_releasable_at(record, 10_500, EPOCH_LEN));

        // A slashed bond has no exit: `Slashed` outranks `Unbonding`, so it never becomes releasable.
        let mut slashed = view.clone();
        slashed.apply(&[PalwProviderBondMutation::Slash(outpoint, 700)]);
        assert!(!is_provider_bond_releasable_at(slashed.get(&outpoint).unwrap(), u64::MAX, EPOCH_LEN));
    }

    /// A pathological delay or request height must saturate at "never releasable" rather than wrap to
    /// an immediate release — the same reason the DNS `bond_release_daa_score` saturates.
    #[test]
    fn econ03_release_daa_saturates_instead_of_wrapping() {
        let (_, payload) = econ03_bond_payload(1_000, 0x41);
        let tx = econ03_bond_tx(payload);
        let outpoint = TransactionOutpoint::new(tx.id(), 0);
        let mut view = ProviderBondView::new();
        view.apply(&palw_provider_bond_mutations_from_accepted_txs(&[tx], 0, 1_000, u64::MAX));
        view.apply(&[PalwProviderBondMutation::Unbond(outpoint, u64::MAX)]);
        let record = view.get(&outpoint).unwrap();
        assert_eq!(record.unbond_delay_epochs, u64::MAX);
        assert_eq!(provider_bond_release_daa_score(record, 100), Some(u64::MAX));
        // Releasable only at u64::MAX itself, never earlier — no wrap made it immediate.
        assert!(!is_provider_bond_releasable_at(record, u64::MAX - 1, 100));

        // A zero epoch length is treated as 1 rather than collapsing the delay to zero DAA, which
        // would make every requested exit releasable in the same block.
        let (_, p2) = econ03_bond_payload(1_000, 0x42);
        let tx2 = econ03_bond_tx(p2);
        let op2 = TransactionOutpoint::new(tx2.id(), 0);
        let mut v2 = ProviderBondView::new();
        // Declared 10 epochs beats the floor 5, so the clamped delay is 10; with epoch_length 0
        // treated as 1 the release is 10 + 10*1 = 20, NOT the request DAA itself.
        v2.apply(&palw_provider_bond_mutations_from_accepted_txs(&[tx2], 0, 1_000, 5));
        v2.apply(&[PalwProviderBondMutation::Unbond(op2, 10)]);
        assert_eq!(v2.get(&op2).unwrap().unbond_delay_epochs, 10);
        assert_eq!(provider_bond_release_daa_score(v2.get(&op2).unwrap(), 0), Some(20));
        assert!(!is_provider_bond_releasable_at(v2.get(&op2).unwrap(), 10, 0), "a zero epoch length must not release on request");
    }

    /// The registry PRODUCER and the authorization VERIFIER must act on the same set of
    /// transactions. They share one decoder so they cannot drift; this pins that they agree.
    #[test]
    fn econ03_unbond_producer_and_verifier_read_the_same_transactions() {
        let req = PalwProviderUnbondRequestV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            bond_outpoint: TransactionOutpoint::new(h(0x30), 0),
            owner_public_key: vec![0x41; STAKE_VALIDATOR_PUBKEY_LEN],
            signature: vec![0x42; STAKE_ATTESTATION_SIG_LEN],
        };
        let unbond_tx =
            |payload: Vec<u8>| Transaction::new(0, vec![], vec![], 0, crate::subnets::SUBNETWORK_ID_PALW_PROVIDER_UNBOND, 0, payload);
        let good = unbond_tx(borsh::to_vec(&req).unwrap());
        // An undecodable 0x37 (unreachable in a valid block — isolation rejects it) and a non-PALW tx.
        let junk = unbond_tx(vec![0xff, 0x00]);
        let unrelated = econ03_bond_tx(econ03_bond_payload(1_000, 0x41).1);
        let txs = vec![good.clone(), junk, unrelated];

        // The verifier sees exactly the one well-formed request...
        let seen = palw_provider_unbond_requests_from_accepted_txs(&txs);
        assert_eq!(seen, vec![(good.id(), req.clone())]);

        // ...and the producer emits exactly one Unbond mutation, for the same bond.
        let unbonds: Vec<_> = palw_provider_bond_mutations_from_accepted_txs(&txs, 42, 1_000, 4)
            .into_iter()
            .filter(|m| matches!(m, PalwProviderBondMutation::Unbond(..)))
            .collect();
        assert_eq!(unbonds, vec![PalwProviderBondMutation::Unbond(req.bond_outpoint, 42)]);
    }

    /// **LEG 3 — the point-of-view read.** A real bond resolves to its REAL amount at a point of
    /// view; anything that does not resolve is worth ZERO, not "whatever it declared".
    #[test]
    fn econ03_a_real_bond_resolves_to_its_real_amount_at_a_point_of_view() {
        let (_, payload) = econ03_bond_payload(1_000, 0x41);
        let tx = econ03_bond_tx(payload);
        let outpoint = TransactionOutpoint::new(tx.id(), 0);
        let muts = palw_provider_bond_mutations_from_accepted_txs(&[tx], 500, 1_000, 4);

        let mut view = ProviderBondView::new();
        view.apply(&muts);

        // Before activation the bond is Pending, so it backs NOTHING.
        assert_eq!(view.resolved_collateral_at(&outpoint, 499), 0);
        assert_eq!(view.active_provider_bond_at(&outpoint, 499), None);
        // At and after activation it resolves to its real, locked amount.
        assert_eq!(view.resolved_collateral_at(&outpoint, 500), 1_000);
        assert_eq!(view.resolved_collateral_at(&outpoint, 10_000), 1_000);
        assert_eq!(view.total_active_provider_stake_at(500), 1_000);

        // An outpoint nobody ever bonded resolves to zero rather than to a declared figure. This is
        // the property the reward path needs to be total.
        assert_eq!(view.resolved_collateral_at(&TransactionOutpoint::new(h(0xEE), 0), 500), 0);

        // Unbonding and slashing both drop the resolved collateral to zero from their stamp onward.
        let mut unbonding = view.clone();
        unbonding.apply(&[PalwProviderBondMutation::Unbond(outpoint, 600)]);
        assert_eq!(unbonding.resolved_collateral_at(&outpoint, 599), 1_000);
        assert_eq!(unbonding.resolved_collateral_at(&outpoint, 600), 0);

        let mut slashed = view.clone();
        slashed.apply(&[PalwProviderBondMutation::Slash(outpoint, 700)]);
        assert_eq!(slashed.resolved_collateral_at(&outpoint, 699), 1_000);
        assert_eq!(slashed.resolved_collateral_at(&outpoint, 700), 0);
    }

    /// Status is DERIVED from DAA stamps, never read from the stored `status` field — the
    /// order-independence property at the record level. Mutating the field alone must change nothing.
    #[test]
    fn econ03_status_is_derived_from_daa_not_from_the_stored_field() {
        let (_, payload) = econ03_bond_payload(1_000, 0x41);
        let tx = econ03_bond_tx(payload);
        let muts = palw_provider_bond_mutations_from_accepted_txs(&[tx], 500, 1_000, 4);
        let PalwProviderBondMutation::Insert(_, record) = &muts[0] else { panic!() };

        // There is no mutable `status` field to lie with — status is a pure function of DAA stamps.
        // If a field is ever reinstated, `econ03_view_apply_and_revert_are_exact_inverses` fails.
        assert_eq!(effective_provider_bond_status(record, 500), PalwProviderBondStatus::Active);
        assert_eq!(effective_provider_bond_status(record, 499), PalwProviderBondStatus::Pending);
        assert!(is_provider_bond_active_at(record, 500));

        // Precedence: slashed outranks unbonding outranks activation.
        let mut both = record.clone();
        both.unbond_request_daa_score = Some(600);
        both.slashed_at_daa_score = Some(700);
        assert_eq!(effective_provider_bond_status(&both, 550), PalwProviderBondStatus::Active);
        assert_eq!(effective_provider_bond_status(&both, 650), PalwProviderBondStatus::Unbonding);
        assert_eq!(effective_provider_bond_status(&both, 750), PalwProviderBondStatus::Slashed);
    }

    /// `apply` and `revert` are EXACT INVERSES — the reason two nodes reaching the same block by
    /// different reorg paths hold an identical view. `revert` iterates in reverse so a mutation whose
    /// `Insert` is reverted in the same diff is handled.
    #[test]
    fn econ03_view_apply_and_revert_are_exact_inverses() {
        let (_, payload) = econ03_bond_payload(1_000, 0x41);
        let tx = econ03_bond_tx(payload);
        let outpoint = TransactionOutpoint::new(tx.id(), 0);
        let base_muts = palw_provider_bond_mutations_from_accepted_txs(&[tx], 500, 1_000, 4);

        let mut view = ProviderBondView::new();
        view.apply(&base_muts);
        let anchor = view.clone();

        // A diff that unbonds then slashes, reverted, must restore the anchor byte-for-byte.
        let diff = vec![PalwProviderBondMutation::Unbond(outpoint, 600), PalwProviderBondMutation::Slash(outpoint, 700)];
        view.apply(&diff);
        assert_ne!(view, anchor);
        view.revert(&diff);
        assert_eq!(view, anchor, "apply then revert must be the identity");

        // An Insert reverted in the SAME diff as a later Slash of it: reverse order makes this total.
        let mut fresh = ProviderBondView::new();
        let combined: Vec<_> = base_muts.iter().cloned().chain([PalwProviderBondMutation::Slash(outpoint, 700)]).collect();
        fresh.apply(&combined);
        fresh.revert(&combined);
        assert_eq!(fresh, ProviderBondView::new(), "reverting an insert+slash diff must empty the view");
    }

    /// kaspa-pq **ADR-0040 ECON-03 (THE WIRE) — the resolution predicate the reward path applies.**
    ///
    /// `palw_work_reward_class` pays the 77 % provider base only when BOTH of a leaf's bond outpoints
    /// resolve `Active` at the paying block's point of view, via
    /// [`ProviderBondView::active_provider_bond_at`]. This test is that predicate stated as a rule
    /// rather than a comment: it walks every way a bond can FAIL to resolve and asserts each yields
    /// `None`, and it asserts the one way it succeeds.
    ///
    /// Each failure mode is a distinct attack that was open while the 77 % base was gated only on
    /// `provider_a_bond != provider_b_bond`:
    ///   * UNKNOWN — the original ECON-03 hole: name two outpoints that were never bonded at all.
    ///   * SUB-FLOOR — bond one sompi, split it a hundred ways (SEL-01 Sybil).
    ///   * PENDING — be paid before the collateral is live.
    ///   * UNBONDING — request the exit, keep collecting the base while the coins walk out.
    ///   * SLASHED — keep collecting after the collateral is forfeit.
    #[test]
    fn econ03_only_active_bonds_resolve_as_collateral() {
        let (_, payload) = econ03_bond_payload(1_000, 0x41);
        let tx = econ03_bond_tx(payload);
        let outpoint = TransactionOutpoint::new(tx.id(), 0);
        let unknown = TransactionOutpoint::new(h(0xfe), 0);

        // Accepted at DAA 500 with floor 1_000 (exactly at the floor ⇒ admitted).
        let muts = palw_provider_bond_mutations_from_accepted_txs(std::slice::from_ref(&tx), 500, 1_000, 4);
        let mut view = ProviderBondView::new();
        view.apply(&muts);

        // The ONE way it resolves: known, admitted, and Active at this point of view.
        assert_eq!(view.active_provider_bond_at(&outpoint, 500).map(|r| r.amount_sompi), Some(1_000));
        assert_eq!(view.resolved_collateral_at(&outpoint, 500), 1_000);

        // UNKNOWN — the outpoint names nothing. This is the ECON-03 finding in one line.
        assert!(view.active_provider_bond_at(&unknown, 500).is_none());
        assert_eq!(view.resolved_collateral_at(&unknown, 500), 0, "an unbonded outpoint is worth zero, not 'unknown'");

        // PENDING — one DAA before activation it is not yet collateral.
        assert!(view.active_provider_bond_at(&outpoint, 499).is_none());

        // SUB-FLOOR — raise the floor above the declared amount and the bond never enters the
        // registry at all, so there is nothing to resolve. This is where the SEL-01 floor bites.
        let mut sub_floor = ProviderBondView::new();
        sub_floor.apply(&palw_provider_bond_mutations_from_accepted_txs(&[tx], 500, 1_001, 4));
        assert!(sub_floor.is_empty(), "a sub-floor bond must be DROPPED, not admitted at a raised amount");
        assert!(sub_floor.active_provider_bond_at(&outpoint, 500).is_none());
        assert_eq!(sub_floor.resolved_collateral_at(&outpoint, 500), 0);

        // UNBONDING — an authorized exit request stops the bond backing a reward immediately, at the
        // REQUEST, not at the release. Collateral that is walking out is not collateral.
        let mut unbonding = view.clone();
        unbonding.apply(&[PalwProviderBondMutation::Unbond(outpoint, 600)]);
        assert!(unbonding.active_provider_bond_at(&outpoint, 600).is_none());
        assert_eq!(unbonding.resolved_collateral_at(&outpoint, 600), 0);
        // ...but strictly before the request it was still backing (the status is DAA-derived).
        assert_eq!(unbonding.resolved_collateral_at(&outpoint, 599), 1_000);

        // SLASHED — forfeit principal backs nothing.
        let mut slashed = view.clone();
        slashed.apply(&[PalwProviderBondMutation::Slash(outpoint, 800)]);
        assert!(slashed.active_provider_bond_at(&outpoint, 800).is_none());
        assert_eq!(slashed.resolved_collateral_at(&outpoint, 800), 0);
    }

    /// kaspa-pq **ADR-0040 ECON-03 (THE WIRE) — order independence of the registry walk.**
    ///
    /// The registry writer (`stage_palw_provider_bond_mutations`) and the in-memory view are driven by
    /// the SAME per-chain-block mutation lists, applied forward on blocks joining the selected chain
    /// and reverted on blocks leaving it. If that walk were not order-independent, two nodes reaching
    /// the same sink by different reorg paths would resolve a leaf's collateral differently and the
    /// chain would split at the reward — the exact failure the view was designed to prevent.
    ///
    /// This models the reorg the writer performs: chain `A → B` is walked, then `B` is detached and
    /// `C` attached. The result must equal the view a node that walked `A → C` directly holds, for
    /// every field, not merely for the resolved statuses.
    ///
    /// It also covers the case the DNS precedent gets wrong: `B` UNBONDS a bond that `A` created. The
    /// DNS record would restore `status = Active` on revert regardless of the prior status; here there
    /// is no status field to restore, so the revert clears exactly the one stamp it set.
    #[test]
    fn econ03_registry_walk_is_reorg_path_independent() {
        // Block A creates two bonds.
        let (_, p1) = econ03_bond_payload(1_000, 0x41);
        let (_, p2) = econ03_bond_payload(2_000, 0x42);
        let tx1 = econ03_bond_tx(p1);
        let tx2 = econ03_bond_tx(p2);
        let op1 = TransactionOutpoint::new(tx1.id(), 0);
        let op2 = TransactionOutpoint::new(tx2.id(), 0);
        let block_a = palw_provider_bond_mutations_from_accepted_txs(&[tx1, tx2], 500, 1_000, 4);

        // Block B (the branch that loses) unbonds bond 1 and slashes bond 2.
        let block_b = vec![PalwProviderBondMutation::Unbond(op1, 600), PalwProviderBondMutation::Slash(op2, 600)];
        // Block C (the branch that wins) unbonds bond 2 instead, at a different DAA.
        let block_c = vec![PalwProviderBondMutation::Unbond(op2, 610)];

        // Path 1: walk A → B, then reorg (revert B, apply C).
        let mut reorged = ProviderBondView::new();
        reorged.apply(&block_a);
        reorged.apply(&block_b);
        reorged.revert(&block_b);
        reorged.apply(&block_c);

        // Path 2: walk A → C directly.
        let mut direct = ProviderBondView::new();
        direct.apply(&block_a);
        direct.apply(&block_c);

        assert_eq!(reorged, direct, "two reorg paths to the same sink must yield byte-identical registries");

        // And the resolved economics agree, which is what the reward path actually reads.
        assert_eq!(reorged.resolved_collateral_at(&op1, 700), 1_000, "bond 1's unbond was on the losing branch");
        assert_eq!(reorged.resolved_collateral_at(&op2, 700), 0, "bond 2 is unbonding on the winning branch");
        assert_eq!(reorged.total_active_provider_stake_at(700), direct.total_active_provider_stake_at(700));

        // Reverting the whole walk empties the registry — no residue from the branch that lost.
        let mut drained = reorged.clone();
        drained.revert(&block_c);
        drained.revert(&block_a);
        assert_eq!(drained, ProviderBondView::new());
    }

    /// kaspa-pq **ADR-0040 ECON-03 (THE WIRE)** — the mutation list a block contributes depends only
    /// on its accepted transactions and its own DAA, never on which OTHER blocks were processed first.
    ///
    /// This is the property that lets the writer re-derive a chain block's mutations from retained
    /// acceptance data at revert time (`palw_provider_bond_mutations_for_chain_block`) and get exactly
    /// what was applied. If the derivation carried any hidden dependence on prior state, revert would
    /// subtract something other than what apply added, and the registry would drift.
    #[test]
    fn econ03_block_mutations_are_a_pure_function_of_the_block() {
        let (_, p1) = econ03_bond_payload(1_000, 0x41);
        let (_, p2) = econ03_bond_payload(3_000, 0x43);
        let tx1 = econ03_bond_tx(p1);
        let tx2 = econ03_bond_tx(p2);

        let first = palw_provider_bond_mutations_from_accepted_txs(&[tx1.clone(), tx2.clone()], 500, 1_000, 4);
        // Re-derived later, from the same inputs — must be identical, element for element.
        let redrived = palw_provider_bond_mutations_from_accepted_txs(&[tx1.clone(), tx2.clone()], 500, 1_000, 4);
        assert_eq!(first, redrived, "re-derivation at revert time must reproduce what apply added");

        // Reversing the TX order changes only the order of the emitted Inserts, never their content —
        // and since Inserts are keyed by distinct outpoints, the resulting view is the same either way.
        let swapped = palw_provider_bond_mutations_from_accepted_txs(&[tx2, tx1], 500, 1_000, 4);
        let mut a = ProviderBondView::new();
        a.apply(&first);
        let mut b = ProviderBondView::new();
        b.apply(&swapped);
        assert_eq!(a, b, "two distinct bonds in one block resolve identically regardless of tx order");
    }

    fn valid_palw_overlay_payloads() -> Vec<(u8, Vec<u8>)> {
        let provider = PalwProviderBondPayloadV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            owner_public_key: vec![0x41; STAKE_VALIDATOR_PUBKEY_LEN],
            operator_group_id: h(1),
            runtime_classes: vec![h(2), h(3)],
            capacity_by_shape: vec![(1, 10), (2, 20)],
            reward_key_root: h(4),
            amount_sompi: 1_000,
            unbond_delay_epochs: 10,
        };
        let manifest = PalwBatchManifestV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            batch_id: h(5),
            registration_epoch: 7,
            model_profile_id: h(6),
            runtime_class_id: h(7),
            leaf_count: 1,
            chunk_count: 1,
            leaf_root: h(8),
            descriptor_root: h(9),
            total_leaf_bond_sompi: 10,
            audit_policy_id: h(10),
            activation_not_before_epoch: 9,
            expiry_epoch: 15,
        };
        let mut leaf = sample_leaf();
        leaf.batch_id = h(5);
        let chunk = PalwLeafChunkV1 {
            version: PALW_LEAF_CHUNK_VERSION_V3,
            batch_id: h(5),
            chunk_index: 0,
            proofs: vec![palw_leaf_merkle_proof(&[leaf.leaf_hash()], 0).unwrap()],
            witnesses: vec![dummy_external_witness()],
            leaves: vec![leaf],
        };
        let certificate = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: h(5),
            manifest_hash: h(11),
            leaf_root: h(8),
            audit_beacon_epoch: 8,
            audit_sample_root: h(12),
            passed_leaf_count: 1,
            rejected_leaf_bitmap_root: h(13),
            certificate_epoch: 9,
            activation_epoch: 10,
            expiry_epoch: 15,
            auditor_set_commitment: h(14),
            approving_stake: 0,
            votes: vec![PalwAuditorVoteV2 {
                bond_outpoint: op(0x42, 0),
                vote: 1,
                checked_leaf_bitmap_root: h(15),
                passed_leaf_count: 1,
                rejected_leaf_bitmap_root: h(13),
                signature: vec![0x55; STAKE_ATTESTATION_SIG_LEN],
            }],
        };
        let revocation = PalwRevocationV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            batch_id: h(5),
            effective_daa_score: 1_000,
            reason_code: 1,
            evidence_hash: h(16),
        };
        let bond = op(0x43, 1);
        let random = [0x66; 64];
        let commit = PalwBeaconCommitV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            epoch: 12,
            bond_outpoint: bond,
            commitment: beacon_commitment(12, &random, &bond),
            signature: vec![0x77; STAKE_ATTESTATION_SIG_LEN],
        };
        let reveal = PalwBeaconRevealV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            epoch: 12,
            bond_outpoint: bond,
            random_64: random,
            signature: vec![0x88; STAKE_ATTESTATION_SIG_LEN],
        };
        vec![
            (0x30, borsh::to_vec(&provider).unwrap()),
            (0x31, borsh::to_vec(&manifest).unwrap()),
            (0x32, borsh::to_vec(&chunk).unwrap()),
            (0x33, borsh::to_vec(&certificate).unwrap()),
            (0x34, borsh::to_vec(&revocation).unwrap()),
            (0x35, borsh::to_vec(&commit).unwrap()),
            (0x36, borsh::to_vec(&reveal).unwrap()),
        ]
    }

    /// ADR-0040 **P0-4 / gate G2** — ECON-01 (coinbase poison) closure.
    ///
    /// The invariant: **every reward script a leaf can carry past admission must be emitable as a
    /// coinbase output.** If it were not, the algo-4 source block would still be body-valid and enter the
    /// DAG, while every descendant merging it derived an unrepresentable coinbase — a permanent halt that
    /// the honest template builder would keep reproducing.
    ///
    /// Both directions are asserted, because only the pair is meaningful: rejecting everything would
    /// trivially satisfy the safety half.
    #[test]
    fn palw_reward_script_admission_matches_coinbase_representability() {
        const COINBASE_SPK_MAX_LEN: usize = 150; // params.coinbase_payload_script_public_key_max_len

        // ---- accepted ⇒ payable ----
        for b in [0x00u8, 0x01, 0x7f, 0xa0, 0xff] {
            let s = reward_spk(b);
            assert!(palw_reward_script_is_coinbase_representable(&s), "the canonical ML-DSA-87 P2PKH template must be payable");
            // ...and it satisfies the two rules the coinbase path actually enforces.
            assert_eq!(s.version(), 0);
            assert_eq!(s.script().len(), 69, "the one PQ-standard class has exactly one layout");
            assert!(s.script().len() <= COINBASE_SPK_MAX_LEN, "must fit the coinbase script bound");
        }

        // ---- rejected ⇒ each is a shape that would have bricked the chain ----
        let cases: Vec<(&str, ScriptPublicKey)> = vec![
            // The pre-fix hazard class 1: a non-PQ script. Short enough for the 150-byte bound, so ONLY
            // the class rule would have caught it at coinbase time — i.e. too late.
            ("non-PQ 3-byte stub", ScriptPublicKey::from_vec(0, vec![0xa0, 0xa0, 0xa0])),
            // The pre-fix hazard class 2: 151..=1024 bytes — admitted by the old 1024 bound, rejected by
            // the coinbase length rule.
            ("151 bytes (over coinbase bound, under old 1024)", ScriptPublicKey::from_vec(0, vec![0x51; 151])),
            ("1024 bytes (the old bound exactly)", ScriptPublicKey::from_vec(0, vec![0x51; 1024])),
            // Near-misses on the template itself: right length, wrong opcodes.
            ("right length, wrong leading opcode", {
                let mut v = reward_spk(0x11).script().to_vec();
                v[0] = 0x77;
                ScriptPublicKey::from_vec(0, v)
            }),
            ("right length, wrong trailing checksig", {
                let mut v = reward_spk(0x11).script().to_vec();
                v[68] = 0xa5;
                ScriptPublicKey::from_vec(0, v)
            }),
            ("right bytes, wrong spk version", ScriptPublicKey::from_vec(1, reward_spk(0x11).script().to_vec())),
            ("empty", ScriptPublicKey::from_vec(0, vec![])),
        ];
        for (name, s) in &cases {
            assert!(!palw_reward_script_is_coinbase_representable(s), "{name} must NOT be admissible as a reward script");
        }

        // ---- and the predicate is actually wired into leaf admission, per side ----
        let base = sample_leaf();
        for (name, bad) in &cases {
            let mut l = base.clone();
            l.provider_a_reward_script = bad.clone();
            assert_eq!(
                validate_public_leaf(&l, &l.batch_id),
                Err(PalwTxError::InvalidField("leaf.reward_script")),
                "provider A: {name} must be rejected at leaf admission"
            );
            let mut l = base.clone();
            l.provider_b_reward_script = bad.clone();
            assert_eq!(
                validate_public_leaf(&l, &l.batch_id),
                Err(PalwTxError::InvalidField("leaf.reward_script")),
                "provider B: {name} must be rejected at leaf admission"
            );
        }
        // The honest leaf still passes — the guard is not a blanket rejection.
        assert_eq!(validate_public_leaf(&base, &base.batch_id), Ok(()));
    }

    #[test]
    fn palw_stateless_payload_validator_accepts_all_current_kinds() {
        for (kind, payload) in valid_palw_overlay_payloads() {
            assert_eq!(validate_palw_overlay_payload(kind, &payload), Ok(()), "kind 0x{kind:02x}");
        }
        // 0x37 acceptance is open (ADR-0040 ECON-03 leg 5) and enforces shape: an empty payload is a
        // DECODE failure, not `UnsupportedKind`. What acceptance does and does not prove is pinned by
        // `econ03_unbond_acceptance_is_shape_only`.
        assert_eq!(validate_palw_overlay_payload(0x37, &[]), Err(PalwTxError::Decode));
    }

    #[test]
    fn certificate_v2_rejects_legacy_version_and_summary_repackaging() {
        let (_, bytes) = valid_palw_overlay_payloads().into_iter().find(|(kind, _)| *kind == 0x33).unwrap();
        let cert = PalwBatchCertificateV2::try_from_slice(&bytes).unwrap();

        let mut wrong_version = cert.clone();
        wrong_version.version = PALW_PAYLOAD_VERSION_V1;
        assert_eq!(
            validate_palw_overlay_payload(0x33, &borsh::to_vec(&wrong_version).unwrap()),
            Err(PalwTxError::UnsupportedVersion(PALW_PAYLOAD_VERSION_V1))
        );

        let mut maximum = cert.clone();
        let template = maximum.votes[0].clone();
        maximum.votes = (0..PALW_MAX_AUDITOR_VOTES_V2)
            .map(|index| PalwAuditorVoteV2 {
                bond_outpoint: TransactionOutpoint::new(template.bond_outpoint.transaction_id, index as u32),
                ..template.clone()
            })
            .collect();
        let maximum_bytes = borsh::to_vec(&maximum).unwrap();
        assert!(
            maximum_bytes.len() <= PALW_MAX_OVERLAY_PAYLOAD_BYTES,
            "a maximum-size V2 certificate is {} bytes and must fit the {}-byte overlay cap",
            maximum_bytes.len(),
            PALW_MAX_OVERLAY_PAYLOAD_BYTES
        );
        assert_eq!(validate_palw_overlay_payload(0x33, &maximum_bytes), Ok(()));

        let mut repackaged = cert;
        repackaged.passed_leaf_count += 1;
        repackaged.rejected_leaf_bitmap_root = h(0xee);
        assert_eq!(
            validate_palw_overlay_payload(0x33, &borsh::to_vec(&repackaged).unwrap()),
            Err(PalwTxError::InvalidField("certificate.vote.passed_leaf_count"))
        );

        #[derive(BorshSerialize)]
        struct LegacyAuditorVoteV1 {
            bond_outpoint: TransactionOutpoint,
            vote: u8,
            checked_leaf_bitmap_root: Hash64,
            signature: Vec<u8>,
        }

        #[derive(BorshSerialize)]
        struct LegacyBatchCertificateV1 {
            version: u16,
            batch_id: Hash64,
            manifest_hash: Hash64,
            leaf_root: Hash64,
            audit_beacon_epoch: u64,
            audit_sample_root: Hash64,
            passed_leaf_count: u32,
            rejected_leaf_bitmap_root: Hash64,
            certificate_epoch: u64,
            activation_epoch: u64,
            expiry_epoch: u64,
            auditor_set_commitment: Hash64,
            approving_stake: u128,
            votes: Vec<LegacyAuditorVoteV1>,
        }

        let legacy = LegacyBatchCertificateV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            batch_id: h(5),
            manifest_hash: h(11),
            leaf_root: h(8),
            audit_beacon_epoch: 8,
            audit_sample_root: h(12),
            passed_leaf_count: 1,
            rejected_leaf_bitmap_root: h(13),
            certificate_epoch: 9,
            activation_epoch: 10,
            expiry_epoch: 15,
            auditor_set_commitment: h(14),
            approving_stake: 0,
            votes: vec![LegacyAuditorVoteV1 {
                bond_outpoint: op(0x42, 0),
                vote: 1,
                checked_leaf_bitmap_root: h(15),
                signature: vec![0x55; STAKE_ATTESTATION_SIG_LEN],
            }],
        };
        assert!(
            validate_palw_overlay_payload(0x33, &borsh::to_vec(&legacy).unwrap()).is_err(),
            "legacy V1 certificate bytes must never decode as an accepted V2 certificate"
        );
    }

    #[test]
    fn palw_stateless_payload_validator_rejects_malformed_noncanonical_and_oversized() {
        let payloads = valid_palw_overlay_payloads();

        // Strict Borsh decoding rejects trailing bytes after a valid object.
        let mut trailing = payloads.iter().find(|(kind, _)| *kind == 0x31).unwrap().1.clone();
        trailing.push(0);
        assert_eq!(validate_palw_overlay_payload(0x31, &trailing), Err(PalwTxError::Decode));

        let mut bad_commit: PalwBeaconCommitV1 =
            borsh::from_slice(&payloads.iter().find(|(kind, _)| *kind == 0x35).unwrap().1).unwrap();
        bad_commit.version = 2;
        assert_eq!(validate_palw_overlay_payload(0x35, &borsh::to_vec(&bad_commit).unwrap()), Err(PalwTxError::UnsupportedVersion(2)));
        bad_commit.version = 1;
        bad_commit.signature.pop();
        assert_eq!(
            validate_palw_overlay_payload(0x35, &borsh::to_vec(&bad_commit).unwrap()),
            Err(PalwTxError::InvalidSignatureLen(STAKE_ATTESTATION_SIG_LEN - 1))
        );

        let mut chunk: PalwLeafChunkV1 = borsh::from_slice(&payloads.iter().find(|(kind, _)| *kind == 0x32).unwrap().1).unwrap();
        chunk.leaves.push(chunk.leaves[0].clone());
        // Keep the proof AND witness vectors index-aligned so this still lands on the NULLIFIER-
        // uniqueness check and not on the (earlier) arity checks (§5.15 proofs, D3-b witnesses).
        chunk.proofs.push(chunk.proofs[0].clone());
        chunk.witnesses.push(chunk.witnesses[0].clone());
        assert_eq!(
            validate_palw_overlay_payload(0x32, &borsh::to_vec(&chunk).unwrap()),
            Err(PalwTxError::NonCanonical("leaf_chunk.ticket_nullifiers"))
        );

        let oversized = vec![0u8; PALW_MAX_OVERLAY_PAYLOAD_BYTES + 1];
        assert_eq!(
            validate_palw_overlay_payload(0x30, &oversized),
            Err(PalwTxError::PayloadTooLarge { len: PALW_MAX_OVERLAY_PAYLOAD_BYTES + 1, max: PALW_MAX_OVERLAY_PAYLOAD_BYTES })
        );
        assert_eq!(validate_palw_overlay_payload(0x2f, &[]), Err(PalwTxError::UnsupportedKind(0x2f)));
    }

    #[test]
    fn params_default_is_inert_and_structurally_valid() {
        let p = PalwParams::testnet_inert_default();
        assert_eq!(p.activation_daa_score, u64::MAX);
        assert!(!p.is_active_at(0));
        assert!(!p.is_active_at(u64::MAX - 1));
        assert!(p.is_structurally_valid());
        // the committed 10-BPS PALW split: 2 + 8 = 10, 1:4 cap, 20 % hash floor.
        assert_eq!(p.total_bps, 10);
        assert_eq!((p.hash_lane_bps, p.replica_lane_bps), (2, 8));
        assert_eq!(p.hash_lane_bps + p.replica_lane_bps, p.total_bps);
        assert_eq!(p.replica_lane_bps, p.compute_to_hash_cap * p.hash_lane_bps);
        assert_eq!(p.hash_lane_bps * 5, p.total_bps); // 2/10 = 20 %
        // wall-clock-preserving DAA windows scaled by 1/4 vs the 40-BPS profile.
        assert_eq!(p.epoch_length_daa, 100); // ≈ 10 s at 10 BPS
        assert_eq!(p.nullifier_retention_daa, 1_200); // ≈ 120 s

        // borsh roundtrip.
        let back = PalwParams::try_from_slice(&borsh::to_vec(&p).unwrap()).unwrap();
        assert_eq!(p, back);

        // a malformed split is rejected.
        let mut bad = p.clone();
        bad.replica_lane_bps = 33; // 2 + 33 != 10 and 33 > 4·2
        assert!(!bad.is_structurally_valid());
    }

    #[test]
    fn palw_params_pcpb_windows_are_consistent() {
        let p = PalwParams::testnet_inert_default();
        // The sole PalwParams preset satisfies the §5.14.7.5 PCPB window invariants.
        assert!(p.is_structurally_valid());
        assert!(p.snapshot_lag_epochs >= 1 && p.post_commit_delta_epochs >= 1);
        assert!(p.freshness_window_epochs >= p.post_commit_delta_epochs);
        assert!(p.snapshot_lag_epochs + p.post_commit_delta_epochs <= p.evidence_window_epochs);

        // Each invariant is load-bearing: violating any one is rejected at config-build time.
        let mut z = p.clone();
        z.snapshot_lag_epochs = 0;
        assert!(!z.is_structurally_valid(), "k = 0 must be rejected");
        let mut z = p.clone();
        z.post_commit_delta_epochs = 0;
        assert!(!z.is_structurally_valid(), "Δ = 0 must be rejected");
        let mut z = p.clone();
        z.freshness_window_epochs = p.post_commit_delta_epochs - 1;
        assert!(!z.is_structurally_valid(), "w < Δ must be rejected");
        let mut z = p.clone();
        z.snapshot_lag_epochs = p.evidence_window_epochs; // k + Δ > evidence_window
        assert!(!z.is_structurally_valid(), "k + Δ > evidence_window must be rejected");
    }

    fn w(n: u64) -> BlueWorkType {
        BlueWorkType::from_u64(n)
    }

    #[test]
    fn compute_cap_bounds_effective_work() {
        // §27.6 property tests, cap = 4:
        // (1) C_eff <= cap·H; (2) E = H + C_eff; (3) H <= E <= (cap+1)·H; (4) monotonic; (5) headroom.
        let cap = 4u64;
        for &hh in &[0u64, 1, 7, 1000, 1_000_000] {
            for &cc in &[0u64, 1, hh, 4 * hh, 4 * hh + 5, 100 * hh + 3] {
                let h = w(hh);
                let c = w(cc);
                let ceff = capped_compute_work(c, h, cap);
                let e = effective_blue_work(h, c, cap);
                // C_eff <= cap·H and C_eff <= C.
                assert!(ceff <= w(cap * hh), "C_eff must be <= 4H (H={hh}, C={cc})");
                assert!(ceff <= c);
                // E = H + C_eff.
                assert_eq!(e, h.saturating_add(ceff));
                // H <= E <= 5H.
                assert!(e >= h, "E >= H (H={hh}, C={cc})");
                assert!(e <= w(5 * hh), "E <= 5H (H={hh}, C={cc})");
            }
        }
    }

    #[test]
    fn compute_cap_monotonic_and_headroom_exhaustion() {
        let cap = 4u64;
        let h = w(100);
        // monotonic in compute up to the cap, then flat (headroom exhausted).
        let mut prev = effective_blue_work(h, w(0), cap);
        for cc in [10u64, 50, 100, 300, 400, 401, 500, 10_000] {
            let e = effective_blue_work(h, w(cc), cap);
            assert!(e >= prev, "E must be monotonic non-decreasing in C");
            prev = e;
        }
        // once C >= cap·H the effective work is pinned at (cap+1)·H — extra compute is worthless
        // (design §5.4: zero-headroom PALW blocks are not accepted, so this pin is never exceeded).
        assert_eq!(effective_blue_work(h, w(400), cap), w(500));
        assert_eq!(effective_blue_work(h, w(400), cap), effective_blue_work(h, w(10_000_000), cap));
        assert_eq!(compute_headroom(h, w(399), cap), w(1));
        assert_eq!(compute_headroom(h, w(400), cap), w(0));
        assert_eq!(compute_headroom(h, w(401), cap), w(0));
    }

    #[test]
    fn compute_cap_saturates_and_zero_hash_zeroes_compute() {
        // zero hash floor ⇒ zero credited compute (a compute-only block earns nothing; §5.4).
        assert_eq!(effective_blue_work(w(0), w(1_000_000), 4), w(0));
        assert_eq!(capped_compute_work(w(1_000_000), w(0), 4), w(0));
        // saturating: cap·H near MAX must not wrap.
        let big = BlueWorkType::MAX;
        let capped = capped_compute_work(big, big, 4);
        assert_eq!(capped, big); // min(MAX, sat(4·MAX)=MAX) = MAX
        assert_eq!(effective_blue_work(big, big, 4), big); // sat(MAX + MAX) = MAX
        assert_eq!(compute_headroom(big, w(0), 4), big); // sat(4·MAX) - 0 = MAX
        assert_eq!(compute_headroom(big, big, 4), w(0));
        assert_eq!(compute_headroom(w(100), w(0), 0), w(0));
    }

    #[test]
    fn batch_state_machine_happy_path_and_terminals() {
        use PalwBatchEvent::*;
        use PalwBatchStatus::*;
        // full happy path Missing → ... → Active.
        let mut s = Missing;
        for (ev, expect) in [
            (ManifestAccepted, Registering),
            (ChunksAndBondsComplete, Committed),
            (AuditBeaconReached, Auditing),
            (CertificateQuorum, Certified),
            (ActivationReached, Active),
        ] {
            s = s.next(ev).unwrap();
            assert_eq!(s, expect);
            assert!(!s.is_terminal());
        }
        assert!(s.is_block_eligible());

        // only Active is block-eligible.
        for st in [Missing, Registering, Committed, Auditing, Certified, Slashed, Expired, Revoked] {
            assert!(!st.is_block_eligible());
        }

        // fraud after activation revokes; expiry expires.
        assert_eq!(Active.next(FraudEvidence), Some(Revoked));
        assert_eq!(Active.next(ExpiryReached), Some(Expired));
    }

    #[test]
    fn batch_state_machine_rejects_invalid_and_terminal_transitions() {
        use PalwBatchEvent::*;
        use PalwBatchStatus::*;
        // an incomplete batch (Registering + timeout) expires and can never become Active.
        assert_eq!(Registering.next(Timeout), Some(Expired));
        // a failed audit slashes.
        assert_eq!(Auditing.next(AuditFailed), Some(Slashed));
        // terminal states have no outgoing edges for any event.
        for term in [Slashed, Expired, Revoked] {
            assert!(term.is_terminal());
            for ev in [
                ManifestAccepted,
                ChunksAndBondsComplete,
                AuditBeaconReached,
                CertificateQuorum,
                AuditFailed,
                Timeout,
                ActivationReached,
                ExpiryReached,
                FraudEvidence,
            ] {
                assert_eq!(term.next(ev), None, "{term:?} must be terminal for {ev:?}");
            }
        }
        // out-of-order events are rejected (e.g. activate before certified, quorum before auditing).
        assert_eq!(Missing.next(ChunksAndBondsComplete), None);
        assert_eq!(Committed.next(ActivationReached), None);
        assert_eq!(Registering.next(CertificateQuorum), None);
        assert_eq!(Active.next(CertificateQuorum), None);
    }

    /// ADR-0045 D1 (SS-04): `palw_batch_referenceable` must treat `Revoked` NON-retroactively — a block
    /// before the revocation's effective DAA (`revoked == false`) still resolves the batch, exactly like
    /// its pre-revocation Active/Certified state; only at/after the effective DAA (`revoked == true`) is
    /// it dropped. `Slashed`/`Expired` stay retroactively unreferenceable regardless of the flag.
    #[test]
    fn revoked_batch_is_referenceable_non_retroactively() {
        use PalwBatchStatus::*;
        // registration_epoch=0, expiry_epoch=100, lead=2, audit=6, current epoch=10 (well inside window).
        let refable = |status, revoked| palw_batch_referenceable(status, revoked, 0, 100, 10, 2, 6);

        // Before the effective DAA (revoked=false): a revoked batch resolves like Active/Certified.
        assert!(refable(Revoked, false), "Revoked before effective DAA must still be referenceable (§9.5)");
        assert!(refable(Active, false));
        assert!(refable(Certified, false));
        // At/after the effective DAA (revoked=true): every non-terminal status is dropped.
        assert!(!refable(Revoked, true), "Revoked at/after effective DAA must be dropped");
        assert!(!refable(Active, true), "a future-dated revocation drops Active only once it takes effect");
        // Slashed/Expired never validly backed a block ⇒ retroactively unreferenceable either way.
        for st in [Slashed, Expired, Missing] {
            assert!(!refable(st, false), "{st:?} must never be referenceable");
            assert!(!refable(st, true));
        }
        // A revoked batch past its expiry epoch is dropped even before the effective DAA (window closed).
        assert!(!palw_batch_referenceable(Revoked, false, 0, 5, 10, 2, 6), "past expiry ⇒ dropped regardless");
    }

    #[test]
    fn provider_pair_selection_is_deterministic_distinct_and_gated() {
        let seed = h(0x51);
        let cap = h(0x52); // job capability
        // deterministic + distinct + within range.
        let (a, b) = select_provider_pair(&seed, &cap, 10, 32, |_, _| true).unwrap();
        assert!(a < 10 && b < 10 && a != b);
        assert_eq!(select_provider_pair(&seed, &cap, 10, 32, |_, _| true), Some((a, b)));
        // count < 2 ⇒ no pair.
        assert_eq!(select_provider_pair(&seed, &cap, 1, 32, |_, _| true), None);
        // the accept predicate is honored: reject the found pair ⇒ re-sample to a different pair
        // (or None). Rejecting *everything* yields None.
        assert_eq!(select_provider_pair(&seed, &cap, 10, 32, |_, _| false), None);
        // rejecting the specific first pair forces a different acceptable one.
        let alt = select_provider_pair(&seed, &cap, 10, 32, |x, y| (x, y) != (a, b));
        assert!(alt.is_some() && alt != Some((a, b)));
    }

    #[test]
    fn auditor_selection_is_deterministic_and_score_bounded() {
        let prev = h(0x61);
        let batch = h(0x62);
        let bonds: Vec<TransactionOutpoint> = (0..8).map(|i| op(0x70 + i, i as u32)).collect();
        let top3 = sample_auditors_by_score(&prev, &batch, &bonds, 3);
        assert_eq!(top3.len(), 3);
        // deterministic.
        assert_eq!(top3, sample_auditors_by_score(&prev, &batch, &bonds, 3));
        // the chosen three are exactly the smallest-score bonds.
        let mut all: Vec<(Hash64, TransactionOutpoint)> = bonds.iter().map(|b| (auditor_score(&prev, &batch, b), *b)).collect();
        all.sort_by(|x, y| x.0.as_byte_slice().cmp(y.0.as_byte_slice()));
        let expect: Vec<TransactionOutpoint> = all.into_iter().take(3).map(|(_, b)| b).collect();
        assert_eq!(top3, expect);
        // a different batch id reshuffles the winners (score depends on batch).
        assert_ne!(sample_auditors_by_score(&prev, &h(0x63), &bonds, 3), top3);
    }

    #[test]
    fn palw_batch_completeness_da_and_template_selection() {
        // §9.3: a batch is incomplete until every chunk is on-chain.
        let mut t = PalwBatchChunkTracker::new(3);
        assert!(!t.is_complete() && t.missing_count() == 3);
        assert!(t.record(0) && t.record(2));
        assert!(!t.record(2)); // duplicate
        assert!(!t.record(5)); // out of range
        assert!(!t.is_complete() && t.missing_count() == 1);
        assert!(t.record(1));
        assert!(t.is_complete() && t.missing_count() == 0);

        // §18 (I-6): DA bundle needs manifest + all chunks + cert + beacon.
        assert!(palw_da_bundle_complete(true, &t, true, true));
        assert!(!palw_da_bundle_complete(false, &t, true, true)); // manifest missing
        let incomplete = PalwBatchChunkTracker::new(2); // 0 chunks recorded
        assert!(!palw_da_bundle_complete(true, &incomplete, true, true)); // chunks missing

        // §22: the template picks the first Active ticket that WINS the draw (lenient bits ⇒ zero digest wins).
        let bits = 0x2100ffff_u32;
        let losing = PalwTemplateCandidate { eligibility_digest: Hash64::from_bytes([0xff; 64]), nonce: 0, ticket_nullifier: h(1) };
        let nf = h(2);
        let winner = PalwTemplateCandidate {
            eligibility_digest: Hash64::from_bytes([0u8; 64]),
            nonce: u64::from_le_bytes([2u8; 8]), // low64(h(2))
            ticket_nullifier: nf,
        };
        // At a tight target the "losing" digest (all-ones) misses, and the second candidate wins.
        assert_eq!(palw_select_template_ticket(&[losing.clone(), winner.clone()], 0x1c00ffff), Some(1));
        // No winner ⇒ None (⇒ mine algo-3 instead).
        let only_losers = [PalwTemplateCandidate { nonce: 999, ..losing.clone() }];
        assert_eq!(palw_select_template_ticket(&only_losers, 0x1c00ffff), None);
        // With lenient bits the first candidate (zero digest, matching nonce) wins.
        let first = PalwTemplateCandidate {
            eligibility_digest: Hash64::from_bytes([0u8; 64]),
            nonce: u64::from_le_bytes([1u8; 8]),
            ticket_nullifier: h(1),
        };
        assert_eq!(palw_select_template_ticket(&[first, winner], bits), Some(0));
    }

    #[test]
    fn palw_ticket_verify_predicate() {
        let nf = h(11);
        let nonce = u64::from_le_bytes([11u8; 8]); // low64(nf), since nf == [11; 64]
        let dig = Hash64::from_bytes([0u8; 64]); // Uint512 = 0 ⇒ wins any target
        let bits = 0x2100ffff_u32; // very high target
        let binding = PalwTicketBinding {
            // I-13: the leaf stores the COMMITMENT; verify opens it with the header's raw `nf`.
            ticket_nullifier_commitment: ticket_nullifier_commitment(&nf),
            proof_type: 1,
            leaf_activation_epoch: 7,
            leaf_expiry_epoch: 13,
            target_daa_interval: 100,
        };
        let cc = h(20);
        // Happy path: every rule satisfied.
        assert_eq!(verify_palw_ticket(&nf, 1, &cc, bits, nonce, 100, &dig, &binding, true, 10, &cc, bits, true), Ok(()));
        // Each rule, one violation at a time.
        assert_eq!(
            verify_palw_ticket(&h(99), 1, &cc, bits, nonce, 100, &dig, &binding, true, 10, &cc, bits, true),
            Err(PalwTicketReject::NullifierMismatch)
        );
        assert_eq!(
            verify_palw_ticket(&nf, 2, &cc, bits, nonce, 100, &dig, &binding, true, 10, &cc, bits, true),
            Err(PalwTicketReject::ProofTypeMismatch)
        );
        assert_eq!(
            verify_palw_ticket(&nf, 1, &cc, bits, nonce, 100, &dig, &binding, true, 6, &cc, bits, true),
            Err(PalwTicketReject::LeafNotActive)
        );
        assert_eq!(
            verify_palw_ticket(&nf, 1, &cc, bits, nonce, 100, &dig, &binding, false, 10, &cc, bits, true),
            Err(PalwTicketReject::CertNotActive)
        );
        assert_eq!(
            verify_palw_ticket(&nf, 1, &cc, bits, nonce, 999, &dig, &binding, true, 10, &cc, bits, true),
            Err(PalwTicketReject::IntervalMismatch)
        );
        assert_eq!(
            verify_palw_ticket(&nf, 1, &h(21), bits, nonce, 100, &dig, &binding, true, 10, &cc, bits, true),
            Err(PalwTicketReject::ChainCommitMismatch)
        );
        assert_eq!(
            verify_palw_ticket(&nf, 1, &cc, 0x1d00ffff, nonce, 100, &dig, &binding, true, 10, &cc, bits, true),
            Err(PalwTicketReject::LaneBitsMismatch)
        );
        assert_eq!(
            verify_palw_ticket(&nf, 1, &cc, bits, nonce, 100, &dig, &binding, true, 10, &cc, bits, false),
            Err(PalwTicketReject::ComputeCapExhausted)
        );
        // wrong nonce (not low64(nullifier)) ⇒ draw not won.
        assert_eq!(
            verify_palw_ticket(&nf, 1, &cc, bits, nonce ^ 1, 100, &dig, &binding, true, 10, &cc, bits, true),
            Err(PalwTicketReject::EligibilityMiss)
        );
        // a losing digest (all-ones ⇒ max Uint512) misses even with the right nonce, at a tight target.
        let tight = 0x1c00ffff_u32;
        let losing = Hash64::from_bytes([0xff; 64]);
        assert_eq!(
            verify_palw_ticket(&nf, 1, &cc, tight, nonce, 100, &losing, &binding, true, 10, &cc, tight, true),
            Err(PalwTicketReject::EligibilityMiss)
        );
    }

    #[test]
    fn beacon_fallback_and_degraded_mode() {
        // fallback seed is distinct from the primary seed and depends on the lagged window.
        let f1 = beacon_fallback_seed(&h(1), &h(2), &h(3), 9);
        let f2 = beacon_fallback_seed(&h(1), &h(2), &h(9), 9);
        assert_ne!(f1, f2);
        assert_ne!(f1, beacon_seed(&h(1), &h(2), &h(3), &h(0), 9));
        // mode transitions: healthy → grace (within window) → halted (past window).
        assert_eq!(beacon_mode(true, true, 0, 1), PalwBeaconMode::Healthy);
        assert_eq!(beacon_mode(false, true, 1, 1), PalwBeaconMode::DegradedGrace);
        assert_eq!(beacon_mode(true, false, 1, 1), PalwBeaconMode::DegradedGrace);
        assert_eq!(beacon_mode(false, false, 2, 1), PalwBeaconMode::Halted);

        // K5 fallback policy: full weight healthy, halved in grace, zero when halted; monotone.
        assert_eq!(effective_compute_work_scale(8, PalwBeaconMode::Healthy), 8);
        assert_eq!(effective_compute_work_scale(8, PalwBeaconMode::DegradedGrace), 4);
        assert_eq!(effective_compute_work_scale(8, PalwBeaconMode::Halted), 0);
        for base in [0u64, 1, 2, 3, 40] {
            let (hh, dg, ht) = (
                effective_compute_work_scale(base, PalwBeaconMode::Healthy),
                effective_compute_work_scale(base, PalwBeaconMode::DegradedGrace),
                effective_compute_work_scale(base, PalwBeaconMode::Halted),
            );
            assert!(hh >= dg && dg >= ht, "monotone Healthy>=DegradedGrace>=Halted for base {base}");
        }
    }

    /// K5 (§11.3): halted_since closed form, the lagged buried seed-carry run, and the activation gate.
    #[test]
    fn k5_halt_signal_and_gates() {
        // halted_since = epoch + grace + 1 - degraded_epochs, only when mode == Halted.
        let grace = 3u64;
        let state = |epoch: u64, mode: PalwBeaconMode, degraded: u64| PalwBeaconStateV1 {
            version: 1,
            closed_seeds: Vec::new(),
            prev_closer: None,
            epoch,
            seed: h(0),
            dns_anchor: h(0),
            anchor_blue_score: 0,
            anchor_daa_score: 0,
            anchor_overlay_root: h(0),
            valid_reveals_root: h(0),
            missing_commitments_root: h(0),
            mode: mode.to_u8(),
            degraded_epochs: degraded,
            valid_reveal_count: 0,
            missing_commit_count: 0,
        };
        // first Halted epoch: degraded == grace+1 at epoch E ⇒ halted_since == E (the run just crossed).
        assert_eq!(state(100, PalwBeaconMode::Halted, grace + 1).halted_since(grace), Some(100));
        // deep halt: degraded == grace+5 at epoch 104 ⇒ run started at 104+3+1-8 = 100.
        assert_eq!(state(104, PalwBeaconMode::Halted, grace + 5).halted_since(grace), Some(100));
        // not Halted ⇒ None (grace-epoch and healthy blocks are never classified halted).
        assert_eq!(state(100, PalwBeaconMode::DegradedGrace, grace).halted_since(grace), None);
        assert_eq!(state(100, PalwBeaconMode::Healthy, 0).halted_since(grace), None);
        // from_u8 round-trips the discriminant.
        for m in [PalwBeaconMode::Healthy, PalwBeaconMode::DegradedGrace, PalwBeaconMode::Halted] {
            assert_eq!(PalwBeaconMode::from_u8(m.to_u8()), Some(m));
        }
        assert_eq!(PalwBeaconMode::from_u8(9), None);

        // seed-carry run: longest equal suffix ending at the newest sample, measured in epochs.
        assert_eq!(palw_seed_carry_run(&[(1, h(1)), (2, h(1)), (3, h(1))]), 2); // 3 - 1
        assert_eq!(palw_seed_carry_run(&[(1, h(1)), (2, h(2)), (3, h(2))]), 1); // 3 - 2 (Healthy advance at 2)
        assert_eq!(palw_seed_carry_run(&[(5, h(9))]), 0); // < 2 samples ⇒ fail-open 0
        assert_eq!(palw_seed_carry_run(&[]), 0);
        // consecutive equal seeds a Healthy advance breaks: a run of 2 needs grace >= 2 to survive clause 10.
        assert!(palw_seed_carry_run(&[(1, h(7)), (2, h(7)), (3, h(7))]) > 2u64.saturating_sub(1));

        // activation gate: the two newest distinct-epoch seeds must DIFFER (a recent Healthy advance).
        assert!(palw_lagged_activation_open(&[(1, h(1)), (2, h(2))])); // advance ⇒ open
        assert!(!palw_lagged_activation_open(&[(1, h(1)), (2, h(1))])); // carry ⇒ frozen
        assert!(!palw_lagged_activation_open(&[(1, h(1))])); // < 2 ⇒ fail-closed
        assert!(!palw_lagged_activation_open(&[]));

        // template lane-open c==v twin: both conjuncts (not Halted AND carry <= grace).
        assert!(palw_template_lane_open(PalwBeaconMode::Healthy.to_u8(), 0, grace));
        assert!(palw_template_lane_open(PalwBeaconMode::DegradedGrace.to_u8(), grace, grace)); // grace-run still open
        assert!(!palw_template_lane_open(PalwBeaconMode::Halted.to_u8(), 0, grace)); // own mode Halted
        assert!(!palw_template_lane_open(PalwBeaconMode::Healthy.to_u8(), grace + 1, grace));
        // post-recovery lag
    }

    /// K5 (§9.5/§11.3): advance_epoch_gated freezes Certified→Active while the gate is closed, but never
    /// cancels — a frozen batch activates on a later open call and still expires on time.
    #[test]
    fn k5_advance_epoch_gated_freeze() {
        let mk = || {
            let mut v = PalwBatchViewV1::new();
            v.batches.insert(
                h(0x42),
                PalwBatchLifecycleV1 {
                    status: PalwBatchStatus::Certified,
                    registration_epoch: 0,
                    activation_not_before_epoch: 5,
                    expiry_epoch: 20,
                    leaf_count: 1,
                    chunk_count: 1,
                    chunks_present: [1, 0, 0, 0],
                    leaf_root: h(0),
                    cert_hash: Some(h(1)),
                    sponsor_tag_hi: 0,
                    sponsor_tag_lo: 100,
                    cert_approving_stake: 0,
                    first_cert_daa: None,
                    revoked_from_daa: None,
                },
            );
            v
        };
        // gate CLOSED at the activation epoch ⇒ stays Certified (frozen, not cancelled).
        let mut frozen = mk();
        frozen.advance_epoch_gated(6, 2, 6, false);
        assert_eq!(frozen.batches[&h(0x42)].status, PalwBatchStatus::Certified);
        // a later OPEN call activates it.
        frozen.advance_epoch_gated(7, 2, 6, true);
        assert_eq!(frozen.batches[&h(0x42)].status, PalwBatchStatus::Active);
        // gate OPEN at the activation epoch ⇒ activates immediately (== the un-gated advance_epoch).
        let mut open = mk();
        open.advance_epoch(6, 2, 6);
        assert_eq!(open.batches[&h(0x42)].status, PalwBatchStatus::Active);
        // frozen past expiry ⇒ Expired even while the gate is closed (the gate delays activation, not expiry).
        let mut expired = mk();
        expired.advance_epoch_gated(20, 2, 6, false);
        assert_eq!(expired.batches[&h(0x42)].status, PalwBatchStatus::Expired);
    }

    /// §11.2 roots: order-independent + collision-free across cardinality, and the two domains disjoint.
    #[test]
    fn beacon_set_roots_canonical() {
        let a = (op(0x50, 0), h(11));
        let b = (op(0x51, 2), h(12));
        // same set in two orders ⇒ identical root (canonical sort inside).
        assert_eq!(beacon_valid_reveals_root(&[a, b]), beacon_valid_reveals_root(&[b, a]));
        // a different member changes the root.
        assert_ne!(beacon_valid_reveals_root(&[a, b]), beacon_valid_reveals_root(&[a]));
        // same entries, different domain (valid vs missing) ⇒ disjoint roots.
        assert_ne!(beacon_valid_reveals_root(&[a, b]), beacon_missing_commitments_root(&[a, b]));
        // empty set is a fixed non-panicking digest.
        assert_eq!(beacon_valid_reveals_root(&[]), beacon_valid_reveals_root(&[]));
    }

    /// §11.2 stake-weighted quorum: revealed stake must reach num/den of committed stake.
    #[test]
    fn beacon_quorum_stake_weighted() {
        let committed = vec![op(0x60, 0), op(0x60, 1), op(0x60, 2)];
        let stake_of = |o: &TransactionOutpoint| -> u128 {
            match o.index {
                0 => 30,
                1 => 30,
                2 => 40,
                _ => 0,
            }
        };
        // reveals 0+1 = 60 of committed 100 at 2/3 (66.67) ⇒ NOT reached.
        assert!(!beacon_quorum_reached(&committed, &[op(0x60, 0), op(0x60, 1)], 2, 3, stake_of));
        // + reveal 2 = 100 ⇒ reached.
        assert!(beacon_quorum_reached(&committed, &committed, 2, 3, stake_of));
        // den==0 ⇒ never reached.
        assert!(!beacon_quorum_reached(&committed, &committed, 1, 0, stake_of));
        // §11.3: empty committed set (total dropout) is NOT reached (degraded, not vacuously Healthy).
        assert!(!beacon_quorum_reached(&[], &[], 2, 3, stake_of));
        // ADR-0040 QUORUM-02: num==0 is the mirror vacuity of den==0 — the RHS becomes 0, so without the
        // guard an EMPTY reveal set would "reach" a 0/3 quorum. Fail-closed on both.
        assert!(!beacon_quorum_reached(&committed, &[], 0, 3, stake_of));
        assert!(!beacon_quorum_reached(&committed, &committed, 0, 3, stake_of));
    }

    /// §11.2/§11.3 derive: Healthy advances the seed via beacon_seed and resets grace; a short quorum
    /// inside the grace window carries the previous seed and increments the grace counter; the missing
    /// set is commits minus valid reveals.
    #[test]
    fn beacon_derive_epoch_state() {
        let unit = |_: &TransactionOutpoint| -> u128 { 1 };
        let commits = vec![(op(0x70, 0), h(21)), (op(0x70, 1), h(22))];
        let reveals = vec![
            (commits[0].0, beacon_reveal_entropy_digest(9, &[0x31; 64], &commits[0].0)),
            (commits[1].0, beacon_reveal_entropy_digest(9, &[0x32; 64], &commits[1].0)),
        ];
        // both revealed ⇒ Healthy (2/2 >= 2/3), seed advances, missing empty.
        let anchor = BeaconDnsAnchor { hash: h(2), blue_score: 77, daa_score: 88, overlay_root: h(9) };
        let inputs = BeaconEpochInputs { commits: commits.clone(), valid_reveals: reveals.clone() };
        assert_eq!(inputs.missing_commitments().len(), 0);
        let st = derive_beacon_epoch_state(9, &h(1), &anchor, &inputs, true, 3, 2, 2, 3, unit);
        assert_eq!(st.mode, PalwBeaconMode::Healthy.to_u8());
        assert_eq!(st.degraded_epochs, 0); // reset on Healthy
        assert_eq!(st.valid_reveal_count, 2);
        assert_eq!(st.missing_commit_count, 0);
        // the seed preimage folds only the anchor HASH; the cert facts ride alongside in the record.
        assert_eq!(st.seed, beacon_seed(&h(1), &h(2), &st.valid_reveals_root, &st.missing_commitments_root, 9));
        assert_eq!(st.anchor(), anchor);
        assert_eq!(st.dns_certificate_hash(), Some(dns_finality_certificate_hash_v1(&anchor)));

        // only 1 of 2 reveals ⇒ quorum short (1/2 < 2/3), still inside grace (prev 0, grace 2) ⇒
        // DegradedGrace carries the previous seed and increments the counter; missing = {bond 1}.
        let inputs2 = BeaconEpochInputs { commits: commits.clone(), valid_reveals: vec![reveals[0]] };
        assert_eq!(inputs2.missing_commitments(), vec![commits[1]]);
        let st2 = derive_beacon_epoch_state(10, &h(5), &anchor, &inputs2, true, 0, 2, 2, 3, unit);
        assert_eq!(st2.mode, PalwBeaconMode::DegradedGrace.to_u8());
        assert_eq!(st2.degraded_epochs, 1);
        assert_eq!(st2.seed, h(5)); // carried, NOT advanced
        assert_eq!(st2.missing_commit_count, 1);

        // grace exhausted (prev 2, grace 2 ⇒ 3 > 2) ⇒ Halted, still carries the seed.
        let st3 = derive_beacon_epoch_state(11, &h(7), &anchor, &inputs2, false, 2, 2, 2, 3, unit);
        assert_eq!(st3.mode, PalwBeaconMode::Halted.to_u8());
        assert_eq!(st3.seed, h(7));

        // bootstrap: an UNCONFIRMED anchor derives a record with NO certificate (fail-closed).
        let st0 = derive_beacon_epoch_state(1, &h(0), &BeaconDnsAnchor::UNCONFIRMED, &inputs, false, 0, 2, 2, 3, unit);
        assert_eq!(st0.dns_certificate_hash(), None);
    }

    /// V2 binds both the beacon-selected sample and the exact certificate-level summary. Changing any
    /// of them changes the digest, so an assembler cannot repackage a signed vote under another result.
    #[test]
    fn auditor_vote_v2_binds_sample_and_certificate_summary() {
        let vote = PalwAuditorVoteV2 {
            bond_outpoint: op(0x40, 0),
            vote: 1,
            checked_leaf_bitmap_root: h(5),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(6),
            signature: vec![],
        };
        let base = vote.signing_hash(0x9107, &h(1), 6, &h(2));
        assert_eq!(
            base.to_string(),
            "cae1dbed453578ecf4dae6d395aeda643caacc5f2d2ce9d07c13ae059edea12b67c1679a704c6626e87d451fb09ca7ff94fec04826a7bd88bf2f8ec8f76fd05c",
            "V2 signing preimage/domain changed; this is a consensus wire break and requires a new version/domain"
        );
        assert_eq!(base, vote.signing_hash(0x9107, &h(1), 6, &h(2)), "deterministic");
        // the beacon-selected sample root is bound: a different sample ⇒ a different message.
        assert_ne!(base, vote.signing_hash(0x9107, &h(1), 6, &h(0xaa)), "audit_sample_root is covered");
        assert_ne!(base, vote.signing_hash(0x9107, &h(1), 7, &h(2)), "audit_beacon_epoch is covered");
        assert_ne!(base, vote.signing_hash(0x9107, &h(0xbb), 6, &h(2)), "batch_id is covered");
        // the verdict + identity are covered.
        let reject = PalwAuditorVoteV2 { vote: 0, ..vote.clone() };
        assert_ne!(base, reject.signing_hash(0x9107, &h(1), 6, &h(2)), "vote is covered");
        let other = PalwAuditorVoteV2 { bond_outpoint: op(0x40, 1), ..vote.clone() };
        assert_ne!(base, other.signing_hash(0x9107, &h(1), 6, &h(2)), "auditor identity is covered");
        let other_pass_count = PalwAuditorVoteV2 { passed_leaf_count: 1, ..vote.clone() };
        assert_ne!(base, other_pass_count.signing_hash(0x9107, &h(1), 6, &h(2)), "passed_leaf_count is covered");
        let other_rejected_root = PalwAuditorVoteV2 { rejected_leaf_bitmap_root: h(0xcc), ..vote.clone() };
        assert_ne!(base, other_rejected_root.signing_hash(0x9107, &h(1), 6, &h(2)), "rejected_leaf_bitmap_root is covered");
    }

    /// I-13 winner secrecy: the leaf's commitment is a deterministic one-way function of the nullifier;
    /// the store-facts verify opens it with the header's DISCLOSED raw nullifier, and a wrong disclosure
    /// (a third party guessing) is rejected. This is what makes a ticket's eligibility uncomputable by
    /// anyone but its holder.
    #[test]
    fn i13_nullifier_commitment_binds_disclosure() {
        let nf = h(0x42);
        let commitment = ticket_nullifier_commitment(&nf);
        assert_eq!(commitment, ticket_nullifier_commitment(&nf), "deterministic");
        assert_ne!(commitment, ticket_nullifier_commitment(&h(0x43)), "distinct nullifiers ⇒ distinct commitments");
        assert_ne!(commitment, nf, "the commitment is not the raw nullifier (one-way)");

        // a binding built from the leaf's commitment: the correct disclosure opens it, a wrong one is a
        // NullifierMismatch (so a third party who only read the public leaf cannot forge the header).
        let binding = PalwTicketBinding {
            ticket_nullifier_commitment: commitment,
            proof_type: 1,
            leaf_activation_epoch: 0,
            leaf_expiry_epoch: 100,
            target_daa_interval: 42,
        };
        assert!(verify_palw_ticket_store_facts(&nf, 1, 42, &binding, true, 10).is_ok());
        assert_eq!(verify_palw_ticket_store_facts(&h(0x43), 1, 42, &binding, true, 10), Err(PalwTicketReject::NullifierMismatch));
    }

    /// kaspa-pq **ADR-0040 §5.15.13 (gate G16)** — the paid-work walk bound is DERIVED from what
    /// `admission_valid` actually enforces, not asserted alongside it.
    ///
    /// The reward-coordinate duplicate-work rule is only sound because a batch's whole referenceable
    /// life fits inside `max_batch_life_epochs`. If a future params edit or an `admission_valid`
    /// relaxation let a manifest outlive that number, the bounded walk would silently start missing
    /// paid nullifiers — a wrong reward set, with nothing failing. So the bound is checked by SEARCH
    /// over admissible manifests rather than by restating the formula:
    ///
    /// * every manifest `admission_valid` ACCEPTS must satisfy `expiry - registration <= bound`;
    /// * and the bound must be TIGHT — some accepted manifest must actually reach it, or the constant
    ///   is loose and the test would keep passing after the derivation drifted.
    #[test]
    fn palw_batch_life_bound_is_what_admission_enforces_and_is_tight() {
        let a = PalwBatchAdmissionParams::INERT;
        let (lead, audit, active) = (a.registration_lead_epochs, a.audit_window_epochs, a.active_window_epochs);
        let bound = a.max_batch_life_epochs();
        let reg = 5u64;

        let mk = |act: u64, exp: u64| {
            let mut m = PalwBatchManifestV1 {
                version: 1,
                batch_id: h(0),
                registration_epoch: reg,
                model_profile_id: h(3),
                runtime_class_id: h(4),
                leaf_count: 100,
                chunk_count: 100u32.div_ceil(PALW_MAX_LEAVES_PER_CHUNK as u32) as u16,
                leaf_root: palw_leaf_merkle_root(&[h(1), h(2)]),
                descriptor_root: h(6),
                total_leaf_bond_sompi: 0,
                audit_policy_id: h(7),
                activation_not_before_epoch: act,
                expiry_epoch: exp,
            };
            m.batch_id = m.content_id();
            m
        };
        let admissible =
            |m: &PalwBatchManifestV1| m.admission_valid(reg, a.max_batch_leaves, a.max_leaf_chunk_leaves, lead, active, audit, 0);

        // EXHAUSTIVE over the whole legal neighbourhood, plus a margin on both sides so the search
        // actually straddles the boundary instead of stopping at it.
        let min_activation = reg + lead + audit;
        let mut tight = false;
        for act in 0..=(min_activation + 2 * lead + 4) {
            for exp in 0..=(act + active + 4) {
                let m = mk(act, exp);
                if !admissible(&m) {
                    continue;
                }
                let life = m.expiry_epoch - m.registration_epoch;
                assert!(
                    life <= bound,
                    "admission accepted a manifest living {life} epochs, but max_batch_life_epochs() says {bound}. \
                     The G16 paid-work walk would miss payouts older than the bound — either the bound is wrong \
                     or admission was relaxed without it."
                );
                tight |= life == bound;
            }
        }
        assert!(tight, "no admissible manifest reaches max_batch_life_epochs() — the bound is loose, so it proves nothing");

        // The two enforced clauses the bound composes, each shown to actually bite: one epoch past the
        // activation slack, and one epoch past the active window, are both REFUSED.
        assert!(admissible(&mk(min_activation + lead, min_activation + lead + active)), "the extremal manifest is admissible");
        assert!(!admissible(&mk(min_activation + lead + 1, min_activation + lead + 1 + active)), "activation slack must bite");
        assert!(!admissible(&mk(min_activation + lead, min_activation + lead + active + 1)), "active window must bite");

        // And the DAA-space conversion is a faithful widening of the epoch bound (never narrower).
        let epoch_len = 100u64;
        assert!(a.paid_work_walk_bound_daa(epoch_len) > bound * epoch_len, "the DAA bound must cover epoch truncation");
    }

    /// §9.2/§9.3/§18.2 C4 content-addressing + view: batch_id must be content-derived; a manifest with a
    /// forged batch_id or an unbounded expiry is inadmissible; leaf_root reduces the ordered leaves; the
    /// compact view gates referenceability + block-eligibility and retains only the reachable set.
    #[test]
    fn c4_content_address_admission_and_view() {
        // leaf_root reduction is order-sensitive + count-prefixed.
        let (la, lb) = (h(1), h(2));
        assert_ne!(palw_leaf_merkle_root(&[la, lb]), palw_leaf_merkle_root(&[lb, la]));
        assert_ne!(palw_leaf_merkle_root(&[la]), palw_leaf_merkle_root(&[la, lb]));

        // build a content-addressed, admissible manifest (registration_epoch = accept_epoch, bounded
        // activation/expiry). max_batch_leaves 256, chunk 64, lead 2, active 6, audit 6.
        let mut m = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(0),
            registration_epoch: 5,
            model_profile_id: h(3),
            runtime_class_id: h(4),
            leaf_count: 100,
            chunk_count: 2,
            leaf_root: palw_leaf_merkle_root(&[la, lb]),
            descriptor_root: h(6),
            total_leaf_bond_sompi: 0,
            audit_policy_id: h(7),
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
        };
        m.batch_id = m.content_id();
        assert!(m.batch_id_is_content_derived());
        assert!(m.admission_valid(5, 256, 64, 2, 6, 6, 0), "well-formed manifest is admissible");

        // ADR-0040 P1-11 (DOS-04): activation is bounded from ABOVE too. min_activation = 5+2+6 = 13 and
        // the slack is one lead window (2), so 13..=15 is admissible and 16 is not. Without the upper
        // bound a manifest could name a far-future activation and pin its per-block-cloned view entry
        // effectively forever — bounding only `expiry` relative to activation does not prevent that.
        for act in 13..=15u64 {
            let mut ok = PalwBatchManifestV1 { activation_not_before_epoch: act, expiry_epoch: act + 6, ..m.clone() };
            ok.batch_id = ok.content_id();
            assert!(ok.admission_valid(5, 256, 64, 2, 6, 6, 0), "activation {act} is within the scheduling slack");
        }
        let mut far = PalwBatchManifestV1 { activation_not_before_epoch: 16, expiry_epoch: 22, ..m.clone() };
        far.batch_id = far.content_id();
        assert!(!far.admission_valid(5, 256, 64, 2, 6, 6, 0), "activation beyond the slack must be inadmissible");
        let mut pinned =
            PalwBatchManifestV1 { activation_not_before_epoch: u64::MAX / 2, expiry_epoch: u64::MAX / 2 + 6, ..m.clone() };
        pinned.batch_id = pinned.content_id();
        assert!(!pinned.admission_valid(5, 256, 64, 2, 6, 6, 0), "a far-future activation must never pin the view");

        // forged batch_id ⇒ inadmissible (content-address broken).
        let mut forged = m.clone();
        forged.batch_id = h(0xff);
        assert!(!forged.batch_id_is_content_derived());
        assert!(!forged.admission_valid(5, 256, 64, 2, 6, 6, 0));

        // unbounded expiry ⇒ inadmissible (would pin the view forever). Re-content-address after edit.
        let mut evil = PalwBatchManifestV1 { expiry_epoch: u64::MAX, ..m.clone() };
        evil.batch_id = evil.content_id();
        assert!(!evil.admission_valid(5, 256, 64, 2, 6, 6, 0));
        // wrong registration epoch (miner re-aim) ⇒ inadmissible.
        assert!(!m.admission_valid(6, 256, 64, 2, 6, 6, 0));
        // chunk_count must be exactly ceil(100/64)=2.
        let mut badchunks = PalwBatchManifestV1 { chunk_count: 3, ..m.clone() };
        badchunks.batch_id = badchunks.content_id();
        assert!(!badchunks.admission_valid(5, 256, 64, 2, 6, 6, 0));

        // R3 (§7 c_saved floor): with a nonzero per-leaf floor, a manifest whose aggregate bond does not
        // cover leaf_count·floor is inadmissible; exactly-covering is admissible. m has leaf_count=100.
        let mut bonded = PalwBatchManifestV1 { total_leaf_bond_sompi: 100 * 50, ..m.clone() };
        bonded.batch_id = bonded.content_id();
        assert!(bonded.admission_valid(5, 256, 64, 2, 6, 6, 50), "aggregate bond exactly covers the floor");
        assert!(!bonded.admission_valid(5, 256, 64, 2, 6, 6, 51), "one sompi short per leaf ⇒ inadmissible");
        // the original (bond 0) is inadmissible the moment a floor exists.
        assert!(!m.admission_valid(5, 256, 64, 2, 6, 6, 1));

        // the compact view: an Active batch inside its windows is resolvable; an Expired / revoked /
        // out-of-window one is not; retain drops the unreachable.
        let entry = |status, revoked| PalwBatchLifecycleV1 {
            status,
            registration_epoch: 5,
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
            leaf_count: 100,
            chunk_count: 2,
            chunks_present: [0b11, 0, 0, 0],
            leaf_root: m.leaf_root,
            cert_hash: Some(h(9)),
            sponsor_tag_hi: 13,
            sponsor_tag_lo: 19,
            cert_approving_stake: 0,
            first_cert_daa: None,
            revoked_from_daa: revoked,
        };
        let mut view = PalwBatchViewV1::new();
        view.batches.insert(m.batch_id, entry(PalwBatchStatus::Active, None));
        assert!(view.resolvable_batch(&m.batch_id, 15, 0).is_some(), "Active + in-window ⇒ resolvable");
        assert!(view.resolvable_batch(&m.batch_id, 19, 0).is_none(), "at expiry ⇒ not resolvable");
        assert!(view.resolvable_batch(&h(0xaa), 15, 0).is_none(), "absent batch ⇒ None");
        // ADR-0040 SS-04: revocation is non-retroactive, so both the eligibility gate and the eviction
        // are keyed on the block's DAA. These assertions changed with the rule — they previously demanded
        // "never resolvable / always dropped", which was the retroactive behaviour §9.5 forbids.
        view.batches.insert(h(0xbb), entry(PalwBatchStatus::Active, Some(900)));
        assert!(view.resolvable_batch(&h(0xbb), 15, 899).is_some(), "below the revocation cutoff ⇒ still resolvable");
        assert!(view.resolvable_batch(&h(0xbb), 15, 900).is_none(), "at/above the cutoff ⇒ revoked");
        view.retain(15, 899, 2, 6);
        assert!(view.entry(&h(0xbb)).is_some(), "a not-yet-effective revocation must NOT evict the batch");
        view.retain(15, 900, 2, 6);
        assert!(view.entry(&h(0xbb)).is_none(), "revoked batch dropped once the revocation is in effect");
        assert!(view.entry(&m.batch_id).is_some(), "in-window Active kept");
        // past expiry, retain drops the Active batch too.
        view.retain(25, 900, 2, 6);
        assert!(view.entry(&m.batch_id).is_none());
    }

    /// Verifies that leaf content does not affect the block-keyed batch view.
    ///
    /// Two mergesets that differ only in leaf `job_nullifier` values must fold to identical views and
    /// the same Registering → Committed transition.
    #[test]
    fn leaf_chunk_fold_is_independent_of_leaf_content() {
        let mut m = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(0),
            registration_epoch: 5,
            model_profile_id: h(3),
            runtime_class_id: h(4),
            leaf_count: 100,
            chunk_count: 2,
            leaf_root: h(8),
            descriptor_root: h(6),
            total_leaf_bond_sompi: 0,
            audit_policy_id: h(7),
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
        };
        m.batch_id = m.content_id();

        // Replays the body-processor's mergeset fold arm for one manifest + its leaf chunks.
        let fold = |chunks: &[PalwLeafChunkV1]| {
            let mut v = PalwBatchViewV1::new();
            v.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT);
            for c in chunks {
                v.apply_leaf_chunk(&c.batch_id, c.chunk_index);
            }
            v
        };
        // `proofs` is deliberately empty: this test drives ONLY the body-coordinate fold
        // (`apply_leaf_chunk`), which takes `(batch_id, chunk_index)` and never looks at leaves. ADR-0040
        // §5.15.4 keeps that coordinate byte-for-byte unchanged; membership proofs are consumed at the
        // ACCEPTANCE coordinate. A chunk with empty proofs would be rejected by `validate_leaf_chunk`,
        // which this test does not call — and must not start calling, or it stops testing the fold.
        let chunk = |chunk_index: u16, nullifiers: &[u8]| PalwLeafChunkV1 {
            version: PALW_LEAF_CHUNK_VERSION_V2,
            proofs: Vec::new(),
            witnesses: Vec::new(),
            batch_id: m.batch_id,
            chunk_index,
            leaves: nullifiers
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let mut leaf = sample_leaf();
                    leaf.batch_id = m.batch_id;
                    leaf.leaf_index = i as u32;
                    leaf.job_nullifier = h(*n);
                    leaf
                })
                .collect(),
        };

        let distinct = fold(&[chunk(0, &[1, 2, 3]), chunk(1, &[4, 5, 6])]);
        // Repeated nullifiers within and across chunks, plus one present under a foreign batch.
        let duplicated = fold(&[chunk(0, &[7, 7, 7]), chunk(1, &[7, 1, 7])]);
        assert_eq!(borsh::to_vec(&distinct).unwrap(), borsh::to_vec(&duplicated).unwrap(), "leaf content must not move the view");

        // Leaf content does not affect chunk application or promotion.
        let e = distinct.entry(&m.batch_id).unwrap();
        assert_eq!(e.status, PalwBatchStatus::Committed, "popcount == chunk_count ⇒ Registering → Committed");
        assert_eq!(e.chunks_present, [0b11, 0, 0, 0]);

        // A refused chunk is a no-op on the view, so a duplicated/out-of-range/unknown chunk cannot
        // deny an honest batch its promotion (the brick the armed P1-9 rejection would have created).
        let noisy = fold(&[chunk(0, &[1]), chunk(0, &[2]), chunk(9, &[3]), chunk(1, &[4])]);
        assert_eq!(borsh::to_vec(&noisy).unwrap(), borsh::to_vec(&distinct).unwrap());
        let mut foreign = chunk(0, &[1]);
        foreign.batch_id = h(0xfe);
        let mut with_foreign = fold(&[chunk(0, &[1]), chunk(1, &[2])]);
        assert!(!with_foreign.apply_leaf_chunk(&foreign.batch_id, foreign.chunk_index), "unknown batch refused");
        assert_eq!(borsh::to_vec(&with_foreign).unwrap(), borsh::to_vec(&distinct).unwrap());
        // A non-Registering batch refuses further chunks.
        assert!(!with_foreign.apply_leaf_chunk(&m.batch_id, 0), "non-Registering batch refused");
    }

    /// §9.5 B-way delta application: manifest → Registering (admission-gated, idempotent), chunks →
    /// Committed on completeness, audit-beacon epoch → Auditing, certificate → Certified, activation
    /// epoch → Active; revocation + expiry.
    /// Static-audit finding H-01 — the sponsorship gate and its quota, both sides of the flag day.
    ///
    /// H-01: registering a batch was free. `total_leaf_bond_sompi` is a self-declared u64 bound to no
    /// UTXO, so raising `min_leaf_bond_sompi` only made attackers write a bigger number; with 1,024
    /// slots, no eviction and no fair queue, anyone who could get transactions mined could hold the
    /// whole view and censor honest batches until they expired.
    #[test]
    fn manifest_sponsorship_gate_and_quota() {
        let mk = |tag: u8| {
            let mut m = PalwBatchManifestV1 {
                version: 1,
                batch_id: Hash64::default(),
                registration_epoch: 5,
                model_profile_id: h(1),
                runtime_class_id: h(2),
                leaf_count: 1,
                chunk_count: 1,
                leaf_root: h(tag),
                descriptor_root: h(4),
                total_leaf_bond_sompi: 0,
                audit_policy_id: h(5),
                activation_not_before_epoch: 13,
                expiry_epoch: 19,
            };
            m.batch_id = m.content_id();
            m
        };
        let sponsored =
            |tag: (u64, u64), quota: u32| PalwManifestSponsorshipV1 { tag, max_view_batches_per_sponsor: quota, required: true };
        let alice = (0x1111u64, 0x2222u64);
        let bob = (0x3333u64, 0x4444u64);

        // BELOW the flag day nothing changes: an unsponsored manifest is admitted exactly as before,
        // and the row it writes is the "unattributed" one every pre-fence row on disk already is.
        let mut pre = PalwBatchViewV1::new();
        assert!(pre.apply_manifest(&mk(0x10), 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
        assert_eq!(pre.entry(&mk(0x10).batch_id).unwrap().sponsor_tag(), PALW_SPONSOR_UNATTRIBUTED);

        // PAST the flag day an unsponsored manifest takes no slot. This is THE H-01 fix: the flood
        // that cost transaction fees now costs bonded collateral, or it buys nothing.
        let mut post = PalwBatchViewV1::new();
        assert!(
            !post.apply_manifest(&mk(0x11), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(PALW_SPONSOR_UNATTRIBUTED, 4)),
            "past the fence an unsponsored manifest must not take a view slot"
        );
        assert_eq!(post.entry(&mk(0x11).batch_id), None);

        // A sponsored manifest is admitted and records who paid for the slot.
        assert!(post.apply_manifest(&mk(0x12), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(alice, 4)));
        assert_eq!(post.entry(&mk(0x12).batch_id).unwrap().sponsor_tag(), alice);
        assert_eq!(post.sponsor_slots(alice), 1);

        // The QUOTA is what makes sponsorship cost anything: without it one bond fills all 1,024
        // slots as cheaply as one. Alice's second slot lands, her third is refused at quota 2.
        let mut q = PalwBatchViewV1::new();
        assert!(q.apply_manifest(&mk(0x20), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(alice, 2)));
        assert!(q.apply_manifest(&mk(0x21), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(alice, 2)));
        assert_eq!(q.sponsor_slots(alice), 2);
        assert!(
            !q.apply_manifest(&mk(0x22), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(alice, 2)),
            "a sponsor at quota takes no further slots"
        );
        // ...and one sponsor's exhausted quota must not censor another's — the failure mode that
        // would turn this fix into the very problem it closes.
        assert!(q.apply_manifest(&mk(0x23), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(bob, 2)), "quota is per sponsor");
        assert_eq!(q.sponsor_slots(bob), 1);

        // A re-broadcast of an already-registered manifest must not burn a slot: the quota check sits
        // AFTER the idempotence return, so a duplicate is free rather than self-censoring.
        assert!(!q.apply_manifest(&mk(0x20), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(alice, 2)), "duplicate is idempotent");
        assert_eq!(q.sponsor_slots(alice), 2, "a duplicate must not consume quota");

        // Quota 0 disables the per-sponsor bound while sponsorship itself stays required.
        let mut unbounded = PalwBatchViewV1::new();
        assert!(unbounded.apply_manifest(&mk(0x30), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(alice, 0)));
        assert!(unbounded.apply_manifest(&mk(0x31), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(alice, 0)));
        assert!(
            !unbounded.apply_manifest(&mk(0x32), 5, 256, 64, 2, 6, 6, 0, 1_024, sponsored(PALW_SPONSOR_UNATTRIBUTED, 0)),
            "quota 0 disables the count, not the sponsorship requirement"
        );
    }

    /// The sponsor tag is a function of the OWNER KEY, so splitting one operator's stake across many
    /// bonds collapses to one tag and buys zero extra slots. That is the property the quota rests on.
    #[test]
    fn sponsor_tag_is_keyed_on_the_owner_not_the_bond() {
        let key_a = vec![0xAA; 32];
        let key_b = vec![0xBB; 32];
        assert_eq!(palw_manifest_sponsor_tag(&key_a), palw_manifest_sponsor_tag(&key_a), "deterministic");
        assert_ne!(palw_manifest_sponsor_tag(&key_a), palw_manifest_sponsor_tag(&key_b), "distinct keys, distinct quota");
        assert_ne!(palw_manifest_sponsor_tag(&key_a), PALW_SPONSOR_UNATTRIBUTED, "a real key is never the sentinel");
    }

    #[test]
    fn c4_view_delta_state_machine() {
        let mut m = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(0),
            registration_epoch: 5,
            model_profile_id: h(3),
            runtime_class_id: h(4),
            leaf_count: 100,
            chunk_count: 2,
            leaf_root: h(8),
            descriptor_root: h(6),
            total_leaf_bond_sompi: 0,
            audit_policy_id: h(7),
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
        };
        m.batch_id = m.content_id();
        let mut v = PalwBatchViewV1::new();

        // manifest ⇒ Registering; a forged/duplicate is a no-op.
        assert!(v.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Registering);
        assert!(!v.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT), "idempotent");
        let mut forged = m.clone();
        forged.batch_id = h(0xff);
        assert!(
            !v.apply_manifest(&forged, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT),
            "forged batch_id rejected"
        );

        // 2 distinct chunks ⇒ Committed on the last; a duplicate index is a no-op.
        assert!(v.apply_leaf_chunk(&m.batch_id, 0));
        assert!(!v.apply_leaf_chunk(&m.batch_id, 0), "duplicate chunk index is idempotent");
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Registering);
        assert!(v.apply_leaf_chunk(&m.batch_id, 1));
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Committed);
        assert!(!v.apply_leaf_chunk(&m.batch_id, 2), "chunk_index out of range");

        // audit-beacon epoch (registration 5 + lead 2 + audit 6 = 13) ⇒ Auditing.
        v.advance_epoch(13, 2, 6);
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Auditing);

        // certificate ⇒ Certified (records the cert hash only — ADR-0040 CERT-TRUST: the body coordinate
        // copies no window and no stake figure out of a certificate it cannot verify).
        assert!(v.apply_certificate(&m.batch_id, h(0x99), 13));
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Certified);
        assert_eq!(v.entry(&m.batch_id).unwrap().cert_hash, Some(h(0x99)));

        // activation epoch (13) ⇒ Active; resolvable in-window.
        v.advance_epoch(13, 2, 6);
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Active);
        assert!(v.resolvable_batch(&m.batch_id, 15, 1_000).is_some());

        // ADR-0040 SS-04: revocation is NON-retroactive from `effective_daa` (§9.5). This assertion
        // changed with the rule: it used to demand `is_none()` at every DAA, which was the retroactive
        // behaviour the code had and the spec did not. Below the cutoff the batch stays resolvable.
        assert!(v.mark_revoked(&m.batch_id, 1500));
        assert!(v.resolvable_batch(&m.batch_id, 15, 1_499).is_some(), "below the cutoff ⇒ still resolvable");
        assert!(v.resolvable_batch(&m.batch_id, 15, 1_500).is_none(), "at the cutoff ⇒ revoked");

        // an incomplete batch expires past its deadline.
        let mut m2 = PalwBatchManifestV1 { registration_epoch: 5, activation_not_before_epoch: 13, expiry_epoch: 19, ..m.clone() };
        m2.model_profile_id = h(0x55); // change content ⇒ distinct batch id
        m2.batch_id = m2.content_id();
        assert!(v.apply_manifest(&m2, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
        v.advance_epoch(14, 2, 6); // 14 > deadline 13 while still Registering
        assert_eq!(v.entry(&m2.batch_id).unwrap().status, PalwBatchStatus::Expired);
    }

    /// Header-v4 canonical certification is a function of verified accepted facts, never raw arrival
    /// order. A junk certificate is represented by the absence of an `apply_verified_certificate`
    /// call: the contextual verifier rejected it before it could reach this transition.
    #[test]
    fn v4_accepted_provenance_from_genesis_matches_raw_adversary_replay() {
        let mut manifest = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(0),
            registration_epoch: 5,
            model_profile_id: h(3),
            runtime_class_id: h(4),
            leaf_count: 1,
            chunk_count: 1,
            leaf_root: h(8),
            descriptor_root: h(6),
            total_leaf_bond_sompi: 0,
            audit_policy_id: h(7),
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
        };
        manifest.batch_id = manifest.content_id();
        let committed = || {
            let mut view = PalwBatchViewV1::new();
            assert!(view.apply_manifest(&manifest, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
            assert!(view.apply_leaf_chunk(&manifest.batch_id, 0));
            view
        };

        let junk = h(0x01);
        let minority = h(0x80);
        let full = h(0x90);
        let mut junk_first = committed();
        // No call for `junk`: failed attestation/DA has no lifecycle effect.
        assert_eq!(junk_first.entry(&manifest.batch_id).unwrap().cert_hash, None);
        assert!(junk_first.apply_verified_certificate(&manifest.batch_id, minority, 60, 5_100));
        assert!(junk_first.apply_verified_certificate(&manifest.batch_id, full, 90, 5_100));

        let mut honest_first = committed();
        assert!(honest_first.apply_verified_certificate(&manifest.batch_id, full, 90, 5_100));
        assert!(!honest_first.apply_verified_certificate(&manifest.batch_id, minority, 60, 5_100));
        assert_eq!(junk_first, honest_first);
        let lifecycle = junk_first.entry(&manifest.batch_id).unwrap();
        assert_eq!(lifecycle.cert_hash, Some(full));
        assert_eq!(lifecycle.cert_approving_stake, 90);
        assert_ne!(lifecycle.cert_hash, Some(junk));

        // Prove the regression is not vacuous: the retired raw-body fold would have pinned the junk
        // and produced bytes different from a clean from-genesis accepted replay.
        let mut retired_raw_fold = committed();
        assert!(retired_raw_fold.apply_certificate(&manifest.batch_id, junk, 5_000));
        assert_ne!(borsh::to_vec(&retired_raw_fold).unwrap(), borsh::to_vec(&honest_first).unwrap());
    }

    #[test]
    fn accepted_certificate_equal_stake_uses_hash_tie_break() {
        let mut manifest = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(0),
            registration_epoch: 5,
            model_profile_id: h(3),
            runtime_class_id: h(4),
            leaf_count: 1,
            chunk_count: 1,
            leaf_root: h(8),
            descriptor_root: h(6),
            total_leaf_bond_sompi: 0,
            audit_policy_id: h(7),
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
        };
        manifest.batch_id = manifest.content_id();
        let mut view = PalwBatchViewV1::new();
        assert!(view.apply_manifest(&manifest, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
        assert!(view.apply_leaf_chunk(&manifest.batch_id, 0));
        assert!(view.apply_verified_certificate(&manifest.batch_id, h(0xf0), 75, 1));
        assert!(view.apply_verified_certificate(&manifest.batch_id, h(0x10), 75, 2));
        assert_eq!(view.entry(&manifest.batch_id).unwrap().cert_hash, Some(h(0x10)));
    }

    /// An unverified `approving_stake` value cannot replace a certified batch's certificate.
    ///
    /// The body/mergeset coordinate has no `ActiveBondView` and cannot verify the declared stake.
    /// Certificate application is therefore write-once and non-destructive at this stage.
    #[test]
    fn inflated_approving_stake_cannot_displace_or_brick_a_certified_batch() {
        let m = {
            let mut m = PalwBatchManifestV1 {
                version: 1,
                batch_id: h(0),
                registration_epoch: 5,
                model_profile_id: h(3),
                runtime_class_id: h(4),
                leaf_count: 100,
                chunk_count: 2,
                leaf_root: h(0x11),
                descriptor_root: h(6),
                total_leaf_bond_sompi: 0,
                audit_policy_id: h(7),
                activation_not_before_epoch: 13,
                expiry_epoch: 19,
            };
            m.batch_id = m.content_id();
            m
        };
        // Drive to Auditing, then certify with the honest certificate.
        let mut v = PalwBatchViewV1::new();
        assert!(v.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
        v.apply_leaf_chunk(&m.batch_id, 0);
        v.apply_leaf_chunk(&m.batch_id, 1);
        v.advance_epoch(13, 2, 6);
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Auditing);

        let honest = h(0xc0);
        assert!(v.apply_certificate(&m.batch_id, honest, 5_000), "the first certificate certifies the batch");
        assert_eq!(v.entry(&m.batch_id).unwrap().cert_hash, Some(honest));
        assert_eq!(v.entry(&m.batch_id).unwrap().first_cert_daa, Some(5_000));
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Certified);

        // REJECT — the attack. A zero-cost forgery (any hash, any declared stake, unattested, possibly
        // never even accepted) reaches this fold. It must change NOTHING.
        let forged = h(0xff);
        assert!(!v.apply_certificate(&m.batch_id, forged, 5_100), "an already-certified batch does not accept a second certificate");
        let e = v.entry(&m.batch_id).unwrap();
        assert_eq!(e.cert_hash, Some(honest), "the honest certificate is NOT evicted");
        assert_eq!(e.first_cert_daa, Some(5_000), "d₀ is not moved");
        assert_eq!(e.status, PalwBatchStatus::Certified);

        // ...and — the part that made this a batch-bricking DoS rather than mere noise — the forgery
        // cannot narrow the eligibility window, because the view no longer carries a certificate window
        // at all. The batch stays block-eligible for its whole manifest-declared life.
        v.advance_epoch(13, 2, 6);
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Active);
        for epoch in 13..19 {
            assert!(v.resolvable_batch(&m.batch_id, epoch, 0).is_some(), "batch must remain resolvable at epoch {epoch}");
        }
        assert!(v.resolvable_batch(&m.batch_id, 19, 0).is_none(), "the batch's OWN manifest expiry still binds");

        // The inert fields are never written by this coordinate, so nothing downstream can be tempted to
        // read an unverified value back out of them.
        let e = v.entry(&m.batch_id).unwrap();
        assert_eq!(e.cert_approving_stake, 0, "cert_approving_stake is inert (CERT-TRUST)");
        // Static-audit finding H-01 — this assertion used to say "the CERT-TRUST pair is inert". It
        // now says the thing that made reusing those two u64s safe: below the sponsorship fence the
        // fold writes PALW_SPONSOR_UNATTRIBUTED, so every row already on disk decodes to
        // "unattributed" under the new meaning — exactly what it meant under the old one.
        assert_eq!(e.sponsor_tag(), PALW_SPONSOR_UNATTRIBUTED, "an unsponsored manifest records no sponsor");
    }

    /// kaspa-pq **ADR-0040 — `d₀` (first certification DAA) is RE-DERIVED per fork, never inherited.**
    ///
    /// The view is a pure function of a fork's accepted effects. Rebuilding either branch reproduces
    /// that branch's `first_cert_daa` without inheriting values from the other branch.
    #[test]
    fn first_cert_daa_is_rederived_per_fork_not_inherited() {
        let m = {
            let mut m = PalwBatchManifestV1 {
                version: 1,
                batch_id: h(0),
                registration_epoch: 5,
                model_profile_id: h(3),
                runtime_class_id: h(4),
                leaf_count: 100,
                chunk_count: 2,
                leaf_root: h(0x11),
                descriptor_root: h(6),
                total_leaf_bond_sompi: 0,
                audit_policy_id: h(7),
                activation_not_before_epoch: 13,
                expiry_epoch: 19,
            };
            m.batch_id = m.content_id();
            m
        };
        let build = |cert_daa: u64| {
            let mut v = PalwBatchViewV1::new();
            assert!(v.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
            v.apply_leaf_chunk(&m.batch_id, 0);
            v.apply_leaf_chunk(&m.batch_id, 1);
            v.advance_epoch(13, 2, 6);
            assert!(v.apply_certificate(&m.batch_id, h(0xc0), cert_daa));
            v
        };

        // Two branches accept the same certificate at different DAA scores.
        let early = build(5_000);
        let late = build(5_900);
        assert_eq!(early.entry(&m.batch_id).unwrap().first_cert_daa, Some(5_000));
        assert_eq!(late.entry(&m.batch_id).unwrap().first_cert_daa, Some(5_900));

        // The decisive property: a reorg REPLACES the view, so the loser's d₀ cannot survive into the
        // winner. Rebuilding from the winning branch's effects reproduces the winner's d₀ exactly,
        // whatever the loser recorded.
        let rebuilt = build(5_900);
        assert_eq!(
            rebuilt.entry(&m.batch_id).unwrap().first_cert_daa,
            late.entry(&m.batch_id).unwrap().first_cert_daa,
            "rebuilding from the same accepted effects must reproduce the same d₀ — it is derived, not stored history"
        );
        assert_ne!(
            rebuilt.entry(&m.batch_id).unwrap().first_cert_daa,
            early.entry(&m.batch_id).unwrap().first_cert_daa,
            "and it must NOT inherit the other branch's d₀"
        );
    }

    /// kaspa-pq **ADR-0040 P1-7 (TGT-01) — REFUTED: the target interval is not miner-chosen.**
    ///
    /// The audit reported the interval as "self-reported by the header rather than derived by
    /// consensus". Clause 5 pins it to the block's own `daa_score`, which consensus derives from the
    /// block's past post-GHOSTDAG — so the only interval a miner can successfully declare is the one it
    /// does not choose. This test pins that derivation against future changes to clause 5.
    #[test]
    fn target_interval_is_pinned_to_daa_score_not_miner_chosen() {
        let binding = PalwTicketBinding {
            ticket_nullifier_commitment: ticket_nullifier_commitment(&h(3)),
            proof_type: 1,
            leaf_activation_epoch: 0,
            leaf_expiry_epoch: 100,
            target_daa_interval: 500,
        };
        let ok = |daa: u64| verify_palw_ticket_store_facts(&h(3), 1, daa, &binding, true, 1);
        // Declaring the interval that equals this block's DAA score is the ONLY accepted choice.
        assert_eq!(ok(500), Ok(()));
        // Any other declared interval — i.e. any attempt to aim at a different slot — is rejected.
        for daa in [499u64, 501, 0, u64::MAX] {
            assert_eq!(ok(daa), Err(PalwTicketReject::IntervalMismatch), "daa {daa} must not satisfy interval 500");
        }
    }

    /// kaspa-pq **ADR-0040 VIEW-01 — a batch is not usable by a ticket in the block that registers it.**
    ///
    /// A block is not in its own mergeset, so its own PALW overlay effects never enter its own view.
    /// The audit read this as a missing self-fold; it is deliberate, and this test says so in the form
    /// of the property it protects: registering and spending a batch atomically would defeat the
    /// registration lead that `admission_valid` imposes.
    #[test]
    fn a_batch_is_not_block_eligible_in_its_own_registration_epoch() {
        let m = {
            let mut m = PalwBatchManifestV1 {
                version: 1,
                batch_id: h(0),
                registration_epoch: 5,
                model_profile_id: h(3),
                runtime_class_id: h(4),
                leaf_count: 100,
                chunk_count: 2,
                leaf_root: h(0x11),
                descriptor_root: h(6),
                total_leaf_bond_sompi: 0,
                audit_policy_id: h(7),
                activation_not_before_epoch: 13,
                expiry_epoch: 19,
            };
            m.batch_id = m.content_id();
            m
        };
        let mut v = PalwBatchViewV1::new();
        assert!(v.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));

        // Registered — but not resolvable at its own registration epoch, nor anywhere before the lead +
        // audit window it declared. The lead is what a same-block register-and-spend would erase.
        assert!(v.resolvable_batch(&m.batch_id, 5, 0).is_none(), "not usable in its registration epoch");
        for e in 5..13 {
            assert!(v.resolvable_batch(&m.batch_id, e, 0).is_none(), "not usable before its declared activation (epoch {e})");
        }
    }

    /// kaspa-pq **ADR-0040 P1-5 / P1-9 — the view carries NO per-leaf state, and the bound is exact.**
    ///
    /// This replaces `global_job_nullifier_rejects_cross_batch_duplicate_work`, which was DELETED
    /// because its subject ceases to exist (a SPEC CHANGE — see the struct doc on [`PalwBatchViewV1`]
    /// and ADR-0040; the deleted test passed, it was not weakened to make anything go green).
    ///
    /// What is asserted instead is the property the removal buys: the persisted view's size is a
    /// function of `batches.len()` ALONE, so no amount of leaf traffic can grow a row. Saturated at the
    /// shipped cap this is well under 400 KB, and the struct is pinned to exactly two fields so a
    /// future slice cannot silently reintroduce a per-leaf map.
    #[test]
    fn view_size_scales_only_with_batch_count() {
        let lifecycle = PalwBatchLifecycleV1 {
            status: PalwBatchStatus::Active,
            registration_epoch: 7,
            activation_not_before_epoch: 8,
            expiry_epoch: 21,
            leaf_count: 256,
            chunk_count: 4,
            chunks_present: [u64::MAX, u64::MAX, u64::MAX, u64::MAX],
            leaf_root: h(0x11),
            cert_hash: Some(h(0x12)),
            sponsor_tag_hi: 0,
            sponsor_tag_lo: 0,
            cert_approving_stake: 0,
            first_cert_daa: Some(1_234),
            revoked_from_daa: None,
        };
        // Exactly two fields: destructuring is exhaustive, so a third field fails to compile here.
        let PalwBatchViewV1 { version: _, batches: _ } = PalwBatchViewV1::new();

        let cap = PalwBatchAdmissionParams::INERT.max_view_batches as usize;
        assert_eq!(cap, 1_024);
        let build = |n: usize| {
            let mut v = PalwBatchViewV1::new();
            for i in 0..n {
                let mut id = [0u8; 64];
                id[..8].copy_from_slice(&(i as u64).to_le_bytes());
                v.batches.insert(Hash64::from_bytes(id), lifecycle.clone());
            }
            borsh::to_vec(&v).unwrap().len()
        };
        let (half, full) = (build(cap / 2), build(cap));
        assert!(full < 400_000, "saturated view must stay well under 400 KB, got {full}");
        // Strictly linear in `batches.len()`: no per-leaf term hides anywhere in the encoding.
        assert_eq!(full - half, half - build(0), "view size must scale strictly with batches.len()");
    }

    /// ADR-0040 S3 vote-censorship behavior. Certificates are
    /// CONTENT-ADDRESSED and COEXIST in `palw_store`; publishing a minority assembly does not suppress a
    /// fuller one, and a miner names whichever attested certificate it wants via
    /// `palw_epoch_certificate_hash`. What the view must guarantee is the negative: a hostile certificate
    /// cannot evict the honest one, cannot shrink anyone's eligibility, and cannot freeze anything. That
    /// is what is asserted here.
    ///
    /// The body-coordinate limitation remains: this view alone cannot decide whether a certificate
    /// omitted selected votes. The VIRTUAL verifier now supplies that missing context by re-deriving the
    /// full selected slate and `audit_sample_root`, using the selected-slate stake as the denominator, and
    /// rejecting a vote-censored certificate that falls below quorum. This test pins only the negative
    /// body-view property; it is not the final certificate-validity verdict.
    #[test]
    fn s3_vote_censorship_is_not_remediable_at_the_body_coordinate() {
        let m = {
            let mut m = PalwBatchManifestV1 {
                version: 1,
                batch_id: h(0),
                registration_epoch: 5,
                model_profile_id: h(3),
                runtime_class_id: h(4),
                leaf_count: 100,
                chunk_count: 2,
                leaf_root: h(0x11),
                descriptor_root: h(6),
                total_leaf_bond_sompi: 0,
                audit_policy_id: h(7),
                activation_not_before_epoch: 13,
                expiry_epoch: 19,
            };
            m.batch_id = m.content_id();
            m
        };
        let mut v = PalwBatchViewV1::new();
        assert!(v.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
        v.apply_leaf_chunk(&m.batch_id, 0);
        v.apply_leaf_chunk(&m.batch_id, 1);
        v.advance_epoch(13, 2, 6);

        // The censor lands its minority-stake certificate first.
        let censored = h(0xc0);
        assert!(v.apply_certificate(&m.batch_id, censored, 5_000));

        // The fuller certificate does NOT supersede in the view — and that is now the correct outcome,
        // because the view has no way to tell which of the two carries more real bonded stake.
        let full = h(0xe0);
        assert!(!v.apply_certificate(&m.batch_id, full, 5_100), "the body coordinate ranks nothing: first arrival stands");
        assert_eq!(v.entry(&m.batch_id).unwrap().cert_hash, Some(censored));

        // The property that MATTERS, and that the old rule did not provide: losing the view race costs
        // the fuller certificate nothing. The view's `cert_hash` is a "has been certified at all" bit,
        // not a permission slip — the batch stays fully block-eligible for its whole declared life, and a
        // header may name the fuller certificate (once attested and stored) instead.
        v.advance_epoch(13, 2, 6);
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Active);
        for epoch in 13..19 {
            assert!(
                v.resolvable_batch(&m.batch_id, epoch, 0).is_some(),
                "eligibility is not hostage to the view race (epoch {epoch})"
            );
        }

        // ...and symmetrically, the censor cannot use a later certificate to shorten or move anything.
        assert!(!v.apply_certificate(&m.batch_id, h(0xc2), 6_000));
        assert_eq!(v.entry(&m.batch_id).unwrap().cert_hash, Some(censored));
        assert!(v.resolvable_batch(&m.batch_id, 18, 0).is_some());
    }

    /// kaspa-pq **ADR-0040 CERT-TRUST — the certificate fold is WRITE-ONCE and MONOTONE.**
    ///
    /// SPEC CHANGE: this test previously pinned the `Δ_super` minimum supersession window
    /// (`effective activation = max(declared activation, d₀ + Δ_super)`). Supersession is gone, so
    /// `Δ_super` has no reader and `certificate_frozen_at` is deleted; what remains to state is the
    /// invariant that replaced them, and the reason it is safe to be blind here.
    ///
    /// Monotonicity is the whole safety argument for an unverifiable coordinate: every transition this
    /// fold can make is in the permissive direction (`→ Certified`, `cert_hash: None → Some`), so an
    /// adversary who can inject arbitrary certificate transactions — which they can, since the fold does
    /// not require acceptance — can only make a batch look MORE certified than it is. That buys nothing,
    /// because mining additionally requires an attested blob out of `palw_store`. Any DESTRUCTIVE
    /// transition here would immediately be a zero-cost censorship primitive.
    #[test]
    fn certificate_fold_is_write_once_and_monotone() {
        let m = {
            let mut m = PalwBatchManifestV1 {
                version: 1,
                batch_id: h(0),
                registration_epoch: 5,
                model_profile_id: h(3),
                runtime_class_id: h(4),
                leaf_count: 100,
                chunk_count: 2,
                leaf_root: h(0x11),
                descriptor_root: h(6),
                total_leaf_bond_sompi: 0,
                audit_policy_id: h(7),
                activation_not_before_epoch: 13,
                expiry_epoch: 19,
            };
            m.batch_id = m.content_id();
            m
        };

        // A certificate for a batch that is not even registered is a no-op (no entry is conjured).
        let mut v = PalwBatchViewV1::new();
        assert!(!v.apply_certificate(&m.batch_id, h(0xc0), 5_000), "no entry ⇒ nothing to certify");
        assert!(v.entry(&m.batch_id).is_none());

        // A certificate for a Registering (incomplete) batch is a no-op too — the chunk-completeness gate
        // is not bypassable by certifying early.
        assert!(v.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, PalwManifestSponsorshipV1::INERT));
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Registering);
        assert!(!v.apply_certificate(&m.batch_id, h(0xc0), 5_000), "Registering is not a certifiable state");
        assert_eq!(v.entry(&m.batch_id).unwrap().cert_hash, None);

        // Committed accepts it (mergeset ordering may deliver the certificate before the audit tick).
        v.apply_leaf_chunk(&m.batch_id, 0);
        v.apply_leaf_chunk(&m.batch_id, 1);
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Committed);
        assert!(v.apply_certificate(&m.batch_id, h(0xc0), 5_000));
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Certified);
        assert_eq!(v.entry(&m.batch_id).unwrap().cert_hash, Some(h(0xc0)));

        // Write-once: neither a lower-sorting nor a higher-sorting hash replaces it, at any DAA, ever.
        for (cand, daa) in [(h(0x01), 5_001), (h(0xfe), 5_002), (h(0xc0), 9_999)] {
            assert!(!v.apply_certificate(&m.batch_id, cand, daa), "cert_hash is write-once");
            assert_eq!(v.entry(&m.batch_id).unwrap().cert_hash, Some(h(0xc0)));
            assert_eq!(v.entry(&m.batch_id).unwrap().first_cert_daa, Some(5_000));
        }

        // Monotone: the status never regresses, and an Expired batch is not resurrected by a certificate.
        v.advance_epoch(19, 2, 6);
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Expired);
        assert!(!v.apply_certificate(&m.batch_id, h(0xaa), 10_000), "an expired batch cannot be re-certified");
        assert_eq!(v.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Expired);
    }

    /// §16.3 lane params: the inert placeholder is structurally valid but FAILS the activation preflight
    /// (zero genesis bits); a recalibrated set passes only when hash genesis bits == the genesis header
    /// bits and min_samples fits both windows. PalwLaneBitsV1 selects/updates per lane.
    #[test]
    fn lane_difficulty_params_and_bits() {
        use crate::pow_layer0::WorkLane;
        let inert = LaneDifficultyParams::INERT;
        assert!(inert.is_structurally_valid());
        // inert placeholder (genesis bits 0) is NEVER activation-consistent.
        assert!(!inert.is_consistent_for_activation(0x1d00ffff));

        // a recalibrated set: real genesis bits, hash bits == genesis header bits.
        let genesis_bits = 0x1d00ffff_u32;
        let good = LaneDifficultyParams { genesis_hash_bits: genesis_bits, genesis_replica_bits: 0x1e00abcd, ..inert.clone() };
        assert!(good.is_consistent_for_activation(genesis_bits));
        // hash genesis bits must equal the genesis header bits (else inert-vs-active HOLD diverges).
        assert!(!good.is_consistent_for_activation(0x1c00ffff));
        // min_samples must fit the window.
        let bad = LaneDifficultyParams { min_samples: 999_999, ..good.clone() };
        assert!(!bad.is_consistent_for_activation(genesis_bits));
        // a zero adjust factor is structurally invalid.
        assert!(!LaneDifficultyParams { max_adjust_factor: 0, ..inert.clone() }.is_structurally_valid());

        // PalwLaneBitsV1 lane selection + update.
        let bits = PalwLaneBitsV1 { hash_bits: 11, replica_bits: 22 };
        assert_eq!(bits.lane_bits(WorkLane::HashFloor), 11);
        assert_eq!(bits.lane_bits(WorkLane::ReplicaPalw), 22);
        assert_eq!(bits.with_lane_bits(WorkLane::ReplicaPalw, 99).replica_bits, 99);
    }

    /// §12.1 clause-6 cert digest: anchor-pure, field-sensitive, domain-disjoint; and the checkpoint
    /// params predicate rejects the current (unrecalibrated) testnet DNS numbers + vacuous depths.
    #[test]
    fn dns_certificate_hash_and_checkpoint_params() {
        let a = BeaconDnsAnchor { hash: h(2), blue_score: 77, daa_score: 88, overlay_root: h(9) };
        let base = dns_finality_certificate_hash_v1(&a);
        // every fact perturbs the digest.
        assert_ne!(base, dns_finality_certificate_hash_v1(&BeaconDnsAnchor { hash: h(3), ..a }));
        assert_ne!(base, dns_finality_certificate_hash_v1(&BeaconDnsAnchor { blue_score: 78, ..a }));
        assert_ne!(base, dns_finality_certificate_hash_v1(&BeaconDnsAnchor { daa_score: 89, ..a }));
        assert_ne!(base, dns_finality_certificate_hash_v1(&BeaconDnsAnchor { overlay_root: h(10), ..a }));
        // domain-disjoint from the beacon commitment domain over comparable input widths.
        assert_ne!(base, beacon_commitment(77, &[2u8; 64], &op(2, 0)));

        // §12.1 LOOKBACK inequality + C6 SLICE 5 band (finality_depth <= burial < pruning_depth). testnet
        // DNS numbers (lag 100 + backoff 20 < horizon 300) FAIL — the PALW re-genesis must recalibrate.
        // GENESIS-style deeper lag inside the band passes; vacuous depths fail. Args:
        // (lag, backoff, max_reorg, finality_depth, pruning_depth, work_depth, stake_depth).
        let w = |v: u64| BlueWorkType::from(v);
        assert!(!palw_checkpoint_params_consistent(100, 20, 300, 300, 100_000, w(100), 5000)); // burial 120 < reorg 300
        assert!(palw_checkpoint_params_consistent(300, 20, 300, 300, 100_000, w(100), 5000)); // burial 320 in band
        assert!(palw_checkpoint_params_consistent(280, 20, 300, 300, 100_000, w(0), 5000)); // stake depth alone suffices
        assert!(!palw_checkpoint_params_consistent(300, 20, 300, 300, 100_000, w(0), 0)); // both depths zero = vacuous
        // C6 SLICE 5 band edges: burial >= max_reorg but < finality_depth ⇒ FAIL (not externally settled);
        // burial >= pruning_depth ⇒ FAIL (anchor header would be pruned, read unrunnable).
        assert!(!palw_checkpoint_params_consistent(300, 20, 300, 500, 100_000, w(100), 5000)); // burial 320 < finality 500
        assert!(!palw_checkpoint_params_consistent(300, 20, 300, 300, 310, w(100), 5000));
        // burial 320 >= pruning 310
    }

    #[test]
    fn certificate_stake_weighted_quorum() {
        // three auditors; A + B vote pass (stake 30 + 30 = 60), C rejects (stake 40). Total 100.
        let vote = |idx: u32, v: u8| PalwAuditorVoteV2 {
            bond_outpoint: op(0x40, idx),
            vote: v,
            checked_leaf_bitmap_root: h(6),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(5),
            signature: vec![],
        };
        let mut cert = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: h(1),
            manifest_hash: h(2),
            leaf_root: h(3),
            audit_beacon_epoch: 5,
            audit_sample_root: h(4),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(5),
            certificate_epoch: 6,
            activation_epoch: 7,
            expiry_epoch: 13,
            auditor_set_commitment: h(7),
            approving_stake: 0,
            votes: vec![vote(0, 1), vote(1, 1), vote(2, 0)],
        };
        let stake_of = |o: &TransactionOutpoint| -> u128 {
            match o.index {
                0 => 30,
                1 => 30,
                2 => 40,
                _ => 0,
            }
        };
        assert_eq!(cert.pass_stake(stake_of), 60);
        // 2/3 of 100 = 66.67 → 60 < 66.67 ⇒ NOT reached.
        assert!(!cert.quorum_reached(100, 2, 3, stake_of));
        // if C also passes: 100 >= 66.67 ⇒ reached.
        cert.votes[2].vote = 1;
        assert!(cert.quorum_reached(100, 2, 3, stake_of));
        // exact boundary: pass 67 of 100 at 2/3 ⇒ 67*3=201 >= 100*2=200 ⇒ reached.
        let stake_bound = |o: &TransactionOutpoint| -> u128 { if o.index < 2 { 33 } else { 34 } };
        cert.votes[2].vote = 0;
        // pass = 33+33 = 66; 66*3=198 < 200 ⇒ not reached at the boundary-1.
        assert!(!cert.quorum_reached(100, 2, 3, stake_bound));

        // ADR-0040 P0-5 — the two vacuity guards. Each of these is `0 >= 0` without the guard, i.e. a
        // certificate carrying NO passing stake at all would be accepted as quorum-valid.
        let no_stake = |_: &TransactionOutpoint| -> u128 { 0 };
        cert.votes.iter_mut().for_each(|v| v.vote = 0);
        // (a) total_auditor_stake == 0 — no eligible auditor stake exists. Zero PASS must NOT reach.
        assert!(!cert.quorum_reached(0, 2, 3, no_stake), "zero total auditor stake must never reach quorum");
        assert!(!cert.quorum_reached(0, 2, 3, stake_of), "zero total is fail-closed regardless of stake_of");
        // (b) num == 0 — a degenerate 0/den threshold must NOT admit everything.
        assert!(!cert.quorum_reached(100, 0, 3, stake_of), "a 0/den threshold must never reach quorum");
        // (c) den == 0 remains rejected (pre-existing guard).
        assert!(!cert.quorum_reached(100, 2, 0, stake_of));
        // Sanity: with all three degenerate inputs excluded, a genuine unanimous quorum still reaches.
        cert.votes.iter_mut().for_each(|v| v.vote = 1);
        assert!(cert.quorum_reached(100, 2, 3, stake_of), "guards must not break the honest path");
    }

    #[test]
    fn lane_daa_targets_and_params() {
        // 10 BPS PALW split: hash 2 → 500 ms, replica 8 → 125 ms. Total 10 → 100 ms block interval.
        assert_eq!(lane_target_time_ms(2), 500);
        assert_eq!(lane_target_time_ms(8), 125);
        assert_eq!(lane_target_time_ms(10), 100);
        // (the future Stage-B 40-BPS profile: hash 8 → 125, replica 32 → 31 (31.25 rounded), total 25.)
        assert_eq!(lane_target_time_ms(32), 31);
        assert_eq!(lane_target_time_ms(40), 25);
        assert_eq!(lane_target_time_ms(0), 1000); // guarded: bps<1 → treat as 1
        let p = LaneDifficultyParams::testnet_default();
        assert_eq!((p.hash_target_time_ms, p.replica_target_time_ms), (500, 125));
        assert_eq!(p.compute_work_scale, 1);
        assert!(p.is_structurally_valid());
    }

    #[test]
    fn lane_daa_sampling_and_retarget() {
        // §16.2 sampling: hash lane samples algo-3 blues; compute lane samples unique-active algo-4 blues.
        assert!(lane_daa_sample_eligible(WorkLane::HashFloor, POW_ALGO_ID_BLAKE2B_SHA3, true, false));
        assert!(!lane_daa_sample_eligible(WorkLane::HashFloor, POW_ALGO_ID_PALW_REPLICA, true, true)); // algo-4 not in hash window
        assert!(lane_daa_sample_eligible(WorkLane::ReplicaPalw, POW_ALGO_ID_PALW_REPLICA, true, true));
        assert!(!lane_daa_sample_eligible(WorkLane::ReplicaPalw, POW_ALGO_ID_PALW_REPLICA, true, false)); // not unique/active (dup/revoked)
        assert!(!lane_daa_sample_eligible(WorkLane::ReplicaPalw, POW_ALGO_ID_PALW_REPLICA, false, true)); // not credited blue (red)
        assert!(!lane_daa_sample_eligible(WorkLane::HashFloor, POW_ALGO_ID_BLAKE2B_SHA3, false, false)); // red hash source

        // §16.3 retarget: hold below min_samples; else clamp measured to [target/4, target*4].
        assert_eq!(lane_retarget_decision(10, 60, 100, 125, 4), LaneRetargetDecision::HoldLastBits);
        assert_eq!(lane_retarget_decision(60, 60, 125, 125, 4), LaneRetargetDecision::Adjust { clamped_measured_ms: 125 });
        // a stall (measured 10 000 ms) clamps to target*4 = 500.
        assert_eq!(lane_retarget_decision(60, 60, 10_000, 125, 4), LaneRetargetDecision::Adjust { clamped_measured_ms: 500 });
        // a burst (measured 1 ms) clamps to target/4 = 31.
        assert_eq!(lane_retarget_decision(60, 60, 1, 125, 4), LaneRetargetDecision::Adjust { clamped_measured_ms: 31 });
    }

    #[test]
    fn palw_active_nullifier_window() {
        let mut s = PalwActiveNullifierSet::new();
        assert!(s.is_empty());
        // first-seen inserts succeed; a repeat is a duplicate.
        assert!(s.insert(h(1), 100));
        assert!(s.insert(h(2), 110));
        assert!(!s.insert(h(1), 120)); // duplicate ⇒ false, keeps earliest daa
        assert!(s.contains(&h(1)) && s.contains(&h(2)));
        assert_eq!(s.len(), 2);
        // canonical sorted order.
        let order: Vec<Hash64> = s.iter_sorted().map(|(n, _)| *n).collect();
        assert_eq!(order, vec![h(1), h(2)]);
        // prune everything first-seen before the retention floor (105) ⇒ drops nf 1 (seen@100).
        assert_eq!(s.prune_below(105), 1);
        assert!(!s.contains(&h(1)) && s.contains(&h(2)));

        // apply_mergeset seeded from the parent past: within-mergeset + seed duplicates recolor.
        let mut seeded = PalwActiveNullifierSet::new();
        seeded.insert(h(5), 200); // active in the selected parent's past
        let mergeset = [Some((h(5), 210)), None, Some((h(6), 210)), Some((h(6), 210))];
        assert_eq!(seeded.apply_mergeset(&mergeset), vec![0, 3]); // h(5) seed-dup, second h(6) within-mergeset dup
        assert!(seeded.contains(&h(6)));
    }

    #[test]
    fn palw_duplicate_ticket_dedup_is_deterministic() {
        let seed = HashSet::new();
        // Inert / all non-PALW ⇒ no duplicates, coloring unchanged.
        assert_eq!(palw_duplicate_ticket_positions(&[None, None, None], &seed), Vec::<usize>::new());
        // First-seen tickets are all kept.
        assert_eq!(palw_duplicate_ticket_positions(&[Some(h(1)), Some(h(2)), Some(h(3))], &seed), Vec::<usize>::new());
        // Within-mergeset re-use: the SECOND occurrence (position 2) is the duplicate; first is kept.
        assert_eq!(palw_duplicate_ticket_positions(&[Some(h(1)), Some(h(2)), Some(h(1))], &seed), vec![2]);
        // algo-3 (None) interleaved is skipped; the repeat of ticket 1 at position 3 is the duplicate.
        assert_eq!(palw_duplicate_ticket_positions(&[None, Some(h(1)), None, Some(h(1))], &seed), vec![3]);
        // A nullifier already active in the selected parent's past (seed) makes the FIRST mergeset use a
        // duplicate too.
        let seed1: HashSet<Hash64> = [h(1)].into_iter().collect();
        assert_eq!(palw_duplicate_ticket_positions(&[Some(h(1)), Some(h(2))], &seed1), vec![0]);
        assert_eq!(palw_duplicate_ticket_positions(&[Some(h(2)), Some(h(1)), Some(h(2))], &seed1), vec![1, 2]);
    }

    /// kaspa-pq ADR-0040 **TGT-02 + TGT-03** — the replacement source of truth for the target interval.
    ///
    /// `slot_digest` / `target_daa_interval` are deleted (zero production callers, and the derivation
    /// was removed once already for contradicting clause 5). This is the test that the capability they
    /// nominally provided — "a miner cannot pick a favourable draw interval", invariant I-3 — is
    /// actually held by what remains: clause 5 of [`verify_palw_ticket_store_facts`] pins the leaf's
    /// `target_daa_interval` to the header's own `daa_score`, and `daa_score` is consensus-derived from
    /// the block's past rather than miner-chosen.
    #[test]
    fn target_interval_is_pinned_to_daa_score_not_a_slot_draw() {
        const INTERVAL: u64 = 4_242;
        let nullifier = h(0x5A);
        let binding = PalwTicketBinding {
            ticket_nullifier_commitment: ticket_nullifier_commitment(&nullifier),
            proof_type: PalwProofType::ReplicaExactV1 as u8,
            leaf_activation_epoch: 0,
            leaf_expiry_epoch: 100,
            target_daa_interval: INTERVAL,
        };
        let verify =
            |daa: u64| verify_palw_ticket_store_facts(&nullifier, PalwProofType::ReplicaExactV1 as u8, daa, &binding, true, 10);

        // The one accepted interval is the block's own DAA score — exactly one value, no window.
        assert_eq!(verify(INTERVAL), Ok(()));
        // Every other interval is rejected, in both directions and at the adjacent values. There is no
        // `active_window_intervals` to collapse to a single slot (TGT-03) because there is no window.
        for daa in [0, 1, INTERVAL - 1, INTERVAL + 1, u64::MAX] {
            assert_eq!(verify(daa), Err(PalwTicketReject::IntervalMismatch), "daa {daa} must not resolve to interval {INTERVAL}");
        }
    }

    /// kaspa-pq ADR-0040 **SS-04** — revocation is non-retroactive from `effective_daa` (§9.5).
    ///
    /// Before this change `is_block_eligible_at` read `revoked_from_daa.is_none()`, so ANY revocation
    /// killed the batch at EVERY coordinate: a leaf whose interval had already been drawn and paid
    /// became retroactively invalid. `revoked_from_daa` was written and never compared to anything, and
    /// `mark_revoked`'s doc comment asserted the §9.5 property the code did not implement. This pins the
    /// boundary in both directions so that divergence cannot silently return.
    #[test]
    fn revocation_is_non_retroactive_from_effective_daa() {
        const D: u64 = 1_500;
        let mut e = PalwBatchLifecycleV1 {
            status: PalwBatchStatus::Active,
            registration_epoch: 5,
            activation_not_before_epoch: 6,
            expiry_epoch: 100,
            leaf_count: 1,
            chunk_count: 1,
            chunks_present: [0; 4],
            leaf_root: h(0x11),
            cert_hash: Some(h(0x99)),
            sponsor_tag_hi: 0,
            sponsor_tag_lo: 100,
            cert_approving_stake: 0,
            first_cert_daa: Some(0),
            revoked_from_daa: None,
        };
        // Un-revoked: eligible at any DAA.
        assert!(e.is_block_eligible_at(10, 0));
        assert!(e.is_block_eligible_at(10, u64::MAX));

        e.revoked_from_daa = Some(D);
        // Strictly below the cutoff ⇒ still eligible (intervals already drawn keep their value).
        assert!(e.is_block_eligible_at(10, 0), "revocation must not reach back to DAA 0");
        assert!(e.is_block_eligible_at(10, D - 1), "the sompi below the cutoff is still eligible");
        // At and above ⇒ rejected.
        assert!(!e.is_block_eligible_at(10, D), "the cutoff itself is revoked (half-open [D, ∞))");
        assert!(!e.is_block_eligible_at(10, D + 1));
        assert!(!e.is_block_eligible_at(10, u64::MAX));

        // Revocation composes with — and never rescues — the other clauses: an expired batch stays
        // ineligible below the revocation cutoff too.
        assert!(!e.is_block_eligible_at(100, D - 1), "expiry still binds independently of revocation");

        // `revoked_from_daa == 0` is the fully-retroactive special case, and only that value produces it.
        let e0 = PalwBatchLifecycleV1 { revoked_from_daa: Some(0), ..e.clone() };
        assert!(!e0.is_block_eligible_at(10, 0));

        // Same boundary through the view-level resolver the pipeline actually calls.
        let mut v = PalwBatchViewV1::default();
        let id = h(0x77);
        v.batches.insert(id, e);
        assert!(v.resolvable_batch(&id, 10, D - 1).is_some());
        assert!(v.resolvable_batch(&id, 10, D).is_none());
    }

    /// kaspa-pq ADR-0040 **ECON-04** — pins the split PRODUCTION actually performs.
    ///
    /// This test previously pinned `provider_pair_split` (`subsidy · 7700/10000`, then halved), which
    /// no production path ever called, under a name that implied it was the coinbase rule. That helper
    /// is deleted; this asserts the real composition used by
    /// `CoinbaseManager::expected_coinbase_transaction`:
    ///
    /// ```text
    /// base = split_block_subsidy(subsidy, &fee_split.palw_lane()).worker_base_sompi   // REMAINDER
    /// (a, b, rem) = premium_split(base, replica_count, π)
    /// ```
    ///
    /// The remainder form is what makes `a + b + inclusion + validator == subsidy` exactly; the deleted
    /// multiply form could fall up to 2 sompi short of it.
    #[test]
    fn coinbase_provider_split_matches_production_composition() {
        use crate::config::params::PRODUCTION_DNS_PARAMS;
        use crate::dns_finality::split_block_subsidy;
        use crate::palw_premium::{PALW_PREMIUM_BPS_ONE, premium_split};

        // ADR-0039 §17.1 (amended): the PALW-lane split is 77 / 8 / 15 and sums to 10 000.
        assert_eq!(PALW_PROVIDER_BASE_BPS + PALW_INCLUSION_BPS + PALW_VALIDATOR_BPS, 10_000);
        assert_eq!(PALW_PROVIDER_BASE_BPS, 7700);
        assert_eq!(PALW_VALIDATOR_BPS, 1500);

        let palw = PRODUCTION_DNS_PARAMS.reward_params.fee_split.palw_lane();
        assert_eq!(palw.subsidy_worker_base_bps, PALW_PROVIDER_BASE_BPS);
        assert_eq!(palw.subsidy_worker_inclusion_bps, PALW_INCLUSION_BPS);
        assert_eq!(palw.subsidy_validator_bps, PALW_VALIDATOR_BPS);
        assert_eq!(palw.subsidy_service_bps, 0);

        // The exact split the coinbase performs, at the neutral premium (v1 leaves carry m = 1, and the
        // single remainder folds into B — see the `ReplicaPalw` arm in `processes/coinbase.rs`).
        let production_pair = |subsidy: u64| -> (u64, u64, u64) {
            let s = split_block_subsidy(subsidy, &palw);
            let (a, b, rem) = premium_split(s.worker_base_sompi, 1, PALW_PREMIUM_BPS_ONE);
            (a, b + rem, s.worker_base_sompi)
        };

        // 1000 sompi: inclusion = 80, validator = 150, service = 0 ⇒ base = 770 (remainder), 385 each.
        let (a, b, base) = production_pair(1000);
        assert_eq!(base, 770);
        assert_eq!((a, b), (385, 385));

        // Odd base ⇒ B gets the extra sompi, and `a + b == base` exactly (no minting, no burning).
        // 999: inclusion = 79, validator = 149 ⇒ base = 771. Note the DELETED helper produced 769 here
        // (`999·7700/10000`), i.e. it under-paid the pair by 2 sompi relative to consensus.
        let (a, b, base) = production_pair(999);
        assert_eq!(base, 771, "the base is the REMAINDER after inclusion/validator, not a bps multiply");
        assert_ne!(base, (999u128 * 7700 / 10_000) as u64, "remainder ≠ truncating multiply — this is ECON-04");
        assert_eq!((a, b), (385, 386));
        assert_eq!(a + b, base);

        // Conservation across the whole PALW-lane subsidy, for a spread of values: nothing is minted or
        // lost between the subsidy and the four carves.
        for subsidy in [0u64, 1, 7, 999, 1000, 123_456_789, u64::MAX] {
            let s = split_block_subsidy(subsidy, &palw);
            let (a, b, rem) = premium_split(s.worker_base_sompi, 1, PALW_PREMIUM_BPS_ONE);
            assert_eq!(a + b + rem, s.worker_base_sompi, "provider pair must conserve the base (subsidy {subsidy})");
            assert_eq!(
                s.worker_base_sompi + s.worker_inclusion_sompi + s.validator_sompi + s.service_sompi,
                subsidy,
                "the PALW-lane carves must conserve the subsidy (subsidy {subsidy})"
            );
        }

        // red/duplicate ⇒ nothing to the pair (§17.4: unminted, not rerouted to the includer).
        assert_eq!(palw_red_or_duplicate_provider_reward(), (0, 0));
    }

    // =========================================================================================
    // kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2) — leaf Merkle construction.
    // =========================================================================================

    /// Construction vector. `leaf_root` is a consensus value inside
    /// `content_id()`, hence inside every `batch_id`. ADR-0040 §5.15.10 warns that the persisted-layout
    /// pin uses a LITERAL `h(0x43)` for `leaf_root` and therefore cannot detect a change of shape at
    /// all — a green pin is not evidence. This test is the evidence.
    ///
    /// It pins the shape TWICE over, so a refactor cannot quietly redefine it:
    /// 1. against an INDEPENDENT, straight-line re-derivation written out from ADR-0040 §5.15.4 using
    ///    raw `blake2b_512_keyed` calls and no helper from the implementation, and
    /// 2. against a hex literal, which is what catches a change that is applied to both the
    ///    implementation and the re-derivation.
    ///
    /// The fixture is 3 leaves precisely because 3 is not a power of two: it exercises the uniform
    /// `H_EMPTY` padding slot, which is the half of the construction most likely to be "simplified"
    /// back into tail duplication.
    #[test]
    fn palw_leaf_merkle_root_construction_golden() {
        let leaves = [h(1), h(2), h(3)];

        // --- independent re-derivation, straight from the spec text ------------------------------
        let k = blake2b_512_keyed;
        let leaf_node = |i: u32, x: &Hash64| {
            let mut p = Vec::new();
            p.extend_from_slice(&i.to_le_bytes());
            p.extend_from_slice(x.as_byte_slice());
            k(PALW_LEAF_MERKLE_LEAF_DOMAIN, &p)
        };
        let node = |l: &Hash64, r: &Hash64| {
            let mut p = Vec::new();
            p.extend_from_slice(l.as_byte_slice());
            p.extend_from_slice(r.as_byte_slice());
            k(PALW_LEAF_MERKLE_NODE_DOMAIN, &p)
        };
        let h_empty = k(PALW_LEAF_MERKLE_EMPTY_DOMAIN, &[]);
        // d = ceil(log2(3)) = 2, so level 0 is [leaf0, leaf1, leaf2, H_EMPTY].
        let (n0, n1, n2, n3) = (leaf_node(0, &leaves[0]), leaf_node(1, &leaves[1]), leaf_node(2, &leaves[2]), h_empty);
        let apex = node(&node(&n0, &n1), &node(&n2, &n3));
        let mut pre = Vec::new();
        pre.extend_from_slice(&3u64.to_le_bytes());
        pre.extend_from_slice(apex.as_byte_slice());
        let expected = k(PALW_LEAF_ROOT_DOMAIN, &pre);

        assert_eq!(palw_leaf_merkle_root(&leaves), expected, "implementation diverged from the ADR-0040 §5.15.4 construction");

        // --- the value pin ------------------------------------------------------------------------
        assert_eq!(
            palw_leaf_merkle_root(&leaves).to_string(),
            "2db16054770aa70787b31e9eed4ac52a44317d8be6f2087e532ddddcfeedee09\
             6d8c389f23a3f55918278527e88952bb805b6300d0a21584a26184460765a2e2",
            "the leaf_root construction changed; every batch_id moves with it — this needs a re-genesis, \
             not a fixture edit"
        );

        // Depth is uniform and matches the 256-leaf hard cap's ceil(log2) = 8.
        assert_eq!(palw_leaf_merkle_depth(0), 0);
        assert_eq!(palw_leaf_merkle_depth(1), 0);
        assert_eq!(palw_leaf_merkle_depth(2), 1);
        assert_eq!(palw_leaf_merkle_depth(3), 2);
        assert_eq!(palw_leaf_merkle_depth(4), 2);
        assert_eq!(palw_leaf_merkle_depth(5), 3);
        assert_eq!(palw_leaf_merkle_depth(PALW_MAX_BATCH_LEAVES_V1 as u32), PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN as u32);
        assert_eq!(palw_leaf_merkle_depth(PALW_MAX_BATCH_LEAVES_V1 as u32 - 1), PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN as u32);
    }

    /// Consensus-side cross-crate vector (ADR-0040 §5.15.9 step (iv), §5.15.12).
    ///
    /// The mirror of `manifest_leaf_root_is_pinned_to_the_consensus_cross_crate_golden_vector` in
    /// `mil/miner/src/registration.rs`. Both tests pin the SAME two literals: a three-element leaf-hash
    /// vector and the root it reduces to. That shared CONSTANT — not a shared function call — is what
    /// makes producer/verifier drift a build failure instead of a silent lane outage.
    ///
    /// Why a constant and not `assert_eq!(miner_root, consensus_root)`: the crates cannot see each
    /// other's tests, and even if they could, computing both sides with the same function proves only
    /// that the function is deterministic. A literal is the only artifact both crates can be held to
    /// independently.
    ///
    /// The leaf hashes are the `batch_id`-zeroed `PalwPublicLeafV1::leaf_hash()` values of the miner's
    /// `golden_leaf(0..3)` fixture. This crate does not rebuild those leaves — it does not need to; the
    /// miner side pins leaf → hash, this side pins hash → root, and the two literals are the seam.
    ///
    /// If this test and the miner's both fail with the same new value, that is a CONSTRUCTION CHANGE:
    /// `leaf_root` lives inside `content_id()`, so every `batch_id` in existence moves and the answer is
    /// a re-genesis, not a fixture edit. If only ONE of them fails, that is the drift this exists to
    /// catch, and the failing side is the side that is wrong.
    #[test]
    fn palw_leaf_merkle_root_cross_crate_golden_vector() {
        // MIRRORED VERBATIM from mil/miner/src/registration.rs. Keep the two copies textually identical;
        // do NOT factor them into a shared item, which would defeat the purpose. The permitted moves are
        // explicit re-genesis cutovers: the Header-v4/Object-v2 one paired with DB v14, and the ADR-0045
        // D3-b PCPB LeafV2 one paired with DB v16 (five new leaf fields ⇒ every `leaf_hash` moves ⇒ every
        // `leaf_root`, `content_id()` and `batch_id` moves). An in-place upgrade must not refresh these
        // values, because doing so redefines every content-addressed batch.
        const CROSS_CRATE_GOLDEN_LEAF_HASHES: [&str; 3] = [
            "98d226d4654d55e5c118ad2e4adaae03cbfcd86cc8990c7da9e407084f119021\
             3e789dd59e646feec887fa876bb22015187350993b50756a02d74007c8138c40",
            "10f1f3b308b3ab2fcbdd7189676c751f39fe2ed30144cc73e71298ad85e95199\
             b19d6d3d472f8aab4f83b710d943cea6ed8a6d3ddcb8555220026d50f42c68c5",
            "77e420786bfd24315022c4f305a405550d3136ef25e705c206b4a8c59b62eedf\
             ad63c352130f7104550113fa290169d5ca280f25dad40037c40cceaef412e988",
        ];
        const CROSS_CRATE_GOLDEN_LEAF_ROOT: &str = "bc2b08b3ee25a4c761fec1e5313f06b2fde1ab205e1b1cd734578d92e645b1166\
             ffe1f6eacf8140d0b8abcd78efa079cd38dd6292dad5446c70811ba480bbfb4";

        let hashes: Vec<Hash64> = CROSS_CRATE_GOLDEN_LEAF_HASHES.iter().map(|s| s.parse::<Hash64>().expect("hex")).collect();
        let root = palw_leaf_merkle_root(&hashes);
        assert_eq!(
            root.to_string(),
            CROSS_CRATE_GOLDEN_LEAF_ROOT,
            "consensus-core no longer reduces the cross-crate golden vector to its pinned root — the \
             miner's mirror of this constant will now disagree, and on-chain the disagreement is SILENT \
             (apply_palw_overlay_effect's result is discarded at virtual_processor/processor.rs:1800)"
        );

        // The golden is a 3-leaf (non-power-of-two) vector precisely so the H_EMPTY padding slot is
        // inside the pinned value: a "simplification" back to tail duplication moves this literal.
        assert_eq!(palw_leaf_merkle_depth(hashes.len() as u32), 2);

        // And every member of the golden vector opens the golden root under the verifier the acceptance
        // gate will use — so the constant pins the PROOF path, not merely the root path.
        for (i, h) in hashes.iter().enumerate() {
            let proof = palw_leaf_merkle_proof(&hashes, i as u32).expect("in-range");
            assert_eq!(proof.len(), 2, "uniform depth");
            assert!(palw_verify_leaf_membership(h, i as u32, 3, &proof, &root), "golden leaf {i} must open the golden root");
        }
    }

    /// The two properties the FLAT `palw_leaf_root` asserted (order sensitivity, count sensitivity)
    /// survive the move to a Merkle root — order via the index-bound leaf nodes, count via the `u64`-LE
    /// prefix in the final root. Losing either would let one leaf multiset address two batches, or two
    /// multisets address one.
    #[test]
    fn palw_leaf_merkle_root_is_order_and_count_sensitive() {
        let (la, lb, lc) = (h(1), h(2), h(3));
        assert_ne!(palw_leaf_merkle_root(&[la, lb]), palw_leaf_merkle_root(&[lb, la]), "order must matter");
        assert_ne!(palw_leaf_merkle_root(&[la]), palw_leaf_merkle_root(&[la, lb]), "count must matter");
        // Count sensitivity is NOT merely a consequence of the padding: 3 and 4 leaves share a depth, so
        // only the count prefix separates a 3-leaf tree from the 4-leaf tree whose last leaf hashes to
        // the padding constant. (It cannot, but the prefix means we never have to argue about it.)
        assert_ne!(palw_leaf_merkle_root(&[la, lb, lc]), palw_leaf_merkle_root(&[la, lb, lc, palw_leaf_merkle_empty()]));
    }

    /// A well-formed proof verifies; a proof with a corrupted sibling, a proof presented under the wrong
    /// index, and the RIGHT leaf presented at the WRONG index all fail.
    ///
    /// The last of these is the one that matters for CHUNK-INDEX SQUAT: it is exactly the move a
    /// squatter makes when it copies a public `batch_id` and tries to place a member leaf somewhere it
    /// can control. Index binding inside the leaf node is what forecloses it.
    #[test]
    fn palw_leaf_membership_proof_verifies_and_rejects_forgeries() {
        let leaves: Vec<Hash64> = (1u8..=5).map(h).collect();
        let root = palw_leaf_merkle_root(&leaves);
        let n = leaves.len() as u32;

        for i in 0..n {
            let proof = palw_leaf_merkle_proof(&leaves, i).expect("in-range index has a proof");
            assert!(palw_verify_leaf_membership(&leaves[i as usize], i, n, &proof, &root), "honest leaf {i} must verify");
        }
        assert!(palw_leaf_merkle_proof(&leaves, n).is_none(), "out-of-range index has no proof");

        let proof0 = palw_leaf_merkle_proof(&leaves, 0).unwrap();

        // (a) swapped/corrupted sibling.
        let mut swapped = proof0.clone();
        swapped.siblings[0] = h(0xee);
        assert!(!palw_verify_leaf_membership(&leaves[0], 0, n, &swapped, &root), "a corrupted sibling must not verify");

        // (b) leaf 0's proof replayed under a different index (the path is wrong AND the node is wrong).
        assert!(!palw_verify_leaf_membership(&leaves[0], 1, n, &proof0, &root), "leaf 0's proof must not verify at index 1");

        // (c) the RIGHT leaf at the WRONG index, carrying that index's own proof. Only index binding
        //     stops this one — the fold alone would accept it in a tree without it.
        let proof1 = palw_leaf_merkle_proof(&leaves, 1).unwrap();
        assert!(!palw_verify_leaf_membership(&leaves[0], 1, n, &proof1, &root), "a member leaf must not move to another index");

        // (d) a leaf that is not a member at all.
        assert!(!palw_verify_leaf_membership(&h(0x77), 0, n, &proof0, &root), "a non-member must not verify");

        // (e) an INTERNAL digest offered as a leaf. The disjoint leaf/node domains make the classic
        //     leaf/internal confusion unrepresentable.
        let internal = palw_leaf_merkle_internal_node(&h(1), &h(2));
        assert!(!palw_verify_leaf_membership(&internal, 0, n, &proof0, &root), "an internal digest is not a leaf");

        // (f) wrong leaf_count ⇒ wrong depth and wrong root prefix.
        assert!(!palw_verify_leaf_membership(&leaves[0], 0, n + 1, &proof0, &root));
        assert!(!palw_verify_leaf_membership(&leaves[0], 0, n - 1, &proof0, &root));
    }

    /// Uniform padding: for a leaf count that is NOT a power of two, every proof is exactly
    /// `d = ceil(log2(n))` siblings — including the proofs of the leaves adjacent to the padded slots.
    ///
    /// This is the property that lets the acceptance gate demand an EXACT length (making the proof for a
    /// given `(leaf, index, root)` unique) and the context-free validator demand a static `<= 8`. Tail
    /// duplication would break it and reopen the odd-arity second-preimage family.
    #[test]
    fn palw_leaf_merkle_padding_is_uniform_for_non_power_of_two() {
        for n in [1usize, 2, 3, 5, 7, 9, 100, 255, 256] {
            let leaves: Vec<Hash64> = (0..n).map(|i| Hash64::from_bytes([(i % 251) as u8 + 1; 64])).collect();
            let root = palw_leaf_merkle_root(&leaves);
            let d = palw_leaf_merkle_depth(n as u32);
            assert!(d as usize <= PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN, "n={n}: depth must fit the static wire bound");
            for i in 0..n as u32 {
                let proof = palw_leaf_merkle_proof(&leaves, i).unwrap();
                assert_eq!(proof.len() as u32, d, "n={n}, i={i}: proof length must be the uniform depth");
                assert!(palw_verify_leaf_membership(&leaves[i as usize], i, n as u32, &proof, &root), "n={n}, i={i}");
            }
        }

        // The loop above CANNOT detect the mutation this test's doc names. If the root builder and the
        // proof builder BOTH switched to tail duplication, every proof would still open its root and
        // every length would still be `d` — the loop only proves the two halves agree with each other,
        // not that they agree with the SPEC. An adversarial review confirmed exactly that blind spot.
        //
        // So pin the padding by VALUE at the one place the two constructions differ: for a leaf count
        // that is not a power of two, the last leaf's level-0 sibling is the constant `H_EMPTY` under
        // uniform padding, and would be a COPY OF THAT LEAF'S OWN NODE under tail duplication.
        let three: Vec<Hash64> = (0..3u8).map(|i| Hash64::from_bytes([i + 1; 64])).collect();
        let last = palw_leaf_merkle_proof(&three, 2).expect("index 2 is a member of a 3-leaf tree");
        assert_eq!(
            last.siblings[0],
            palw_leaf_merkle_empty(),
            "n=3: the level-0 sibling of the last leaf must be the H_EMPTY constant. If this is instead a \
             copy of leaf 2's own node, the padding regressed to tail duplication, which reopens the \
             odd-arity second-preimage family (ADR-0040 §5.15.4)."
        );
    }

    /// Second preimage across POSITIONS — the property index binding buys, stated on its own so a
    /// future "the index is redundant, the path already encodes it" cleanup fails loudly.
    ///
    /// Two leaves with IDENTICAL content at different indices produce different level-0 nodes, and
    /// neither one's proof transfers to the other's slot.
    #[test]
    fn palw_leaf_merkle_leaf_cannot_be_replayed_at_another_index() {
        let dup = h(0x5a);
        let leaves = [dup, h(0x01), dup, h(0x02)];
        let root = palw_leaf_merkle_root(&leaves);

        assert_ne!(
            palw_leaf_merkle_leaf_node(0, &dup),
            palw_leaf_merkle_leaf_node(2, &dup),
            "identical leaf content at different indices must be different nodes"
        );

        let p0 = palw_leaf_merkle_proof(&leaves, 0).unwrap();
        let p2 = palw_leaf_merkle_proof(&leaves, 2).unwrap();
        assert!(palw_verify_leaf_membership(&dup, 0, 4, &p0, &root));
        assert!(palw_verify_leaf_membership(&dup, 2, 4, &p2, &root));
        // ...but the proofs do not transfer, even though the leaf content is byte-identical.
        assert!(!palw_verify_leaf_membership(&dup, 2, 4, &p0, &root));
        assert!(!palw_verify_leaf_membership(&dup, 0, 4, &p2, &root));
    }

    /// The context-free half of the ADR-0040 §5.15 + D3-b leaf-chunk rules: v3 is MANDATORY, `proofs`
    /// and `witnesses` are arity-checked against `leaves`, and proof length has a static upper bound.
    /// The EXACT length check is deliberately absent here — see `validate_leaf_chunk`'s doc for why the
    /// split exists.
    #[test]
    fn palw_leaf_chunk_v3_is_mandatory_and_proofs_are_arity_bounded() {
        let mk = |n: u32| {
            let leaves: Vec<PalwPublicLeafV1> = (0..n)
                .map(|i| {
                    let mut l = sample_leaf();
                    l.leaf_index = i;
                    l.job_nullifier = h(0x10 + i as u8);
                    l.ticket_nullifier_commitment = h(0x20 + i as u8);
                    l
                })
                .collect();
            let hashes: Vec<Hash64> = leaves.iter().map(|l| l.leaf_hash()).collect();
            let proofs: Vec<_> = (0..n).map(|i| palw_leaf_merkle_proof(&hashes, i).unwrap()).collect();
            let witnesses: Vec<_> = (0..n).map(|_| dummy_external_witness()).collect();
            PalwLeafChunkV1 {
                version: PALW_LEAF_CHUNK_VERSION_V3,
                batch_id: leaves[0].batch_id,
                chunk_index: 0,
                leaves,
                proofs,
                witnesses,
            }
        };

        let good = mk(3);
        assert_eq!(validate_palw_overlay_payload(0x32, &borsh::to_vec(&good).unwrap()), Ok(()));

        // D3-b §8: the v3 leaf cap is enforced context-free — a chunk one leaf over
        // `PALW_MAX_LEAVES_PER_CHUNK` is refused before any contextual work, so the payload budget
        // asserted below (a FULL chunk fits the byte cap) is a bound on the largest admissible
        // chunk, not on a shape an oversized chunk could exceed.
        let over = mk(PALW_MAX_LEAVES_PER_CHUNK as u32 + 1);
        assert_eq!(
            validate_palw_overlay_payload(0x32, &borsh::to_vec(&over).unwrap()),
            Err(PalwTxError::InvalidCount {
                field: "leaf_chunk.leaves",
                count: PALW_MAX_LEAVES_PER_CHUNK + 1,
                min: 1,
                max: PALW_MAX_LEAVES_PER_CHUNK
            })
        );

        // v1 is REJECTED. Nothing "falls back" to an empty `proofs` — that lenient parse is the hole.
        let mut v1 = good.clone();
        v1.version = PALW_PAYLOAD_VERSION_V1;
        assert_eq!(validate_palw_overlay_payload(0x32, &borsh::to_vec(&v1).unwrap()), Err(PalwTxError::UnsupportedVersion(1)));
        // v2 is REJECTED one version up, for the D3-b reason: an empty-defaulted `witnesses` would ship
        // a leaf the PCPB gate never examined — the exact partial-gating state clause 12 forbids.
        let mut v2 = good.clone();
        v2.version = PALW_LEAF_CHUNK_VERSION_V2;
        assert_eq!(validate_palw_overlay_payload(0x32, &borsh::to_vec(&v2).unwrap()), Err(PalwTxError::UnsupportedVersion(2)));
        // And an unknown future version is refused too.
        let mut v4 = good.clone();
        v4.version = 4;
        assert_eq!(validate_palw_overlay_payload(0x32, &borsh::to_vec(&v4).unwrap()), Err(PalwTxError::UnsupportedVersion(4)));

        // witnesses.len() must equal leaves.len(), in both directions (D3-b arity).
        let mut wshort = good.clone();
        wshort.witnesses.pop();
        assert_eq!(
            validate_palw_overlay_payload(0x32, &borsh::to_vec(&wshort).unwrap()),
            Err(PalwTxError::InvalidCount { field: "leaf_chunk.witnesses", count: 2, min: 3, max: 3 })
        );
        let mut wlong = good.clone();
        wlong.witnesses.push(wlong.witnesses[0].clone());
        assert_eq!(
            validate_palw_overlay_payload(0x32, &borsh::to_vec(&wlong).unwrap()),
            Err(PalwTxError::InvalidCount { field: "leaf_chunk.witnesses", count: 4, min: 3, max: 3 })
        );

        // proofs.len() must equal leaves.len(), in both directions.
        let mut short = good.clone();
        short.proofs.pop();
        assert_eq!(
            validate_palw_overlay_payload(0x32, &borsh::to_vec(&short).unwrap()),
            Err(PalwTxError::InvalidCount { field: "leaf_chunk.proofs", count: 2, min: 3, max: 3 })
        );
        let mut long = good.clone();
        long.proofs.push(long.proofs[0].clone());
        assert_eq!(
            validate_palw_overlay_payload(0x32, &borsh::to_vec(&long).unwrap()),
            Err(PalwTxError::InvalidCount { field: "leaf_chunk.proofs", count: 4, min: 3, max: 3 })
        );

        // A proof longer than the 256-leaf static bound is refused cheaply, before any Merkle work.
        let mut oversized = good.clone();
        oversized.proofs[1].siblings = vec![h(0xcc); PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN + 1];
        assert_eq!(
            validate_palw_overlay_payload(0x32, &borsh::to_vec(&oversized).unwrap()),
            Err(PalwTxError::InvalidCount {
                field: "leaf_chunk.proof_len",
                count: PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN + 1,
                min: 0,
                max: PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN
            })
        );
        // ...but a proof of the WRONG length within the bound passes here. That is by design: the exact
        // check needs `manifest.leaf_count` and lives at the acceptance gate.
        let mut wrong_len = good.clone();
        wrong_len.proofs[1].siblings.pop();
        assert_eq!(validate_palw_overlay_payload(0x32, &borsh::to_vec(&wrong_len).unwrap()), Ok(()));

        // D3-b payload budget (design memo §5): a FULL 24-leaf chunk with maximum-depth membership
        // proofs and REALISTIC worst-case SelfSerial witnesses (16-sibling snapshot/assignment paths =
        // 65 536 providers, a 4 KiB receipt preimage, real ML-DSA-87 key/signature widths) must fit the
        // 512 KiB cap — measured on the exact encoding, not estimated.
        let full = {
            let mut c = mk(PALW_MAX_LEAVES_PER_CHUNK as u32);
            for p in c.proofs.iter_mut() {
                p.siblings = vec![h(0xcc); PALW_MAX_LEAF_MEMBERSHIP_PROOF_LEN];
            }
            let deep = |b: u8| PalwMerkleMembership { index: 0, leaf: h(b), siblings: vec![h(b); 16] };
            let entry = |b: u8| PalwProviderSnapshotEntry {
                provider_id: h(b),
                ml_dsa_pk_hash: h(b.wrapping_add(1)),
                bond_sompi: 10,
                reward_script_commitment: h(b.wrapping_add(2)),
            };
            for w in c.witnesses.iter_mut() {
                w.dispatch = PalwDispatchEvidence::SelfSerial(SelfSerialProof {
                    a_commit: h(0xa1),
                    a_entry: entry(0xa2),
                    a_snapshot_membership: deep(0xa3),
                    b_entry: entry(0xb2),
                    b_snapshot_membership: deep(0xb3),
                    b_interval: PalwAssignmentInterval { provider_id: h(0xb2), bond_sompi: 10, cumulative_lo: 0 },
                    b_assignment_membership: deep(0xb4),
                    b_ml_dsa_pk: vec![0x41; STAKE_VALIDATOR_PUBKEY_LEN],
                    b_receipt_preimage: vec![0x42; 4096],
                    b_signature: vec![0x43; STAKE_ATTESTATION_SIG_LEN],
                });
            }
            c
        };
        let bytes = borsh::to_vec(&full).unwrap();
        assert!(
            bytes.len() < PALW_MAX_OVERLAY_PAYLOAD_BYTES,
            "a full {PALW_MAX_LEAVES_PER_CHUNK}-leaf v3 chunk with worst-case honest witnesses is {} bytes and must fit the {PALW_MAX_OVERLAY_PAYLOAD_BYTES}-byte cap",
            bytes.len()
        );
        // The all-maximal ADVERSARIAL shape (32-sibling paths + 8 KiB preimages) deliberately exceeds
        // the cap — the byte-size gate rejects it BEFORE decoding, which is the intended ordering (the
        // shape bounds exist to bound decode work on payloads that pass the size gate, not to admit
        // every combination of individual maxima).
        let hostile = {
            let mut c = full.clone();
            for w in c.witnesses.iter_mut() {
                if let PalwDispatchEvidence::SelfSerial(p) = &mut w.dispatch {
                    p.a_snapshot_membership.siblings = vec![h(0xdd); PALW_MAX_PCPB_MEMBERSHIP_SIBLINGS];
                    p.b_snapshot_membership.siblings = vec![h(0xdd); PALW_MAX_PCPB_MEMBERSHIP_SIBLINGS];
                    p.b_assignment_membership.siblings = vec![h(0xdd); PALW_MAX_PCPB_MEMBERSHIP_SIBLINGS];
                    p.b_receipt_preimage = vec![0x44; PALW_MAX_PCPB_RECEIPT_PREIMAGE_BYTES];
                }
            }
            c
        };
        let hostile_bytes = borsh::to_vec(&hostile).unwrap();
        assert!(hostile_bytes.len() > PALW_MAX_OVERLAY_PAYLOAD_BYTES, "expected the all-maximal shape to exceed the size gate");
        assert_eq!(
            validate_palw_overlay_payload(0x32, &hostile_bytes),
            Err(PalwTxError::PayloadTooLarge { len: hostile_bytes.len(), max: PALW_MAX_OVERLAY_PAYLOAD_BYTES })
        );
    }

    /// ADR-0040 §5.15.12 — a TRUE pairwise-distinctness assertion over the PALW keyed domains.
    ///
    /// `retired_slot_domain_is_never_reused` only checks each domain against the one retired string; it
    /// is not a distinctness test and it cannot notice an unregistered new constant. Two live domains
    /// sharing a string would silently merge two hash families.
    #[test]
    fn palw_keyed_domains_are_pairwise_distinct() {
        let domains: &[(&str, &[u8])] = &[
            ("PALW_LEAF_DOMAIN", PALW_LEAF_DOMAIN),
            ("PALW_LEAF_ROOT_DOMAIN", PALW_LEAF_ROOT_DOMAIN),
            ("PALW_LEAF_MERKLE_LEAF_DOMAIN", PALW_LEAF_MERKLE_LEAF_DOMAIN),
            ("PALW_LEAF_MERKLE_NODE_DOMAIN", PALW_LEAF_MERKLE_NODE_DOMAIN),
            ("PALW_LEAF_MERKLE_EMPTY_DOMAIN", PALW_LEAF_MERKLE_EMPTY_DOMAIN),
            ("PALW_BATCH_ID_DOMAIN", PALW_BATCH_ID_DOMAIN),
            ("PALW_CHAIN_COMMIT_DOMAIN", PALW_CHAIN_COMMIT_DOMAIN),
            ("PALW_ELIGIBILITY_DOMAIN", PALW_ELIGIBILITY_DOMAIN),
            ("PALW_AUTHORIZATION_DOMAIN", PALW_AUTHORIZATION_DOMAIN),
            ("PALW_BEACON_DOMAIN", PALW_BEACON_DOMAIN),
            ("PALW_BEACON_COMMIT_DOMAIN", PALW_BEACON_COMMIT_DOMAIN),
            ("PALW_BEACON_COMMIT_SIGNING_DOMAIN", PALW_BEACON_COMMIT_SIGNING_DOMAIN),
            ("PALW_BEACON_REVEAL_SIGNING_DOMAIN", PALW_BEACON_REVEAL_SIGNING_DOMAIN),
            ("PALW_BEACON_REVEAL_ENTROPY_DOMAIN", PALW_BEACON_REVEAL_ENTROPY_DOMAIN),
            ("PALW_DNS_CERT_DOMAIN", PALW_DNS_CERT_DOMAIN),
            ("PALW_AUDITOR_VOTE_V2_DOMAIN", PALW_AUDITOR_VOTE_V2_DOMAIN),
            ("PALW_AUDITOR_SELECT_DOMAIN", PALW_AUDITOR_SELECT_DOMAIN),
            ("PALW_AUDITOR_SET_DOMAIN", PALW_AUDITOR_SET_DOMAIN),
            ("PALW_PROVIDER_SELECT_DOMAIN", PALW_PROVIDER_SELECT_DOMAIN),
            ("PALW_TICKET_NULLIFIER_COMMIT_DOMAIN", PALW_TICKET_NULLIFIER_COMMIT_DOMAIN),
            ("PALW_MATCH_DOMAIN", PALW_MATCH_DOMAIN),
            ("PALW_RECEIPT_DOMAIN", PALW_RECEIPT_DOMAIN),
            ("PALW_MISMATCH_ESCALATE_DOMAIN", PALW_MISMATCH_ESCALATE_DOMAIN),
            ("PALW_WEIGHTED_DRAW_DOMAIN", PALW_WEIGHTED_DRAW_DOMAIN),
            ("PALW_AUDIT_SAMPLE_DOMAIN", PALW_AUDIT_SAMPLE_DOMAIN),
            ("PALW_AUDIT_SAMPLE_ROOT_DOMAIN", PALW_AUDIT_SAMPLE_ROOT_DOMAIN),
            ("PALW_PCPB_DOMAIN", PALW_PCPB_DOMAIN),
            ("PALW_RETIRED_SLOT_DOMAIN", PALW_RETIRED_SLOT_DOMAIN),
            ("PALW_RETIRED_AUDITOR_VOTE_V1_DOMAIN", PALW_RETIRED_AUDITOR_VOTE_V1_DOMAIN),
            ("PALW_PROVIDER_UNBOND_DOMAIN", PALW_PROVIDER_UNBOND_DOMAIN),
            ("PALW_SNAPSHOT_LEAF_DOMAIN", PALW_SNAPSHOT_LEAF_DOMAIN),
            ("PALW_SNAPSHOT_NODE_DOMAIN", PALW_SNAPSHOT_NODE_DOMAIN),
            ("PALW_ASSIGNMENT_LEAF_DOMAIN", PALW_ASSIGNMENT_LEAF_DOMAIN),
            ("PALW_ASSIGNMENT_NODE_DOMAIN", PALW_ASSIGNMENT_NODE_DOMAIN),
            ("PALW_ASSIGNMENT_DRAW_DOMAIN", PALW_ASSIGNMENT_DRAW_DOMAIN),
            ("PALW_PROVIDER_PK_HASH_DOMAIN", PALW_PROVIDER_PK_HASH_DOMAIN),
            ("PALW_PROVIDER_ID_DOMAIN", PALW_PROVIDER_ID_DOMAIN),
            ("PALW_JOB_CHALLENGE_DOMAIN", PALW_JOB_CHALLENGE_DOMAIN),
            // Model-agnostic Compute Set registry (palw_compute_set.rs) — same keyed-domain
            // namespace, so they join this single distinctness registry.
            ("PALW_COMPUTE_SET_ID_DOMAIN", crate::palw_compute_set::PALW_COMPUTE_SET_ID_DOMAIN),
            ("PALW_COMPUTE_VM_ID_DOMAIN", crate::palw_compute_set::PALW_COMPUTE_VM_ID_DOMAIN),
            ("PALW_COMPUTE_POLICY_ID_DOMAIN", crate::palw_compute_set::PALW_COMPUTE_POLICY_ID_DOMAIN),
            ("PALW_ALLOCATION_PLAN_ID_DOMAIN", crate::palw_compute_set::PALW_ALLOCATION_PLAN_ID_DOMAIN),
            ("PALW_COMPUTE_IR_PROGRAM_DOMAIN", crate::palw_compute_ir::PALW_COMPUTE_IR_PROGRAM_DOMAIN),
        ];
        for (i, (na, a)) in domains.iter().enumerate() {
            for (nb, b) in domains.iter().skip(i + 1) {
                assert_ne!(a, b, "PALW keyed domains {na} and {nb} are the same string");
            }
            assert!(a.len() <= 64, "domain {na} exceeds the BLAKE2b key limit");
        }
    }

    /// ADR-0040 TGT-02 — the retired slot domain stays retired. Deleting `slot_digest` freed its keyed
    /// domain string, and silently reusing it for a NEW derivation would let a future digest collide
    /// with one produced by the removed function. The reservation was written as a comment, which
    /// cannot fail a build; this is the enforcement.
    #[test]
    fn retired_slot_domain_is_never_reused() {
        for (name, d) in [
            ("PALW_LEAF_DOMAIN", PALW_LEAF_DOMAIN),
            ("PALW_CHAIN_COMMIT_DOMAIN", PALW_CHAIN_COMMIT_DOMAIN),
            ("PALW_ELIGIBILITY_DOMAIN", PALW_ELIGIBILITY_DOMAIN),
            ("PALW_AUTHORIZATION_DOMAIN", PALW_AUTHORIZATION_DOMAIN),
            ("PALW_BEACON_DOMAIN", PALW_BEACON_DOMAIN),
            ("PALW_BEACON_COMMIT_DOMAIN", PALW_BEACON_COMMIT_DOMAIN),
            ("PALW_DNS_CERT_DOMAIN", PALW_DNS_CERT_DOMAIN),
            ("PALW_LEAF_ROOT_DOMAIN", PALW_LEAF_ROOT_DOMAIN),
            ("PALW_LEAF_MERKLE_LEAF_DOMAIN", PALW_LEAF_MERKLE_LEAF_DOMAIN),
            ("PALW_LEAF_MERKLE_NODE_DOMAIN", PALW_LEAF_MERKLE_NODE_DOMAIN),
            ("PALW_LEAF_MERKLE_EMPTY_DOMAIN", PALW_LEAF_MERKLE_EMPTY_DOMAIN),
            ("PALW_BATCH_ID_DOMAIN", PALW_BATCH_ID_DOMAIN),
            ("PALW_TICKET_NULLIFIER_COMMIT_DOMAIN", PALW_TICKET_NULLIFIER_COMMIT_DOMAIN),
            ("PALW_AUDITOR_VOTE_V2_DOMAIN", PALW_AUDITOR_VOTE_V2_DOMAIN),
            ("PALW_MATCH_DOMAIN", PALW_MATCH_DOMAIN),
            ("PALW_RECEIPT_DOMAIN", PALW_RECEIPT_DOMAIN),
            ("PALW_PROVIDER_SELECT_DOMAIN", PALW_PROVIDER_SELECT_DOMAIN),
            ("PALW_AUDITOR_SELECT_DOMAIN", PALW_AUDITOR_SELECT_DOMAIN),
            ("PALW_MISMATCH_ESCALATE_DOMAIN", PALW_MISMATCH_ESCALATE_DOMAIN),
            ("PALW_WEIGHTED_DRAW_DOMAIN", PALW_WEIGHTED_DRAW_DOMAIN),
            ("PALW_AUDIT_SAMPLE_DOMAIN", PALW_AUDIT_SAMPLE_DOMAIN),
            ("PALW_AUDIT_SAMPLE_ROOT_DOMAIN", PALW_AUDIT_SAMPLE_ROOT_DOMAIN),
        ] {
            assert_ne!(d, PALW_RETIRED_SLOT_DOMAIN, "{name} reuses the retired ADR-0040 TGT-02 slot domain");
        }
    }

    #[test]
    fn domain_strings_are_pinned_and_fit_key_limit() {
        assert_eq!(PALW_LEAF_DOMAIN, b"misaka-palw-v1/leaf");
        assert_eq!(PALW_CHAIN_COMMIT_DOMAIN, b"misaka-palw-chain-commit-v1");
        assert_eq!(PALW_ELIGIBILITY_DOMAIN, b"misaka-palw-eligibility-v1");
        assert_eq!(PALW_BEACON_DOMAIN, b"misaka-palw-beacon-v1");
        assert_eq!(PALW_BEACON_COMMIT_DOMAIN, b"misaka-palw-beacon-commit-v1");
        assert_eq!(PALW_DNS_CERT_DOMAIN, b"misaka-palw-dns-cert-v1");
        assert_eq!(PALW_LEAF_ROOT_DOMAIN, b"misaka-palw-leaf-root-v1");
        assert_eq!(PALW_LEAF_MERKLE_LEAF_DOMAIN, b"misaka-palw-leaf-merkle-leaf-v1");
        assert_eq!(PALW_LEAF_MERKLE_NODE_DOMAIN, b"misaka-palw-leaf-merkle-node-v1");
        assert_eq!(PALW_LEAF_MERKLE_EMPTY_DOMAIN, b"misaka-palw-leaf-merkle-empty-v1");
        assert_eq!(PALW_BATCH_ID_DOMAIN, b"misaka-palw-batch-id-v1");
        assert_eq!(PALW_TICKET_NULLIFIER_COMMIT_DOMAIN, b"misaka-palw-ticket-nf-commit-v1");
        assert_eq!(PALW_AUDITOR_VOTE_V2_DOMAIN, b"misaka-palw-auditor-vote-v2");
        assert_eq!(PALW_RETIRED_AUDITOR_VOTE_V1_DOMAIN, b"misaka-palw-auditor-vote-v1");
        assert_eq!(PALW_MATCH_DOMAIN, b"misaka-palw-match-v1");
        assert_eq!(PALW_RECEIPT_DOMAIN, b"misaka-palw-replica-receipt-v1");
        assert_eq!(PALW_PROVIDER_SELECT_DOMAIN, b"misaka-palw-provider-select-v1");
        assert_eq!(PALW_AUDITOR_SELECT_DOMAIN, b"misaka-palw-auditor-select-v1");
        assert_eq!(PALW_AUDITOR_SET_DOMAIN, b"misaka-palw-auditor-set-v1");
        assert_eq!(PALW_MISMATCH_ESCALATE_DOMAIN, b"misaka-palw-mismatch-escalate-v1");
        assert_eq!(PALW_WEIGHTED_DRAW_DOMAIN, b"misaka-palw-weighted-draw-v1");
        assert_eq!(PALW_AUDIT_SAMPLE_DOMAIN, b"misaka-palw-audit-sample-v1");
        assert_eq!(PALW_AUDIT_SAMPLE_ROOT_DOMAIN, b"misaka-palw-audit-sample-root-v1");
        // ADR-0040 §5.14.7 PCPB verifiable-evidence domains.
        assert_eq!(PALW_SNAPSHOT_LEAF_DOMAIN, b"misaka-palw-pcpb-snapshot-leaf-v1");
        assert_eq!(PALW_SNAPSHOT_NODE_DOMAIN, b"misaka-palw-pcpb-snapshot-node-v1");
        assert_eq!(PALW_ASSIGNMENT_LEAF_DOMAIN, b"misaka-palw-pcpb-assign-leaf-v1");
        assert_eq!(PALW_ASSIGNMENT_NODE_DOMAIN, b"misaka-palw-pcpb-assign-node-v1");
        assert_eq!(PALW_ASSIGNMENT_DRAW_DOMAIN, b"misaka-palw-pcpb-assign-draw-v1");
        assert_eq!(PALW_PROVIDER_PK_HASH_DOMAIN, b"misaka-palw-pcpb-provider-pk-v1");
        assert_eq!(PALW_PROVIDER_ID_DOMAIN, b"misaka-palw-pcpb-provider-id-v1");
        assert_eq!(PALW_JOB_CHALLENGE_DOMAIN, b"misaka-palw-bridge-v1/job-challenge");
        assert_eq!(PALW_PCPB_RECEIPT_TAG, b"misaka-palw-pcpb-receipt-binds-a-commit-v1");
        // ML-DSA-87 FIPS-204 `ctx` strings (disjoint per operation).
        assert_eq!(PALW_BEACON_MLDSA87_CONTEXT, b"PALWBeaconV1");
        assert_eq!(PALW_AUDITOR_V2_MLDSA87_CONTEXT, b"PALWAuditorVoteV2");
        assert_eq!(PALW_RETIRED_AUDITOR_V1_MLDSA87_CONTEXT, b"PALWAuditorVoteV1");
        assert_eq!(PALW_AUTHORIZATION_MLDSA87_CONTEXT, b"PALWBlockAuthorizationV1");
        assert_eq!(PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, b"PALWPcpbReceiptV1");
        for d in [
            PALW_LEAF_DOMAIN,
            PALW_LEAF_ROOT_DOMAIN,
            PALW_LEAF_MERKLE_LEAF_DOMAIN,
            PALW_LEAF_MERKLE_NODE_DOMAIN,
            PALW_LEAF_MERKLE_EMPTY_DOMAIN,
            PALW_CHAIN_COMMIT_DOMAIN,
            PALW_ELIGIBILITY_DOMAIN,
            PALW_BEACON_DOMAIN,
            PALW_BEACON_COMMIT_DOMAIN,
            PALW_MATCH_DOMAIN,
            PALW_RECEIPT_DOMAIN,
            PALW_PROVIDER_SELECT_DOMAIN,
            PALW_AUDITOR_SELECT_DOMAIN,
            PALW_AUDITOR_SET_DOMAIN,
            PALW_MISMATCH_ESCALATE_DOMAIN,
            PALW_WEIGHTED_DRAW_DOMAIN,
            PALW_AUDIT_SAMPLE_DOMAIN,
            PALW_AUDIT_SAMPLE_ROOT_DOMAIN,
            PALW_SNAPSHOT_LEAF_DOMAIN,
            PALW_SNAPSHOT_NODE_DOMAIN,
            PALW_ASSIGNMENT_LEAF_DOMAIN,
            PALW_ASSIGNMENT_NODE_DOMAIN,
            PALW_ASSIGNMENT_DRAW_DOMAIN,
            PALW_PROVIDER_PK_HASH_DOMAIN,
            PALW_PROVIDER_ID_DOMAIN,
            PALW_JOB_CHALLENGE_DOMAIN,
        ] {
            assert!(d.len() <= 64, "domain {:?} exceeds BLAKE2b key limit", core::str::from_utf8(d));
        }
    }

    /// `auditor_set_commitment` is deterministic and independent of the caller's input order (it sorts
    /// the bonds), and distinguishes different auditor slates.
    #[test]
    fn auditor_set_commitment_is_order_independent_and_binding() {
        let op = |n: u8| TransactionOutpoint { transaction_id: h(n), index: n as u32 };
        let a = auditor_set_commitment(&[op(3), op(1), op(2)]);
        let b = auditor_set_commitment(&[op(1), op(2), op(3)]);
        assert_eq!(a, b, "commitment is independent of input order");
        assert_ne!(a, auditor_set_commitment(&[op(1), op(2)]), "a different slate ⇒ a different commitment");
        assert_ne!(a, auditor_set_commitment(&[op(1), op(2), op(4)]), "swapping one auditor changes the commitment");
    }

    // ADR-0040 SEL-01 (§5.17.5) — the credential-aggregated, bond-weighted, non-replacement sampler.

    /// A provider-bond record with a chosen credential (`owner_pubkey_hash`), operator group, resolved
    /// collateral, activation DAA, and bond outpoint. `owner_public_key` is unused by aggregation
    /// (which keys on the hash), so it is left empty.
    fn prov_rec(
        credential: Hash64,
        op_group: Hash64,
        amount: u64,
        activation: u64,
        bond: TransactionOutpoint,
    ) -> PalwProviderBondRecord {
        PalwProviderBondRecord {
            version: PALW_PAYLOAD_VERSION_V1,
            bond_outpoint: bond,
            owner_pubkey_hash: credential,
            owner_public_key: Vec::new(),
            operator_group_id: op_group,
            runtime_classes: Vec::new(),
            capacity_by_shape: Vec::new(),
            reward_key_root: Hash64::default(),
            amount_sompi: amount,
            activation_daa_score: activation,
            created_daa_score: activation,
            unbond_delay_epochs: 10,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        }
    }

    fn view_of(recs: &[PalwProviderBondRecord]) -> ProviderBondView {
        ProviderBondView::from_records(recs.iter().map(|r| (r.bond_outpoint, r.clone())))
    }

    /// SEL-01 split-bond case. Splitting one credential's bond of `N` into `k` outpoints
    /// of `N/k` must NOT increase its selections. Because [`aggregate_provider_credentials_at`] collapses
    /// a credential's outpoints into one weight BEFORE the draw, the split world produces the identical
    /// candidate set — hence a byte-identical committee for every seed, and an identical count of how
    /// often the attacker is selected across many independent draws. `k ∈ {2, 10, 100}`, `N` divisible
    /// by all three.
    #[test]
    fn sel01_credential_aggregation_makes_bond_splitting_worthless() {
        const POV: u64 = 1_000;
        const N: u64 = 1_000_000;
        let (ca, cb, cd, grp) = (h(0xA0), h(0xB0), h(0xD0), h(0x01));
        let empty: HashSet<Hash64> = HashSet::new();

        // Two decoy credentials shared by both worlds, so the attacker actually competes for the slate.
        let decoys = |recs: &mut Vec<PalwProviderBondRecord>| {
            recs.push(prov_rec(cb, grp, 400_000, 0, op(0xB0, 0)));
            recs.push(prov_rec(cd, grp, 600_000, 0, op(0xD0, 0)));
        };

        // WHOLE: credential A is a single bond of N.
        let mut whole = vec![prov_rec(ca, grp, N, 0, op(0xA0, 0))];
        decoys(&mut whole);
        let whole_view = view_of(&whole);

        for k in [2u32, 10, 100] {
            // SPLIT: credential A is k bonds of N/k under the SAME owner key. The smallest outpoint
            // (index 0) is the canonical representative in both worlds, so even it matches.
            let mut split = Vec::new();
            for i in 0..k {
                split.push(prov_rec(ca, grp, N / k as u64, 0, op(0xA0, i)));
            }
            decoys(&mut split);
            let split_view = view_of(&split);

            // (1) Aggregation yields the identical candidate set: same credential, same summed weight N.
            let agg_w = aggregate_provider_credentials_at(&whole_view, POV, &empty, &empty);
            let agg_s = aggregate_provider_credentials_at(&split_view, POV, &empty, &empty);
            assert_eq!(agg_w, agg_s, "k={k}: splitting aggregates to the identical (credential, weight, representative) set");
            assert_eq!(
                agg_s.iter().find(|c| c.credential == ca).unwrap().weight,
                N as u128,
                "k={k}: A's aggregated stake is N no matter how many outpoints it is split across"
            );

            // (2) Byte-identical committee for every seed, and (3) an IDENTICAL selection count for A
            //     across many independent draws — the numerical anti-split assertion.
            let (a_rep, mut a_whole, mut a_split) = (op(0xA0, 0), 0u32, 0u32);
            for s in 0..200u8 {
                let (seed, batch) = (h(s ^ 0x11), h(s.wrapping_add(0x40)));
                let (bw, cw) = select_auditor_committee(&seed, &batch, &whole_view, POV, &empty, &empty, 2);
                let (bs, cs) = select_auditor_committee(&seed, &batch, &split_view, POV, &empty, &empty, 2);
                assert_eq!(bw, bs, "k={k} s={s}: identical committee whole vs split");
                assert_eq!(cw, cs, "k={k} s={s}: identical auditor_set_commitment whole vs split");
                let (ww, wc) = select_weighted_auditor_committee(&seed, &batch, &whole_view, POV, &empty, &empty, 2);
                let (ws, sc) = select_weighted_auditor_committee(&seed, &batch, &split_view, POV, &empty, &empty, 2);
                assert_eq!(ww, ws, "k={k} s={s}: selected aggregate quorum weights must also be split-invariant");
                assert_eq!(wc, sc);
                if let Some(member) = ws.iter().find(|member| member.credential == ca) {
                    assert_eq!(member.weight, N as u128, "k={k} s={s}: quorum must retain A's aggregate N stake");
                }
                a_whole += u32::from(bw.contains(&a_rep));
                a_split += u32::from(bs.contains(&a_rep));
            }
            assert_eq!(a_whole, a_split, "k={k}: splitting A's bond does not change how often A is selected");
            assert!(
                0 < a_whole && a_whole < 200,
                "k={k}: sanity — A (weight N of 2N total) is sometimes but not always in the size-2 slate"
            );
        }
    }

    /// A bond below the ECON-03 registry floor is DROPPED before the registry, so it never resolves to
    /// collateral and can never become a selection candidate. Driven through the real registry producer,
    /// where the floor lives — not by hand-excluding it here.
    #[test]
    fn sel01_below_minimum_bond_never_becomes_a_candidate() {
        const POV: u64 = 1_000;
        const FLOOR: u64 = 1_000;
        let empty: HashSet<Hash64> = HashSet::new();
        let at_floor = econ03_bond_tx(econ03_bond_payload(FLOOR, 0x41).1);
        let sub_floor = econ03_bond_tx(econ03_bond_payload(FLOOR - 1, 0x42).1);
        let mut view = ProviderBondView::new();
        view.apply(&palw_provider_bond_mutations_from_accepted_txs(&[at_floor, sub_floor], 0, FLOOR, 4));

        let cands = aggregate_provider_credentials_at(&view, POV, &empty, &empty);
        assert_eq!(cands.len(), 1, "the sub-floor bond must not appear as a selection candidate");
        assert_eq!(cands[0].credential, validator_id_from_pubkey(&econ03_bond_payload(FLOOR, 0x41).0.owner_public_key));
        assert_eq!(cands[0].weight, FLOOR as u128);
    }

    /// ADR-0045 D3-a — the retention window is a function of PARAMETERS, never of chain length.
    ///
    /// That is the whole reason the store can exist: an unbounded seed history would grow forever,
    /// and a window shorter than the verification lookback would make honest tickets fail closed.
    /// The window is the epoch-space form of the rule the paid-work walk already obeys.
    #[test]
    fn beacon_seed_history_window_covers_the_verification_lookback() {
        let admission = PalwBatchAdmissionParams::INERT;
        let epoch_len = 100u64;
        let (w, k) = (6u64, 2u64); // the PCPB windows (mirrors PalwParams::testnet_inert_default)
        let window = palw_beacon_seed_history_window_epochs(&admission, epoch_len, w);

        // It covers the whole batch lifecycle (the paid-work walk bound, in epochs)...
        let walk_epochs = admission.paid_work_walk_bound_daa(epoch_len).div_ceil(epoch_len);
        assert!(window > walk_epochs, "a ticket bound anywhere in the batch lifecycle must resolve");
        // ...plus the certificate-inclusion window, plus the D3-b freshness lookback (clause 11
        // re-derives the challenge under `R_{issued}`, up to `w` epochs behind registration), plus
        // boundary slack.
        assert_eq!(window, walk_epochs + palw_audit_epoch_inclusion_window_epochs(&admission) + w + 2);
        // The D3-a window (no freshness lookback) was exactly `w` narrower — the amendment is real.
        assert_eq!(palw_beacon_seed_history_window_epochs(&admission, epoch_len, 0) + w, window);

        // The provider-snapshot history reaches one snapshot lag deeper still (clause 12 resolves the
        // snapshot at `anchor − k`).
        assert_eq!(palw_provider_snapshot_history_window_epochs(&admission, epoch_len, w, k), window + k);

        // Independent of chain length: the same parameters give the same window at any epoch.
        assert_eq!(palw_beacon_seed_history_window_epochs(&admission, epoch_len, w), window);
        // A longer epoch does not shrink the covered SPAN below the lifecycle.
        for len in [1u64, 7, 100, 10_000] {
            let win = palw_beacon_seed_history_window_epochs(&admission, len, w);
            assert!(win >= admission.paid_work_walk_bound_daa(len).div_ceil(len));
        }
        // Degenerate epoch length is clamped, not a division by zero.
        assert!(palw_beacon_seed_history_window_epochs(&admission, 0, w) > 0);

        // The floor is the oldest epoch still answerable; below it a read is `None` (fail-closed).
        assert_eq!(palw_beacon_seed_history_floor(1_000, window), 1_000 - window);
        assert_eq!(palw_beacon_seed_history_floor(3, 1_000), 0, "early chain saturates rather than wrapping");
    }

    /// REG-CAP-01 — the capacity MEASUREMENT counts independence, not appearances.
    ///
    /// The threshold field was compared against a `measured_auditor_capacity` no caller supplied.
    /// Supplying one only helps if the measurement is hard to inflate, so these are the three
    /// cheap ways to look like a crowd while being one party.
    #[test]
    fn measured_auditor_capacity_resists_splitting_and_concentration() {
        const POV: u64 = 1_000;
        // Splitting one credential across many bonds does not multiply it: aggregation collapses
        // outpoints per credential BEFORE anything is counted (the SEL-01 property, reused).
        let split = view_of(&[
            prov_rec(h(0xA1), h(1), 250, 0, op(0xA1, 0)),
            prov_rec(h(0xA1), h(1), 250, 0, op(0xA1, 1)),
            prov_rec(h(0xA1), h(1), 250, 0, op(0xA1, 2)),
            prov_rec(h(0xA1), h(1), 250, 0, op(0xA1, 3)),
        ]);
        let capacity = measure_auditor_capacity(&split, POV);
        assert_eq!(capacity.eligible_credentials, 1, "four outpoints are still one credential");
        assert_eq!(capacity.operator_groups, 1);
        assert_eq!(capacity.largest_credential_share_bps, 10_000);
        assert!(PalwAuditorCapacityGateV1::TESTNET.admits(&capacity).is_err());

        // Distinct credentials behind ONE operator group clear a naive credential count but not
        // the group count — which is the point of having both.
        let one_operator = view_of(&[
            prov_rec(h(0xB1), h(7), 250, 0, op(0xB1, 0)),
            prov_rec(h(0xB2), h(7), 250, 0, op(0xB2, 0)),
            prov_rec(h(0xB3), h(7), 250, 0, op(0xB3, 0)),
            prov_rec(h(0xB4), h(7), 250, 0, op(0xB4, 0)),
        ]);
        let capacity = measure_auditor_capacity(&one_operator, POV);
        assert_eq!(capacity.eligible_credentials, 4, "the credential count alone looks fine");
        assert_eq!(capacity.operator_groups, 1);
        assert_eq!(capacity.largest_group_share_bps, 10_000);
        assert!(matches!(
            PalwAuditorCapacityGateV1::TESTNET.admits(&capacity),
            Err(PalwAuditorCapacityShortfall::TooFewOperatorGroups { .. })
        ));

        // Counts satisfied, but one credential holds 97 %: the committee is stake-weighted, so it
        // is effectively that credential's committee.
        let whale = view_of(&[
            prov_rec(h(0xC1), h(1), 97_000, 0, op(0xC1, 0)),
            prov_rec(h(0xC2), h(2), 1_000, 0, op(0xC2, 0)),
            prov_rec(h(0xC3), h(3), 1_000, 0, op(0xC3, 0)),
            prov_rec(h(0xC4), h(4), 1_000, 0, op(0xC4, 0)),
        ]);
        let capacity = measure_auditor_capacity(&whale, POV);
        assert_eq!(capacity.eligible_credentials, 4);
        assert_eq!(capacity.operator_groups, 4);
        assert!(capacity.largest_credential_share_bps > 9_000);
        assert!(matches!(
            PalwAuditorCapacityGateV1::TESTNET.admits(&capacity),
            Err(PalwAuditorCapacityShortfall::CredentialTooConcentrated { .. })
        ));

        // A genuinely distributed set clears every dimension.
        let healthy = view_of(&[
            prov_rec(h(0xD1), h(1), 1_000, 0, op(0xD1, 0)),
            prov_rec(h(0xD2), h(2), 1_000, 0, op(0xD2, 0)),
            prov_rec(h(0xD3), h(3), 1_000, 0, op(0xD3, 0)),
            prov_rec(h(0xD4), h(4), 1_000, 0, op(0xD4, 0)),
        ]);
        let capacity = measure_auditor_capacity(&healthy, POV);
        assert_eq!((capacity.eligible_credentials, capacity.operator_groups), (4, 4));
        assert_eq!(capacity.total_matured_bond, 4_000);
        assert_eq!(capacity.largest_credential_share_bps, 2_500);
        assert_eq!(PalwAuditorCapacityGateV1::TESTNET.admits(&capacity), Ok(()));

        // Bonds that are not yet matured back nothing, so they cannot pad the count — the
        // measurement uses the same `Active`-at-POV rule the sampler does.
        let immature = view_of(&[
            prov_rec(h(0xD1), h(1), 1_000, 0, op(0xD1, 0)),
            prov_rec(h(0xE2), h(2), 1_000, POV + 1, op(0xE2, 0)),
            prov_rec(h(0xE3), h(3), 1_000, POV + 1, op(0xE3, 0)),
            prov_rec(h(0xE4), h(4), 1_000, POV + 1, op(0xE4, 0)),
        ]);
        let capacity = measure_auditor_capacity(&immature, POV);
        assert_eq!(capacity.eligible_credentials, 1, "immature bonds are not audit capacity");
        assert!(PalwAuditorCapacityGateV1::TESTNET.admits(&capacity).is_err());

        // An empty view is short, never vacuously adequate.
        let empty_capacity = measure_auditor_capacity(&ProviderBondView::new(), POV);
        assert_eq!(empty_capacity.eligible_credentials, 0);
        assert_eq!(empty_capacity.largest_credential_share_bps, 0);
        assert!(PalwAuditorCapacityGateV1::TESTNET.admits(&empty_capacity).is_err());
    }

    /// The draw is WEIGHTED by aggregated stake, not uniform: a credential with 100× the stake is drawn
    /// vastly more often. This is the property `provider_index` / `auditor_score` lacked.
    #[test]
    fn sel01_draw_is_weighted_by_aggregated_stake() {
        const POV: u64 = 1_000;
        let empty: HashSet<Hash64> = HashSet::new();
        let (heavy, light) = (h(0xE0), h(0x0E));
        let view = view_of(&[prov_rec(heavy, h(1), 1_000_000, 0, op(0xE0, 0)), prov_rec(light, h(1), 10_000, 0, op(0x0E, 0))]);
        let cands = aggregate_provider_credentials_at(&view, POV, &empty, &empty);
        let (mut h_cnt, mut l_cnt) = (0u32, 0u32);
        for s in 0..300u16 {
            let sel = palw_weighted_sample_without_replacement(&h((s & 0xff) as u8), &h(((s >> 3) as u8) ^ 0x33), &cands, 1);
            if sel[0].credential == heavy {
                h_cnt += 1;
            } else {
                l_cnt += 1;
            }
        }
        assert!(h_cnt > l_cnt * 10, "heavy ({h_cnt}) must vastly outdraw light ({l_cnt}) — the draw is stake-weighted, not uniform");
    }

    /// Order independence (the SEL-01 disqualifier): the committee is a pure function of the view's
    /// CONTENTS and the beacon seed, identical no matter what reorg (apply) order built the view or what
    /// order the caller lists the candidates.
    #[test]
    fn sel01_selection_is_order_independent_across_reorg_paths() {
        const POV: u64 = 1_000;
        let empty: HashSet<Hash64> = HashSet::new();
        let recs = vec![
            prov_rec(h(0x21), h(1), 300_000, 0, op(0x21, 0)),
            prov_rec(h(0x22), h(1), 500_000, 0, op(0x22, 0)),
            prov_rec(h(0x23), h(2), 200_000, 0, op(0x23, 0)),
        ];
        // Two views that reach the SAME final bond set by opposite insertion (reorg) orders.
        let mut fwd = ProviderBondView::new();
        for r in &recs {
            fwd.apply(&[PalwProviderBondMutation::Insert(r.bond_outpoint, r.clone())]);
        }
        let mut rev = ProviderBondView::new();
        for r in recs.iter().rev() {
            rev.apply(&[PalwProviderBondMutation::Insert(r.bond_outpoint, r.clone())]);
        }
        assert_eq!(fwd, rev, "same final bond set ⇒ identical view regardless of apply order");

        let (seed, batch) = (h(0x9c), h(0x42));
        assert_eq!(
            select_auditor_committee(&seed, &batch, &fwd, POV, &empty, &empty, 2),
            select_auditor_committee(&seed, &batch, &rev, POV, &empty, &empty, 2),
            "committee is a pure function of the view contents, not the reorg path"
        );

        // The pure sampler is also independent of the candidate LISTING order (it re-sorts internally).
        let cands = aggregate_provider_credentials_at(&fwd, POV, &empty, &empty);
        let mut reversed = cands.clone();
        reversed.reverse();
        assert_eq!(
            palw_weighted_sample_without_replacement(&seed, &batch, &cands, 3),
            palw_weighted_sample_without_replacement(&seed, &batch, &reversed, 3),
            "input order cannot change the sampler result"
        );
    }

    /// The §4.1 / §5.17.4-step-2 exclusions and the eligibility floor: an excluded credential, an
    /// operator-group sibling, a Pending (immature) bond, and an Unbonding bond all drop out.
    #[test]
    fn sel01_exclusions_and_inactive_bonds_drop_candidates() {
        const POV: u64 = 1_000;
        let (grp_x, grp_y) = (h(0x0A), h(0x0B));
        let (c1, c2, c3, c4) = (h(0x31), h(0x32), h(0x33), h(0x34));
        let view = view_of(&[
            prov_rec(c1, grp_x, 100_000, 0, op(0x31, 0)),     // eligible
            prov_rec(c2, grp_x, 100_000, 0, op(0x32, 0)),     // eligible unless grp_x excluded
            prov_rec(c3, grp_y, 100_000, 2_000, op(0x33, 0)), // Pending at POV=1000 (activation 2000) ⇒ 0
            prov_rec(c4, grp_y, 100_000, 0, op(0x34, 0)),     // eligible unless credential excluded
        ]);
        let empty: HashSet<Hash64> = HashSet::new();
        let creds = |c: &HashSet<Hash64>, g: &HashSet<Hash64>| -> HashSet<Hash64> {
            aggregate_provider_credentials_at(&view, POV, c, g).iter().map(|x| x.credential).collect()
        };

        // No exclusions: the immature (Pending) c3 backs nothing at the point of view.
        assert_eq!(creds(&empty, &empty), [c1, c2, c4].into_iter().collect(), "a Pending bond is not eligible collateral");
        // Exclude credential c4 (own credential / same delegation root / past partner).
        assert_eq!(creds(&[c4].into_iter().collect(), &empty), [c1, c2].into_iter().collect());
        // Exclude operator group grp_x ⇒ both c1 and c2 fall out.
        assert_eq!(creds(&empty, &[grp_x].into_iter().collect()), [c4].into_iter().collect());

        // An Unbonding bond also contributes nothing.
        let mut unbonding = view.clone();
        unbonding.apply(&[PalwProviderBondMutation::Unbond(op(0x31, 0), 500)]);
        let after: HashSet<Hash64> =
            aggregate_provider_credentials_at(&unbonding, POV, &empty, &empty).iter().map(|x| x.credential).collect();
        assert!(!after.contains(&c1), "an unbonding bond is not eligible collateral");
    }

    /// Non-replacement: no credential is selected twice even under a heavy stake skew, and the committee
    /// caps at the candidate count when `committee_size` exceeds the pool.
    #[test]
    fn sel01_non_replacement_selects_distinct_credentials_and_caps_at_pool_size() {
        const POV: u64 = 1_000;
        let empty: HashSet<Hash64> = HashSet::new();
        let view = view_of(&[
            prov_rec(h(0x51), h(1), 900_000, 0, op(0x51, 0)), // dominant 90% stake
            prov_rec(h(0x52), h(1), 50_000, 0, op(0x52, 0)),
            prov_rec(h(0x53), h(1), 50_000, 0, op(0x53, 0)),
        ]);
        // committee_size 5 > pool of 3 ⇒ at most 3, and never a repeat despite the skew.
        let (bonds, _commit) = select_auditor_committee(&h(0x7a), &h(0x42), &view, POV, &empty, &empty, 5);
        assert_eq!(bonds.len(), 3, "non-replacement caps the committee at the candidate count");
        assert_eq!(bonds.iter().copied().collect::<HashSet<_>>().len(), 3, "no credential is selected twice");
    }

    // =====================================================================================================
    // ADR-0040 §5.17 — the CROSS-CRATE GOLDEN for AUTHSET-01 / SAMPLE-01 (Phase 4, producer/verifier tie).
    //
    // `verify_certificate_attestation` (crate `kaspa-consensus`) RE-DERIVES a certificate's
    // `auditor_set_commitment` via `select_auditor_committee` and its `audit_sample_root` via
    // `palw_deterministic_sample` + `palw_audit_sample_root`. The certificate PRODUCER (crate
    // `misaka-palw-miner`, `mil/miner/src/audit.rs`) MUST emit exactly those values, or every certificate
    // it builds is rejected on-chain — and the overlay-effect error is discarded, so it fails SILENTLY.
    //
    // Both sides call these very consensus-core functions, so they cannot drift for a FIXED input — but a
    // silent change to the SHARED selection/sampling logic would move the value on both sides together,
    // turning what should be a deliberate re-genesis decision into an invisible refactor. The two hex
    // constants below pin the value for one shared fixture; the miner mirrors them BYTE-FOR-BYTE in
    // `mil/miner/src/audit.rs::tests` (`CROSS_CRATE_GOLDEN_AUDITOR_SET_COMMITMENT` /
    // `CROSS_CRATE_GOLDEN_AUDIT_SAMPLE_ROOT`). Change the selector or the sampler and BOTH crates go red —
    // loudly — exactly like `palw_leaf_merkle_root_cross_crate_golden_vector` does for `leaf_root`.
    //
    // The fixture is built from FULLY EXPLICIT records (not the `prov_rec` / miner `provider_view` helpers,
    // whose defaults differ across the crate boundary), so the two crates construct a byte-identical view
    // and leaf-DA-root set from the same literals. Committee is 3 of 5 distinct-credential candidates and
    // the sample is 2 of 4 leaves, so the pinned values exercise the WEIGHTED non-replacement draw and the
    // partial Fisher–Yates sample rather than a degenerate select-all.
    // =====================================================================================================

    /// The shared golden fixture, verifier side. Kept in one function so the miner's mirror can copy the
    /// exact same literals. `(credential, operator_group, amount_sompi, bond)` per row; every other field
    /// is selection-neutral and set to the canonical zero/empty so the two crates agree byte-for-byte.
    fn cross_crate_golden_provider_records() -> Vec<PalwProviderBondRecord> {
        [(0x71u8, 0x81u8, 500_000u64), (0x72, 0x82, 400_000), (0x73, 0x83, 300_000), (0x74, 0x84, 200_000), (0x75, 0x85, 100_000)]
            .into_iter()
            .map(|(cred, grp, amount)| PalwProviderBondRecord {
                version: 1,
                bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([cred; 64]), 0),
                owner_pubkey_hash: Hash64::from_bytes([cred; 64]),
                owner_public_key: Vec::new(),
                operator_group_id: Hash64::from_bytes([grp; 64]),
                runtime_classes: Vec::new(),
                capacity_by_shape: Vec::new(),
                reward_key_root: Hash64::default(),
                amount_sompi: amount,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbond_delay_epochs: 10,
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
            })
            .collect()
    }

    /// The shared golden fixture's sampled DA roots, verifier side: four leaves with distinct
    /// `receipt_da_root`s. The miner builds four real `PalwPublicLeafV1` leaves carrying the SAME four
    /// roots and lets `derive_audit_sample_root` select the same 2 of 4.
    fn cross_crate_golden_receipt_da_roots() -> Vec<Hash64> {
        (0..4u8).map(|i| Hash64::from_bytes([0xD0 + i; 64])).collect()
    }

    /// Verifier-side cross-crate vector. Pins the consensus re-derivation of
    /// `auditor_set_commitment` and `audit_sample_root` for the shared fixture. The identical literals are
    /// asserted from the PRODUCER in `misaka-palw-miner`, so producer/verifier drift breaks the build.
    #[test]
    fn cross_crate_golden_auditor_set_and_sample_root() {
        const CROSS_CRATE_GOLDEN_SEED: u8 = 0xC0;
        const CROSS_CRATE_GOLDEN_BATCH: u8 = 0x42;
        const POV: u64 = 1_000;
        const COMMITTEE_SIZE: usize = 3;
        const SAMPLE_SIZE: u32 = 2;
        // Keep these two identical to `mil/miner/src/audit.rs::tests`.
        const CROSS_CRATE_GOLDEN_AUDITOR_SET_COMMITMENT: &str = "f6b70c92baebadc4849b4f0ce44b1d166989f340b8d6d95cfbd30e51236161eb\
             372c28580814c3c202fc406fd8e901bddfd8703950f0a3bd28179fd89095980d";
        const CROSS_CRATE_GOLDEN_AUDIT_SAMPLE_ROOT: &str = "6abe582463e5bbb8e654ae3e0bab5aad4a2d0dfd8cec49daf3a497b1e71dec8a\
             49605cedc7d3349f5c01bc60255df01c4f39628a4f28d1987130dd11dff2e852";

        let empty: HashSet<Hash64> = HashSet::new();
        let seed = h(CROSS_CRATE_GOLDEN_SEED);
        let batch = h(CROSS_CRATE_GOLDEN_BATCH);

        // AUTHSET-01: the beacon-selected committee's commitment.
        let view = view_of(&cross_crate_golden_provider_records());
        let (_slate, commitment) = select_auditor_committee(&seed, &batch, &view, POV, &empty, &empty, COMMITTEE_SIZE);
        assert_eq!(commitment.to_string(), CROSS_CRATE_GOLDEN_AUDITOR_SET_COMMITMENT);

        // SAMPLE-01: the re-derived sample root over the beacon-selected leaves' DA roots.
        let roots = cross_crate_golden_receipt_da_roots();
        let sampled = palw_deterministic_sample(&seed, &batch, roots.len() as u32, SAMPLE_SIZE);
        let sampled_roots: Vec<Hash64> = sampled.iter().map(|&i| roots[i as usize]).collect();
        let sample_root = palw_audit_sample_root(&sampled_roots);
        assert_eq!(sample_root.to_string(), CROSS_CRATE_GOLDEN_AUDIT_SAMPLE_ROOT);
    }

    /// R4 (§24.5) — mismatch attribution + escalation are deterministic and never slash the honest
    /// partner. The escalation draw is beacon-derived; the inert params escalate nothing.
    #[test]
    fn r4_mismatch_attribution_and_escalation() {
        let op = |n: u8| TransactionOutpoint { transaction_id: h(n), index: n as u32 };
        let (pa, pb) = (op(1), op(2));
        // a genuine mismatch: the two replicas committed different outputs.
        let rec =
            PalwMismatchRecordV1 { batch_id: h(10), leaf_index: 7, provider_a: pa, provider_b: pb, output_a: h(20), output_b: h(21) };
        // reference confirms a ⇒ b is the deviator ⇒ slash b only (honest a keeps its bond).
        assert_eq!(rec.attribute(&h(20)), PalwMismatchVerdict::SlashB);
        assert_eq!(rec.slash_targets(PalwMismatchVerdict::SlashB), vec![pb]);
        // reference confirms b ⇒ slash a only.
        assert_eq!(rec.attribute(&h(21)), PalwMismatchVerdict::SlashA);
        // reference matches neither ⇒ both deviated ⇒ slash both.
        assert_eq!(rec.attribute(&h(99)), PalwMismatchVerdict::SlashBoth);
        assert_eq!(rec.slash_targets(PalwMismatchVerdict::SlashBoth), vec![pa, pb]);
        // a non-mismatch (equal outputs) attributes to nothing and slashes nobody.
        let eq = PalwMismatchRecordV1 { output_b: h(20), ..rec.clone() };
        assert_eq!(eq.attribute(&h(20)), PalwMismatchVerdict::NotAMismatch);
        assert!(eq.slash_targets(PalwMismatchVerdict::NotAMismatch).is_empty());
        assert!(
            !eq.is_escalated(&h(5), &PalwMismatchParams { escalation_rate_ppm: 1_000_000, repeat_offender_threshold: 1 }, 99, 99),
            "an equal-output record is not a mismatch and is never escalated"
        );

        // escalation draw is deterministic in [0, 1_000_000) and beacon-sensitive.
        let d1 = rec.escalation_draw(&h(5));
        assert_eq!(d1, rec.escalation_draw(&h(5)), "deterministic");
        assert!(d1 < 1_000_000);
        assert_ne!(d1, rec.escalation_draw(&h(6)), "different beacon seed ⇒ different draw");
        // inert params escalate nothing regardless of prior counts.
        assert!(!rec.is_escalated(&h(5), &PalwMismatchParams::INERT, 1000, 1000));
        // full-rate escalation always fires; repeat-offender path fires when a count reaches the threshold.
        assert!(rec.is_escalated(&h(5), &PalwMismatchParams { escalation_rate_ppm: 1_000_000, repeat_offender_threshold: 0 }, 0, 0));
        assert!(rec.is_escalated(&h(5), &PalwMismatchParams { escalation_rate_ppm: 0, repeat_offender_threshold: 3 }, 3, 0));
        assert!(!rec.is_escalated(&h(5), &PalwMismatchParams { escalation_rate_ppm: 0, repeat_offender_threshold: 3 }, 2, 2));
    }

    /// ADR-0040 §5.17.3 — the bounded inclusion window `N` is anchored to the widest legal batch
    /// lifecycle span: `2·registration_lead + audit_window + active_window`. On the INERT admission
    /// params (lead 2, audit 6, active 6) that is `2·2 + 6 + 6 = 16`. Saturating so a params overflow
    /// cannot wrap `N` down to a stranding-small value.
    #[test]
    fn palw_audit_epoch_inclusion_window_matches_lifecycle_span() {
        let a = PalwBatchAdmissionParams::INERT;
        assert_eq!(palw_audit_epoch_inclusion_window_epochs(&a), 2 * 2 + 6 + 6);
        assert_eq!(palw_audit_epoch_inclusion_window_epochs(&a), 16);

        // A widened lifecycle widens the window by exactly the same amount (never strands a valid cert).
        let wide = PalwBatchAdmissionParams { registration_lead_epochs: 5, audit_window_epochs: 10, active_window_epochs: 20, ..a };
        assert_eq!(palw_audit_epoch_inclusion_window_epochs(&wide), 2 * 5 + 10 + 20);

        // Overflow cannot wrap: a u64::MAX lead saturates the window UP (never down to a tiny value).
        let huge = PalwBatchAdmissionParams { registration_lead_epochs: u64::MAX, ..a };
        assert_eq!(palw_audit_epoch_inclusion_window_epochs(&huge), u64::MAX);
    }

    /// ADR-0040 SAMPLE-01 (§5.17.6) — [`palw_deterministic_sample`] is a pure, order-independent,
    /// non-replacement selector of leaf indices, and [`palw_audit_sample_root`] is a canonical
    /// commitment over the sampled leaves' `receipt_da_root` values.
    #[test]
    fn palw_deterministic_sample_and_root_are_pure_and_binding() {
        let seed = h(0xA1);
        let batch = h(0xB2);

        // Deterministic: same inputs ⇒ same index set, regardless of when called.
        let s1 = palw_deterministic_sample(&seed, &batch, 100, 8);
        let s2 = palw_deterministic_sample(&seed, &batch, 100, 8);
        assert_eq!(s1, s2, "sample is a pure function of its inputs");

        // Exactly `sample_size` distinct indices, all in range, returned in ASCENDING order.
        assert_eq!(s1.len(), 8);
        assert!(s1.windows(2).all(|w| w[0] < w[1]), "indices are strictly ascending and distinct");
        assert!(s1.iter().all(|&i| i < 100), "indices are in range");

        // Seed-sensitive and batch-sensitive: a different beacon seed / batch samples differently.
        assert_ne!(s1, palw_deterministic_sample(&h(0xA2), &batch, 100, 8), "different seed ⇒ different sample");
        assert_ne!(s1, palw_deterministic_sample(&seed, &h(0xB3), 100, 8), "different batch ⇒ different sample");

        // Saturation: sample_size ≥ leaf_count selects EVERY leaf; empty leaf set or zero sample is empty.
        assert_eq!(palw_deterministic_sample(&seed, &batch, 5, 5), vec![0, 1, 2, 3, 4]);
        assert_eq!(palw_deterministic_sample(&seed, &batch, 5, 99), vec![0, 1, 2, 3, 4]);
        assert!(palw_deterministic_sample(&seed, &batch, 0, 8).is_empty());
        assert!(palw_deterministic_sample(&seed, &batch, 100, 0).is_empty());

        // The root binds the ordered `receipt_da_root` list: order- and count-sensitive, domain-separated.
        let roots = vec![h(1), h(2), h(3)];
        assert_eq!(palw_audit_sample_root(&roots), palw_audit_sample_root(&roots), "deterministic");
        assert_ne!(palw_audit_sample_root(&roots), palw_audit_sample_root(&[h(1), h(3), h(2)]), "order-sensitive");
        assert_ne!(palw_audit_sample_root(&roots), palw_audit_sample_root(&[h(1), h(2)]), "count-sensitive");
        // A sample root can never equal the leaf-merkle root of the same list (distinct domains) — the
        // property the dedicated `PALW_AUDIT_SAMPLE_ROOT_DOMAIN` buys.
        assert_ne!(palw_audit_sample_root(&roots), palw_leaf_merkle_root(&roots), "sample root ≠ leaf root");
    }

    /// ADR-0040 §5.17.3 — the bounded inclusion PREDICATE: reject a future audit epoch, accept anything
    /// within the window, reject anything beyond it. The boundary (`block − audit == N`) is INCLUSIVE.
    #[test]
    fn palw_certificate_inclusion_window_predicate() {
        let n = 16;
        // exactly at the audit epoch: included.
        assert!(palw_certificate_included_within_audit_window(100, 100, n));
        // one epoch later: included.
        assert!(palw_certificate_included_within_audit_window(100, 101, n));
        // exactly N epochs later: still included (inclusive boundary — a cert active to expiry-1 is valid).
        assert!(palw_certificate_included_within_audit_window(100, 116, n));
        // N+1 epochs later: rejected (past expiry / seed at pruning risk ⇒ sound fail-closed).
        assert!(!palw_certificate_included_within_audit_window(100, 117, n));
        // a FUTURE audit epoch (audit > block): rejected by clause (1), via saturating sub not underflow.
        assert!(!palw_certificate_included_within_audit_window(101, 100, n));
        assert!(!palw_certificate_included_within_audit_window(u64::MAX, 0, n));
        // a zero-width window admits only same-epoch inclusion.
        assert!(palw_certificate_included_within_audit_window(50, 50, 0));
        assert!(!palw_certificate_included_within_audit_window(50, 51, 0));
    }

    /// ADR-0040 §5.17.3 — the PURE audit-epoch seed selector. Covers the `Some` path (which a real
    /// empty chain cannot exercise — beacon seeds stay zero without bonded reveals), newest-in-epoch
    /// selection, target = audit − 1, and every fail-closed branch. Facts are NEWEST→OLDEST, epoch =
    /// daa/epoch_len.
    #[test]
    fn palw_audit_epoch_seed_select_logic() {
        const V3: u16 = crate::constants::PALW_HEADER_VERSION;
        // epoch_len = 2. Two epochs of two headers each, plus lower epochs. Newest→oldest.
        // daa 7,6 ⇒ epoch 3 ; daa 5,4 ⇒ epoch 2 ; daa 3,2 ⇒ epoch 1.
        let chain = |seed_e2_newest: Hash64| {
            vec![
                (V3, 7u64, h(0x77)),
                (V3, 6, h(0x66)),
                (V3, 5, seed_e2_newest), // NEWEST header of epoch 2
                (V3, 4, h(0x44)),        // older header of epoch 2 — must be ignored
                (V3, 3, h(0x33)),
                (V3, 2, h(0x22)),
            ]
        };

        // Some: audit_beacon_epoch 3 ⇒ target epoch 2 ⇒ the NEWEST epoch-2 header's seed (daa 5), not daa 4.
        assert_eq!(palw_audit_epoch_seed_select(3, 0, 2, chain(h(0x55)).into_iter()), Some(h(0x55)));
        // A different target: audit 4 ⇒ epoch 3 ⇒ newest is daa 7.
        assert_eq!(palw_audit_epoch_seed_select(4, 0, 2, chain(h(0x55)).into_iter()), Some(h(0x77)));

        // Fail-closed: audit_beacon_epoch 0 has no previous epoch.
        assert_eq!(palw_audit_epoch_seed_select(0, 0, 2, chain(h(0x55)).into_iter()), None);

        // Fail-closed: FUTURE audit epoch — first fact (epoch 3) is already below the target epoch (9).
        assert_eq!(palw_audit_epoch_seed_select(10, 0, 2, chain(h(0x55)).into_iter()), None);

        // Fail-closed: the first header is a zero-seed / pre-v3 / pre-activation boundary header.
        assert_eq!(palw_audit_epoch_seed_select(3, 0, 2, vec![(V3, 5u64, Hash64::default())].into_iter()), None);
        assert_eq!(palw_audit_epoch_seed_select(3, 0, 2, vec![(1u16, 5u64, h(0x55))].into_iter()), None);
        assert_eq!(palw_audit_epoch_seed_select(3, 100, 2, vec![(V3, 5u64, h(0x55))].into_iter()), None);

        // Fail-closed: sequence exhausted before the target epoch (no epoch-0 header for target 0).
        assert_eq!(palw_audit_epoch_seed_select(1, 0, 2, vec![(V3, 3u64, h(0x33)), (V3, 2, h(0x22))].into_iter()), None);

        // Fail-closed: a daa GAP skips the target epoch (epoch 3 then epoch 1, no epoch 2).
        assert_eq!(palw_audit_epoch_seed_select(3, 0, 2, vec![(V3, 7u64, h(0x77)), (V3, 2, h(0x22))].into_iter()), None);

        // epoch_len == 0 is treated as 1 (never divides by zero): audit 6 ⇒ target 5 ⇒ the daa-5 header.
        assert_eq!(palw_audit_epoch_seed_select(6, 0, 0, chain(h(0x55)).into_iter()), Some(h(0x55)));
    }

    /// D3 (§18.3): a PalwBeaconCheckpointV1 projects a beacon state + verifies the R_E hash-chain step;
    /// the epoch-proof bundle's beacon_chain links iff it is a contiguous ascending run whose seeds chain
    /// from the boundary seed. borsh round-trips.
    #[test]
    fn d3_epoch_proof_bundle_beacon_chain_links() {
        // build a 3-epoch seed chain from a boundary seed, healthy anchor facts fixed across the run.
        let boundary = h(0x40);
        let (anchor, vrr, mcr) = (h(0x50), h(0x51), h(0x52));
        let mut cps = Vec::new();
        let mut prev = boundary;
        for epoch in 10u64..=12 {
            let seed = beacon_seed(&prev, &anchor, &vrr, &mcr, epoch);
            cps.push(PalwBeaconCheckpointV1 {
                epoch,
                seed,
                dns_anchor: anchor,
                anchor_blue_score: 700,
                anchor_daa_score: 900,
                anchor_overlay_root: h(0x53),
                valid_reveals_root: vrr,
                missing_commitments_root: mcr,
                mode: 0,
                degraded_epochs: 0,
            });
            assert!(cps.last().unwrap().seed_follows(&prev));
            prev = seed;
        }
        let bundle = PalwEpochProofBundleV2 {
            version: PALW_EPOCH_PROOF_BUNDLE_VERSION_V2,
            from_epoch: 10,
            to_epoch: 12,
            beacon_chain: cps.clone(),
            batch_manifests: vec![],
            leaf_chunks: vec![],
            certificates: vec![],
            revocations: vec![],
            nullifier_frontier_root: h(0x60),
            active_set_records: vec![],
        };
        assert!(bundle.beacon_chain_links(&boundary), "correct contiguous chain from the boundary seed links");
        let mut legacy_version = bundle.clone();
        legacy_version.version = 1;
        assert!(!legacy_version.beacon_chain_links(&boundary), "legacy/unknown bundle versions fail closed");
        assert!(!bundle.beacon_chain_links(&h(0xff)), "a wrong boundary seed breaks the first link");

        // a broken middle seed fails linkage.
        let mut tampered = bundle.clone();
        tampered.beacon_chain[1].seed = h(0xaa);
        assert!(!tampered.beacon_chain_links(&boundary));
        // wrong length / non-contiguous epoch fails.
        let mut short = bundle.clone();
        short.beacon_chain.pop();
        assert!(!short.beacon_chain_links(&boundary));

        // borsh round-trips.
        let bytes = borsh::to_vec(&bundle).unwrap();
        assert_eq!(PalwEpochProofBundleV2::try_from_slice(&bytes).unwrap(), bundle);
        assert!(
            PalwEpochProofBundleV2::try_from_slice(&bytes[2..]).is_err(),
            "the legacy no-version-prefix bundle shape must not decode as V2"
        );

        // from_state projection matches.
        let st = PalwBeaconStateV1 {
            version: 1,
            closed_seeds: Vec::new(),
            prev_closer: None,
            epoch: 12,
            seed: prev,
            dns_anchor: anchor,
            anchor_blue_score: 700,
            anchor_daa_score: 900,
            anchor_overlay_root: h(0x53),
            valid_reveals_root: vrr,
            missing_commitments_root: mcr,
            mode: 0,
            degraded_epochs: 0,
            valid_reveal_count: 5,
            missing_commit_count: 1,
        };
        let cp = PalwBeaconCheckpointV1::from_state(&st);
        assert_eq!(cp.epoch, 12);
        assert_eq!(cp.seed, prev);
        assert_eq!(cp.anchor(), st.anchor());
    }

    /// C6 / §22 — CONSTRUCTION == VALIDATION, proved in-process (no network): a mining template that
    /// (1) builds candidates via `palw_template_candidate` (the validator's own `eligibility_hash` +
    /// canonical nonce), (2) selects a winner with `palw_select_template_ticket`, and (3) fills the
    /// Header-v3 fields from the winner, produces a header that `verify_palw_ticket` ACCEPTS on all nine
    /// clauses — because both sides share the identical pure eligibility / chain_commit / draw functions.
    /// The header's `chain_commit`/`bits` are set to the consensus-derived `expected_chain_commit`/lane
    /// bits, and `daa_score`/nonce are pinned; a validator that re-derives those same values agrees by
    /// construction. This is the in-process c==v proof for the algo-4 mining template.
    #[test]
    fn template_ticket_construction_equals_validation() {
        let net = 0x9107u32;
        let eligibility_beacon = h(0x77); // the lagged R_E = a finality-buried anchor's palw_beacon_seed
        let expected_chain_commit = h(0x88); // consensus-derived (clause 6); the template SETS header.chain_commit = this
        let target_interval = 600u64;
        let batch_id = h(0x10);
        let epoch = 5u64;
        // An EASY lane target: `from_compact_target_bits_512(0x2100ffff)` ≈ 2^512·(1−2^-16), so a real
        // 512-bit eligibility draw wins with probability ≈ 1 − 2^-16 — the first inventory ticket wins in
        // practice (never flakes) while exercising the REAL draw path, not a contrived zero digest.
        let lane_bits = 0x2100ffff_u32;

        let (cert_activation_epoch, cert_expiry_epoch) = (4u64, 12u64);
        let cert_active = cert_activation_epoch <= epoch && epoch < cert_expiry_epoch;

        // The miner's inventory of Active tickets (same batch/cert, distinct leaves ⇒ distinct draws).
        let ticket = |i: u8| {
            let nf = h(0xA0u8.wrapping_add(i));
            let leaf_hash = h(0x40u8.wrapping_add(i));
            let leaf_index = i as u32;
            let cand = palw_template_candidate(
                net,
                &eligibility_beacon,
                &expected_chain_commit,
                target_interval,
                &batch_id,
                leaf_index,
                &leaf_hash,
                &nf,
            );
            let binding = PalwTicketBinding {
                ticket_nullifier_commitment: ticket_nullifier_commitment(&nf),
                proof_type: 1,
                leaf_activation_epoch: 4,
                leaf_expiry_epoch: 12,
                target_daa_interval: target_interval,
            };
            (nf, cand, binding)
        };
        let inv: Vec<_> = (0..64u8).map(ticket).collect();
        let cands: Vec<PalwTemplateCandidate> = inv.iter().map(|(_, c, _)| c.clone()).collect();

        // Template: pick the first ticket whose draw wins the current lane bits.
        let win_i = palw_select_template_ticket(&cands, lane_bits).expect("a ticket wins the easy target");
        let (nf, cand, binding) = &inv[win_i];

        // Validation over the header the template built (chain_commit/bits set to the consensus values,
        // nonce pinned, daa == the ticket's interval). All nine clauses pass by construction.
        assert_eq!(
            verify_palw_ticket(
                nf,                       // header.palw_ticket_nullifier
                binding.proof_type,       // header.palw_proof_type
                &expected_chain_commit,   // header.palw_chain_commit (template SET = expected)
                lane_bits,                // header.bits (template SET = lane bits)
                cand.nonce,               // header.nonce = low64(nullifier)
                target_interval,          // header.daa_score (== binding.target_daa_interval, clause 5)
                &cand.eligibility_digest, // clause-9 draw digest (template + validator agree)
                binding,
                cert_active,
                epoch,
                &expected_chain_commit, // validator's re-derived chain_commit (clause 6)
                lane_bits,              // validator's re-derived lane bits (clause 7)
                true,                   // compute headroom (clause 8, header stage)
            ),
            Ok(()),
            "a template-built winning ticket must pass all nine verify_palw_ticket clauses"
        );

        // Non-vacuous: a header claiming a different chain_commit than the validator derives is rejected.
        assert_eq!(
            verify_palw_ticket(
                nf,
                binding.proof_type,
                &h(0xEE),
                lane_bits,
                cand.nonce,
                target_interval,
                &cand.eligibility_digest,
                binding,
                cert_active,
                epoch,
                &expected_chain_commit,
                lane_bits,
                true,
            ),
            Err(PalwTicketReject::ChainCommitMismatch),
        );
        // And a losing draw (a huge digest that no easy target admits) is rejected at clause 9.
        let losing = PalwTemplateCandidate { eligibility_digest: h(0xFF), ..cand.clone() };
        assert_eq!(
            verify_palw_ticket(
                &losing.ticket_nullifier,
                binding.proof_type,
                &expected_chain_commit,
                0x1d00ffff,
                losing.nonce,
                target_interval,
                &losing.eligibility_digest,
                binding,
                cert_active,
                epoch,
                &expected_chain_commit,
                0x1d00ffff,
                true,
            ),
            Err(PalwTicketReject::EligibilityMiss),
        );
    }

    /// kaspa-pq **ADR-0040 STORE-VERSION — on-disk layout pin.**
    ///
    /// These three structs are persisted with **bincode**, which is POSITIONAL and carries no field
    /// names, no tags and no length prefix for the struct itself. Adding, removing or reordering a
    /// field therefore silently changes the on-disk encoding: an old row is strictly shorter than the
    /// new decoder expects, so `bincode::deserialize` terminates in EOF and the `.unwrap()` in the
    /// body-processor worker turns that into a panic on a node that merely upgraded its binary.
    ///
    /// That is exactly what happened once already. ADR-0040 added `approving_stake` (mid-struct, into
    /// `PalwBatchCertificateV2`), `cert_approving_stake` + `first_cert_daa` (mid-struct, into
    /// `PalwBatchLifecycleV1`) and `job_nullifiers` (trailing, into `PalwBatchViewV1`) WITHOUT bumping
    /// `LATEST_DB_VERSION`. Because `TESTNET_PALW_PARAMS` / `DEVNET_PALW_PARAMS` ship
    /// `palw_activation_daa_score = 0`, real old-shape rows existed on disk, and the version-mismatch
    /// prompt that exists to warn the operator was bypassed by the missing bump.
    ///
    /// # If this test fails
    ///
    /// You changed a persisted PALW layout. That is allowed — no PALW network is live — but it is NOT
    /// free. Do BOTH of these, then update the constants below:
    ///
    /// 1. Bump `LATEST_DB_VERSION` in `consensus/src/consensus/factory.rs` (currently **14**), and
    /// 2. extend the `version <= N` hard-reset arm in `kaspad/src/daemon.rs`'s `'db_upgrade` loop to
    ///    cover the version you just left behind.
    ///
    /// Bumping the constant WITHOUT the daemon arm is strictly worse than doing nothing: the loop is
    /// entered, matches no arm, and trips its trailing `assert_eq!` — trading a bincode panic for an
    /// assertion panic with even less diagnostic value.
    /// # 9 -> 10 is the case this test CANNOT see, and that is why it is documented here
    ///
    /// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2) bumped the version while **every constant below stayed
    /// put** — deliberately. `leaf_root` is still one `Hash64` in the same position; what changed is its
    /// CONSTRUCTION (flat keyed hash → uniform-depth Merkle root, §5.15.4). So the bytes are identical
    /// and the meaning is not.
    ///
    /// A green run of this test is therefore **not** evidence that a change is format-neutral. It cannot
    /// be: the `manifest` fixture below writes a LITERAL `h(0x43)` into `leaf_root`, and a literal by
    /// construction cannot move when the construction moves (§5.15.10). The tests that actually detect
    /// that class of change are the construction golden
    /// (`palw_leaf_merkle_root_construction_golden`) and the cross-crate golden
    /// (`palw_leaf_merkle_root_cross_crate_golden_vector` / its mirror in `mil/miner`). Do not read this
    /// pin as covering them.
    ///
    /// What this pin DOES buy for 9 -> 10 is the negative: it proves the Merkle slice did not move any
    /// persisted field while it was changing what one of them means.
    ///
    /// # 10 -> 11 (ADR-0040 §5.15.13, gate G16) — again a bump with NOTHING below moving
    ///
    /// The G16 slice adds a NEW block-keyed column family (`DatabaseStorePrefixes::PalwPaidWork`) and a
    /// TRAILING `WorkRewardClass` variant (`ReplicaPalwDuplicateWork`, which rides the persisted
    /// `VirtualState`). Neither touches any type pinned below, which is exactly why the bump is paid
    /// here rather than inferred from a red constant: a new CF is a persisted-format ADDITION that this
    /// pin is structurally blind to. The trailing-variant discriminants are asserted separately, on the
    /// wire bytes, in `consensus/core/src/coinbase.rs`.
    #[test]
    fn palw_persisted_layouts_are_pinned_to_latest_db_version_16() {
        // Pinned encodings as of LATEST_DB_VERSION = 14. DA Object-v2 extends the persisted public
        // leaf with an object version and Receipt-v3 selected-chain expectations; the same cutover
        // also advances the pruning snapshot schema. The certificate fixture deliberately carries
        // a non-empty V2 vote: an empty Vec would hide any future change to the persisted vote element.
        const LIFECYCLE_LEN: usize = 253;
        const VIEW_LEN: usize = 335;
        const CERT_LEN: usize = 730;
        const LIFECYCLE_FNV: u64 = 0x5b97_11bf_4e7c_0b6d;
        const VIEW_FNV: u64 = 0x2d33_af70_53e7_9fcd;
        const CERT_FNV: u64 = 0xad07_47fa_c108_45c8;
        // ADR-0040 P1-10 survey, incidental finding: `PalwPublicLeafV1` and `PalwBatchManifestV1` are
        // ALSO bincode-persisted (`DbPalwStore::{leaves, manifests}`, consensus/src/model/stores/palw.rs)
        // yet were absent from this pin, so the guard that exists precisely because ADR-0040 once shipped
        // an unbumped layout change had a hole in exactly the two structs any future LeafV2 slice touches
        // first. Re-pinned at LATEST_DB_VERSION = 14 for Object-v2.
        const LEAF_LEN: usize = 1189;
        const MANIFEST_LEN: usize = 472;
        const LEAF_FNV: u64 = 0x39fa_19e9_f393_efef;
        const MANIFEST_FNV: u64 = 0x7daa_fe6a_cc52_faa3;

        // A canonical, fully-populated lifecycle: every field non-default, so a reorder shows up as a
        // byte-hash change even when it preserves the total length.
        let lifecycle = PalwBatchLifecycleV1 {
            status: PalwBatchStatus::Active,
            registration_epoch: 7,
            activation_not_before_epoch: 8,
            expiry_epoch: 21,
            leaf_count: 4,
            chunk_count: 2,
            chunks_present: [0x3, 0, 0, 0],
            leaf_root: h(0x11),
            cert_hash: Some(h(0x12)),
            sponsor_tag_hi: 0,
            sponsor_tag_lo: 0,
            cert_approving_stake: 0,
            first_cert_daa: Some(1_234),
            revoked_from_daa: None,
        };
        let mut batches = BTreeMap::new();
        batches.insert(h(0x10), lifecycle.clone());
        let view = PalwBatchViewV1 { version: 1, batches };

        let cert = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id: h(1),
            manifest_hash: h(2),
            leaf_root: h(3),
            audit_beacon_epoch: 5,
            audit_sample_root: h(4),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: ZERO_HASH64,
            certificate_epoch: 6,
            activation_epoch: 7,
            expiry_epoch: 13,
            auditor_set_commitment: h(5),
            approving_stake: 0,
            votes: vec![PalwAuditorVoteV2 {
                bond_outpoint: TransactionOutpoint::new(h(0x06), 3),
                vote: 1,
                checked_leaf_bitmap_root: h(0x07),
                passed_leaf_count: 2,
                rejected_leaf_bitmap_root: ZERO_HASH64,
                signature: vec![0x55, 0x56, 0x57],
            }],
        };

        // A canonical, fully-populated leaf + manifest: every field distinct and non-default, and the
        // two `ScriptPublicKey`s given DIFFERENT lengths so a swap of the two reward scripts (the P1-1
        // reward-theft field pair) changes the byte digest.
        let leaf = PalwPublicLeafV1 {
            version: 1,
            batch_id: h(0x30),
            leaf_index: 3,
            job_nullifier: h(0x31),
            ticket_nullifier_commitment: h(0x32),
            model_profile_id: h(0x33),
            runtime_class_id: h(0x34),
            shape_id: 5,
            quantum_count: 9,
            proof_type: PalwProofType::ReplicaExactV1.as_u8(),
            provider_a_bond: TransactionOutpoint::new(h(0x35), 1),
            provider_b_bond: TransactionOutpoint::new(h(0x36), 2),
            provider_a_reward_script: ScriptPublicKey::from_vec(0, vec![0xaa, 0xbb]),
            provider_b_reward_script: ScriptPublicKey::from_vec(1, vec![0xcc, 0xdd, 0xee]),
            ticket_authority_pk_hash: h(0x37),
            private_match_commitment: h(0x38),
            receipt_da_object_version: da::PALW_RECEIPT_DA_OBJECT_VERSION_V1,
            receipt_da_root: h(0x39),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: 7,
            activation_epoch: 9,
            expiry_epoch: 21,
            leaf_bond_sompi: 1_000_000,
            // D3-b PCPB fields — every value distinct and non-default so the FNV digest is sensitive
            // to each (same rule as the rest of this canonical pin fixture).
            a_commit: h(0x3a),
            a_commit_epoch: 6,
            provider_snapshot_root: h(0x3b),
            assignment_proof_root: h(0x3c),
            dispatch_kind: PALW_DISPATCH_KIND_SELF_SERIAL,
        };
        let manifest = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(0x40),
            registration_epoch: 7,
            model_profile_id: h(0x41),
            runtime_class_id: h(0x42),
            leaf_count: 4,
            chunk_count: 2,
            leaf_root: h(0x43),
            descriptor_root: h(0x44),
            total_leaf_bond_sompi: 4_000_000,
            audit_policy_id: h(0x45),
            activation_not_before_epoch: 9,
            expiry_epoch: 21,
        };

        // Length pins catch add/remove; the byte pins additionally catch reorder and type changes.
        let lifecycle_bytes = bincode::serialize(&lifecycle).unwrap();
        let view_bytes = bincode::serialize(&view).unwrap();
        let cert_bytes = bincode::serialize(&cert).unwrap();
        let leaf_bytes = bincode::serialize(&leaf).unwrap();
        let manifest_bytes = bincode::serialize(&manifest).unwrap();

        assert_eq!(
            lifecycle_bytes.len(),
            LIFECYCLE_LEN,
            "PalwBatchLifecycleV1 bincode layout changed - bump LATEST_DB_VERSION (see this test's docs)"
        );
        assert_eq!(
            view_bytes.len(),
            VIEW_LEN,
            "PalwBatchViewV1 bincode layout changed - bump LATEST_DB_VERSION (see this test's docs)"
        );
        assert_eq!(
            cert_bytes.len(),
            CERT_LEN,
            "PalwBatchCertificateV2 bincode layout changed - bump LATEST_DB_VERSION (see this test's docs)"
        );
        assert_eq!(
            leaf_bytes.len(),
            LEAF_LEN,
            "PalwPublicLeafV1 bincode layout changed - bump LATEST_DB_VERSION (see this test's docs)"
        );
        assert_eq!(
            manifest_bytes.len(),
            MANIFEST_LEN,
            "PalwBatchManifestV1 bincode layout changed - bump LATEST_DB_VERSION (see this test's docs)"
        );

        // A cheap order-sensitive digest over the encodings (FNV-1a 64), so a same-length field
        // reorder is caught too.
        fn fnv1a(bytes: &[u8]) -> u64 {
            let mut hash = 0xcbf2_9ce4_8422_2325_u64;
            for b in bytes {
                hash ^= *b as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash
        }
        assert_eq!(fnv1a(&lifecycle_bytes), LIFECYCLE_FNV, "PalwBatchLifecycleV1 field order/types changed");
        assert_eq!(fnv1a(&view_bytes), VIEW_FNV, "PalwBatchViewV1 field order/types changed");
        assert_eq!(fnv1a(&cert_bytes), CERT_FNV, "PalwBatchCertificateV2 field order/types changed");
        assert_eq!(fnv1a(&leaf_bytes), LEAF_FNV, "PalwPublicLeafV1 field order/types changed");
        assert_eq!(fnv1a(&manifest_bytes), MANIFEST_FNV, "PalwBatchManifestV1 field order/types changed");

        // The pins are only meaningful if the encodings actually round-trip.
        assert_eq!(bincode::deserialize::<PalwBatchLifecycleV1>(&lifecycle_bytes).unwrap(), lifecycle);
        assert_eq!(bincode::deserialize::<PalwBatchViewV1>(&view_bytes).unwrap(), view);
        assert_eq!(bincode::deserialize::<PalwBatchCertificateV2>(&cert_bytes).unwrap(), cert);
        assert_eq!(bincode::deserialize::<PalwPublicLeafV1>(&leaf_bytes).unwrap(), leaf);
        assert_eq!(bincode::deserialize::<PalwBatchManifestV1>(&manifest_bytes).unwrap(), manifest);

        // The defect this pin exists to prevent, demonstrated: an encoding produced BEFORE the
        // ADR-0040 fields were added is strictly shorter, and the current decoder fails on it rather
        // than silently misreading it. Truncating at the point the old struct ended reproduces that.
        let old_shape_view = &view_bytes[..view_bytes.len() - (64 + 8 + 8)];
        assert!(
            bincode::deserialize::<PalwBatchViewV1>(old_shape_view).is_err(),
            "an old-shape row must FAIL to decode, never decode into a plausible-but-wrong value"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // ADR-0040 §7.2 — the source-of-truth gate→verifier registry, plus the meta-test that keeps the
    // ADR gate table honest. This exists because of the G3 accident: the gate table named a verifier
    // (`palw_leaf_membership_and_immutability`) that never existed in the tree, so a 77%-reward-theft
    // hole read as a CLOSED gate on a phantom test. This registry is the single place the gate→test
    // mapping lives, and `palw_gate_table_verifiers_all_resolve` fails the build if any VerifierExists
    // verifier does not resolve to a real `fn <name>`, or if a Measurement/Unimplemented verifier
    // silently gains a real `fn` without its status being raised. It is a test + registry only — no
    // consensus behavior depends on it.

    /// Whether a gate's named verifier(s) can (and therefore MUST) exist as real code today.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum GateVerifierStatus {
        /// A real verifier test with the named `fn` EXISTS in-tree — every name MUST resolve. This is a
        /// claim about VERIFIER RESOLUTION only (the G3-recurrence guard: no gate may name a phantom
        /// test), NOT a claim that the gate is operationally satisfied. G16, for instance, has real
        /// named tests yet is only bounded-window closed; the ADR row's prose carries that caveat.
        VerifierExists,
        /// Needs real hardware / elapsed time / cross-device measurement. The verifier does not and
        /// cannot exist in code yet; it MUST NOT resolve to a real `fn`.
        Measurement,
        /// The mechanism itself is not built. The verifier does not exist yet; it MUST NOT resolve.
        Unimplemented,
    }

    /// The 16 ADR-0040 activation gates (G1..G16) → verifier status → verifier test name(s).
    ///
    /// The VerifierExists names are the REAL tests (never the phantom names the ADR table historically
    /// carried). The Measurement/Unimplemented names are the intended future verifier names; the
    /// meta-test asserts they do NOT yet exist, so landing one for such a gate forces a status bump to VerifierExists.
    const PALW_GATE_VERIFIERS: &[(&str, GateVerifierStatus, &[&str])] = &[
        // G1: released PALW presets track activation; an explicitly closed copy still rejects
        // algo-4 headers before GHOSTDAG.
        (
            "G1",
            GateVerifierStatus::VerifierExists,
            &["palw_algo4_rejected_while_accept_lever_closed", "palw_activated_presets_bound_the_view"],
        ),
        // G2 — StopShip: every admitted reward script yields a coinbase-representable script; rule
        // violations are rejected at admission.
        ("G2", GateVerifierStatus::VerifierExists, &["palw_reward_script_admission_matches_coinbase_representability"]),
        // G3 — StopShip: leaf membership binds to `manifest.leaf_root`; index-squat rejected before
        // store; write-once; reward scripts immutable after acceptance; construction golden vectors.
        (
            "G3",
            GateVerifierStatus::VerifierExists,
            &[
                "chunk_index_squat_is_rejected_before_the_leaf_is_stored",
                "leaf_chunk_admission_binds_to_manifest_and_is_write_once",
                "a_member_leaf_cannot_be_replayed_at_another_index",
                "membership_proof_length_is_exact_in_both_directions",
                "leaf_write_once_still_fires_after_the_membership_gate",
                "reward_scripts_are_immutable_after_acceptance",
                "palw_leaf_merkle_root_construction_golden",
                "palw_leaf_merkle_root_cross_crate_golden_vector",
            ],
        ),
        // G4 — StopShip: forged-sig / zero-stake / num==0 / unselected-auditor / inconsistent-root
        // certificates are all rejected (quorum arithmetic in core; attestation in processes).
        (
            "G4",
            GateVerifierStatus::VerifierExists,
            &["certificate_stake_weighted_quorum", "certificate_attestation_rederives_committee_sample_and_signatures"],
        ),
        // G5 — Activation: AUTH-02 — a won ticket cannot be re-minted, and authorization binds every
        // header field.
        (
            "G5",
            GateVerifierStatus::VerifierExists,
            &["palw_algo4_reminted_ticket_is_rejected_auth02", "palw_algo4_authorization_binds_every_header_field_auth02"],
        ),
        // G6 — DOS-01: the threshold-less production-pipeline measurement harness now exists, so the
        // named verifier resolves. This is verifier provenance only: thresholds, representative
        // hardware, concurrent flood/soak and signoff remain a Measurement gate.
        ("G6", GateVerifierStatus::VerifierExists, &["palw_header_spam_bounded"]),
        // G7 — regression coverage table.
        ("G7", GateVerifierStatus::VerifierExists, &["palw_prod_findings_all_covered"]),
        // G8 — INT: two-party independent canonical-artifact reproduction. Cross-machine measurement.
        ("G8", GateVerifierStatus::Measurement, &["palw_artifact_reproducible"]),
        // G9 — INT-01: all K0/K1 vectors match CPU reference across backends. Cross-machine.
        ("G9", GateVerifierStatus::Measurement, &["palw_conformance_vectors_match"]),
        // G10 — MATCH-01: pairwise cross-backend zero-mismatch over the job matrix. Cross-device.
        ("G10", GateVerifierStatus::Measurement, &["palw_cross_backend_pairwise"]),
        // G11 — 72h soak with zero mismatch. Elapsed-time measurement.
        ("G11", GateVerifierStatus::Measurement, &["palw_soak_72h"]),
        // G12 — PCPB / escrow / reroll / timeout / global-nullifier multi-node E2E. Not built.
        ("G12", GateVerifierStatus::Unimplemented, &["palw_pcpb_e2e"]),
        // G13 — auditor-quorum producer/verifier integration. Multi-node withholding and reorg paths
        // remain integration-test concerns.
        (
            "G13",
            GateVerifierStatus::VerifierExists,
            &[
                "producer_built_certificate_round_trips_through_verify_certificate_attestation",
                "certificate_attestation_rederives_committee_sample_and_signatures",
                "certificate_stake_weighted_quorum",
                "sel01_credential_aggregation_makes_bond_splitting_worthless",
            ],
        ),
        // G14 — WeightRaise: β auto-degradation runs live and observed concentration stays below β_max.
        // Live measurement + signoff.
        ("G14", GateVerifierStatus::Measurement, &["palw_beta_degradation_live"]),
        // G15 — enforcement-point sweep (§2.6), including the current off-chain PMC-01 gap.
        ("G15", GateVerifierStatus::VerifierExists, &["palw_enforcement_points_total"]),
        // G16 — job-nullifier duplicate-work rejection at the reward/virtual coordinate, and no
        // first-claim-wins registry at the body/mergeset coordinate.
        (
            "G16",
            GateVerifierStatus::VerifierExists,
            &[
                "palw_job_nullifier_reland_at_reward_coordinate",
                "palw_job_nullifier_reland_dedups_across_chain_blocks",
                "no_job_nullifier_registry_at_the_body_coordinate",
            ],
        ),
    ];

    /// True iff `src` declares `name` as a function (`fn <name>` / `async fn <name>` / `pub fn <name>`
    /// …), matching `fn` and `<name>` as whole words. Rejects `myfn name`, `<name>_suffix`, and any
    /// appearance of `name` that is not a real `fn` declaration (e.g. a string literal or a call).
    fn source_declares_fn(src: &str, name: &str) -> bool {
        fn is_ident_byte(b: u8) -> bool {
            b.is_ascii_alphanumeric() || b == b'_'
        }
        let bytes = src.as_bytes();
        let mut start = 0usize;
        while let Some(rel) = src[start..].find(name) {
            let idx = start + rel;
            let end = idx + name.len();
            // `name` must end on a word boundary (next byte is not an identifier byte).
            let after_ok = bytes.get(end).is_none_or(|&b| !is_ident_byte(b));
            // Immediately before `name` there must be whitespace, then the whole word `fn`.
            let mut j = idx;
            let mut saw_ws = false;
            while j > 0 && (bytes[j - 1] as char).is_whitespace() {
                j -= 1;
                saw_ws = true;
            }
            let fn_ok = saw_ws && j >= 2 && &src[j - 2..j] == "fn" && (j == 2 || !is_ident_byte(bytes[j - 3]));
            if after_ok && fn_ok {
                return true;
            }
            start = end;
        }
        false
    }

    /// Walk up from `CARGO_MANIFEST_DIR` to the directory whose `Cargo.toml` declares `[workspace]`.
    fn workspace_root() -> std::path::PathBuf {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            if let Ok(s) = std::fs::read_to_string(dir.join("Cargo.toml"))
                && s.contains("[workspace]")
            {
                return dir;
            }
            assert!(dir.pop(), "no [workspace] Cargo.toml found walking up from CARGO_MANIFEST_DIR");
        }
    }

    /// Collect every `*.rs` under `root`, excluding `target/` and `.git/`.
    fn collect_rs_files(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                if name == "target" || name == ".git" {
                    continue;
                }
                collect_rs_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// Verifies the PALW gate registry against workspace functions.
    ///
    /// `VerifierExists` rows must resolve to a function, measurement-only rows must not claim one, and
    /// the registry must contain the 16 unique gate ids G1 through G16.
    #[test]
    fn palw_gate_table_verifiers_all_resolve() {
        // Structural check: exactly 16 gates with no duplicate ids.
        assert_eq!(PALW_GATE_VERIFIERS.len(), 16, "the PALW gate table has exactly 16 gates (G1..G16)");
        let mut ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (gate, ..) in PALW_GATE_VERIFIERS {
            assert!(ids.insert(gate), "duplicate gate id in PALW_GATE_VERIFIERS: {gate}");
        }

        // Walk the workspace once and record which named verifiers are declared as real `fn`s.
        let root = workspace_root();
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        assert!(files.len() > 100, "workspace walk found suspiciously few .rs files ({}) under {}", files.len(), root.display());

        let mut targets: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (_, _, names) in PALW_GATE_VERIFIERS {
            for name in *names {
                targets.insert(name);
            }
        }
        let mut declared: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for path in &files {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for name in targets.iter().copied() {
                if !declared.contains(name) && source_declares_fn(&content, name) {
                    declared.insert(name);
                }
            }
        }

        // (a) VerifierExists ⇒ must resolve; (b) Measurement/Unimplemented ⇒ must NOT resolve.
        let mut problems: Vec<String> = Vec::new();
        for (gate, status, names) in PALW_GATE_VERIFIERS {
            for name in *names {
                let exists = declared.contains(name);
                match status {
                    GateVerifierStatus::VerifierExists if !exists => problems.push(format!(
                        "{gate} (VerifierExists) names verifier `{name}` but no `fn {name}` exists in the workspace \
                         — a PHANTOM verifier (the G3 accident). Either the test was renamed/removed, or the registry is wrong."
                    )),
                    GateVerifierStatus::Measurement | GateVerifierStatus::Unimplemented if exists => problems.push(format!(
                        "{gate} ({status:?}) marks verifier `{name}` as not-yet-existing, but a real `fn {name}` now \
                         exists — flip this gate to VerifierExists and reconcile the ADR-0040 gate table."
                    )),
                    _ => {}
                }
            }
        }
        assert!(problems.is_empty(), "PALW gate-table verifier reconciliation failed:\n{}", problems.join("\n"));
    }

    // ---------------------------------------------------------------------------------------------
    // ADR-0040 §7.2 G7 (`palw_prod_findings_all_covered`) — the per-finding regression-coverage table,
    // same shape as `PALW_GATE_VERIFIERS` / `palw_gate_table_verifiers_all_resolve`. This is the single
    // place mapping each §2 PROD finding to the REAL regression test(s) that cover it. The meta-test
    // (a) fails if a `Covered` finding names a test that does not resolve to a real `fn` (the G3 phantom
    // guard, applied per finding — the same accident that hid a 77%-reward-theft hole behind a named-but-
    // nonexistent verifier), (b) fails if a `Measurement`/`Unimplemented` finding's verifier silently
    // gains a real `fn` (forcing a status bump), and (c) fails unless the table's ID set is EXACTLY the
    // §2 PROD finding set — so adding a PROD row to ADR-0040 without registering a test here breaks the
    // build. Test + registry only; no consensus behavior depends on it.

    /// Whether a §2 PROD finding's regression coverage can (and therefore MUST) exist as real code today.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum FindingCoverage {
        /// A real in-tree regression test covers the finding — every named `fn` MUST resolve.
        Covered,
        /// Closing the finding needs real hardware / elapsed time / cross-device measurement; the verifier
        /// cannot exist in code yet and MUST NOT resolve. No CURRENT PROD finding is here — the audit's
        /// measurement-bound findings (INT-01 / MATCH-01) are `PRIM` reachability and belong to the G9 /
        /// G10 measurement gates, not this PROD table — but the arm is kept for parity with the gate table
        /// and to receive a future PROD finding of that kind.
        #[allow(dead_code)]
        Measurement,
        /// The consensus-visible mechanism the finding needs is not built; the named verifier MUST NOT
        /// resolve. No CURRENT PROD finding is here — PCPB-01 was promoted to `Covered` when ADR-0045
        /// D3-b landed its verifier (`palw_pcpb_ticket_binding_enforced`); the arm is kept for parity
        /// with the gate table and to receive a future such finding.
        #[allow(dead_code)]
        Unimplemented,
    }

    /// Every ADR-0040 §2 PROD-reachability finding → coverage → the REAL regression test fn name(s) that
    /// cover it (`Covered`), or the intended-but-absent verifier name (`Measurement` / `Unimplemented`,
    /// which MUST NOT resolve). "PROD" is defined mechanically: the finding's Reachability column in
    /// §2.1 / §2.2 / §2.3 has base tag `PROD` (`PROD`, `PROD → LANDED`, or `PROD → 部分 CLOSED`). `PRIM` /
    /// `DOC` / `CLOSED（削除）` rows are excluded (e.g. SEL-01/INT-01/MATCH-01/AO-02/03/SS-04/05/ECON-04/
    /// TGT-02/03 = PRIM; SLASH-01/DOC-01/02 = DOC; DOS-02 = deleted). `EXPECTED_PROD_FINDING_IDS` in the
    /// meta-test hard-codes that membership.
    const PALW_PROD_FINDINGS: &[(&str, FindingCoverage, &[&str])] = &[
        // ---- §2.1 Critical (closed by StopShip gates G1..G4) ----
        // BIND-01 — leaf blob bound to `manifest.leaf_root`; injection rejected; content-addressed write-once.
        (
            "BIND-01",
            FindingCoverage::Covered,
            &["leaf_chunk_admission_binds_to_manifest_and_is_write_once", "chunk_index_squat_is_rejected_before_the_leaf_is_stored"],
        ),
        // CERT-01 — real ML-DSA signatures over active bonded stake, re-derived committee/sample, quorum.
        // The G13 cross-crate E2E additionally drives a REAL miner-produced certificate through the real
        // verifier (accept + every rejection variant), so a producer/verifier drift breaks it too.
        (
            "CERT-01",
            FindingCoverage::Covered,
            &[
                "certificate_attestation_rederives_committee_sample_and_signatures",
                "certificate_stake_weighted_quorum",
                "producer_built_certificate_round_trips_through_verify_certificate_attestation",
            ],
        ),
        // LEAF-01 — reward scripts immutable after acceptance; write-once leaf store.
        (
            "LEAF-01",
            FindingCoverage::Covered,
            &["reward_scripts_are_immutable_after_acceptance", "leaf_chunk_admission_binds_to_manifest_and_is_write_once"],
        ),
        // ECON-01 — every admitted reward script yields a coinbase-representable script (G2).
        ("ECON-01", FindingCoverage::Covered, &["palw_reward_script_admission_matches_coinbase_representability"]),
        // DOS-01 — algo-4 headers are rejected pre-GHOSTDAG while the accept lever is closed (no free
        // header-stage store writes). The eventual flood THRESHOLD is the G6 measurement gate; the current
        // reject path is what is testable today.
        ("DOS-01", FindingCoverage::Covered, &["palw_algo4_rejected_while_accept_lever_closed"]),
        // ---- §2.2 High ----
        // AUTH-01 — authorization is generated, transported, and verified; signature is context-bound.
        (
            "AUTH-01",
            FindingCoverage::Covered,
            &[
                "miner_authorization_satisfies_every_clause_7_check",
                "authorization_signature_is_context_bound",
                "palw_algo4_authorization_binds_every_header_field_auth02",
            ],
        ),
        // AUTH-02 — a won ticket cannot be re-minted; authorization binds every header field.
        (
            "AUTH-02",
            FindingCoverage::Covered,
            &["palw_algo4_reminted_ticket_is_rejected_auth02", "palw_algo4_authorization_binds_every_header_field_auth02"],
        ),
        // AUTH-03 — leaf `ticket_authority_pk_hash` binds the block's signing authority.
        (
            "AUTH-03",
            FindingCoverage::Covered,
            &["pk_hash_matches_the_consensus_authority_derivation", "every_bound_field_is_load_bearing"],
        ),
        // TGT-01 — REFUTED: the target interval is consensus-derived (pinned to `daa_score`), not miner-chosen.
        ("TGT-01", FindingCoverage::Covered, &["target_interval_is_pinned_to_daa_score_not_miner_chosen"]),
        // BIND-02 — a header may not name a certificate certifying a DIFFERENT batch (CertBatchMismatch).
        (
            "BIND-02",
            FindingCoverage::Covered,
            &["palw_algo4_cert_belonging_to_another_batch_rejected_e2e", "certificate_must_bind_to_its_batch_manifest_and_leaf_root"],
        ),
        // BIND-03 — coordinate decision (view stays at body/mergeset; resolution at reward/virtual). The
        // decision is encoded by the test asserting there is NO first-claim registry at the body coordinate.
        (
            "BIND-03",
            FindingCoverage::Covered,
            &["no_job_nullifier_registry_at_the_body_coordinate", "s3_vote_censorship_is_not_remediable_at_the_body_coordinate"],
        ),
        // BIND-04 — the authenticated sidecar machinery exists, but its lifecycle source is the raw
        // body/mergeset view while content provenance is written only for virtually accepted txs. A
        // Header-v4 lifecycle provenance is virtual/UTXO-accepted, block-keyed and transported with
        // the exact named certificate blob; raw body order cannot change capture.
        ("BIND-04", FindingCoverage::Covered, &["palw_pruning_snapshot_uses_accepted_block_keyed_lifecycle_provenance"]),
        // VIEW-01 — a batch is not usable by a ticket in the block that registers it (deliberate, pinned).
        ("VIEW-01", FindingCoverage::Covered, &["a_batch_is_not_block_eligible_in_its_own_registration_epoch"]),
        // DOS-03 — the fork-relative view is bounded (`max_view_batches`); activated presets must set it non-zero.
        ("DOS-03", FindingCoverage::Covered, &["palw_activated_presets_bound_the_view", "view_size_scales_only_with_batch_count"]),
        // DOS-04 — admission bounds `activation_not_before_epoch` from above (no permanent view pin).
        ("DOS-04", FindingCoverage::Covered, &["c4_content_address_admission_and_view"]),
        // SS-01 — deterministic/atomic capture, import and recovery over accepted provenance.
        ("SS-01", FindingCoverage::Covered, &["palw_pruned_ibd_matches_from_genesis_under_raw_overlay_adversary"]),
        // DA-01 — the public admission seam resolves the selected-chain leaf/provider state, verifies
        // both owner-authorized Receipt-v3 envelopes and their exact pair, then durably stores the bytes.
        (
            "DA-01",
            FindingCoverage::Covered,
            &[
                "admission_requires_the_leaf_coordinate_in_a_live_sink_view",
                "public_da_admission_runs_full_v2_semantics_before_root_only_storage",
            ],
        ),
        // SAMPLE-01 — §5.17 CERT-REDERIVE: `verify_certificate_attestation` re-derives `audit_sample_root`.
        ("SAMPLE-01", FindingCoverage::Covered, &["certificate_attestation_rederives_committee_sample_and_signatures"]),
        // AUTHSET-01 — §5.17 CERT-REDERIVE: `select_auditor_committee` re-derivation + slate-external vote reject.
        ("AUTHSET-01", FindingCoverage::Covered, &["certificate_attestation_rederives_committee_sample_and_signatures"]),
        // PCPB-01 — CLOSED by ADR-0045 D3-b: clauses 11/12 gate every leaf at acceptance (challenge
        // re-derivation + dispatch-evidence re-run against store-resolved context) and clause 13
        // re-checks the binding at mint through the real `check_palw_ticket`. Coverage spans all
        // three coordinates: the pure-evidence tautology killers (fake root swap/clause 0, undrawn
        // provider, identical pair, forged signature, a_commit not embedded, branch↔kind mismatch),
        // the acceptance-arm §8 negatives (free epochs / sentinels / registry both-directions /
        // seat binding), and the mint-time clause-13 enforcement on real blocks.
        (
            "PCPB-01",
            FindingCoverage::Covered,
            &[
                "palw_pcpb_ticket_binding_enforced",
                "pcpb_beacon_arm_validates_and_is_not_a_tautology",
                "pcpb_beacon_arm_rejects_same_provider_for_both_slots",
                "pcpb_self_arm_rejects_unbound_forged_or_swapped",
                "pcpb_clause11_acceptance_rejects_free_epochs_and_sentinels",
                "pcpb_clause12_acceptance_selfserial_registry_and_seats",
            ],
        ),
        // ECON-03 — the 77% provider base is paid only against resolved, locked, owner-bound collateral
        // (value-lock / spend-gate / ownership). NOTE: ECON-03 is not FULLY closed — slashing is still open
        // (§2.3′) — but the closed legs have real regression tests, which is what G7 requires per finding.
        (
            "ECON-03",
            FindingCoverage::Covered,
            &[
                "econ03_a_provider_bond_with_no_backing_is_rejected",
                "palw_algo4_unbacked_provider_bond_pays_nothing_e2e",
                "palw_algo4_leaf_naming_unowned_bond_pays_nothing_e2e",
                "econ03_only_the_bond_owner_can_request_an_exit",
            ],
        ),
        // ---- §2.3 Medium / Low ----
        // QUORUM-02 — `beacon_quorum_reached` guards `num == 0` (mirror vacuity of `den == 0`).
        ("QUORUM-02", FindingCoverage::Covered, &["beacon_quorum_stake_weighted", "certificate_stake_weighted_quorum"]),
        // SHAPE-01 — the per-field non-zero detector the shape rule enforces (pre/post-activation halves).
        ("SHAPE-01", FindingCoverage::Covered, &["has_nonzero_palw_fields_detects_each_field"]),
        // ECON-02 — the PALW coinbase output cap widens under fence; unchanged on value-bearing presets.
        (
            "ECON-02",
            FindingCoverage::Covered,
            &[
                "coinbase_output_cap_widens_to_three_per_blue_on_a_palw_lane",
                "worst_case_palw_coinbase_needs_the_widened_cap",
                "coinbase_output_cap_ignores_validator_and_bounty_tail",
            ],
        ),
        // DEMO-01 — the in-node demo mint is devnet-palw ONLY; the wrong-net refusal path is pinned.
        ("DEMO-01", FindingCoverage::Covered, &["mint_error_classification_quiets_expected_preconditions"]),
    ];

    /// ADR-0040 §7.2 G7 — every §2 PROD finding has a real, in-tree regression test (catch-all forbidden).
    #[test]
    fn palw_prod_findings_all_covered() {
        // (c) The table's ID set must be EXACTLY the §2 PROD finding set. A PROD row added to §2 without a
        //     coverage entry here — or a stale entry — fails the build.
        const EXPECTED_PROD_FINDING_IDS: &[&str] = &[
            // §2.1 Critical
            "BIND-01",
            "CERT-01",
            "LEAF-01",
            "ECON-01",
            "DOS-01",
            // §2.2 High
            "AUTH-01",
            "AUTH-02",
            "AUTH-03",
            "TGT-01",
            "BIND-02",
            "BIND-03",
            "BIND-04",
            "VIEW-01",
            "DOS-03",
            "DOS-04",
            "SS-01",
            "DA-01",
            "SAMPLE-01",
            "AUTHSET-01",
            "PCPB-01",
            "ECON-03",
            // §2.3 Medium / Low
            "QUORUM-02",
            "SHAPE-01",
            "ECON-02",
            "DEMO-01",
        ];

        let mut table_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (id, ..) in PALW_PROD_FINDINGS {
            assert!(table_ids.insert(id), "duplicate finding id in PALW_PROD_FINDINGS: {id}");
        }
        let expected: std::collections::HashSet<&str> = EXPECTED_PROD_FINDING_IDS.iter().copied().collect();
        assert_eq!(EXPECTED_PROD_FINDING_IDS.len(), expected.len(), "duplicate id in EXPECTED_PROD_FINDING_IDS");
        let missing: Vec<&str> = expected.difference(&table_ids).copied().collect();
        let extra: Vec<&str> = table_ids.difference(&expected).copied().collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "PALW_PROD_FINDINGS must cover EXACTLY the §2 PROD findings. missing (in ADR §2, not registered here): {missing:?}; \
             extra (registered here, not a §2 PROD row): {extra:?}"
        );

        // Walk the workspace once and record which named verifiers resolve to a real `fn`.
        let root = workspace_root();
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        assert!(files.len() > 100, "workspace walk found suspiciously few .rs files ({}) under {}", files.len(), root.display());

        let mut targets: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (_, _, names) in PALW_PROD_FINDINGS {
            for name in *names {
                targets.insert(name);
            }
        }
        let mut declared: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for path in &files {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for name in targets.iter().copied() {
                if !declared.contains(name) && source_declares_fn(&content, name) {
                    declared.insert(name);
                }
            }
        }

        // (a) Covered ⇒ every named test resolves; (b) Measurement/Unimplemented ⇒ named verifier does NOT.
        let mut problems: Vec<String> = Vec::new();
        for (id, coverage, names) in PALW_PROD_FINDINGS {
            for name in *names {
                let exists = declared.contains(name);
                match coverage {
                    FindingCoverage::Covered if !exists => problems.push(format!(
                        "{id} (Covered) names regression test `{name}` but no `fn {name}` exists in the workspace \
                         — a PHANTOM verifier (the G3 accident, per finding)."
                    )),
                    FindingCoverage::Measurement | FindingCoverage::Unimplemented if exists => problems.push(format!(
                        "{id} ({coverage:?}) marks `{name}` as not-yet-existing, but a real `fn {name}` now exists \
                         — flip this finding to Covered and reconcile the ADR-0040 §2 / §7.2 tables."
                    )),
                    _ => {}
                }
            }
        }
        assert!(problems.is_empty(), "PALW per-finding coverage reconciliation failed:\n{}", problems.join("\n"));
    }

    // ---------------------------------------------------------------------------------------------
    // ADR-0040 §2.6 / §7.2 G15 (`palw_enforcement_points_total`) — the enforcement-point scan. Every
    // hash-committed object whose rule claims a preimage PROPERTY must have a consensus-visible point
    // where the bytes enter view and the property is checked; otherwise it is a gap. The scan started at
    // 4 gaps (DA-01 / SAMPLE-01 / AUTHSET-01 / PMC-01); §5.17 CERT-REDERIVE landed enforcement points for
    // SAMPLE-01 and AUTHSET-01, and DA Object V2 admission closed DA-01, shrinking the gap to 1 (PMC-01,
    // off-chain). The meta-test
    // pins the enforced fns AND the exact gap set, so a future silent regression that drops an enforcement
    // point (as SAMPLE-01 / AUTHSET-01 were before CERT-REDERIVE) fails the build.

    /// Whether a hash-committed PALW object's claimed preimage-property has a consensus-visible enforcement point.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum EnforcementStatus {
        /// A real enforcing `fn` puts the preimage bytes in consensus view and checks the property — the
        /// named `fn` MUST resolve.
        Enforced,
        /// The committed content is OFF-CHAIN, so consensus can never re-derive/check the property; it is
        /// inherently a data-availability / measurement problem, not a missing code check. The third field
        /// is a human reason, NOT a fn name.
        GapOffchain,
        /// The enforcement mechanism is simply unbuilt with no off-chain reason. No current object is here
        /// (the one live gap is off-chain); kept for parity and to receive a future such gap.
        #[allow(dead_code)]
        GapUnimplemented,
    }

    /// §2.6.1 enforcement-point scan (current). Each hash-committed object / preimage-property claim →
    /// status → enforcing fn (`Enforced`) or gap reason (gaps). SAMPLE-01 / AUTHSET-01 are Enforced via
    /// §5.17 CERT-REDERIVE; DA-01 is Enforced via the selected-chain admission seam; PMC-01 remains an
    /// off-chain gap.
    const PALW_ENFORCEMENT_POINTS: &[(&str, EnforcementStatus, &str)] = &[
        // AUTH-01 — authorization commits to the block's whole header preimage; body clause 7 enforces it.
        ("AUTH-01 header_preimage_commitment", EnforcementStatus::Enforced, "palw_authorization_commitment"),
        // BIND-01 — stored leaf is content-addressed and write-once against `manifest.leaf_root`.
        ("BIND-01 manifest.leaf_root <-> stored leaf", EnforcementStatus::Enforced, "insert_leaf"),
        // SAMPLE-01 — §5.17 CERT-REDERIVE: the certificate verifier re-derives `audit_sample_root`.
        ("SAMPLE-01 audit_sample_root (§5.17 CERT-REDERIVE)", EnforcementStatus::Enforced, "verify_certificate_attestation"),
        // AUTHSET-01 — §5.17 CERT-REDERIVE: the auditor slate is re-derived and slate-external votes rejected.
        ("AUTHSET-01 auditor_set_commitment (§5.17 CERT-REDERIVE)", EnforcementStatus::Enforced, "select_auditor_committee"),
        // ticket_nullifier_commitment — pure byte match with a live enforcement point that discloses it.
        ("ticket_nullifier_commitment", EnforcementStatus::Enforced, "verify_palw_ticket_store_facts"),
        // PalwBeaconCommitV1.commitment — commit→reveal→deadline→default; reveal preimage recomputes the commit.
        ("PalwBeaconCommitV1.commitment", EnforcementStatus::Enforced, "beacon_commitment"),
        // reward_set_root / provider reward script — §3.4.1: real bytes live in registry state, hash is an id.
        ("reward_set_root / provider reward script", EnforcementStatus::Enforced, "palw_work_reward_class"),
        // DA-01 — the complete V2 object enters the selected-chain admission seam. The verifier binds
        // its root/length/chunks, bonds, owner-authorized session keys, two Receipt-v3 envelopes, epochs,
        // replica slots, and exact matched pair before the durable store becomes reachable.
        ("DA-01 receipt_da_root", EnforcementStatus::Enforced, "verify_palw_da_object_for_admission"),
        // PMC-01 — the private-match equality is only ever checked in an off-chain canary dispute.
        (
            "PMC-01 private_match_commitment",
            EnforcementStatus::GapOffchain,
            "equality is only checked in an off-chain canary dispute; consensus never sees the match preimage \
             — measurement / dispute-bound",
        ),
    ];

    /// ADR-0040 §2.6 / §7.2 G15 — every claimed enforcement point resolves, and the gap set is EXACTLY the
    /// one off-chain gap (PMC-01) after §5.17 CERT-REDERIVE closed SAMPLE-01 / AUTHSET-01 and DA Object
    /// V2 admission closed DA-01.
    #[test]
    fn palw_enforcement_points_total() {
        // Structural: no duplicate object labels.
        let mut objs: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (obj, ..) in PALW_ENFORCEMENT_POINTS {
            assert!(objs.insert(obj), "duplicate enforcement-point object: {obj}");
        }

        // Walk the workspace once and record which enforcing fns resolve.
        let root = workspace_root();
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        assert!(files.len() > 100, "workspace walk found suspiciously few .rs files ({}) under {}", files.len(), root.display());

        let mut targets: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (_, status, name) in PALW_ENFORCEMENT_POINTS {
            if matches!(status, EnforcementStatus::Enforced) {
                targets.insert(name);
            }
        }
        let mut declared: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for path in &files {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for name in targets.iter().copied() {
                if !declared.contains(name) && source_declares_fn(&content, name) {
                    declared.insert(name);
                }
            }
        }

        // Enforced ⇒ its named fn must resolve; every gap is collected.
        let mut problems: Vec<String> = Vec::new();
        let mut gaps: Vec<&str> = Vec::new();
        for (obj, status, detail) in PALW_ENFORCEMENT_POINTS {
            match status {
                EnforcementStatus::Enforced => {
                    if !declared.contains(detail) {
                        problems.push(format!(
                            "{obj}: Enforced names enforcing `{detail}` but no `fn {detail}` exists in the workspace \
                             — a PHANTOM enforcement point (the G3 accident, applied to §2.6)."
                        ));
                    }
                }
                EnforcementStatus::GapOffchain | EnforcementStatus::GapUnimplemented => gaps.push(obj),
            }
        }
        assert!(problems.is_empty(), "PALW enforcement-point reconciliation failed:\n{}", problems.join("\n"));

        // §2.6.1: after §5.17 CERT-REDERIVE and DA Object V2 admission the gap is EXACTLY 1 — PMC-01.
        assert_eq!(gaps.len(), 1, "expected exactly 1 enforcement-point gap (PMC-01), found {}: {gaps:?}", gaps.len());
        for (_, status, _) in PALW_ENFORCEMENT_POINTS {
            assert!(
                !matches!(status, EnforcementStatus::GapUnimplemented),
                "no enforcement-point gap may be GapUnimplemented — the current PMC-01 gap is off-chain"
            );
        }
        let pmc = gaps.iter().any(|o| o.contains("PMC-01"));
        assert!(pmc, "the sole enforcement-point gap must be PMC-01, found {gaps:?}");
    }
}

/// ADR-0040 §5.14.7.1 / §5.14.7.9 — the INERT verifiable-evidence redesign, exercised end-to-end against
/// REAL ML-DSA-87 keypairs, REAL keyed-BLAKE2b Merkle trees, and the REAL weighted draw. These are the
/// G7 degeneracy-regression tests: every negative proves that NO caller-asserted value can force accept —
/// each branch is recomputed by the verifier (the failure mode `PalwDispatchProof` had; §5.14.2).
#[cfg(test)]
mod pcpb_evidence_tests {
    use super::{
        BeaconAssignedProof, PALW_ASSIGNMENT_DRAW_DOMAIN, PALW_ASSIGNMENT_NODE_DOMAIN, PALW_DISPATCH_KIND_BEACON_ASSIGNED,
        PALW_DISPATCH_KIND_SELF_SERIAL, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, PALW_SNAPSHOT_NODE_DOMAIN, PalwAssignmentInterval,
        PalwDispatchEvidence, PalwDispatchLeafFacts, PalwMerkleMembership, PalwMlDsaVerifier, PalwProviderSnapshotEntry,
        PalwSnapshotCommitment, SelfSerialProof, SlotAssignmentWitness, palw_assignment_draw_seed, palw_assignment_interval_hash,
        palw_audit_epoch_seed_select, palw_build_snapshot_witnesses, palw_dispatch_evidence_valid, palw_epoch_seed_at,
        palw_pcpb_derive_b, palw_pcpb_receipt_preimage, palw_provider_pk_hash, palw_snapshot_entry_hash,
    };
    use crate::constants::PALW_HEADER_VERSION;
    use kaspa_hashes::{Hash64, blake2b_512_keyed};
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;

    /// The real portable ML-DSA-87 verify, injected exactly as production will inject it.
    struct LibcruxVerifier;
    impl PalwMlDsaVerifier for LibcruxVerifier {
        fn verify(&self, vk: &[u8], msg: &[u8], ctx: &[u8], sig: &[u8]) -> bool {
            const PK: usize = 2592; // ML-DSA-87 (ADR-0019)
            const SG: usize = 4627;
            let vk_arr: [u8; PK] = match vk.try_into() {
                Ok(a) => a,
                Err(_) => return false,
            };
            let sig_arr: [u8; SG] = match sig.try_into() {
                Ok(a) => a,
                Err(_) => return false,
            };
            let vk_obj = mldsa::MLDSA87VerificationKey::new(vk_arr);
            let sig_obj = mldsa::MLDSA87Signature::new(sig_arr);
            mldsa::portable::verify(&vk_obj, msg, ctx, &sig_obj).is_ok()
        }
    }

    fn th(n: u8) -> Hash64 {
        blake2b_512_keyed(b"pcpb-test-hash", &[n])
    }

    // Test-side replicas of the module's PRIVATE draw derivation, kept byte-identical so the tests select
    // the same provider the verifier will.
    fn tdraw_seed(beacon: &Hash64, slot: u8) -> Hash64 {
        let mut p = Vec::new();
        p.extend_from_slice(beacon.as_byte_slice());
        p.push(slot);
        blake2b_512_keyed(PALW_ASSIGNMENT_DRAW_DOMAIN, &p)
    }
    fn treduce(h: &Hash64, total: u128) -> u128 {
        let lo: [u8; 16] = h.as_byte_slice()[..16].try_into().unwrap();
        u128::from_le_bytes(lo) % total.max(1)
    }
    fn node_hash(domain: &[u8], l: &Hash64, r: &Hash64) -> Hash64 {
        let mut p = Vec::new();
        p.extend_from_slice(l.as_byte_slice());
        p.extend_from_slice(r.as_byte_slice());
        blake2b_512_keyed(domain, &p)
    }
    fn build_levels(leaves: &[Hash64], domain: &[u8]) -> Vec<Vec<Hash64>> {
        let mut levels = vec![leaves.to_vec()];
        while levels.last().unwrap().len() > 1 {
            let cur = levels.last().unwrap().clone();
            let mut next = Vec::new();
            let mut i = 0;
            while i < cur.len() {
                let l = cur[i];
                let r = if i + 1 < cur.len() { cur[i + 1] } else { cur[i] };
                next.push(node_hash(domain, &l, &r));
                i += 2;
            }
            levels.push(next);
        }
        levels
    }
    fn membership_of(levels: &[Vec<Hash64>], index: usize) -> PalwMerkleMembership {
        let leaf = levels[0][index];
        let mut siblings = Vec::new();
        let mut idx = index;
        for level in &levels[..levels.len() - 1] {
            let sib =
                if idx.is_multiple_of(2) { if idx + 1 < level.len() { level[idx + 1] } else { level[idx] } } else { level[idx - 1] };
            siblings.push(sib);
            idx /= 2;
        }
        PalwMerkleMembership { index: index as u32, leaf, siblings }
    }

    struct Setup {
        kps: Vec<mldsa::MLDSA87KeyPair>,
        entries: Vec<PalwProviderSnapshotEntry>,
        snap_levels: Vec<Vec<Hash64>>,
        snap_root: Hash64,
        intervals: Vec<PalwAssignmentInterval>,
        assign_levels: Vec<Vec<Hash64>>,
        assign_root: Hash64,
        total: u128,
    }

    fn setup(bonds: &[u64]) -> Setup {
        let mut kps = Vec::new();
        let mut entries = Vec::new();
        let mut intervals = Vec::new();
        let mut cum: u128 = 0;
        for (i, &bond) in bonds.iter().enumerate() {
            let kp = mldsa::generate_key_pair([i as u8 + 1; 32]);
            let pk = kp.verification_key.as_ref().to_vec();
            let provider_id = th(0x10 + i as u8);
            entries.push(PalwProviderSnapshotEntry {
                provider_id,
                ml_dsa_pk_hash: palw_provider_pk_hash(&pk),
                bond_sompi: bond,
                reward_script_commitment: th(0x50 + i as u8),
            });
            intervals.push(PalwAssignmentInterval { provider_id, bond_sompi: bond, cumulative_lo: cum });
            cum += bond as u128;
            kps.push(kp);
        }
        let snap_leaves: Vec<Hash64> = entries.iter().map(palw_snapshot_entry_hash).collect();
        let snap_levels = build_levels(&snap_leaves, PALW_SNAPSHOT_NODE_DOMAIN);
        let snap_root = snap_levels.last().unwrap()[0];
        let assign_leaves: Vec<Hash64> = intervals.iter().map(palw_assignment_interval_hash).collect();
        let assign_levels = build_levels(&assign_leaves, PALW_ASSIGNMENT_NODE_DOMAIN);
        let assign_root = assign_levels.last().unwrap()[0];
        Setup { kps, entries, snap_levels, snap_root, intervals, assign_levels, assign_root, total: cum }
    }

    fn drawn_index(s: &Setup, seed: &Hash64) -> usize {
        let ticket = treduce(seed, s.total);
        s.intervals
            .iter()
            .position(|iv| iv.cumulative_lo <= ticket && ticket < iv.cumulative_lo + iv.bond_sompi as u128)
            .expect("some interval contains the ticket")
    }

    fn witness(s: &Setup, i: usize) -> SlotAssignmentWitness {
        SlotAssignmentWitness {
            entry: s.entries[i].clone(),
            snapshot_membership: membership_of(&s.snap_levels, i),
            interval: s.intervals[i].clone(),
            assignment_membership: membership_of(&s.assign_levels, i),
        }
    }

    /// The store-resolved snapshot commitment a wired verifier would read from the per-epoch history.
    fn resolved(s: &Setup) -> PalwSnapshotCommitment {
        PalwSnapshotCommitment {
            snapshot_root: s.snap_root,
            assignment_root: s.assign_root,
            total_bond: s.total,
            provider_count: s.entries.len() as u32,
        }
    }

    /// LeafV2-committed facts for an EXTERNAL leaf whose declared seats are providers `ia`/`ib`.
    fn external_facts(s: &Setup, ia: usize, ib: usize) -> PalwDispatchLeafFacts {
        PalwDispatchLeafFacts {
            snapshot_root: s.snap_root,
            assignment_root: s.assign_root,
            a_commit: Hash64::default(),
            dispatch_kind: PALW_DISPATCH_KIND_BEACON_ASSIGNED,
            provider_a_id: s.entries[ia].provider_id,
            provider_b_id: s.entries[ib].provider_id,
        }
    }

    /// LeafV2-committed facts for a SELF leaf: A declared at `ia`, drawn B at `ib`.
    fn self_facts(s: &Setup, a_commit: &Hash64, ia: usize, ib: usize) -> PalwDispatchLeafFacts {
        PalwDispatchLeafFacts {
            snapshot_root: s.snap_root,
            assignment_root: s.assign_root,
            a_commit: *a_commit,
            dispatch_kind: PALW_DISPATCH_KIND_SELF_SERIAL,
            provider_a_id: s.entries[ia].provider_id,
            provider_b_id: s.entries[ib].provider_id,
        }
    }

    #[test]
    fn pcpb_beacon_arm_validates_and_is_not_a_tautology() {
        let s = setup(&[250, 250, 250, 250]); // total 1000
        // Find a beacon whose two slots draw DISTINCT providers.
        let mut beacon = th(0xB0);
        let (mut ia, mut ib) = (0usize, 0usize);
        for k in 0..64u8 {
            beacon = th(0xB0u8.wrapping_add(k));
            ia = drawn_index(&s, &tdraw_seed(&beacon, 0));
            ib = drawn_index(&s, &tdraw_seed(&beacon, 1));
            if ia != ib {
                break;
            }
        }
        assert_ne!(ia, ib, "setup: no beacon drew two distinct providers");
        let v = LibcruxVerifier;

        let good = PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof { slot_a: witness(&s, ia), slot_b: witness(&s, ib) });
        assert!(palw_dispatch_evidence_valid(&good, &resolved(&s), &beacon, &external_facts(&s, ia, ib), &v));

        // Tautology-killer: swap slot_a to a provider the slot-0 draw did NOT select → reject.
        let wrong = (0..4).find(|&c| c != ia).unwrap();
        let bad = PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof { slot_a: witness(&s, wrong), slot_b: witness(&s, ib) });
        assert!(
            !palw_dispatch_evidence_valid(&bad, &resolved(&s), &beacon, &external_facts(&s, wrong, ib), &v),
            "external arm accepted a provider the draw did not select (tautology!)"
        );

        // Clause 0: a leaf snapshot root disagreeing with the resolved root → reject.
        let mut facts = external_facts(&s, ia, ib);
        facts.snapshot_root = th(0xDE);
        assert!(
            !palw_dispatch_evidence_valid(&good, &resolved(&s), &beacon, &facts, &v),
            "clause 0 did not bind the leaf snapshot root"
        );

        // D3-b seat binding: valid draw, but the LEAF declares a different provider in seat A → reject.
        // Without this, evidence proves "some pair was drawn" while the leaf names whoever it likes.
        assert!(
            !palw_dispatch_evidence_valid(&good, &resolved(&s), &beacon, &external_facts(&s, wrong, ib), &v),
            "external arm accepted a leaf whose declared seat A is not the drawn provider"
        );

        // D3-b branch binding: same evidence, but the leaf committed to the SELF branch → reject.
        let mut facts = external_facts(&s, ia, ib);
        facts.dispatch_kind = PALW_DISPATCH_KIND_SELF_SERIAL;
        assert!(
            !palw_dispatch_evidence_valid(&good, &resolved(&s), &beacon, &facts, &v),
            "external evidence satisfied a leaf committed to the self branch (kind not bound)"
        );

        // D3-b external sentinel: an external leaf carrying a non-zero a_commit is a category error.
        let mut facts = external_facts(&s, ia, ib);
        facts.a_commit = th(0xAC);
        assert!(
            !palw_dispatch_evidence_valid(&good, &resolved(&s), &beacon, &facts, &v),
            "external arm accepted a leaf smuggling a self-arm a_commit"
        );
    }

    #[test]
    fn pcpb_beacon_arm_rejects_same_provider_for_both_slots() {
        // One provider dominates so it can be drawn for BOTH slots — isolating the distinctness guard.
        let s = setup(&[970, 10, 10, 10]);
        let mut beacon = th(0xC0);
        let mut found = false;
        for k in 0..128u8 {
            beacon = th(0xC0u8.wrapping_add(k));
            if drawn_index(&s, &tdraw_seed(&beacon, 0)) == 0 && drawn_index(&s, &tdraw_seed(&beacon, 1)) == 0 {
                found = true;
                break;
            }
        }
        assert!(found, "setup: no beacon drew provider 0 for both slots");
        let both0 = PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof { slot_a: witness(&s, 0), slot_b: witness(&s, 0) });
        let v = LibcruxVerifier;
        assert!(
            !palw_dispatch_evidence_valid(&both0, &resolved(&s), &beacon, &external_facts(&s, 0, 0), &v),
            "external arm accepted the SAME provider for both slots (distinctness not enforced)"
        );
    }

    #[test]
    fn pcpb_self_arm_validates_with_real_mldsa_signature() {
        let s = setup(&[250, 250, 250, 250]);
        let a_commit = th(0xA1);
        let beacon = th(0x5E);
        let j = drawn_index(&s, &palw_pcpb_derive_b(&beacon, &a_commit));
        let a = (0..4).find(|&a| a != j).unwrap(); // A: any bonded provider that is not B
        let preimage = palw_pcpb_receipt_preimage(&a_commit, b"pcpb-self-tail");
        let sig =
            mldsa::sign(&s.kps[j].signing_key, preimage.as_slice(), PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x33u8; 32]).expect("sign");
        let proof = PalwDispatchEvidence::SelfSerial(SelfSerialProof {
            a_commit,
            a_entry: s.entries[a].clone(),
            a_snapshot_membership: membership_of(&s.snap_levels, a),
            b_entry: s.entries[j].clone(),
            b_snapshot_membership: membership_of(&s.snap_levels, j),
            b_interval: s.intervals[j].clone(),
            b_assignment_membership: membership_of(&s.assign_levels, j),
            b_ml_dsa_pk: s.kps[j].verification_key.as_ref().to_vec(),
            b_receipt_preimage: preimage,
            b_signature: sig.as_ref().to_vec(),
        });
        let v = LibcruxVerifier;
        assert!(palw_dispatch_evidence_valid(&proof, &resolved(&s), &beacon, &self_facts(&s, &a_commit, a, j), &v));
        // Clause 0 for the self arm too.
        let mut facts = self_facts(&s, &a_commit, a, j);
        facts.assignment_root = th(0xEE);
        assert!(
            !palw_dispatch_evidence_valid(&proof, &resolved(&s), &beacon, &facts, &v),
            "clause 0 did not bind the leaf assignment root on the self arm"
        );
        // D3-b branch binding: self evidence against a leaf committed to the external branch → reject.
        let mut facts = self_facts(&s, &a_commit, a, j);
        facts.dispatch_kind = PALW_DISPATCH_KIND_BEACON_ASSIGNED;
        facts.a_commit = Hash64::default();
        assert!(
            !palw_dispatch_evidence_valid(&proof, &resolved(&s), &beacon, &facts, &v),
            "self evidence satisfied a leaf committed to the external branch (kind not bound)"
        );
    }

    #[test]
    fn pcpb_self_arm_rejects_unbound_forged_or_swapped() {
        let s = setup(&[250, 250, 250, 250]);
        let a_commit = th(0xA1);
        let beacon = th(0x5E);
        let j = drawn_index(&s, &palw_pcpb_derive_b(&beacon, &a_commit));
        let a = (0..4).find(|&a| a != j).unwrap();
        let v = LibcruxVerifier;
        let good_preimage = palw_pcpb_receipt_preimage(&a_commit, b"tail");
        let good_sig = mldsa::sign(&s.kps[j].signing_key, good_preimage.as_slice(), PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x33u8; 32])
            .unwrap()
            .as_ref()
            .to_vec();
        let base = SelfSerialProof {
            a_commit,
            a_entry: s.entries[a].clone(),
            a_snapshot_membership: membership_of(&s.snap_levels, a),
            b_entry: s.entries[j].clone(),
            b_snapshot_membership: membership_of(&s.snap_levels, j),
            b_interval: s.intervals[j].clone(),
            b_assignment_membership: membership_of(&s.assign_levels, j),
            b_ml_dsa_pk: s.kps[j].verification_key.as_ref().to_vec(),
            b_receipt_preimage: good_preimage.clone(),
            b_signature: good_sig,
        };
        let facts = self_facts(&s, &a_commit, a, j);
        let check = |p: SelfSerialProof| -> bool {
            palw_dispatch_evidence_valid(&PalwDispatchEvidence::SelfSerial(p), &resolved(&s), &beacon, &facts, &v)
        };

        // (a) receipt binds a DIFFERENT commit (does not embed a_commit) → reject.
        let wrong_preimage = palw_pcpb_receipt_preimage(&th(0xFF), b"tail");
        let wrong_sig = mldsa::sign(&s.kps[j].signing_key, wrong_preimage.as_slice(), PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x44u8; 32])
            .unwrap()
            .as_ref()
            .to_vec();
        let mut p = base.clone();
        p.b_receipt_preimage = wrong_preimage;
        p.b_signature = wrong_sig;
        assert!(!check(p), "accepted a receipt that does not embed a_commit");

        // (b) forged signature (flip one byte) → reject.
        let mut p = base.clone();
        p.b_signature[0] ^= 0x01;
        assert!(!check(p), "accepted a forged signature");

        // (c) swap B to a provider the post-commit draw did NOT select → reject.
        let k = (0..4).find(|&k| k != j).unwrap();
        let ksig = mldsa::sign(&s.kps[k].signing_key, good_preimage.as_slice(), PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x55u8; 32])
            .unwrap()
            .as_ref()
            .to_vec();
        let mut p = base.clone();
        p.b_entry = s.entries[k].clone();
        p.b_snapshot_membership = membership_of(&s.snap_levels, k);
        p.b_interval = s.intervals[k].clone();
        p.b_assignment_membership = membership_of(&s.assign_levels, k);
        p.b_ml_dsa_pk = s.kps[k].verification_key.as_ref().to_vec();
        p.b_signature = ksig;
        // The swapped B also has to be declared on the leaf for the seat check to be the thing under
        // test (otherwise the seat equality rejects first and the draw check is not exercised).
        let swapped_facts = self_facts(&s, &a_commit, a, k);
        assert!(
            !palw_dispatch_evidence_valid(&PalwDispatchEvidence::SelfSerial(p), &resolved(&s), &beacon, &swapped_facts, &v),
            "accepted a B the post-commit beacon draw did not select"
        );

        // (d) verification key does not hash to the committed pk_hash → reject.
        let other = (0..4).find(|&o| o != j).unwrap();
        let mut p = base.clone();
        p.b_ml_dsa_pk = s.kps[other].verification_key.as_ref().to_vec();
        assert!(!check(p), "accepted a verification key not matching the committed pk_hash");

        // (e) D3-b: A outside the snapshot (fabricated entry, no valid membership) → reject. Self-order
        // is a bonded privilege; without this an unbonded A mints with only B's bond at stake.
        let mut p = base.clone();
        p.a_entry = PalwProviderSnapshotEntry {
            provider_id: th(0x77),
            ml_dsa_pk_hash: th(0x78),
            bond_sompi: 250,
            reward_script_commitment: th(0x79),
        };
        let mut unbonded_facts = facts;
        unbonded_facts.provider_a_id = th(0x77);
        assert!(
            !palw_dispatch_evidence_valid(&PalwDispatchEvidence::SelfSerial(p), &resolved(&s), &beacon, &unbonded_facts, &v),
            "accepted an A with no snapshot membership (unbonded self-order)"
        );

        // (f) D3-b: A == B (self-pairing) → reject even with an otherwise-valid draw.
        let mut p = base.clone();
        p.a_entry = s.entries[j].clone();
        p.a_snapshot_membership = membership_of(&s.snap_levels, j);
        let self_pair_facts = self_facts(&s, &a_commit, j, j);
        assert!(
            !palw_dispatch_evidence_valid(&PalwDispatchEvidence::SelfSerial(p), &resolved(&s), &beacon, &self_pair_facts, &v),
            "accepted A == B (self-pairing defeats k=2)"
        );

        // (g) D3-b: the leaf's declared seat A differs from the evidence's A → reject.
        let third = (0..4).find(|&t| t != j && t != a).unwrap();
        let seat_facts = self_facts(&s, &a_commit, third, j);
        assert!(
            !palw_dispatch_evidence_valid(&PalwDispatchEvidence::SelfSerial(base.clone()), &resolved(&s), &beacon, &seat_facts, &v),
            "accepted evidence whose A is not the leaf's declared seat A"
        );

        // (h) D3-b: zero a_commit on the self branch → reject (the anchor must exist).
        let mut p = base.clone();
        p.a_commit = Hash64::default();
        let mut zero_facts = facts;
        zero_facts.a_commit = Hash64::default();
        assert!(
            !palw_dispatch_evidence_valid(&PalwDispatchEvidence::SelfSerial(p), &resolved(&s), &beacon, &zero_facts, &v),
            "accepted a self leaf with a zero a_commit"
        );

        // Sanity: the untampered proof validates.
        assert!(check(base), "base self-serial proof should validate");
    }

    // ---- ADR-0040 §5.14.7.4 (item 5) — canonical snapshot/assignment builder ----

    #[test]
    fn pcpb_snapshot_builder_is_order_independent_and_partitions() {
        let mk = |pid: u8, bond: u64| PalwProviderSnapshotEntry {
            provider_id: th(pid),
            ml_dsa_pk_hash: th(0x80 + pid),
            bond_sompi: bond,
            reward_script_commitment: th(0x90 + pid),
        };
        // Deliberately unsorted, and includes a zero-bond entry that must be excluded.
        let a = vec![mk(3, 300), mk(1, 100), mk(9, 0), mk(2, 200), mk(4, 400)];
        let mut b = a.clone();
        b.reverse();
        let ws = palw_build_snapshot_witnesses(&a);
        let ws2 = palw_build_snapshot_witnesses(&b);

        assert_eq!(ws.commitment.provider_count, 4, "zero-bond provider must be excluded");
        assert_eq!(ws.commitment.total_bond, 1000);
        assert_eq!(ws.commitment, ws2.commitment, "commitment must be order-independent");
        assert_eq!(ws.slots, ws2.slots, "per-provider witnesses must be order-independent");

        // Intervals partition [0, total) with no gap or overlap, in sorted order.
        let mut expect_lo: u128 = 0;
        for s in &ws.slots {
            assert_eq!(s.interval.cumulative_lo, expect_lo, "interval gap/overlap");
            expect_lo += s.interval.bond_sompi as u128;
        }
        assert_eq!(expect_lo, ws.commitment.total_bond, "intervals must cover exactly [0, total)");
        for w in ws.slots.windows(2) {
            assert!(w[0].entry.provider_id.as_byte_slice() < w[1].entry.provider_id.as_byte_slice(), "providers must be sorted by id");
        }
    }

    #[test]
    fn pcpb_snapshot_builder_output_validates_beacon_arm() {
        let s = setup(&[250, 250, 250, 250]);
        let ws = palw_build_snapshot_witnesses(&s.entries);
        let v = LibcruxVerifier;
        // Use the REAL draw seed + the set's own select() to find a beacon drawing two distinct providers.
        let mut beacon = th(0x40);
        let (mut ia, mut ib) = (None, None);
        for k in 0..64u8 {
            beacon = th(0x40u8.wrapping_add(k));
            ia = ws.select(&palw_assignment_draw_seed(&beacon, 0));
            ib = ws.select(&palw_assignment_draw_seed(&beacon, 1));
            if let (Some(a), Some(b)) = (ia, ib)
                && a != b
            {
                break;
            }
        }
        let (ia, ib) = (ia.unwrap(), ib.unwrap());
        assert_ne!(ia, ib, "setup: no beacon drew two distinct providers");
        let proof =
            PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof { slot_a: ws.slots[ia].clone(), slot_b: ws.slots[ib].clone() });
        let facts = PalwDispatchLeafFacts {
            snapshot_root: ws.commitment.snapshot_root, // a LeafV2 commits the same (resolved) roots
            assignment_root: ws.commitment.assignment_root,
            a_commit: Hash64::default(),
            dispatch_kind: PALW_DISPATCH_KIND_BEACON_ASSIGNED,
            provider_a_id: ws.slots[ia].entry.provider_id,
            provider_b_id: ws.slots[ib].entry.provider_id,
        };
        assert!(
            palw_dispatch_evidence_valid(&proof, &ws.commitment, &beacon, &facts, &v),
            "builder output must validate under the verifier (external arm)"
        );
    }

    #[test]
    fn pcpb_snapshot_builder_output_validates_self_arm() {
        let s = setup(&[250, 250, 250, 250]);
        let ws = palw_build_snapshot_witnesses(&s.entries);
        let v = LibcruxVerifier;
        let a_commit = th(0xA7);
        let beacon = th(0x6C);
        let j = ws.select(&palw_pcpb_derive_b(&beacon, &a_commit)).unwrap();
        // The builder sorts, so map the drawn provider_id back to its keypair.
        let pid = ws.slots[j].entry.provider_id;
        let ki = (0..s.entries.len()).find(|&i| s.entries[i].provider_id == pid).unwrap();
        let aj = (0..ws.slots.len()).find(|&i| i != j).unwrap(); // A: any other builder slot
        let preimage = palw_pcpb_receipt_preimage(&a_commit, b"builder-self-tail");
        let sig = mldsa::sign(&s.kps[ki].signing_key, preimage.as_slice(), PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x66u8; 32]).unwrap();
        let w = &ws.slots[j];
        let proof = PalwDispatchEvidence::SelfSerial(SelfSerialProof {
            a_commit,
            a_entry: ws.slots[aj].entry.clone(),
            a_snapshot_membership: ws.slots[aj].snapshot_membership.clone(),
            b_entry: w.entry.clone(),
            b_snapshot_membership: w.snapshot_membership.clone(),
            b_interval: w.interval.clone(),
            b_assignment_membership: w.assignment_membership.clone(),
            b_ml_dsa_pk: s.kps[ki].verification_key.as_ref().to_vec(),
            b_receipt_preimage: preimage,
            b_signature: sig.as_ref().to_vec(),
        });
        let facts = PalwDispatchLeafFacts {
            snapshot_root: ws.commitment.snapshot_root,
            assignment_root: ws.commitment.assignment_root,
            a_commit,
            dispatch_kind: PALW_DISPATCH_KIND_SELF_SERIAL,
            provider_a_id: ws.slots[aj].entry.provider_id,
            provider_b_id: pid,
        };
        assert!(
            palw_dispatch_evidence_valid(&proof, &ws.commitment, &beacon, &facts, &v),
            "builder output must validate under the verifier (self arm)"
        );
    }

    // ---- ADR-0040 §5.14.7.3 (item 4) — arbitrary-target beacon seed resolver ----

    #[test]
    fn pcpb_epoch_seed_resolver_selects_and_fails_closed() {
        let v = PALW_HEADER_VERSION;
        let (act, len) = (1000u64, 100u64);
        let s_t = th(0x15); // seed carried by every header of target epoch 15
        let hdrs = || vec![(v, 1650u64, th(0x16)), (v, 1550u64, s_t), (v, 1450u64, th(0x14)), (v, 1350u64, th(0x13))].into_iter();

        // Hit: newest header of the target epoch is returned.
        assert_eq!(palw_epoch_seed_at(15, act, len, hdrs()), Some(s_t));
        // Delegation equivalence: audit(E) is exactly general(E-1).
        assert_eq!(palw_audit_epoch_seed_select(16, act, len, hdrs()), palw_epoch_seed_at(15, act, len, hdrs()));

        // Fail-closed: descent past the target without a hit.
        assert_eq!(palw_epoch_seed_at(12, act, len, vec![(v, 1650u64, th(0x16)), (v, 1150u64, th(0x11))].into_iter()), None);
        // Fail-closed: exhaustion before the target.
        assert_eq!(palw_epoch_seed_at(10, act, len, vec![(v, 1650u64, th(0x16))].into_iter()), None);
        // Fail-closed: pre-activation header (daa < activation).
        assert_eq!(palw_epoch_seed_at(9, act, len, vec![(v, 900u64, th(0x09))].into_iter()), None);
        // Fail-closed: zero seed at the activation boundary.
        assert_eq!(palw_epoch_seed_at(15, act, len, vec![(v, 1550u64, Hash64::default())].into_iter()), None);
        // Fail-closed: pre-activation PALW header version.
        assert_eq!(palw_epoch_seed_at(15, act, len, vec![(v - 1, 1550u64, th(0x15))].into_iter()), None);
    }
}
