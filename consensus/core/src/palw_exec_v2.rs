//! RFC-0008 v2 — the unified PALW EXEC lane's **wire layer**: one versioned envelope, two exclusive subtypes.
//!
//! The execution lane (`pow_layer0::POW_ALGO_ID_PALW_ROUND_V1`, algo 10, [`crate::palw_execution_lane_v1`]) carries
//! fee-only transaction blocks beside the PALW chain. RFC-0008 (revised 2026-10-08) widens the *same* lane — no new
//! algorithm id, no new chain class — to carry a second payload, an authenticated **work slice** of a long REAL claim. The
//! two payloads are different authorities with different ledgers, so they are different **subtypes of one envelope** and
//! never one object serving both:
//!
//! | subtype | `tx_permit` | `work_slice` | payload commitment | authority |
//! |---|---|---|---|---|
//! | [`PalwExecSubtypeV2::Tx`] | `Some` | `None` | the header's `hash_merkle_root` (the existing transaction batch) | the existing finalized-credit schedule and permit |
//! | [`PalwExecSubtypeV2::Slice`] | `None` | `Some` | [`palw_work_slice_payload_root_v2`] (work and evidence commitments, no user transactions) | the root claim's plan, an authorised executor bond, the next canonical range |
//!
//! Both present, both absent, a field that does not match the subtype, an unknown subtype and an unknown version each
//! reject the **whole carrier** ([`PalwExecV2Error`]) — there is no partial acceptance. A future dual-payload subtype
//! needs its own version and both authorisations; it cannot turn a slice into a permit or the reverse.
//!
//! # Wire form
//!
//! `"PXE2" ‖ borsh(`[`PalwExecV2Envelope`]`)` in a header's `palw_commitment`. The magic is not the round lane v1's
//! `"PXR1"`: the two are never confused, and a v1 envelope keeps its validation, its hashing and its bytes. Decoding is
//! strict (bounded length, the exact magic, nothing after the body, and the body must re-encode to the bytes read).
//!
//! # What a signature binds
//!
//! One keyed-BLAKE2b-512 message per subtype, in that subtype's **own domain** and under that subtype's **own ML-DSA-87
//! context**, over: the wire version, the subtype, the network, the header position (`pre_pow_hash`, `timestamp`, `nonce`),
//! the carrier anchor, the payload commitment, the executor bond and the full type-specific content (the permit's round
//! and index; the slice's identity, which hashes every slice field). So a signature is bound to its network, its version,
//! its subtype, its anchor and its header, and a permit signature never verifies as a slice signature or the reverse.
//!
//! # What this module does not do
//!
//! It is stateless. Whether a root exists, whether the bond is authorised and funded, whether the range is the next one and
//! whether the permit is granted are the fold's and the processor's questions ([`crate::palw_work_slice_v2`]). It carries no
//! declared work-unit figure: the credited work of a slice is derived from its canonical range against the root's plan.
//!
//! Dormant: `Params::palw_exec_payload_v2` is `None` on every preset and its validator refuses any real height until the
//! gates of `docs/design/palw/rfc-0008-implementation-spec.md` §9 pass.

use crate::Hash64;
use crate::palw_execution_lane_v1::{
    PALW_EXEC_MAX_PERMITS_PER_ROUND_V1, PALW_EXEC_MLDSA87_PUBKEY_LEN, PALW_EXEC_MLDSA87_SIGNATURE_LEN, palw_execution_round_v1,
};
use crate::palw_state_v2::PalwBondKeyV2;

/// The envelope's wire version. v1 is `PalwExecEnvelopeV1`; a version byte other than this one rejects the carrier.
pub const PALW_EXEC_V2_WIRE_VERSION: u8 = 2;

/// The header-carriage magic of a v2 envelope — distinct from `PXR1` and every other PALW carriage.
pub const PALW_EXEC_V2_CARRIAGE_MAGIC: [u8; 4] = *b"PXE2";

/// The most bytes a v2 envelope may occupy in a header, magic included. A worst-case slice envelope is under 8 KiB (an
/// ML-DSA-87 key and signature are 7,219 bytes of it); this bounds the decoder's allocation before it parses anything.
pub const PALW_EXEC_V2_MAX_ENVELOPE_BYTES: usize = 16 * 1024;

/// The most slices one root may be partitioned into. It bounds the root row's boundary list and the per-root ledger.
pub const PALW_EXEC_V2_MAX_SLICES_PER_ROOT: u32 = 128;

/// The `EXEC_TX` signing domain (a keyed-BLAKE2b key).
pub const PALW_EXEC_V2_TX_SIGNING_DOMAIN: &[u8] = b"misaka-palw/exec-v2/tx/signing/v1";
/// The `EXEC_SLICE` signing domain.
pub const PALW_EXEC_V2_SLICE_SIGNING_DOMAIN: &[u8] = b"misaka-palw/exec-v2/slice/signing/v1";
/// The `EXEC_TX` ML-DSA-87 context.
pub const PALW_EXEC_V2_TX_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/exec-v2/tx/mldsa87/v1";
/// The `EXEC_SLICE` ML-DSA-87 context.
pub const PALW_EXEC_V2_SLICE_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/exec-v2/slice/mldsa87/v1";
/// The domain of a permit's content digest.
pub const PALW_EXEC_V2_TX_CONTENT_DOMAIN: &[u8] = b"misaka-palw/exec-v2/tx/content/v1";
/// The domain of a slice's identity.
pub const PALW_EXEC_V2_SLICE_ID_DOMAIN: &[u8] = b"misaka-palw/exec-v2/slice/id/v1";
/// The domain of a slice carrier's payload commitment.
pub const PALW_EXEC_V2_SLICE_PAYLOAD_DOMAIN: &[u8] = b"misaka-palw/exec-v2/slice/payload/v1";
/// The domain of a slice's challenge binding (the `commitment_root` of its `WORK_SLICE` challenge subject).
pub const PALW_EXEC_V2_SLICE_BINDING_DOMAIN: &[u8] = b"misaka-palw/exec-v2/slice/binding/v1";

/// Every keyed-BLAKE2b domain and signing context this module hashes or signs under, for the distinctness test.
pub const PALW_EXEC_V2_ALL_DOMAINS: &[&[u8]] = &[
    PALW_EXEC_V2_TX_SIGNING_DOMAIN,
    PALW_EXEC_V2_SLICE_SIGNING_DOMAIN,
    PALW_EXEC_V2_TX_MLDSA87_CONTEXT,
    PALW_EXEC_V2_SLICE_MLDSA87_CONTEXT,
    PALW_EXEC_V2_TX_CONTENT_DOMAIN,
    PALW_EXEC_V2_SLICE_ID_DOMAIN,
    PALW_EXEC_V2_SLICE_PAYLOAD_DOMAIN,
    PALW_EXEC_V2_SLICE_BINDING_DOMAIN,
];

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn update_bond(state: &mut blake2b_simd::State, bond: &PalwBondKeyV2) {
    state.update(bond.0.transaction_id.as_byte_slice());
    state.update(&bond.0.index.to_le_bytes());
}

/// Is a hash the all-zero default? A default hash agrees with everything, so every identity root a slice carries must be
/// *set*: an all-zero root is refused rather than read as "absent".
fn unset(hash: &Hash64) -> bool {
    hash.as_byte_slice().iter().all(|b| *b == 0)
}

/// **The two payloads of the one EXEC class.** Explicit discriminants: the byte is a wire fact, not an enum order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwExecSubtypeV2 {
    /// A fee-only transaction batch under a finalized-credit permit (ADR-0125's semantics, unchanged).
    Tx = 1,
    /// An authenticated work slice of an open REAL root claim (RFC-0008).
    Slice = 2,
}

/// **The existing round permit's coordinates**: the round (whole seconds from genesis) and the permit index the schedule
/// granted to the envelope's bond. The bond is the envelope's `executor_bond`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecTxPermitV2 {
    pub round: u64,
    pub permit_index: u16,
}

/// **A half-open range `[start, end)` of a root's canonical work**, in the class's canonical work units. A slice's credited
/// work is `end - start`: derived from the range against the root's plan, never declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWorkRangeV1 {
    pub start: u64,
    pub end: u64,
}

impl PalwWorkRangeV1 {
    /// The range's work, or `None` for an empty or inverted range.
    pub fn work(&self) -> Option<u64> {
        self.end.checked_sub(self.start).filter(|work| *work > 0)
    }

    /// Do two ranges share a unit of work?
    pub fn overlaps(&self, other: &PalwWorkRangeV1) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// **One authenticated slice of a root claim's work** (`ClaimSliceV1` of the spec; named apart from the kaspad-private type of
/// that name). Every field is a commitment the chain checks against the **root**, never trusted on its own:
///
/// * `root_claim_id`, `slice_index`, `canonical_range` — which part of which claim this is;
/// * `class_id`, `canonical_job_id`, `kernel_version`, `plan_root` — the class, the job, the active kernel and the
///   `VerificationPlan` root, each of which must equal the root row's (a correct slice borrowed from another job is refused
///   by this binding, not by trusting repeated fields);
/// * `predecessor_state_root`, `result_state_root` — the boundary states the slice transitions between;
/// * `input_root`, `output_root` — the committed material of the range;
/// * `evidence_root` — the evidence and constraint commitment a public verifier localises against;
/// * `da_root` — the data-availability commitment of that material;
/// * `executor_bond` — the authorised bond whose key signs the carrier.
///
/// The signature is the **envelope's** ([`PalwExecV2Envelope::signature`]), which hashes [`Self::slice_id`] — that is the
/// slice's identity — under the slice domain; a second per-slice signature would double the header's signature bytes (4,627)
/// and add no binding (the carrier is public, on-chain evidence).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWorkSliceV1 {
    pub root_claim_id: Hash64,
    pub slice_index: u32,
    pub class_id: Hash64,
    pub canonical_job_id: Hash64,
    pub kernel_version: u32,
    pub plan_root: Hash64,
    pub canonical_range: PalwWorkRangeV1,
    pub predecessor_state_root: Hash64,
    pub result_state_root: Hash64,
    pub input_root: Hash64,
    pub output_root: Hash64,
    pub evidence_root: Hash64,
    pub da_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
}

impl PalwWorkSliceV1 {
    /// **The slice's identity**: a keyed hash of every field, in declaration order, fixed-width little-endian. Two slices that
    /// differ in any field have different identities, so no field can be swapped under a signature.
    pub fn slice_id(&self) -> Hash64 {
        let mut state = keyed(PALW_EXEC_V2_SLICE_ID_DOMAIN);
        state.update(self.root_claim_id.as_byte_slice());
        state.update(&self.slice_index.to_le_bytes());
        state.update(self.class_id.as_byte_slice());
        state.update(self.canonical_job_id.as_byte_slice());
        state.update(&self.kernel_version.to_le_bytes());
        state.update(self.plan_root.as_byte_slice());
        state.update(&self.canonical_range.start.to_le_bytes());
        state.update(&self.canonical_range.end.to_le_bytes());
        state.update(self.predecessor_state_root.as_byte_slice());
        state.update(self.result_state_root.as_byte_slice());
        state.update(self.input_root.as_byte_slice());
        state.update(self.output_root.as_byte_slice());
        state.update(self.evidence_root.as_byte_slice());
        state.update(self.da_root.as_byte_slice());
        update_bond(&mut state, &self.executor_bond);
        finish(state)
    }

    /// **What a stateless reader can say of a slice**: a non-empty range, an index inside the root bound, and every identity
    /// root set. It is shape, not truth — the root decides whether the values are the root's.
    pub fn validate_shape(&self) -> Result<(), PalwExecV2Error> {
        if self.slice_index >= PALW_EXEC_V2_MAX_SLICES_PER_ROOT {
            return Err(PalwExecV2Error::SliceIndexOutOfRange { index: self.slice_index, max: PALW_EXEC_V2_MAX_SLICES_PER_ROOT });
        }
        if self.canonical_range.work().is_none() {
            return Err(PalwExecV2Error::SliceRangeEmpty);
        }
        let roots = [
            ("root_claim_id", &self.root_claim_id),
            ("class_id", &self.class_id),
            ("canonical_job_id", &self.canonical_job_id),
            ("plan_root", &self.plan_root),
            ("predecessor_state_root", &self.predecessor_state_root),
            ("result_state_root", &self.result_state_root),
            ("input_root", &self.input_root),
            ("output_root", &self.output_root),
            ("evidence_root", &self.evidence_root),
            ("da_root", &self.da_root),
        ];
        for (name, root) in roots {
            if unset(root) {
                return Err(PalwExecV2Error::SliceRootUnset(name));
            }
        }
        Ok(())
    }

    /// **The commitment root of this slice's `WORK_SLICE` challenge subject** (RFC-0007 Part VI.2): the pre-beacon binding
    /// of every field the subject's own typed roots do not carry — the root claim, the index, the range, the predecessor, the
    /// output material, the DA commitment and the executor. The subject's typed roots are filled from the slice by
    /// [`PalwWorkSliceSubjectV1`], so the contract's `ChallengeSubjectV1` for `SubjectKindV1::WorkSlice` is a pure function
    /// of the slice and the policy ids, committed before any source window opens.
    pub fn challenge_binding(&self) -> Hash64 {
        let mut state = keyed(PALW_EXEC_V2_SLICE_BINDING_DOMAIN);
        state.update(self.root_claim_id.as_byte_slice());
        state.update(&self.slice_index.to_le_bytes());
        state.update(&self.canonical_range.start.to_le_bytes());
        state.update(&self.canonical_range.end.to_le_bytes());
        state.update(self.predecessor_state_root.as_byte_slice());
        state.update(self.output_root.as_byte_slice());
        state.update(self.da_root.as_byte_slice());
        state.update(self.canonical_job_id.as_byte_slice());
        update_bond(&mut state, &self.executor_bond);
        finish(state)
    }

    /// **The fields of the shared challenge contract's `WORK_SLICE` subject this slice determines** — see
    /// [`PalwWorkSliceSubjectV1`]. `subject_id` is the slice identity.
    pub fn challenge_subject(&self) -> PalwWorkSliceSubjectV1 {
        PalwWorkSliceSubjectV1 {
            subject_id: self.slice_id(),
            kernel_version: self.kernel_version,
            verification_plan_root: self.plan_root,
            input_root: self.input_root,
            state_root: self.result_state_root,
            constraint_root: self.evidence_root,
            commitment_root: self.challenge_binding(),
        }
    }
}

/// **The slice-determined half of `misaka_palw_challenge::ChallengeSubjectV1` for `SubjectKindV1::WorkSlice`.** The other half
/// (`chain_genesis`, `ruleset_id`, `challenge_policy_id`, the program/artifact/schema/layout roots) comes from the network and
/// the root's class/plan, so it is the root's to add; this struct names the part a slice fixes, with the same field names, so
/// the mapping is checked against the contract crate in `misaka-palw-sdk/tests/exec_v2_work_slice_subject.rs` rather than restated here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwWorkSliceSubjectV1 {
    /// The slice identity.
    pub subject_id: Hash64,
    pub kernel_version: u32,
    pub verification_plan_root: Hash64,
    pub input_root: Hash64,
    /// The slice's *result* boundary; the predecessor is bound in [`PalwWorkSliceV1::challenge_binding`].
    pub state_root: Hash64,
    pub constraint_root: Hash64,
    pub commitment_root: Hash64,
}

/// **The payload commitment of a slice carrier**: the slice identity and the two material commitments a verifier fetches
/// (evidence and DA), under the slice payload domain. A slice carrier carries no user transaction batch, so there is no
/// transaction root to commit; this is the digest the envelope's `payload_root` must equal.
pub fn palw_work_slice_payload_root_v2(slice: &PalwWorkSliceV1) -> Hash64 {
    let mut state = keyed(PALW_EXEC_V2_SLICE_PAYLOAD_DOMAIN);
    state.update(slice.slice_id().as_byte_slice());
    state.update(slice.evidence_root.as_byte_slice());
    state.update(slice.da_root.as_byte_slice());
    finish(state)
}

fn tx_content_digest(permit: &PalwExecTxPermitV2) -> Hash64 {
    let mut state = keyed(PALW_EXEC_V2_TX_CONTENT_DOMAIN);
    state.update(&permit.round.to_le_bytes());
    state.update(&permit.permit_index.to_le_bytes());
    finish(state)
}

/// **What a header stage and a mergeset rule read of any lane block, whichever envelope it carries**: its permit coordinates if it
/// holds one (a v1 round block, or a v2 `EXEC_TX`), or the fact that it is a slice (which holds none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwExecLaneCoordsV1 {
    /// A permit holder: `(round, permit index, bond)` — a v1 round block or a v2 `EXEC_TX`.
    Permit { round: u64, permit_index: u16, bond: PalwBondKeyV2 },
    /// A v2 `EXEC_SLICE`: no round, no permit.
    Slice { bond: PalwBondKeyV2 },
}

/// Decode a lane block's `palw_commitment` into [`PalwExecLaneCoordsV1`], by its magic. `Err` names why neither envelope decodes.
pub fn palw_exec_lane_coords_v1(commitment: &[u8]) -> Result<PalwExecLaneCoordsV1, String> {
    if PalwExecV2Envelope::is_v2_carriage(commitment) {
        let envelope = PalwExecV2Envelope::decode(commitment).map_err(|e| e.to_string())?;
        return Ok(match (&envelope.tx_permit, &envelope.work_slice) {
            (Some(permit), None) => {
                PalwExecLaneCoordsV1::Permit { round: permit.round, permit_index: permit.permit_index, bond: envelope.executor_bond }
            }
            (None, Some(_)) => PalwExecLaneCoordsV1::Slice { bond: envelope.executor_bond },
            _ => return Err("a v2 envelope carries exactly one payload".into()),
        });
    }
    let envelope = crate::palw_execution_lane_v1::PalwExecEnvelopeV1::decode(commitment).map_err(|e| e.to_string())?;
    Ok(PalwExecLaneCoordsV1::Permit { round: envelope.round, permit_index: envelope.permit_index, bond: envelope.bond })
}

/// Why a v2 envelope was refused. Every variant rejects the **whole carrier**.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwExecV2Error {
    #[error("exec v2 envelope undecodable: {0}")]
    Undecodable(&'static str),
    #[error("exec v2 envelope is {got} bytes, above the {max}-byte bound")]
    TooLarge { got: usize, max: usize },
    #[error("exec envelope version {got}, expected {expected}")]
    UnsupportedVersion { got: u8, expected: u8 },
    #[error("exec v2 envelope names another network")]
    NetworkDomainMismatch,
    #[error("exec v2 {subtype:?} carrier carries a field of the other subtype or lacks its own")]
    SubtypeFieldMismatch { subtype: PalwExecSubtypeV2 },
    #[error("exec v2 carrier carries both a permit and a work slice")]
    BothPayloads,
    #[error("exec v2 carrier carries neither a permit nor a work slice")]
    NoPayload,
    #[error("exec v2 permit index {index} is not below the widest round ({max})")]
    PermitIndexOutOfRange { index: u16, max: u16 },
    #[error("exec v2 permit names round {declared} but the header's timestamp is in round {actual}")]
    RoundMismatch { declared: u64, actual: u64 },
    #[error("exec v2 slice index {index} is not below the per-root bound ({max})")]
    SliceIndexOutOfRange { index: u32, max: u32 },
    #[error("exec v2 slice has an empty or inverted canonical range")]
    SliceRangeEmpty,
    #[error("exec v2 slice's {0} is unset (all zero)")]
    SliceRootUnset(&'static str),
    #[error("exec v2 envelope public key is {got} bytes, expected {expected}")]
    PublicKeyLength { got: usize, expected: usize },
    #[error("exec v2 envelope signature is {got} bytes, expected {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("exec v2 payload commitment is not the one its subtype derives")]
    PayloadRootMismatch,
    #[error("exec v2 envelope signature does not verify")]
    SignatureInvalid,
}

/// **The envelope a v2 EXEC block carries in `palw_commitment`** (see the module doc). `anchor` is the chain block the
/// carrier hangs from: the processor checks it equals the block's selected parent and lies on the accepting chain inside the
/// attachment window; the signature binds it, so a carrier cannot be re-hung from another anchor.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecV2Envelope {
    pub version: u8,
    pub network_domain: Hash64,
    pub anchor: Hash64,
    pub subtype: PalwExecSubtypeV2,
    pub tx_permit: Option<PalwExecTxPermitV2>,
    pub work_slice: Option<PalwWorkSliceV1>,
    pub payload_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    /// The bond's registered ML-DSA-87 key; acceptance requires equality with the registry.
    pub pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

impl PalwExecV2Envelope {
    /// The header-carriage wire form: magic, then borsh.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_EXEC_V2_CARRIAGE_MAGIC.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh serialization of a plain struct cannot fail"));
        out
    }

    /// **Strict decode**: bounded length, the exact magic, a body that decodes, nothing after it, and a body that re-encodes
    /// to the bytes read (one spelling per envelope). An unknown subtype byte, a bad `Option` tag or a short body is
    /// `Undecodable`; a decodable envelope of another version is refused by [`Self::validate_shape`].
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwExecV2Error> {
        if bytes.len() > PALW_EXEC_V2_MAX_ENVELOPE_BYTES {
            return Err(PalwExecV2Error::TooLarge { got: bytes.len(), max: PALW_EXEC_V2_MAX_ENVELOPE_BYTES });
        }
        let Some(body) = bytes.strip_prefix(&PALW_EXEC_V2_CARRIAGE_MAGIC) else {
            return Err(PalwExecV2Error::Undecodable("payload does not start with the PXE2 magic"));
        };
        let mut slice = body;
        let decoded =
            <Self as borsh::BorshDeserialize>::deserialize(&mut slice).map_err(|_| PalwExecV2Error::Undecodable("borsh body"))?;
        if !slice.is_empty() {
            return Err(PalwExecV2Error::Undecodable("trailing bytes"));
        }
        if decoded.encode() != bytes {
            return Err(PalwExecV2Error::Undecodable("noncanonical encoding"));
        }
        Ok(decoded)
    }

    /// Does `commitment` open with the v2 magic — is this carrier v2 and not v1 (`PXR1`) or something else?
    pub fn is_v2_carriage(commitment: &[u8]) -> bool {
        commitment.starts_with(&PALW_EXEC_V2_CARRIAGE_MAGIC)
    }

    /// **Shape only**: the version, the exclusive subtype dispatch, the permit index or the slice's own shape, and the two
    /// ML-DSA-87 lengths. The dispatch is the spec's table, exhaustively: both present and neither present are named, and a
    /// field that does not belong to the subtype is a mismatch.
    pub fn validate_shape(&self) -> Result<(), PalwExecV2Error> {
        if self.version != PALW_EXEC_V2_WIRE_VERSION {
            return Err(PalwExecV2Error::UnsupportedVersion { got: self.version, expected: PALW_EXEC_V2_WIRE_VERSION });
        }
        match (&self.tx_permit, &self.work_slice) {
            (Some(_), Some(_)) => return Err(PalwExecV2Error::BothPayloads),
            (None, None) => return Err(PalwExecV2Error::NoPayload),
            (Some(permit), None) => {
                if self.subtype != PalwExecSubtypeV2::Tx {
                    return Err(PalwExecV2Error::SubtypeFieldMismatch { subtype: self.subtype });
                }
                if permit.permit_index >= PALW_EXEC_MAX_PERMITS_PER_ROUND_V1 {
                    return Err(PalwExecV2Error::PermitIndexOutOfRange {
                        index: permit.permit_index,
                        max: PALW_EXEC_MAX_PERMITS_PER_ROUND_V1,
                    });
                }
            }
            (None, Some(slice)) => {
                if self.subtype != PalwExecSubtypeV2::Slice {
                    return Err(PalwExecV2Error::SubtypeFieldMismatch { subtype: self.subtype });
                }
                slice.validate_shape()?;
                if slice.executor_bond != self.executor_bond {
                    // The slice names the bond that signs the carrier; two bonds in one carrier would let a payee be
                    // chosen apart from the signer.
                    return Err(PalwExecV2Error::SubtypeFieldMismatch { subtype: self.subtype });
                }
            }
        }
        if self.pubkey.len() != PALW_EXEC_MLDSA87_PUBKEY_LEN {
            return Err(PalwExecV2Error::PublicKeyLength { got: self.pubkey.len(), expected: PALW_EXEC_MLDSA87_PUBKEY_LEN });
        }
        if self.signature.len() != PALW_EXEC_MLDSA87_SIGNATURE_LEN {
            return Err(PalwExecV2Error::SignatureLength { got: self.signature.len(), expected: PALW_EXEC_MLDSA87_SIGNATURE_LEN });
        }
        Ok(())
    }

    /// The payload commitment this envelope's subtype derives: `hash_merkle_root` for `EXEC_TX` (the existing transaction
    /// batch), [`palw_work_slice_payload_root_v2`] for `EXEC_SLICE`. `None` for a shape that has no payload to derive from.
    pub fn expected_payload_root(&self, hash_merkle_root: Hash64) -> Option<Hash64> {
        match (self.subtype, &self.tx_permit, &self.work_slice) {
            (PalwExecSubtypeV2::Tx, Some(_), None) => Some(hash_merkle_root),
            (PalwExecSubtypeV2::Slice, None, Some(slice)) => Some(palw_work_slice_payload_root_v2(slice)),
            _ => None,
        }
    }

    /// The ML-DSA-87 context this envelope's signature is checked under — the subtype's own.
    pub fn mldsa87_context(&self) -> &'static [u8] {
        match self.subtype {
            PalwExecSubtypeV2::Tx => PALW_EXEC_V2_TX_MLDSA87_CONTEXT,
            PalwExecSubtypeV2::Slice => PALW_EXEC_V2_SLICE_MLDSA87_CONTEXT,
        }
    }

    /// **The message the executor signs**, after it has solved the header: the subtype's domain over the version, subtype,
    /// network, header position (`pre_pow_hash`, `timestamp`, `nonce` — the nonce is signed for the reason v1's is), the
    /// anchor, the payload commitment, the bond and the full type-specific content. `None` where the payload does not match the
    /// subtype (nothing to sign).
    pub fn signing_message(&self, pre_pow_hash: Hash64, timestamp_ms: u64, nonce: u64) -> Option<Hash64> {
        let (domain, content) = match (self.subtype, &self.tx_permit, &self.work_slice) {
            (PalwExecSubtypeV2::Tx, Some(permit), None) => (PALW_EXEC_V2_TX_SIGNING_DOMAIN, tx_content_digest(permit)),
            (PalwExecSubtypeV2::Slice, None, Some(slice)) => (PALW_EXEC_V2_SLICE_SIGNING_DOMAIN, slice.slice_id()),
            _ => return None,
        };
        let mut state = keyed(domain);
        state.update(&[self.version]);
        state.update(&[self.subtype as u8]);
        state.update(self.network_domain.as_byte_slice());
        state.update(pre_pow_hash.as_byte_slice());
        state.update(&timestamp_ms.to_le_bytes());
        state.update(&nonce.to_le_bytes());
        state.update(self.anchor.as_byte_slice());
        state.update(self.payload_root.as_byte_slice());
        update_bond(&mut state, &self.executor_bond);
        state.update(content.as_byte_slice());
        Some(finish(state))
    }

    /// **The header stage's whole stateless check**: shape; the network; for a permit, the round recomputed from the header's
    /// own timestamp; the payload commitment against the subtype's derivation; and the signature, in the subtype's context,
    /// under the carried key. Whether the key is the bond's registered key, whether the permit is granted and whether the
    /// slice's root exists are the stateful stage's.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_stateless<V>(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        hash_merkle_root: Hash64,
        timestamp_ms: u64,
        nonce: u64,
        genesis_timestamp_ms: u64,
        verify_mldsa87: V,
    ) -> Result<(), PalwExecV2Error>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.validate_shape()?;
        if self.network_domain != network_domain {
            return Err(PalwExecV2Error::NetworkDomainMismatch);
        }
        if let Some(permit) = &self.tx_permit {
            let actual = palw_execution_round_v1(timestamp_ms, genesis_timestamp_ms);
            if permit.round != actual {
                return Err(PalwExecV2Error::RoundMismatch { declared: permit.round, actual });
            }
        }
        if self.expected_payload_root(hash_merkle_root) != Some(self.payload_root) {
            return Err(PalwExecV2Error::PayloadRootMismatch);
        }
        let message = self.signing_message(pre_pow_hash, timestamp_ms, nonce).ok_or(PalwExecV2Error::NoPayload)?;
        if !verify_mldsa87(&self.pubkey, message.as_byte_slice(), &self.signature, self.mldsa87_context()) {
            return Err(PalwExecV2Error::SignatureInvalid);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The fence: `Params::palw_exec_payload_v2`
// ---------------------------------------------------------------------------------------------

/// **Is the EXEC payload fence armable on any ruleset?** `false` until every gate of
/// `docs/design/palw/rfc-0008-implementation-spec.md` section 9 has passed (root lifecycle through the real pipeline, wire and
/// compatibility, lane isolation, accounting, public verification, liveness and capacity, deterministic recovery) and the
/// release owner has decided a height. While it is `false`, [`Params::validate_palw_exec_payload_v2`] refuses every real height
/// on every ruleset, and `Some(never())` — the dormant spelling — is the only value that passes (it collapses out of the
/// identity). The prerequisites below are enforced regardless, so the day this flips nothing else needs to be written.
pub const PALW_EXEC_PAYLOAD_V2_ARMABLE: bool = false;

/// **Is the fence armable on THIS ruleset?** On a public network only once [`PALW_EXEC_PAYLOAD_V2_ARMABLE`] flips (never, today). On
/// a **salted drill of testnet-12** — the network id is testnet-12's and the genesis is not public testnet-12's, which only
/// `--palw-drill-genesis-salt` produces (`config::drill`) — yes: the spec's section 9 drills are what the drill exists to run, and a drill
/// chain is one nobody else is on. The X8R review's "flip only in the test/drill path": the constant stays `false`, and every other
/// refusal of [`Params::validate_palw_exec_payload_v2`] (the prerequisites, the mirror) still applies on a drill — so a drill arms the
/// payload only where the kernel route it is verified through is itself armed.
pub fn palw_exec_payload_v2_armable_on(params: &Params) -> bool {
    PALW_EXEC_PAYLOAD_V2_ARMABLE
        || (params.net == crate::config::drill::palw_drill_network_v1()
            && params.genesis.hash != crate::config::genesis::PALW_T12_GENESIS.hash)
}

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

impl Params {
    /// **The fence's mirror** on the V2 bundle's state params (`exec_v2_from_daa`), which the fold reads. Written here and
    /// nothing else; `None` where the fence is not armed (or is `never()`).
    pub fn sync_palw_exec_payload_v2(&mut self) {
        let from_daa = self.palw_exec_payload_v2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_exec_v2_from_daa(from_daa);
        }
    }

    /// Is the EXEC v2 payload in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_exec_payload_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_exec_payload_v2.filter(|f| *f != ForkActivation::never()).is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence with the mode folded in**: the EXEC v2 payload widens the execution lane, which only a `ConsensusV2` network
    /// has, so only it answers (and `Some(never())`, the dormant spelling, answers nothing). The pipeline reads this.
    pub fn palw_exec_payload_v2_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_exec_payload_v2) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **The EXEC payload fence's own refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * a V2 bundle whose mirror of the height is not the fence's;
    /// * **any armed height while [`PALW_EXEC_PAYLOAD_V2_ARMABLE`] is `false`** (the section 9 gates are open);
    /// * arming on a ruleset that is not `ConsensusV2`;
    /// * arming without, at or below its height, the fences the lane leans on — `palw_execution_lane` (the lane being widened),
    ///   `palw_lane_accept_parents_first` (parent hygiene the closure carriage extends), `palw_rcore_plus` (the committed
    ///   collateral ledger executor exposure joins, and the deadline function the root holds `Final` through) and
    ///   `palw_canonical_work` (the root's prefix is the claim's chain-derived canonical work) — each named in the refusal;
    /// * arming without `palw_probabilistic_constraints_v1` (the G14 kernel route, the slices' only verification route — amendment 1) in
    ///   force at or below it;
    /// * arming on a ruleset that does not declare `palw_audit_2026_09_11` (A-2's tolerance of undecodable lifecycle payloads, which is
    ///   what keeps a tag-130 carrier block-valid on builds with and without RFC-0008 v2 alike — the X8R review).
    ///
    /// A `Some(never())` value is dormant and passes.
    pub fn validate_palw_exec_payload_v2(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.exec_v2_from_daa(),
            _ => None,
        };
        let armed = self.palw_exec_payload_v2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_exec_payload_v2 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_exec_payload_v2 \
                 after the bundle is assembled",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !palw_exec_payload_v2_armable_on(self) {
            return Err(PalwModeV2Error::Invalid(
                "palw_exec_payload_v2 cannot be armed: RFC-0008 v2's section 9 gates (root lifecycle, wire and compatibility, lane \
                 isolation, accounting, public verification, liveness and capacity, deterministic recovery) are open",
            ));
        }
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_exec_payload_v2 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let in_force = |fence: Option<ForkActivation>| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if self.palw_execution_lane_at(at).is_none() {
            return Err(PalwModeV2Error::Invalid(
                "palw_exec_payload_v2 needs palw_execution_lane open at or below it: it widens that lane",
            ));
        }
        if !in_force(self.palw_lane_accept_parents_first) {
            return Err(PalwModeV2Error::Invalid(
                "palw_exec_payload_v2 needs palw_lane_accept_parents_first in force at or below it: the closure carriage extends that hygiene",
            ));
        }
        if !in_force(self.palw_rcore_plus) {
            return Err(PalwModeV2Error::Invalid(
                "palw_exec_payload_v2 needs palw_rcore_plus in force at or below it: executor exposure joins the committed ledger",
            ));
        }
        if !in_force(self.palw_canonical_work) {
            return Err(PalwModeV2Error::Invalid(
                "palw_exec_payload_v2 needs palw_canonical_work in force at or below it: a root's prefix is the claim's derived work",
            ));
        }
        // Amendment 1 (spec §10.1): a slice is verified only through the G14 kernel route, so the route must be in force with the lane.
        if !in_force(self.palw_probabilistic_constraints_v1) {
            return Err(PalwModeV2Error::Invalid(
                "palw_exec_payload_v2 needs palw_probabilistic_constraints_v1 in force at or below it: a slice is verified, convicted \
                 or defaulted only through the kernel route",
            ));
        }
        // The X8R review: a work-session root declaration (tag 130) is a payload the build before RFC-0008 v2 cannot decode, and the two
        // builds agree on its carrier only where undecodable lifecycle payloads are tolerated (A-2) — the audit fence's declaration.
        if self.palw_audit_2026_09_11_fence().is_none() {
            return Err(PalwModeV2Error::Invalid(
                "palw_exec_payload_v2 needs palw_audit_2026_09_11 declared: only there does a tag-130 carrier ride as an undecodable \
                 payload on a fleet that mixes builds with and without RFC-0008 v2",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_execution_lane_v1::PALW_EXEC_ALL_DOMAINS;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(v), 0))
    }

    pub(crate) fn slice(index: u32, start: u64, end: u64) -> PalwWorkSliceV1 {
        PalwWorkSliceV1 {
            root_claim_id: h(100),
            slice_index: index,
            class_id: h(101),
            canonical_job_id: h(102),
            kernel_version: 3,
            plan_root: h(103),
            canonical_range: PalwWorkRangeV1 { start, end },
            predecessor_state_root: h(104 + index as u64),
            result_state_root: h(105 + index as u64),
            input_root: h(106),
            output_root: h(107 + index as u64),
            evidence_root: h(108 + index as u64),
            da_root: h(109 + index as u64),
            executor_bond: bond(7),
        }
    }

    fn slice_envelope(s: PalwWorkSliceV1) -> PalwExecV2Envelope {
        PalwExecV2Envelope {
            version: PALW_EXEC_V2_WIRE_VERSION,
            network_domain: h(1),
            anchor: h(2),
            subtype: PalwExecSubtypeV2::Slice,
            tx_permit: None,
            payload_root: palw_work_slice_payload_root_v2(&s),
            executor_bond: s.executor_bond,
            work_slice: Some(s),
            pubkey: vec![5; PALW_EXEC_MLDSA87_PUBKEY_LEN],
            signature: vec![6; PALW_EXEC_MLDSA87_SIGNATURE_LEN],
        }
    }

    fn tx_envelope(root: Hash64) -> PalwExecV2Envelope {
        PalwExecV2Envelope {
            version: PALW_EXEC_V2_WIRE_VERSION,
            network_domain: h(1),
            anchor: h(2),
            subtype: PalwExecSubtypeV2::Tx,
            tx_permit: Some(PalwExecTxPermitV2 { round: 42, permit_index: 0 }),
            work_slice: None,
            payload_root: root,
            executor_bond: bond(7),
            pubkey: vec![5; PALW_EXEC_MLDSA87_PUBKEY_LEN],
            signature: vec![6; PALW_EXEC_MLDSA87_SIGNATURE_LEN],
        }
    }

    const GENESIS_TS: u64 = 1_000_000;
    const TS: u64 = GENESIS_TS + 42 * 1_000 + 17;

    /// A mock signer: the message in the first 64 bytes of the signature, valid under one key and one context only.
    fn sign(env: &mut PalwExecV2Envelope, pre: Hash64, nonce: u64) {
        let message = env.signing_message(pre, TS, nonce).expect("a well-formed payload");
        env.signature = vec![0; PALW_EXEC_MLDSA87_SIGNATURE_LEN];
        env.signature[..64].copy_from_slice(message.as_byte_slice());
    }

    fn verify_in(context: &'static [u8]) -> impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool {
        move |key, message, signature, ctx| {
            key == vec![5; PALW_EXEC_MLDSA87_PUBKEY_LEN] && ctx == context && &signature[..64] == message
        }
    }

    fn stateless(env: &PalwExecV2Envelope, pre: Hash64, nonce: u64) -> Result<(), PalwExecV2Error> {
        let ctx = env.mldsa87_context();
        env.validate_stateless(h(1), pre, h(77), TS, nonce, GENESIS_TS, verify_in(ctx))
    }

    #[test]
    fn domains_are_distinct_from_each_other_and_from_the_round_lane_v1() {
        let mut all: Vec<&[u8]> = PALW_EXEC_V2_ALL_DOMAINS.to_vec();
        all.extend(PALW_EXEC_ALL_DOMAINS.iter().copied());
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "two domains collide");
            }
        }
        assert_ne!(PALW_EXEC_V2_CARRIAGE_MAGIC, crate::palw_execution_lane_v1::PALW_EXEC_CARRIAGE_MAGIC_V1);
    }

    /// A slice envelope (an ML-DSA-87 key and signature beside twelve roots) is over the 8 KiB v1 carriage cap, so `PXE2` has its own
    /// cap — the way `PFS4` does — and the shape gate accepts it for algo 10 and nothing else, **where the fence is in force**. Below it
    /// (and on every network that does not arm it) the gate judges the bytes exactly as the build before RFC-0008 v2: a `PXE2` payload
    /// is refused with the error that build gave (the X8R review: the pruning-proof path runs only this gate).
    #[test]
    fn a_slice_envelope_exceeds_the_v1_cap_and_fits_its_own_and_the_shape_gate_takes_it_for_the_lane_only() {
        use crate::pow_layer0::{
            PALW_COMMITMENT_MAX_BYTES, POW_ALGO_ID_PALW_ROUND_V1, PalwAttemptLaneV1, PowLayer0Error,
            check_palw_commitment_shape_exec_at,
        };
        let check_palw_commitment_shape_at =
            |algo, bytes: &[u8], bound, lane| check_palw_commitment_shape_exec_at(algo, bytes, bound, lane, true);
        let bytes = slice_envelope(slice(0, 0, 10)).encode();
        // Fence not in force: the pre-RFC-0008-v2 verdict and error, byte for byte — too long for the v1 cap (a slice), malformed as
        // a v1 envelope (a permit, under the cap).
        let below = |bytes: &[u8]| {
            crate::pow_layer0::check_palw_commitment_shape_at(POW_ALGO_ID_PALW_ROUND_V1, bytes, false, PalwAttemptLaneV1::Unfenced)
        };
        // The pre-RFC-0008-v2 gate for algo 10, spelled out: the 8,192-byte cap, then the v1 decode and shape.
        let pre_v2 = |bytes: &[u8]| -> Result<(), PowLayer0Error> {
            if bytes.len() > PALW_COMMITMENT_MAX_BYTES {
                return Err(PowLayer0Error::PalwCommitmentTooLong { got: bytes.len(), cap: PALW_COMMITMENT_MAX_BYTES });
            }
            crate::palw_execution_lane_v1::PalwExecEnvelopeV1::decode(bytes)
                .and_then(|envelope| envelope.validate_shape())
                .map_err(|e| PowLayer0Error::PalwCommitmentMalformed { algo_id: POW_ALGO_ID_PALW_ROUND_V1, reason: e.to_string() })
        };
        for pxe2 in [bytes.clone(), tx_envelope(h(77)).encode()] {
            assert!(below(&pxe2).is_err(), "a PXE2 payload is no carrier where the fence is not in force");
            assert_eq!(below(&pxe2), pre_v2(&pxe2), "the same error the build before RFC-0008 v2 gave");
        }
        assert!(matches!(below(&bytes), Err(PowLayer0Error::PalwCommitmentTooLong { .. })), "a slice is over the v1 cap");
        assert!(bytes.len() > PALW_COMMITMENT_MAX_BYTES, "that is why the PXE2 cap exists: {} bytes", bytes.len());
        assert!(bytes.len() <= PALW_EXEC_V2_MAX_ENVELOPE_BYTES);
        check_palw_commitment_shape_at(POW_ALGO_ID_PALW_ROUND_V1, &bytes, false, PalwAttemptLaneV1::Unfenced)
            .expect("a PXE2 slice envelope");
        // The permit envelope is smaller and takes the same gate.
        check_palw_commitment_shape_at(POW_ALGO_ID_PALW_ROUND_V1, &tx_envelope(h(77)).encode(), false, PalwAttemptLaneV1::Unfenced)
            .expect("a PXE2 permit envelope");
        // Over its own cap, or malformed, or on another lane: refused.
        let mut huge = bytes.clone();
        huge.resize(PALW_EXEC_V2_MAX_ENVELOPE_BYTES + 1, 0);
        assert!(check_palw_commitment_shape_at(POW_ALGO_ID_PALW_ROUND_V1, &huge, false, PalwAttemptLaneV1::Unfenced).is_err());
        let mut both = slice_envelope(slice(0, 0, 10));
        both.tx_permit = Some(PalwExecTxPermitV2 { round: 1, permit_index: 0 });
        assert!(
            check_palw_commitment_shape_at(POW_ALGO_ID_PALW_ROUND_V1, &both.encode(), false, PalwAttemptLaneV1::Unfenced).is_err()
        );
        assert!(
            check_palw_commitment_shape_at(crate::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1, &bytes, false, PalwAttemptLaneV1::Unfenced)
                .is_err(),
            "no other lane carries a PXE2 envelope"
        );
        // A v1 envelope keeps the 8,192-byte cap.
        let v1_cap =
            vec![b'P', b'X', b'R', b'1'].into_iter().chain(std::iter::repeat_n(0u8, PALW_COMMITMENT_MAX_BYTES)).collect::<Vec<u8>>();
        assert!(check_palw_commitment_shape_at(POW_ALGO_ID_PALW_ROUND_V1, &v1_cap, false, PalwAttemptLaneV1::Unfenced).is_err());
    }

    #[test]
    fn subtype_bytes_are_pinned() {
        assert_eq!(borsh::to_vec(&PalwExecSubtypeV2::Tx).unwrap(), vec![1]);
        assert_eq!(borsh::to_vec(&PalwExecSubtypeV2::Slice).unwrap(), vec![2]);
        assert!(borsh::from_slice::<PalwExecSubtypeV2>(&[0]).is_err());
        assert!(borsh::from_slice::<PalwExecSubtypeV2>(&[3]).is_err());
        assert!(borsh::from_slice::<PalwExecSubtypeV2>(&[255]).is_err());
    }

    #[test]
    fn both_envelopes_round_trip_and_the_magic_is_the_v2_one() {
        for env in [slice_envelope(slice(0, 0, 10)), tx_envelope(h(77))] {
            let bytes = env.encode();
            assert!(bytes.starts_with(b"PXE2"));
            assert!(PalwExecV2Envelope::is_v2_carriage(&bytes));
            assert!(!PalwExecV2Envelope::is_v2_carriage(b"PXR1xxxx"));
            assert_eq!(PalwExecV2Envelope::decode(&bytes).unwrap(), env);
            assert!(bytes.len() < PALW_EXEC_V2_MAX_ENVELOPE_BYTES);
        }
    }

    /// Golden vectors: the slice identity, the payload commitment, the challenge binding and one signing message are pinned,
    /// so a change to a field order, a domain or a width is a visible diff and not a silent fork.
    #[test]
    fn golden_vectors_pin_the_hashes() {
        let s = slice(0, 0, 10);
        let env = slice_envelope(s.clone());
        let msg = env.signing_message(h(9), TS, 11).unwrap();
        let hex = |hash: Hash64| hash.as_byte_slice()[..8].iter().map(|b| format!("{b:02x}")).collect::<String>();
        let pinned = (
            hex(s.slice_id()),
            hex(palw_work_slice_payload_root_v2(&s)),
            hex(s.challenge_binding()),
            hex(msg),
            hex(tx_envelope(h(77)).signing_message(h(9), TS, 11).unwrap()),
        );
        assert_eq!(
            pinned,
            (
                GOLDEN_SLICE_ID.to_string(),
                GOLDEN_PAYLOAD_ROOT.to_string(),
                GOLDEN_BINDING.to_string(),
                GOLDEN_SLICE_SIGNING.to_string(),
                GOLDEN_TX_SIGNING.to_string()
            ),
            "the v2 wire hashes moved"
        );
    }
    const GOLDEN_SLICE_ID: &str = "6ecd080f515bc1df";
    const GOLDEN_PAYLOAD_ROOT: &str = "0a6effad2ba4a791";
    const GOLDEN_BINDING: &str = "78ffd642a754b7bb";
    const GOLDEN_SLICE_SIGNING: &str = "0eef9718807cbe87";
    const GOLDEN_TX_SIGNING: &str = "d2361b279ea38b31";

    #[test]
    fn a_well_formed_carrier_of_each_subtype_validates_and_a_mutation_of_anything_signed_does_not() {
        let mut slice_env = slice_envelope(slice(0, 0, 10));
        sign(&mut slice_env, h(9), 11);
        stateless(&slice_env, h(9), 11).unwrap();
        let mut tx_env = tx_envelope(h(77));
        sign(&mut tx_env, h(9), 11);
        stateless(&tx_env, h(9), 11).unwrap();

        // The header position: another pre-pow hash or another nonce is another message.
        assert_eq!(stateless(&slice_env, h(10), 11), Err(PalwExecV2Error::SignatureInvalid));
        assert_eq!(stateless(&slice_env, h(9), 12), Err(PalwExecV2Error::SignatureInvalid));

        // Every signed field: flip it and the signature stops verifying (the payload root is re-derived for slice fields so the
        // failure is the signature's, not the commitment's).
        let resign_free = |mutate: &dyn Fn(&mut PalwExecV2Envelope)| {
            let mut env = slice_env.clone();
            mutate(&mut env);
            if let Some(s) = &env.work_slice {
                env.payload_root = palw_work_slice_payload_root_v2(s);
            }
            stateless(&env, h(9), 11)
        };
        let reject = Err(PalwExecV2Error::SignatureInvalid);
        assert_eq!(resign_free(&|e| e.anchor = h(3)), reject, "anchor");
        assert_eq!(resign_free(&|e| e.network_domain = h(1)), Ok(()), "(control: no change is no failure)");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().slice_index = 1), reject, "index");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().canonical_range.end = 11), reject, "range end");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().canonical_range.start = 1), reject, "range start");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().result_state_root = h(999)), reject, "result root");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().predecessor_state_root = h(999)), reject, "predecessor");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().class_id = h(999)), reject, "class");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().canonical_job_id = h(999)), reject, "job");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().kernel_version = 4), reject, "kernel");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().plan_root = h(999)), reject, "plan");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().input_root = h(999)), reject, "input");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().output_root = h(999)), reject, "output");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().evidence_root = h(999)), reject, "evidence");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().da_root = h(999)), reject, "da");
        assert_eq!(resign_free(&|e| e.work_slice.as_mut().unwrap().root_claim_id = h(999)), reject, "root claim");
        // Bond (slice and envelope move together, as the shape rule demands).
        assert_eq!(
            resign_free(&|e| {
                e.executor_bond = bond(8);
                e.work_slice.as_mut().unwrap().executor_bond = bond(8);
            }),
            reject,
            "bond"
        );
        // A forged payload commitment is refused before the signature is read.
        let mut forged = slice_env.clone();
        forged.payload_root = h(5);
        assert_eq!(stateless(&forged, h(9), 11), Err(PalwExecV2Error::PayloadRootMismatch));
        // The tx subtype: its commitment is the header's merkle root, and its permit is signed.
        let mut tx_other_root = tx_env.clone();
        tx_other_root.payload_root = h(78);
        assert_eq!(stateless(&tx_other_root, h(9), 11), Err(PalwExecV2Error::PayloadRootMismatch));
        let mut tx_other_permit = tx_env.clone();
        tx_other_permit.tx_permit = Some(PalwExecTxPermitV2 { round: 42, permit_index: 1 });
        assert_eq!(stateless(&tx_other_permit, h(9), 11), reject);
        let mut tx_other_round = tx_env.clone();
        tx_other_round.tx_permit = Some(PalwExecTxPermitV2 { round: 43, permit_index: 0 });
        assert_eq!(stateless(&tx_other_round, h(9), 11), Err(PalwExecV2Error::RoundMismatch { declared: 43, actual: 42 }));
        // Another network.
        let mut other_net = slice_env.clone();
        other_net.network_domain = h(2);
        assert_eq!(stateless(&other_net, h(9), 11), Err(PalwExecV2Error::NetworkDomainMismatch));
    }

    #[test]
    fn a_permit_signature_never_verifies_as_a_slice_signature_nor_the_reverse() {
        // The same message bytes under the other subtype's ML-DSA context are refused: the contexts are the second lock behind
        // the domains.
        let mut slice_env = slice_envelope(slice(0, 0, 10));
        sign(&mut slice_env, h(9), 11);
        let wrong_context =
            slice_env.validate_stateless(h(1), h(9), h(77), TS, 11, GENESIS_TS, verify_in(PALW_EXEC_V2_TX_MLDSA87_CONTEXT));
        assert_eq!(wrong_context, Err(PalwExecV2Error::SignatureInvalid));
        let mut tx_env = tx_envelope(h(77));
        sign(&mut tx_env, h(9), 11);
        let wrong_context =
            tx_env.validate_stateless(h(1), h(9), h(77), TS, 11, GENESIS_TS, verify_in(PALW_EXEC_V2_SLICE_MLDSA87_CONTEXT));
        assert_eq!(wrong_context, Err(PalwExecV2Error::SignatureInvalid));
        // And the messages themselves differ between the subtypes even for the same header position and anchor.
        assert_ne!(slice_env.signing_message(h(9), TS, 11), tx_env.signing_message(h(9), TS, 11));
        // A v1 envelope's signing message is in yet another domain.
        assert_ne!(PALW_EXEC_V2_TX_SIGNING_DOMAIN, crate::palw_execution_lane_v1::PALW_EXEC_SIGNING_DOMAIN);
    }

    #[test]
    fn the_dispatch_table_is_exhaustive_and_every_mismatch_rejects_the_whole_carrier() {
        let s = slice(0, 0, 10);
        let permit = PalwExecTxPermitV2 { round: 42, permit_index: 0 };
        // Both.
        let mut both = slice_envelope(s.clone());
        both.tx_permit = Some(permit);
        assert_eq!(both.validate_shape(), Err(PalwExecV2Error::BothPayloads));
        both.subtype = PalwExecSubtypeV2::Tx;
        assert_eq!(both.validate_shape(), Err(PalwExecV2Error::BothPayloads));
        // Neither.
        let mut neither = slice_envelope(s.clone());
        neither.work_slice = None;
        assert_eq!(neither.validate_shape(), Err(PalwExecV2Error::NoPayload));
        neither.subtype = PalwExecSubtypeV2::Tx;
        assert_eq!(neither.validate_shape(), Err(PalwExecV2Error::NoPayload));
        // A slice field under the Tx subtype, and a permit under the Slice subtype.
        let mut tx_with_slice = slice_envelope(s.clone());
        tx_with_slice.subtype = PalwExecSubtypeV2::Tx;
        assert_eq!(tx_with_slice.validate_shape(), Err(PalwExecV2Error::SubtypeFieldMismatch { subtype: PalwExecSubtypeV2::Tx }));
        let mut slice_with_permit = tx_envelope(h(77));
        slice_with_permit.subtype = PalwExecSubtypeV2::Slice;
        assert_eq!(
            slice_with_permit.validate_shape(),
            Err(PalwExecV2Error::SubtypeFieldMismatch { subtype: PalwExecSubtypeV2::Slice })
        );
        // The slice's bond is the signer's.
        let mut other_bond = slice_envelope(s.clone());
        other_bond.executor_bond = bond(8);
        assert!(other_bond.validate_shape().is_err());
        // Version.
        for version in [0u8, 1, 3, 255] {
            let mut env = slice_envelope(s.clone());
            env.version = version;
            assert_eq!(env.validate_shape(), Err(PalwExecV2Error::UnsupportedVersion { got: version, expected: 2 }));
        }
        // Permit index.
        let mut wide = tx_envelope(h(77));
        wide.tx_permit = Some(PalwExecTxPermitV2 { round: 42, permit_index: PALW_EXEC_MAX_PERMITS_PER_ROUND_V1 });
        assert!(matches!(wide.validate_shape(), Err(PalwExecV2Error::PermitIndexOutOfRange { .. })));
        // Key and signature lengths.
        let mut short_key = slice_envelope(s.clone());
        short_key.pubkey.pop();
        assert!(matches!(short_key.validate_shape(), Err(PalwExecV2Error::PublicKeyLength { .. })));
        let mut short_sig = slice_envelope(s);
        short_sig.signature.push(0);
        assert!(matches!(short_sig.validate_shape(), Err(PalwExecV2Error::SignatureLength { .. })));
    }

    #[test]
    fn slice_shape_refuses_empty_ranges_unset_roots_and_an_index_past_the_bound() {
        assert_eq!(slice(0, 5, 5).validate_shape(), Err(PalwExecV2Error::SliceRangeEmpty));
        assert_eq!(slice(0, 9, 5).validate_shape(), Err(PalwExecV2Error::SliceRangeEmpty));
        assert!(matches!(
            slice(PALW_EXEC_V2_MAX_SLICES_PER_ROOT, 0, 5).validate_shape(),
            Err(PalwExecV2Error::SliceIndexOutOfRange { .. })
        ));
        type Mutation = Box<dyn Fn(&mut PalwWorkSliceV1)>;
        let cases: Vec<(&str, Mutation)> = vec![
            ("root_claim_id", Box::new(|s| s.root_claim_id = Hash64::default())),
            ("class_id", Box::new(|s| s.class_id = Hash64::default())),
            ("canonical_job_id", Box::new(|s| s.canonical_job_id = Hash64::default())),
            ("plan_root", Box::new(|s| s.plan_root = Hash64::default())),
            ("predecessor_state_root", Box::new(|s| s.predecessor_state_root = Hash64::default())),
            ("result_state_root", Box::new(|s| s.result_state_root = Hash64::default())),
            ("input_root", Box::new(|s| s.input_root = Hash64::default())),
            ("output_root", Box::new(|s| s.output_root = Hash64::default())),
            ("evidence_root", Box::new(|s| s.evidence_root = Hash64::default())),
            ("da_root", Box::new(|s| s.da_root = Hash64::default())),
        ];
        for (name, mutate) in cases {
            let mut s = slice(0, 0, 10);
            mutate(&mut s);
            assert_eq!(s.validate_shape(), Err(PalwExecV2Error::SliceRootUnset(name)));
        }
    }

    #[test]
    fn decode_is_strict() {
        let env = slice_envelope(slice(0, 0, 10));
        let good = env.encode();
        // Magic.
        assert!(PalwExecV2Envelope::decode(&good[1..]).is_err());
        let mut v1_magic = good.clone();
        v1_magic[..4].copy_from_slice(b"PXR1");
        assert!(PalwExecV2Envelope::decode(&v1_magic).is_err());
        assert!(PalwExecV2Envelope::decode(&[]).is_err());
        // Trailing bytes.
        let mut trailing = good.clone();
        trailing.push(0);
        assert_eq!(PalwExecV2Envelope::decode(&trailing), Err(PalwExecV2Error::Undecodable("trailing bytes")));
        // Truncation at every length around the header and the end.
        for cut in [5, 6, 40, 100, good.len() - 1] {
            assert!(PalwExecV2Envelope::decode(&good[..cut]).is_err(), "cut at {cut}");
        }
        // Oversize is refused before parsing.
        let mut huge = good.clone();
        huge.resize(PALW_EXEC_V2_MAX_ENVELOPE_BYTES + 1, 0);
        assert!(matches!(PalwExecV2Envelope::decode(&huge), Err(PalwExecV2Error::TooLarge { .. })));
        // The subtype byte sits after magic ‖ version(1) ‖ network(64) ‖ anchor(64): an unknown value is undecodable, never a
        // default subtype.
        let subtype_at = 4 + 1 + 64 + 64;
        assert_eq!(good[subtype_at], 2);
        for bad in [0u8, 3, 9, 255] {
            let mut mutated = good.clone();
            mutated[subtype_at] = bad;
            assert!(PalwExecV2Envelope::decode(&mutated).is_err(), "subtype {bad}");
        }
        // The Option tags (permit then slice) are 0/1 only.
        let permit_tag_at = subtype_at + 1;
        assert_eq!(good[permit_tag_at], 0);
        assert_eq!(good[permit_tag_at + 1], 1);
        for bad in [2u8, 7, 255] {
            let mut mutated = good.clone();
            mutated[permit_tag_at] = bad;
            assert!(PalwExecV2Envelope::decode(&mutated).is_err(), "permit option tag {bad}");
            let mut mutated = good.clone();
            mutated[permit_tag_at + 1] = bad;
            assert!(PalwExecV2Envelope::decode(&mutated).is_err(), "slice option tag {bad}");
        }
        // A single-bit flip anywhere either fails to decode or decodes to a different envelope — never to the same one.
        for at in (4..good.len()).step_by(37) {
            let mut mutated = good.clone();
            mutated[at] ^= 0x01;
            match PalwExecV2Envelope::decode(&mutated) {
                Ok(other) => assert_ne!(other, env, "bit flip at {at} decoded to the same envelope"),
                Err(_) => {}
            }
        }
    }

    #[test]
    fn a_decodable_envelope_of_another_version_is_refused_by_shape_and_decoding_does_not_accept_it() {
        let mut env = slice_envelope(slice(0, 0, 10));
        env.version = 3;
        let decoded = PalwExecV2Envelope::decode(&env.encode()).expect("decodes (the layout is this one)");
        assert_eq!(decoded.validate_shape(), Err(PalwExecV2Error::UnsupportedVersion { got: 3, expected: 2 }));
    }

    #[test]
    fn ranges_report_work_and_overlap_without_overflow() {
        let r = |start, end| PalwWorkRangeV1 { start, end };
        assert_eq!(r(0, 10).work(), Some(10));
        assert_eq!(r(5, 5).work(), None);
        assert_eq!(r(6, 5).work(), None);
        assert_eq!(r(0, u64::MAX).work(), Some(u64::MAX));
        assert!(r(0, 10).overlaps(&r(9, 12)));
        assert!(!r(0, 10).overlaps(&r(10, 12)));
        assert!(!r(10, 12).overlaps(&r(0, 10)));
        assert!(r(0, u64::MAX).overlaps(&r(u64::MAX - 1, u64::MAX)));
    }

    #[test]
    fn the_challenge_subject_is_a_pure_function_of_the_slice_and_moves_with_every_bound_field() {
        let base = slice(2, 20, 30);
        let subject = base.challenge_subject();
        assert_eq!(subject.subject_id, base.slice_id());
        assert_eq!(subject.verification_plan_root, base.plan_root);
        assert_eq!(subject.input_root, base.input_root);
        assert_eq!(subject.state_root, base.result_state_root);
        assert_eq!(subject.constraint_root, base.evidence_root);
        // The predecessor, the output, the DA root, the job, the index, the range and the executor are bound by the commitment
        // root even though the subject has no typed field for them.
        type Mutation = Box<dyn Fn(&mut PalwWorkSliceV1)>;
        let mutations: Vec<(&str, Mutation)> = vec![
            ("predecessor", Box::new(|s| s.predecessor_state_root = h(900))),
            ("output", Box::new(|s| s.output_root = h(900))),
            ("da", Box::new(|s| s.da_root = h(900))),
            ("job", Box::new(|s| s.canonical_job_id = h(900))),
            ("index", Box::new(|s| s.slice_index = 3)),
            ("range", Box::new(|s| s.canonical_range.end = 31)),
            ("root", Box::new(|s| s.root_claim_id = h(900))),
            ("executor", Box::new(|s| s.executor_bond = bond(9))),
        ];
        for (name, mutate) in mutations {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert_ne!(changed.challenge_subject().commitment_root, subject.commitment_root, "{name}");
            assert_ne!(changed.challenge_subject().subject_id, subject.subject_id, "{name}");
        }
    }
}
