//! Current PALW consensus types: bonded producer identity, future-anchor
//! selection, panel verification, and compute/court transactions.
//!
//! DNS is intentionally outside this module's consensus decisions; it is used
//! only by the node's peer-discovery service.
//!
//! Hash widths follow [ADR-0008](../../docs/adr/0008-hash64-consensus-identity.md)
//! and the current PALW architecture in
//! [ADR-0026](../../docs/adr/0026-current-palw-testnet11-architecture.md): `validator_id`,
//! `target_hash`, `validator_set_commitment`, and the owner /
//! validator pubkey hashes inside the registry types are all 64-byte
//! [`Hash64`]. `TransactionOutpoint.transaction_id` is the upstream
//! 32-byte alias today and widens to Hash64 in the PR-9.5 cascade —
//! callers must not assume 32 bytes there long-term.
//!
//! All payload and state types derive `BorshSerialize` /
//! `BorshDeserialize` so they round-trip through the existing wRPC
//! Borsh path; `serde` JSON is added via manual impls in the
//! consumer-facing RPC types only.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Display, Formatter};

use blake2b_simd::Params as Blake2bParams;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64, blake2b_512_keyed};
use kaspa_utils::mem_size::MemSizeEstimator;

/// serde for the 64-byte ML-DSA P2PKH reward payload (ADR-0019 §8).
///
/// `serde` only auto-derives `Serialize`/`Deserialize` for `[u8; N]` up to
/// `N == 32`, and `kaspa_utils::serde_bytes_fixed` is likewise capped at 32
/// (serde array-impl limit). The reward payload is 64 bytes, so the
/// serde-deriving [`StakeBondRecord`] needs a `#[serde(with = ...)]` helper.
/// Mirrors the hand-rolled [`kaspa_hashes::Hash64`] serde: a 128-char hex
/// string for human-readable encoders, raw bytes otherwise. (borsh derives a
/// 64-byte array natively, so only serde needs this.)
mod serde_reward_payload64 {
    use serde::de::{self, Visitor};
    use serde::ser::SerializeTuple;
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S: Serializer>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            let mut hex = [0u8; 128];
            faster_hex::hex_encode(bytes, &mut hex).map_err(serde::ser::Error::custom)?;
            // safety: hex output is always ASCII.
            serializer.serialize_str(unsafe { std::str::from_utf8_unchecked(&hex) })
        } else {
            let mut t = serializer.serialize_tuple(64)?;
            for b in bytes {
                t.serialize_element(b)?;
            }
            t.end()
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 64], D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = [u8; 64];
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a 64-byte array (128-char hex or 64 raw bytes)")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<[u8; 64], E> {
                let mut out = [0u8; 64];
                faster_hex::hex_decode(s.as_bytes(), &mut out).map_err(de::Error::custom)?;
                Ok(out)
            }
            fn visit_bytes<E: de::Error>(self, b: &[u8]) -> Result<[u8; 64], E> {
                <[u8; 64]>::try_from(b).map_err(|_| E::invalid_length(b.len(), &self))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<[u8; 64], A::Error> {
                let mut out = [0u8; 64];
                for (i, slot) in out.iter_mut().enumerate() {
                    *slot = seq.next_element::<u8>()?.ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                Ok(out)
            }
        }
        if deserializer.is_human_readable() { deserializer.deserialize_str(V) } else { deserializer.deserialize_tuple(64, V) }
    }
}

use crate::palw::bond::p2pkh_mldsa87_spk;
use crate::subnets::{
    SUBNETWORK_ID_COMPUTE_CAPABILITY, SUBNETWORK_ID_COMPUTE_CERTIFICATE, SUBNETWORK_ID_COMPUTE_CHALLENGE,
    SUBNETWORK_ID_COMPUTE_COMMITMENT, SUBNETWORK_ID_COMPUTE_VERDICT, SUBNETWORK_ID_STAKE_BOND, SUBNETWORK_ID_STAKE_UNBOND,
    SubnetworkId,
};
use crate::{
    BlockHash, TransactionId,
    palw::compute::{
        ComputeCapabilityPayload, ComputeCertificatePayload, ComputeChallengePayload, ComputeCommitmentPayload, ComputeFraudKind,
        ComputeVerdictPayload, MAX_JOB_INPUT_BYTES, MAX_PANEL_ATTESTATIONS, PalwComputeParams, VerifierAttestation,
    },
    tx::{Transaction, TransactionOutpoint, TransactionOutput},
};

/// 2592 bytes — matches `kaspa_txscript::MLDSA87_PK_LEN`. Repeated
/// here so this module does not have to depend on `kaspa-txscript`;
/// asserted-equal by [`tests::palw_constants_have_expected_values`].
pub const STAKE_VALIDATOR_PUBKEY_LEN: usize = 2592;

/// 4627 bytes — matches `kaspa_txscript::MLDSA87_SIG_LEN`. Same
/// re-export rationale as [`STAKE_VALIDATOR_PUBKEY_LEN`].
pub const MLDSA87_SIG_LEN: usize = 4627;

/// Fixed-point scale for [`bond participation score`] / [`PalwStatus`] integer
/// arithmetic. Always 10^9, so a "full one-vote epoch" contributes
/// exactly `STAKE_SCORE_SCALE` to the score. Keeps consensus arithmetic
/// integer-only.
pub const STAKE_SCORE_SCALE: u128 = 1_000_000_000;

/// kaspa-pq Phase 10 wire-format version of every payload struct.
/// Bumped only by a hard-fork ADR; consumers reject foreign versions.
pub const PALW_PAYLOAD_VERSION_V1: u16 = 1;

/// kaspa-pq H-05 (ADR-0026 "Unbonding"): ML-DSA-87 signing context for a
/// `StakeUnbondRequest`. Distinct from the tx / panel verifier / takeover contexts
/// so an unbond authorization can never be replayed as any of those.
pub const UNBOND_REQUEST_CONTEXT: &[u8] = b"kaspa-pq-v1/unbond/mldsa87";

/// kaspa-pq H-05: BLAKE2b-256 domain key for the unbond-request message the
/// owner signs over (binds the authorization to the specific bond outpoint).
pub const UNBOND_REQUEST_MESSAGE_DOMAIN: &[u8] = b"kaspa-pq-v1/unbond-request";

/// Lifecycle state of a producer bond.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum BondStatus {
    #[default]
    Pending = 0,
    Active = 1,
    Unbonding = 2,
    Slashed = 3,
}

/// On-chain producer bond payload.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeBondPayload {
    pub version: u16,
    pub owner_pubkey_hash: Hash64,
    pub validator_pubkey_hash: Hash64,
    pub validator_pubkey: Vec<u8>,
    pub amount: u64,
    pub activation_daa_score: u64,
    pub unbonding_period_blocks: u64,
    pub owner_reward_spk_payload: [u8; 64],
}

/// Derived producer bond state keyed by the bond outpoint.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct StakeBondRecord {
    pub version: u16,
    pub bond_outpoint: TransactionOutpoint,
    pub owner_pubkey_hash: Hash64,
    pub validator_pubkey_hash: Hash64,
    pub validator_pubkey: Vec<u8>,
    pub amount: u64,
    pub activation_daa_score: u64,
    pub created_daa_score: u64,
    pub unbonding_period_blocks: u64,
    #[serde(with = "serde_reward_payload64")]
    pub owner_reward_spk_payload: [u8; 64],
    pub unbond_request_daa_score: Option<u64>,
    pub slashed_at_daa_score: Option<u64>,
    pub status: BondStatus,
}

impl MemSizeEstimator for StakeBondRecord {}

/// Filter and pagination parameters for producer bonds.
#[derive(Clone, Debug, Default)]
pub struct StakeBondQuery {
    pub owner_pubkey_hash: Option<Hash64>,
    pub status_in: Option<Vec<BondStatus>>,
    pub cursor: Option<TransactionOutpoint>,
    pub limit: usize,
    pub pov_daa_score: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct StakeBondPage {
    pub bonds: Vec<StakeBondRecord>,
    pub next_cursor: Option<TransactionOutpoint>,
    pub pov_daa_score: u64,
}

/// Owner-authorized request to begin releasing a producer bond.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeUnbondRequestPayload {
    pub version: u16,
    pub bond_outpoint: TransactionOutpoint,
    pub owner_pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

pub fn validator_id_from_pubkey(validator_pubkey: &[u8]) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(Blake2bParams::new().hash_length(64).to_state().update(validator_pubkey).finalize().as_bytes());
    Hash64::from_bytes(out)
}

pub fn unbond_request_message(network_id: &[u8], bond_outpoint: TransactionOutpoint) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(UNBOND_REQUEST_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    Hash::from_bytes(hasher.finalize().as_bytes().try_into().expect("BLAKE2b-256 output"))
}

/// Consensus configuration for the current PALW path.
///
/// The fields deliberately describe only producer bonds, future-anchor
/// selection, compute metering, and the activation fences for those paths.
/// Peer discovery is node-local and is not represented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwParams {
    pub palw_activation_daa_score: u64,
    pub min_bond_amount_sompi: u64,
    pub unbonding_period_blocks: u64,
    pub future_anchor_epoch_length_blue_score: u64,
    pub future_anchor_lag_blue_score: u64,
    pub future_anchor_backoff_blue_score: u64,
    pub future_anchor_window_blue_score: u64,
    pub bond_spend_gate_mergeset_activation_daa_score: u64,
    pub palw_compute: PalwComputeParams,
    pub palw_compute_window_blue_score: u64,
    pub unbond_authz_mergeset_activation_daa_score: u64,
}

/// The deterministic future anchor selected for one blue-score epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalLaggedEpochAnchor {
    pub epoch: u64,
    pub epoch_start_blue_score: u64,
    pub epoch_end_blue_score: u64,
    pub cutoff_blue_score: u64,
    pub anchor_hash: Hash64,
    pub anchor_blue_score: u64,
    pub anchor_daa_score: u64,
    pub duplicate_of_previous_anchor: bool,
}

pub fn epoch_start_blue_score(epoch: u64, epoch_len_blue_score: u64) -> u64 {
    epoch.saturating_mul(epoch_len_blue_score.max(1))
}

pub fn epoch_end_blue_score(epoch: u64, epoch_len_blue_score: u64) -> u64 {
    epoch_start_blue_score(epoch.saturating_add(1), epoch_len_blue_score).saturating_sub(1)
}

pub fn anchor_cutoff_blue_score(epoch: u64, epoch_len_blue_score: u64, backoff_blue_score: u64) -> u64 {
    epoch_end_blue_score(epoch, epoch_len_blue_score).saturating_sub(backoff_blue_score)
}

pub fn ready_epoch_from_tip_blue_score(tip_blue_score: u64, epoch_len_blue_score: u64, lag_blue_score: u64) -> Option<u64> {
    let epoch_len = epoch_len_blue_score.max(1);
    let safe = tip_blue_score.checked_sub(lag_blue_score)?;
    let completed = safe.checked_add(1)? / epoch_len;
    if completed == 0 { None } else { Some(completed - 1) }
}

pub fn voting_epoch_for_target(target_epoch: u64, epoch_len_blue_score: u64, lag_blue_score: u64) -> u64 {
    let epoch_len = epoch_len_blue_score.max(1);
    epoch_start_blue_score(target_epoch.saturating_add(1), epoch_len).saturating_add(lag_blue_score) / epoch_len
}

pub fn canonical_lagged_epoch_anchor(
    epoch: u64,
    epoch_len_blue_score: u64,
    backoff_blue_score: u64,
    ancestors: &[(Hash64, u64, u64)],
) -> Option<CanonicalLaggedEpochAnchor> {
    let epoch_len = epoch_len_blue_score.max(1);
    let cutoff = anchor_cutoff_blue_score(epoch, epoch_len, backoff_blue_score);
    let (anchor_hash, anchor_blue_score, anchor_daa_score) = *ancestors.iter().find(|(_, blue_score, _)| *blue_score <= cutoff)?;
    let duplicate_of_previous_anchor = if epoch == 0 {
        false
    } else {
        let previous_cutoff = anchor_cutoff_blue_score(epoch - 1, epoch_len, backoff_blue_score);
        ancestors.iter().find(|(_, blue_score, _)| *blue_score <= previous_cutoff).map(|(hash, _, _)| *hash) == Some(anchor_hash)
    };
    Some(CanonicalLaggedEpochAnchor {
        epoch,
        epoch_start_blue_score: epoch_start_blue_score(epoch, epoch_len),
        epoch_end_blue_score: epoch_end_blue_score(epoch, epoch_len),
        cutoff_blue_score: cutoff,
        anchor_hash,
        anchor_blue_score,
        anchor_daa_score,
        duplicate_of_previous_anchor,
    })
}

impl PalwParams {
    pub fn palw_compute_active_at(&self, daa_score: u64) -> bool {
        self.palw_compute.shadow_active_at(daa_score)
    }
}

/// Coarse local validator status exposed by the current producer service.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ValidatorStatus {
    #[default]
    NodeNotSynced = 0,
    BondNotFound = 1,
    BondPending = 2,
    ActiveIdle = 3,
    ActiveEligible = 4,
    SignedThisEpoch = 5,
    Unbonding = 6,
    Slashed = 7,
    DryRun = 8,
}

// =====================================================================
// PR-10.4: future-anchor confirmation overlay transaction kinds + stateless payload
// validation (ADR-0026).
//
// `palw_tx_kind` maps a routed subnetwork id to its payload kind; the
// three `validate_*_payload` functions perform *stateless* checks only:
// borsh-decodability, payload version, the fixed ML-DSA length
// invariants (2592-byte pubkey / 4627-byte signature), shard cardinality
// + single-anchor tuple consistency, and equivocation well-formedness.
// The consensus pipeline calls these from `check_transaction_subnetwork`
// (PR-10.4 wiring in `tx_validation_in_isolation.rs`).
//
// Deferred to later PRs (they need DAG / UTXO / rollout context): the
// on-chain bond existence + `pubkey_hash == BLAKE2b-512(pubkey)` binding,
// rollout-stage gating, ML-DSA-87 signature verification against the
// committed validator set, the `U ≥ R + E` dominance bound, the
// `(bond_outpoint, validator_id, epoch)` on-chain uniqueness rule, and
// the `evidence_window_blocks` recency of slashing evidence.
// =====================================================================

/// Payload kind carried by a current PALW transaction, keyed by
/// its routed subnetwork id (`SubnetworkId::is_palw_overlay`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PalwTxKind {
    /// `SUBNETWORK_ID_STAKE_BOND` — [`StakeBondPayload`].
    StakeBond,
    /// `SUBNETWORK_ID_STAKE_UNBOND` — [`StakeUnbondRequestPayload`].
    StakeUnbond,
    /// `SUBNETWORK_ID_COMPUTE_CERTIFICATE` — [`crate::palw::compute::ComputeCertificatePayload`].
    ComputeCertificate,
    /// `SUBNETWORK_ID_COMPUTE_CHALLENGE` — [`crate::palw::compute::ComputeChallengePayload`].
    ComputeChallenge,
    /// `SUBNETWORK_ID_COMPUTE_CAPABILITY` — [`crate::palw::compute::ComputeCapabilityPayload`].
    ComputeCapability,
    /// `SUBNETWORK_ID_COMPUTE_COMMITMENT` — [`crate::palw::compute::ComputeCommitmentPayload`].
    ComputeCommitment,
    /// `SUBNETWORK_ID_COMPUTE_VERDICT` — [`crate::palw::compute::ComputeVerdictPayload`].
    ComputeVerdict,
}

/// Maps a subnetwork id to its PALW payload kind, or `None` for a
/// non-overlay subnetwork (native / coinbase / registry / unknown). The
/// mirror of [`SubnetworkId::is_palw_overlay`] that also names the kind.
pub fn palw_tx_kind(subnetwork_id: &SubnetworkId) -> Option<PalwTxKind> {
    if *subnetwork_id == SUBNETWORK_ID_STAKE_BOND {
        Some(PalwTxKind::StakeBond)
    } else if *subnetwork_id == SUBNETWORK_ID_STAKE_UNBOND {
        Some(PalwTxKind::StakeUnbond)
    } else if *subnetwork_id == SUBNETWORK_ID_COMPUTE_CERTIFICATE {
        Some(PalwTxKind::ComputeCertificate)
    } else if *subnetwork_id == SUBNETWORK_ID_COMPUTE_CHALLENGE {
        Some(PalwTxKind::ComputeChallenge)
    } else if *subnetwork_id == SUBNETWORK_ID_COMPUTE_CAPABILITY {
        Some(PalwTxKind::ComputeCapability)
    } else if *subnetwork_id == SUBNETWORK_ID_COMPUTE_COMMITMENT {
        Some(PalwTxKind::ComputeCommitment)
    } else if *subnetwork_id == SUBNETWORK_ID_COMPUTE_VERDICT {
        Some(PalwTxKind::ComputeVerdict)
    } else {
        None
    }
}

/// Stateless validation failure for a current PALW path transaction payload.
/// The consensus tx-validation layer wraps this in
/// `TxRuleError::InvalidPalwPayload`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTxError {
    /// Payload bytes did not borsh-decode into the expected type (also
    /// fires on trailing bytes after an otherwise-valid prefix).
    Decode,
    /// The `version` field is not `PALW_PAYLOAD_VERSION_V1`.
    UnsupportedVersion(u16),
    /// Stake-bond amount is zero (a bond must lock non-zero stake).
    ZeroBondAmount,
    /// Validator public key is not exactly `STAKE_VALIDATOR_PUBKEY_LEN`.
    InvalidPubKeyLen(usize),
    /// The declared `validator_pubkey_hash` is not the canonical
    /// `validator_id_from_pubkey(validator_pubkey)` (audit H-04): the overlay
    /// identity must be the unkeyed BLAKE2b-512 of the bond's own key.
    ValidatorPubkeyHashMismatch,
    /// An panel verifier signature is not exactly `MLDSA87_SIG_LEN`.
    InvalidSignatureLen(usize),
    /// ADR-0016 D.1: a `StakeBond` tx has no output-0 to lock the stake in.
    MissingBondOutput,
    /// ADR-0016 D.1: the bond output-0 value does not equal `amount`
    /// (`expected` = payload `amount`, `got` = output-0 value).
    BondOutputValueMismatch { expected: u64, got: u64 },
    /// ADR-0016 D.1: the bond output-0 script is not the owner's
    /// canonical P2PKH-ML-DSA script over `owner_reward_spk_payload`.
    BondOutputScriptMismatch,
    /// A compute challenge is an evidence-only transaction and must not create outputs.
    ComputeChallengeHasOutputs(usize),
    /// The job's `v_j` is not consensus-eligible in this version (only
    /// [`crate::palw::compute::VerificationScheme::CanonicalFullReplay`] is).
    IneligibleVerificationScheme,
    /// The job's `q` is an unknown [`crate::palw::compute::QuantizationProfile`] discriminant.
    UnknownQuantizationProfile,
    /// A compute certificate carries no verifier panel verifier — an unverified receipt, which by
    /// §3.2 eq. (4) mints no PALW and so has no business on chain.
    NoVerifierAttestations,
    /// A compute certificate exceeds [`crate::palw::compute::MAX_PANEL_ATTESTATIONS`].
    TooManyVerifierAttestations(usize),
    /// One `validator_id` appears twice among a certificate's verifiers — an attempt to reach the
    /// confirmation threshold with a single key.
    DuplicateVerifier,
    /// The executor appears among its own verifiers (§6 separates the two roles).
    ExecutorVerifiedOwnJob,
    /// A [`crate::palw::compute::ComputeFraudKind::ContradictoryVerification`] challenge does not carry two
    /// genuinely conflicting verdicts by one verifier, or another challenge kind carries verdicts
    /// it has no use for.
    MalformedContradictionProof,
    /// A verdict contradicts its own two receipt hashes — it claims `Confirmed` over a hash it
    /// did not reproduce, or `Refuted` over one it did.
    SelfContradictoryVerdict,
    /// A job commitment's published input is empty or exceeds [`crate::palw::compute::MAX_JOB_INPUT_BYTES`].
    /// Empty is refused too: a job with no input has nothing for a verifier to replay.
    JobInputSize(usize),
}

impl Display for PalwTxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PalwTxError::Decode => write!(f, "current PALW path payload failed to decode"),
            PalwTxError::UnsupportedVersion(v) => write!(f, "unsupported current PALW path payload version {v}"),
            PalwTxError::ZeroBondAmount => write!(f, "stake-bond amount must be non-zero"),
            PalwTxError::InvalidPubKeyLen(n) => write!(f, "validator public key length {n} is invalid"),
            PalwTxError::ValidatorPubkeyHashMismatch => {
                write!(f, "stake-bond validator_pubkey_hash is not BLAKE2b-512(validator_pubkey)")
            }
            PalwTxError::InvalidSignatureLen(n) => write!(f, "panel verifier signature length {n} is invalid"),
            PalwTxError::MissingBondOutput => write!(f, "stake-bond tx has no output-0 to lock the stake"),
            PalwTxError::BondOutputValueMismatch { expected, got } => {
                write!(f, "stake-bond output-0 value {got} does not equal the bonded amount {expected}")
            }
            PalwTxError::BondOutputScriptMismatch => write!(f, "stake-bond output-0 is not the owner's P2PKH-ML-DSA script"),
            PalwTxError::ComputeChallengeHasOutputs(n) => write!(f, "compute-challenge tx must declare no outputs but has {n}"),
            PalwTxError::IneligibleVerificationScheme => {
                write!(f, "compute certificate declares a verification scheme that is not consensus-eligible")
            }
            PalwTxError::UnknownQuantizationProfile => write!(f, "compute certificate declares an unknown quantization profile"),
            PalwTxError::NoVerifierAttestations => write!(f, "compute certificate carries no verifier panel verifier"),
            PalwTxError::TooManyVerifierAttestations(n) => {
                write!(f, "compute certificate carries {n} verifier panel verifier signatures, above the permitted maximum")
            }
            PalwTxError::DuplicateVerifier => write!(f, "compute certificate lists the same verifier twice"),
            PalwTxError::ExecutorVerifiedOwnJob => write!(f, "compute certificate lists its own executor as a verifier"),
            PalwTxError::MalformedContradictionProof => {
                write!(f, "compute challenge does not carry a well-formed contradiction proof")
            }
            PalwTxError::SelfContradictoryVerdict => {
                write!(f, "compute verdict contradicts its own executor/replay receipt hashes")
            }
            PalwTxError::JobInputSize(n) => {
                write!(f, "compute commitment publishes a {n}-byte job input, which is empty or above the permitted maximum")
            }
        }
    }
}

/// Borsh-decode a current PALW path payload, mapping any decode failure (bad
/// bytes *or* trailing data — `borsh::from_slice` rejects both) to
/// [`PalwTxError::Decode`].
fn decode_palw_payload<T: BorshDeserialize>(payload: &[u8]) -> Result<T, PalwTxError> {
    borsh::from_slice::<T>(payload).map_err(|_| PalwTxError::Decode)
}

/// Stateless validation of an owner-authorized bond-unbond request.
pub fn validate_stake_unbond_payload(payload: &[u8]) -> Result<(), PalwTxError> {
    let request: StakeUnbondRequestPayload = decode_palw_payload(payload)?;
    if request.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(request.version));
    }
    if request.owner_pubkey.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(PalwTxError::InvalidPubKeyLen(request.owner_pubkey.len()));
    }
    if request.signature.len() != MLDSA87_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(request.signature.len()));
    }
    Ok(())
}

/// Extracts decodable unbond requests from accepted current-PALW transactions.
pub fn unbond_requests_from_accepted_txs(txs: &[Transaction]) -> Vec<(TransactionId, StakeUnbondRequestPayload)> {
    txs.iter()
        .filter(|tx| palw_tx_kind(&tx.subnetwork_id) == Some(PalwTxKind::StakeUnbond))
        .filter_map(|tx| borsh::from_slice::<StakeUnbondRequestPayload>(&tx.payload).ok().map(|request| (tx.id(), request)))
        .collect()
}

/// Stateless validation of a [`StakeBondPayload`] (subnetwork
/// `SUBNETWORK_ID_STAKE_BOND`): decodability, payload version, non-zero
/// bonded amount, the fixed 2592-byte ML-DSA-87 validator public-key
/// length, and the canonical identity binding `validator_pubkey_hash ==
/// validator_id_from_pubkey(validator_pubkey)` (audit H-04 — a bond may
/// not declare an overlay identity not derived from its own key). The
/// `owner_pubkey_hash`↔funding-input binding remains stateful (checked at
/// acceptance). Returns the decoded payload so the tx-level
/// [`validate_stake_bond_tx`] check can reuse it.
pub fn validate_stake_bond_payload(payload: &[u8]) -> Result<(), PalwTxError> {
    decode_and_check_stake_bond_payload(payload).map(|_| ())
}

fn decode_and_check_stake_bond_payload(payload: &[u8]) -> Result<StakeBondPayload, PalwTxError> {
    let bond: StakeBondPayload = decode_palw_payload(payload)?;
    if bond.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(bond.version));
    }
    if bond.amount == 0 {
        return Err(PalwTxError::ZeroBondAmount);
    }
    if bond.validator_pubkey.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(PalwTxError::InvalidPubKeyLen(bond.validator_pubkey.len()));
    }
    // audit H-04 / ADR-0008/0012: bind the declared overlay identity to the key.
    // `validator_id` is the canonical unkeyed BLAKE2b-512(validator_pubkey); a bond
    // must not declare a `validator_pubkey_hash` that is not derived from its own
    // key, or validator identity stops being uniquely key-derived (breaking
    // dup-detection, the validator-set commitment, and external key->id monitoring).
    // Purely intra-payload, so enforced statelessly here.
    if bond.validator_pubkey_hash != validator_id_from_pubkey(&bond.validator_pubkey) {
        return Err(PalwTxError::ValidatorPubkeyHashMismatch);
    }
    Ok(bond)
}

/// Stateless validation of a whole `StakeBond` **transaction** (ADR-0016
/// D.1 stake-lock rule, on top of [`validate_stake_bond_payload`]): its
/// output-0 (the bond outpoint, ADR-0026.1) must lock the
/// declared stake — `value == amount` and `script_public_key ==
/// p2pkh_mldsa87_spk(owner_reward_spk_payload)` (the owner's P2PKH-ML-DSA
/// address). This makes `amount` real, owner-controlled coins parked at
/// output-0 rather than a self-declared number, closing the fake-stake
/// hole and giving slashing (ADR-0026) something to consume.
///
/// Stateless (it inspects only the transaction's own output-0), so it runs
/// in the PR-10.4 isolation validator alongside the payload checks. Like
/// all PALW stateless validation it is "always-on" for
/// `StakeBond`-subnetwork txs, but inert in practice: no `StakeBond` tx is
/// submitted on any network while the overlay is dormant.
pub fn validate_stake_bond_tx(payload: &[u8], outputs: &[TransactionOutput]) -> Result<(), PalwTxError> {
    let bond = decode_and_check_stake_bond_payload(payload)?;
    let output0 = outputs.first().ok_or(PalwTxError::MissingBondOutput)?;
    if output0.value != bond.amount {
        return Err(PalwTxError::BondOutputValueMismatch { expected: bond.amount, got: output0.value });
    }
    if output0.script_public_key != p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload) {
        return Err(PalwTxError::BondOutputScriptMismatch);
    }
    Ok(())
}

// =====================================================================
// PR-10.9 (foundation): pure stake-bond lifecycle helpers.
//
// These are deliberately store- and DAG-free pure functions so they can
// be unit-tested in isolation. They are the shared building blocks for:
//   - PR-10.9b bond-store population (`stake_bond_record_from_payload`
//     when an accepted stake-bond tx is recorded), and
//   - PR-10.9c stateful tx validation (`is_bond_active_at` /
//     `effective_bond_status` to gate panel verifier/slashing txs against an
//     existing, active bond at the point-of-view DAA score).
//
// `effective_bond_status` derives the bond's status purely from its
// DAA-stamped fields (activation / unbond-request / slash height) rather
// than trusting the cached `status` field, so a single source of truth
// governs eligibility regardless of when the cached field was last
// written. ADR-0026.
// =====================================================================

/// Builds the initial [`StakeBondRecord`] for a freshly-accepted
/// [`StakeBondPayload`]. `bond_outpoint` is the outpoint of the
/// stake-bond transaction's bond output (the consensus key for the
/// `StakeBonds` store). The record starts `Pending`; the
/// `Pending → Active` transition is purely a function of
/// `activation_daa_score` (see [`effective_bond_status`]) and needs no
/// later write. `unbond_request`/`slashed_at` are set later when the
/// corresponding txs are processed.
pub fn stake_bond_record_from_payload(payload: &StakeBondPayload, bond_outpoint: TransactionOutpoint) -> StakeBondRecord {
    StakeBondRecord {
        version: payload.version,
        bond_outpoint,
        owner_pubkey_hash: payload.owner_pubkey_hash,
        validator_pubkey_hash: payload.validator_pubkey_hash,
        validator_pubkey: payload.validator_pubkey.clone(),
        amount: payload.amount,
        activation_daa_score: payload.activation_daa_score,
        // ADR-0022: set by `bond_mutations_from_accepted_txs` to the acceptance DAA
        // (the bond's true creation point); 0 here as a placeholder.
        created_daa_score: 0,
        unbonding_period_blocks: payload.unbonding_period_blocks,
        owner_reward_spk_payload: payload.owner_reward_spk_payload,
        unbond_request_daa_score: None,
        slashed_at_daa_score: None,
        status: BondStatus::Pending,
    }
}

/// The DAA score at which an unbonding bond's stake is released
/// (`unbond_request_daa_score + unbonding_period_blocks`), or `None` if
/// no unbond has been requested. `saturating_add` so a pathological
/// `u64::MAX` request height never wraps to an early release.
pub fn bond_release_daa_score(record: &StakeBondRecord) -> Option<u64> {
    record.unbond_request_daa_score.map(|u| u.saturating_add(record.unbonding_period_blocks))
}

/// Derives a bond's effective [`BondStatus`] as observed from
/// `pov_daa_score`, purely from its DAA-stamped fields (precedence:
/// slashed → unbonding → time-based activation):
///
/// 1. `slashed_at_daa_score ≤ pov` ⇒ `Slashed` (terminal).
/// 2. `unbond_request_daa_score ≤ pov` ⇒ `Unbonding` (no new
///    panel verifier signatures accepted, per ADR-0026).
/// 3. `activation_daa_score ≤ pov` ⇒ `Active`, else `Pending`.
pub fn effective_bond_status(record: &StakeBondRecord, pov_daa_score: u64) -> BondStatus {
    if record.slashed_at_daa_score.is_some_and(|s| pov_daa_score >= s) {
        return BondStatus::Slashed;
    }
    if record.unbond_request_daa_score.is_some_and(|u| pov_daa_score >= u) {
        return BondStatus::Unbonding;
    }
    if pov_daa_score >= record.activation_daa_score { BondStatus::Active } else { BondStatus::Pending }
}

/// `true` iff the bond is `Active` at `pov_daa_score` — the eligibility
/// predicate the PR-10.9c panel verifier/slashing stateful checks apply to a
/// referenced bond (`bond ∈ active_bonds`, ADR-0026).
pub fn is_bond_active_at(record: &StakeBondRecord, pov_daa_score: u64) -> bool {
    effective_bond_status(record, pov_daa_score) == BondStatus::Active
}

/// Default page size for [`paginate_stake_bonds`] when a query requests `limit == 0`.
pub const STAKE_BONDS_DEFAULT_PAGE_LIMIT: usize = 256;
/// Hard cap on [`paginate_stake_bonds`] page size, so an unbounded scan is never
/// materialized on the RPC wire.
pub const STAKE_BONDS_MAX_PAGE_LIMIT: usize = 1000;

/// kaspa-pq: apply a [`StakeBondQuery`] to a full set of bond records — the pure
/// core of [`ConsensusApi::get_stake_bonds`](crate::api::ConsensusApi::get_stake_bonds).
/// Owner and effective-status filters, deterministic ordering by
/// `(transaction_id, index)`, an exclusive `cursor`, and `limit` (0 → default,
/// clamped to [`STAKE_BONDS_MAX_PAGE_LIMIT`]) are all applied here, so the
/// store-scanning wrapper stays trivial and this logic is unit-testable.
/// `next_cursor` is `Some` iff at least one further match exists past the page.
pub fn paginate_stake_bonds(records: Vec<StakeBondRecord>, query: &StakeBondQuery, pov_daa_score: u64) -> StakeBondPage {
    let limit = if query.limit == 0 { STAKE_BONDS_DEFAULT_PAGE_LIMIT } else { query.limit.min(STAKE_BONDS_MAX_PAGE_LIMIT) };

    let mut matching: Vec<StakeBondRecord> = records
        .into_iter()
        .filter(|rec| match query.owner_pubkey_hash {
            Some(owner) => rec.owner_pubkey_hash == owner,
            None => true,
        })
        .filter(|rec| match &query.status_in {
            Some(statuses) if !statuses.is_empty() => statuses.contains(&effective_bond_status(rec, pov_daa_score)),
            _ => true,
        })
        .collect();
    // TransactionOutpoint is not `Ord`; order by (transaction_id, index) so the
    // cursor contract is deterministic and stable across nodes.
    matching.sort_by(|a, b| {
        (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
    });

    let start = match query.cursor {
        Some(cursor) => matching.partition_point(|rec| {
            (rec.bond_outpoint.transaction_id, rec.bond_outpoint.index) <= (cursor.transaction_id, cursor.index)
        }),
        None => 0,
    };
    let remaining = &matching[start..];
    let bonds: Vec<StakeBondRecord> = remaining.iter().take(limit).cloned().collect();
    let next_cursor = if remaining.len() > limit { bonds.last().map(|r| r.bond_outpoint) } else { None };
    StakeBondPage { bonds, next_cursor, pov_daa_score }
}

/// A mutation to the `StakeBonds` consensus store derived from accepted
/// PALW transactions on the selected chain (ADR-0026.4).
/// The virtual processor **applies** these for a block joining the selected
/// chain and **reverts** them for a block leaving it (reorg), exactly like
/// the UTXO set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BondMutation {
    /// A new bond created by a `StakeBondPayload` tx, keyed by its output-0
    /// outpoint (Addendum A.1). Apply = insert; revert = delete.
    Insert(TransactionOutpoint, StakeBondRecord),
    /// A bond burned by accepted `ComputeFraudProofPayload`, stamped with the
    /// accepting block's DAA score. Apply = set `slashed_at_daa_score`;
    /// revert = clear it.
    Slash(TransactionOutpoint, u64),
    /// A bond moved to `Unbonding` by an accepted `StakeUnbondRequest` (audit
    /// H-05), stamped with the accepting block's DAA score. Apply = set
    /// `unbond_request_daa_score`; revert = clear it. The stateful rule rejects
    /// unbond on a non-`Pending`/`Active` bond, so at most one applies per bond
    /// per chain (clean revert).
    ///
    /// The third field carries the **requester's claimed `validator_id`**
    /// (`validator_id_from_pubkey(payload.owner_pubkey)`) once
    /// [`PalwParams::unbond_authz_mergeset_activation_daa_score`] is reached, and
    /// `None` below it — so pre-fence behaviour stays byte-identical. When it is
    /// `Some`, apply/revert refuse to touch a record whose `owner_pubkey_hash`
    /// differs: the own-body H-05 block gate never sees a request that arrives by
    /// MERGE, so the owner binding has to be enforced here too. The condition is
    /// identical in both directions, so a reorg reverts exactly what it applied.
    Unbond(TransactionOutpoint, u64, Option<Hash64>),
}

/// Derives the ordered [`BondMutation`]s implied by a chain block's
/// **accepted** transactions (ADR-0026.4 / A.1).
///
/// Pure: decodes the PALW payloads (defensively skipping any that
/// fail to decode — a committed block's txs already passed PR-10.4 stateless
/// validation, so this is belt-and-suspenders) and maps them to store
/// mutations. The caller decides which txs count as "accepted"; this
/// function is agnostic to that selection so it stays unit-testable.
///
/// - `StakeBond` → `Insert` at `bond_outpoint = (tx.id(), 0)`.
/// - `ComputeFraudProof` → `Slash(payload.bond_outpoint, accepted_daa_score)`.
/// - retired sidecar records → nothing (they are not part of the current bond set).
///
/// Bond *activation* is **not** stamped here; it is derived at read time
/// from the payload's `activation_daa_score` (see [`effective_bond_status`]).
pub fn bond_mutations_from_accepted_txs(
    txs: &[Transaction],
    accepted_daa_score: u64,
    min_bond_amount_sompi: u64,
    unbonding_floor_blocks: u64,
    // `accepted_daa_score >= unbond_authz_mergeset_activation_daa_score`. When set, each
    // `Unbond` mutation carries the requester's claimed `validator_id` so the applier can bind it
    // to the record's `owner_pubkey_hash`; below the fence they carry `None` and apply
    // unconditionally, byte-identically to the pre-fix behaviour.
    enforce_unbond_authz: bool,
) -> Vec<BondMutation> {
    let mut muts = Vec::new();
    for tx in txs {
        match palw_tx_kind(&tx.subnetwork_id) {
            Some(PalwTxKind::StakeBond) => {
                if let Ok(payload) = borsh::from_slice::<StakeBondPayload>(&tx.payload) {
                    // Per-bond minimum stake: a bond below the network floor is NOT admitted to
                    // the registry (so it can never become Active / attest). Deterministic, so
                    // the coinbase-construction and validation passes agree.
                    if payload.amount < min_bond_amount_sompi {
                        continue;
                    }
                    let outpoint = TransactionOutpoint::new(tx.id(), 0);
                    let mut record = stake_bond_record_from_payload(&payload, outpoint);
                    // PALW activation is NON-RETROACTIVE. A bond may not declare a
                    // past `activation_daa_score` and thereby insert itself into a historical epoch's
                    // active set, which would retroactively change that epoch's bond participation score / reward
                    // denominator (the lagged-anchor denominator is evaluated at a past anchor DAA).
                    // Clamp activation up to the bond's acceptance DAA so it can only affect future
                    // epochs. Deterministic, so coinbase construction and validation agree.
                    record.activation_daa_score = record.activation_daa_score.max(accepted_daa_score);
                    // ADR-0022: the bond's exact creation point (acceptance DAA) — used to
                    // reconstruct the as-of-pruning-point bond set on a serving node.
                    record.created_daa_score = accepted_daa_score;
                    // Clamp the operator-declared unbonding window up to the network floor, so a
                    // validator cannot shorten its exit-lock (and thus its slashable window) by
                    // declaring a tiny `unbonding_period_blocks`.
                    record.unbonding_period_blocks = record.unbonding_period_blocks.max(unbonding_floor_blocks);
                    muts.push(BondMutation::Insert(outpoint, record));
                }
            }
            Some(PalwTxKind::StakeUnbond) => {
                // audit H-05: an accepted unbond request stamps the bond's unbond clock.
                //
                // The `unbond_request_authorized` block rule enforces authorization over the
                // block's OWN body only, so a request that arrives via the MERGESET reaches here
                // unchecked (incident 2026-08-07). We therefore carry the requester's claimed
                // `validator_id` on the mutation and bind it to the record's `owner_pubkey_hash`
                // in `ActiveBondView::apply`/`revert`; the caller separately drops mutations whose
                // ML-DSA-87 signature does not verify. Both layers are gated on
                // `unbond_authz_mergeset_activation_daa_score`.
                if let Ok(req) = borsh::from_slice::<StakeUnbondRequestPayload>(&tx.payload) {
                    let claimed = enforce_unbond_authz.then(|| validator_id_from_pubkey(&req.owner_pubkey));
                    muts.push(BondMutation::Unbond(req.bond_outpoint, accepted_daa_score, claimed));
                }
            }
            Some(PalwTxKind::ComputeChallenge) => {
                // PALW paper §7(b): only an **objectively provable** offence may slash at
                // acceptance, and of the four fraud kinds exactly one is.
                //
                // `ContradictoryVerification` is proved by two signatures from one verifier over
                // one certificate that cannot both be true. Nothing has to be re-executed and
                // nothing has to be looked up: `compute_challenge_genuine` reconstructs both
                // verdict digests from the payload and verifies them against the named bond's own
                // key, so a forged proof cannot reach acceptance.
                //
                // The other three are claims, not proofs. `ForgedReceipt` says "I re-ran the job
                // and got something else" — unverifiable without re-running it, which consensus
                // cannot do. `InvalidCertificate` and `FailedChallenge` need an adjudication the
                // protocol does not yet have. Slashing on an unprovable claim is worse than not
                // slashing at all: it lets any bonded party burn any other party's stake. They
                // still deny the certificate its credit (`aggregate_compute_credits` drops any
                // challenged certificate), which is the part that IS decidable from chain data.
                if let Ok(c) = borsh::from_slice::<ComputeChallengePayload>(&tx.payload)
                    && c.kind == ComputeFraudKind::ContradictoryVerification
                {
                    muts.push(BondMutation::Slash(c.target_bond_outpoint, accepted_daa_score));
                }
            }
            // A compute certificate mints PALW credit and a capability declaration only joins a
            // sortition pool — neither is a bond mutation. A future-anchor commitment is round 2 of finality: it
            // feeds the confirmation gate, not the bond set. (A future-anchor commitment that *contradicts* an
            // earlier one is slashable, but only on presented evidence, like every other
            // equivocation — never as a side effect of accepting one.)
            Some(PalwTxKind::ComputeCertificate)
            | Some(PalwTxKind::ComputeCapability)
            | Some(PalwTxKind::ComputeCommitment)
            | Some(PalwTxKind::ComputeVerdict)
            | None => {}
        }
    }
    muts
}

/// Apply one non-`Insert` [`BondMutation`] to a bond record — the single definition of the
/// stamp transition, shared by the in-memory [`ActiveBondView`] and the persisted-store path
/// (`stage_palw_bond_mutations`) so the two can never drift.
///
/// **FIRST STAMP WINS**, and that is what makes `apply`/`revert` exact inverses.
///
/// ## The bug this closes (overlay-commitment divergence class)
///
/// The previous transition overwrote unconditionally: `apply(Slash(X, d))` set
/// `slashed_at_daa_score = Some(d)` even when the bond was already slashed by an EARLIER chain
/// block, and `revert(Slash(X, d))` then cleared the field outright. So for a chain where the
/// COMMON PREFIX slashes bond X and a branch block slashes it again — reachable, because
/// `slashing_evidence_genuine` judges the bond's status at the *panel verifier's target* DAA, not at
/// the including block, so a second genuine evidence for an already-slashed bond is accepted —
/// reverting that branch RESURRECTED the bond as un-slashed:
///
/// ```text
///   prefix P slashes X at 100        → slashed_at = Some(100)
///   branch A block slashes X at 150  → slashed_at = Some(150)   (old: overwrite)
///   reorg A → B: revert A            → slashed_at = None        (old: P's slash LOST)
/// ```
///
/// A node that walked straight to B kept `Some(100)`. Two nodes then hold different bond sets for
/// byte-identical history, which changes `effective_bond_status`, the reward fan-out's rewarded
/// keys, and therefore `Header::overlay_commitment_root` — surfacing as `BadOverlayCommitment`
/// (the `[overlay-diag]` warning) and mutual block rejection. Because the per-block overlay rows
/// (`rewarded_epochs_store` / `reserve_balance_store`) are written once at first validation and
/// never recomputed, a single transient discrepancy is then frozen into every later commitment
/// that includes those blocks in its window.
///
/// With first-stamp-wins the second `Slash` is a no-op and its revert matches nothing, so the
/// prefix's stamp survives — the state at a block becomes a pure function of that block's chain,
/// independent of the traversal path that reached it. Selected-chain DAA scores strictly increase
/// (`daa_score = sp.daa_score + |DAA-eligible mergeset|`, and the selected parent is always
/// DAA-eligible), so two stamps from *different* chain blocks always carry different DAA scores
/// and `revert_bond_stamp`'s equality test cannot clear the wrong one. Two stamps inside ONE
/// block share its DAA, and reverting that block's mutations in reverse order still lands on the
/// correct end state (the first revert clears, the rest no-op).
///
/// Note this also pins the canonical value on a linear chain: a twice-slashed bond now records
/// the FIRST slash's DAA rather than the last. That is the semantically right stamp (the bond has
/// been slashed since then) and it is only reachable in the double-slash case, which had no
/// well-defined value before — it depended on each node's reorg history.
pub fn apply_bond_stamp(record: &mut StakeBondRecord, mutation: &BondMutation) {
    match mutation {
        BondMutation::Insert(..) => {}
        BondMutation::Slash(_, daa) => {
            if record.slashed_at_daa_score.is_none() {
                record.slashed_at_daa_score = Some(*daa);
                record.status = BondStatus::Slashed;
            }
        }
        BondMutation::Unbond(_, daa, claimed_owner) => {
            // Owner binding (incident 2026-08-07). The H-05 rule `unbond_request_authorized` only
            // sees a block's OWN body, but these mutations come from its ACCEPTED txs — so a
            // request that arrives by MERGE is never authorization-checked upstream. Bind it to
            // the record here, where both directions already meet. `None` = below
            // `unbond_authz_mergeset_activation_daa_score`: unconditional, as before.
            if claimed_owner.is_none_or(|c| record.owner_pubkey_hash == c) && record.unbond_request_daa_score.is_none() {
                record.unbond_request_daa_score = Some(*daa);
            }
        }
    }
}

/// Exact inverse of [`apply_bond_stamp`]: undo a stamp **only if this mutation is the one that
/// set it** (the recorded DAA matches). A stamp left by another chain block is not this
/// mutation's to remove — see [`apply_bond_stamp`] for why that distinction is load-bearing.
pub fn revert_bond_stamp(record: &mut StakeBondRecord, mutation: &BondMutation) {
    match mutation {
        BondMutation::Insert(..) => {}
        BondMutation::Slash(_, daa) => {
            if record.slashed_at_daa_score == Some(*daa) {
                record.slashed_at_daa_score = None;
                // The raw `status` field is vestigial — every consumer normalizes through
                // `effective_bond_status`, which reads only the canonical timing fields. Restoring
                // `Active` here matches the pre-existing behaviour; the normalization is what makes
                // a never-slashed and a slashed-then-reverted bond compare equal.
                record.status = BondStatus::Active;
            }
        }
        BondMutation::Unbond(_, daa, claimed_owner) => {
            // The owner condition is repeated verbatim from `apply_bond_stamp`, not implied by the
            // DAA match: two mutations inside ONE block share a DAA, so a forged request could
            // otherwise revert the real owner's stamp from the same block.
            if claimed_owner.is_none_or(|c| record.owner_pubkey_hash == c) && record.unbond_request_daa_score == Some(*daa) {
                record.unbond_request_daa_score = None;
            }
        }
    }
}

/// Per-block **active-bond view** (ADR-0026).
///
/// An in-memory snapshot of the `StakeBonds` set as-of a specific block,
/// built by composing per-block [`BondMutation`] diffs along the block's
/// selected-chain prefix — the bond analogue of the per-block UTXO view
/// (`selected_parent_utxo_view.compose(&mergeset_diff)`). Pure and
/// deterministic, so the per-block validator-reward coinbase fan-out
/// (ADR-0026) and the Model-B block-validity rule can resolve
/// `bond_outpoint → active bond record` **identically on every node**,
/// rather than reading the point-of-view-dependent virtual-commit-time
/// global store (which would chain-split — see Addendum B §B.0).
///
/// [`Self::apply`] / [`Self::revert`] mirror the virtual processor's
/// `stage_palw_bond_mutations` byte-for-byte (the persisted-store path),
/// so the in-memory view and the on-disk store can never diverge:
/// `Insert` ⇒ insert / delete; `Slash` ⇒ set / clear
/// `slashed_at_daa_score` + `status`. Bond *activation* (`Pending →
/// Active`) is **not** stored — it is derived at read time from
/// `activation_daa_score` via [`effective_bond_status`] (Addendum A.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveBondView {
    bonds: HashMap<TransactionOutpoint, StakeBondRecord>,
}

impl ActiveBondView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a view from an existing set of `(bond_outpoint, record)` pairs —
    /// used to seed the per-block walk from the virtual-tip bond set (the
    /// `StakeBonds` store snapshot at the previous sink) in `resolve_virtual`.
    /// Records are inserted verbatim (including any persisted
    /// `slashed_at_daa_score` / `status`), so the seed matches the store.
    pub fn from_records(records: impl IntoIterator<Item = (TransactionOutpoint, StakeBondRecord)>) -> Self {
        Self { bonds: records.into_iter().collect() }
    }

    /// Apply one block's `bond_diff` (mutations in tx order). Mirrors the
    /// `ChainPath.added` branch of `stage_palw_bond_mutations`; both route the
    /// stamp transitions through [`apply_bond_stamp`].
    pub fn apply(&mut self, mutations: &[BondMutation]) {
        for mutation in mutations {
            match mutation {
                BondMutation::Insert(outpoint, record) => {
                    self.bonds.insert(*outpoint, record.clone());
                }
                BondMutation::Slash(outpoint, _) | BondMutation::Unbond(outpoint, _, _) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        apply_bond_stamp(record, mutation);
                    }
                }
            }
        }
    }

    /// Revert one block's `bond_diff` (mutations in **reverse** order, so a
    /// `Slash` whose `Insert` is reverted in the same diff is handled
    /// gracefully). Mirrors the `ChainPath.removed` branch of
    /// `stage_palw_bond_mutations`; both route the stamp transitions through
    /// [`revert_bond_stamp`], which undoes a stamp only if this mutation set it.
    pub fn revert(&mut self, mutations: &[BondMutation]) {
        for mutation in mutations.iter().rev() {
            match mutation {
                BondMutation::Insert(outpoint, _) => {
                    self.bonds.remove(outpoint);
                }
                BondMutation::Slash(outpoint, _) | BondMutation::Unbond(outpoint, _, _) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        revert_bond_stamp(record, mutation);
                    }
                }
            }
        }
    }

    /// Resolve a bond that is `Active` at `pov_daa_score` (Addendum B
    /// §B.3(a)). `None` if the outpoint is absent or the bond is not
    /// `Active` at that DAA score.
    pub fn active_bond_at(&self, outpoint: &TransactionOutpoint, pov_daa_score: u64) -> Option<&StakeBondRecord> {
        let record = self.bonds.get(outpoint)?;
        is_bond_active_at(record, pov_daa_score).then_some(record)
    }

    /// Raw lookup regardless of status (diagnostics / tests).
    pub fn get(&self, outpoint: &TransactionOutpoint) -> Option<&StakeBondRecord> {
        self.bonds.get(outpoint)
    }

    /// Snapshot every bond record in the view (any status). Used by the ADR-0026
    /// reorg gate to score a *candidate* branch's bond participation score-since-common-ancestor under
    /// that branch's OWN bond set (the in-loop view advanced to the candidate) — scoring a
    /// branch under the wrong view would mis-credit it and risk wrongly accepting a reorg
    /// that abandons confirmed history. Callers gate each record by [`is_bond_active_at`].
    pub fn records(&self) -> Vec<StakeBondRecord> {
        self.bonds.values().cloned().collect()
    }

    /// Total stake of all bonds that are `Active` at `pov_daa_score`. The ADR-0026
    /// expected-stake **denominator** for the per-block validator-reward distribution
    /// ([`validator_participation_reward_outputs`]): normalising by *expected* (not included)
    /// stake is the anti-capture property — a minority earns only its proportional slice.
    /// Bounded by the active validator count.
    pub fn total_active_stake_at(&self, pov_daa_score: u64) -> u64 {
        self.bonds.values().filter(|b| is_bond_active_at(b, pov_daa_score)).fold(0u64, |acc, b| acc.saturating_add(b.amount))
    }

    pub fn len(&self) -> usize {
        self.bonds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bonds.is_empty()
    }
}

// =====================================================================

/// §5's `capability_set_root`: a commitment over the capability declarations live at a pin — the
/// pool a verifier committee is drawn from — in canonical order. The caller supplies the records
/// already filtered to the pin (liveness at the pin's DAA plus ancestry in the pin's chain);
/// this sorts and hashes, so the root is a function of the SET and never of collection order.
pub fn capability_set_root(records: &mut [ComputeCapabilityRecord]) -> Hash64 {
    records.sort_by(|a, b| {
        (a.validator_id, a.bond_outpoint.transaction_id, a.bond_outpoint.index, a.model_weights_hash, a.runtime_hash).cmp(&(
            b.validator_id,
            b.bond_outpoint.transaction_id,
            b.bond_outpoint.index,
            b.model_weights_hash,
            b.runtime_hash,
        ))
    });
    let mut hasher = Blake2bParams::new().hash_length(64).key(b"misaka-palw-capability-set-v1").to_state();
    hasher.update(&(records.len() as u64).to_le_bytes());
    for r in records {
        hasher.update(r.validator_id.as_byte_slice());
        hasher.update(r.bond_outpoint.transaction_id.as_byte_slice());
        hasher.update(&r.bond_outpoint.index.to_le_bytes());
        hasher.update(r.model_weights_hash.as_byte_slice());
        hasher.update(r.runtime_hash.as_byte_slice());
        hasher.update(r.runtime_class_id.as_byte_slice());
        hasher.update(&r.accepted_daa_score.to_le_bytes());
        hasher.update(&r.expiry_daa_score.to_le_bytes());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// kaspa-pq ADR-0022 — the keyed BLAKE2b-512 domain for the L1 header
/// `overlay_commitment_root` (distinct from `EvmCommitment64` / `EvmPayload64`).
pub const MISAKA_OVERLAY_COMMITMENT_CONTEXT: &[u8] = b"OverlayCommit64";

/// The current PALW state committed for pruned IBD at block `B`.
///
/// Only the selected-parent bond set is carried. The digest is committed in
/// `Header::overlay_commitment_root`; construction and validation derive it from
/// the same selected-parent view, so an imported snapshot cannot act as a second
/// consensus authority.
///
/// **FROZEN canonical encoding (hard fork to change):** [`Self::canonicalize`]
/// sorts every component into the order below, then [`Self::commitment_preimage`]
/// is the borsh encoding of the sorted struct. The empty snapshot (genesis / a
/// pre-bond chain) hashes to a fixed value reachable as
/// `OverlaySnapshot::default().commitment_root()`.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct OverlaySnapshot {
    /// Live bond set; canonical order = ascending by `(bond_outpoint.transaction_id, index)`.
    pub bonds: Vec<StakeBondRecord>,
}

impl OverlaySnapshot {
    /// Sorts every component into the FROZEN canonical order, in place. Idempotent.
    /// All sort keys use `Hash64`'s derived `Ord` (`TransactionId`/`BlockHash` are
    /// `Hash64`) so the order is byte-deterministic across nodes.
    pub fn canonicalize(&mut self) {
        self.bonds.sort_by(|a, b| {
            (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
        });
    }

    /// The canonical commitment preimage = borsh of the canonicalized snapshot.
    /// Canonicalizes a clone so the caller's ordering is irrelevant.
    pub fn commitment_preimage(&self) -> Vec<u8> {
        let mut canonical = self.clone();
        canonical.canonicalize();
        borsh::to_vec(&canonical).expect("OverlaySnapshot borsh serialization is infallible")
    }

    /// `overlay_commitment_root(B)` (ADR-0022) — keyed BLAKE2b-512 over the
    /// canonical preimage under [`MISAKA_OVERLAY_COMMITMENT_CONTEXT`], producing
    /// the 64-byte digest carried in `Header::overlay_commitment_root`.
    pub fn commitment_root(&self) -> Hash64 {
        blake2b_512_keyed(MISAKA_OVERLAY_COMMITMENT_CONTEXT, &self.commitment_preimage())
    }

    /// The same commitment root is returned twice for compatibility with the
    /// diagnostic tuple used by the virtual-state code. The snapshot has one
    /// consensus component: the bond set.
    pub fn component_digests(&self) -> (Hash64, Hash64) {
        let root = self.commitment_root();
        (root, root)
    }
}

impl MemSizeEstimator for OverlaySnapshot {}

/// kaspa-pq ADR-0022 — an [`OverlaySnapshot`] tagged with the pruning point it is
/// taken as-of. Persisted (singleton, `PruningPointOverlaySnapshot` prefix) at
/// pruning-advance, served to peers during their headers-proof IBD, and consulted
/// by `compute_overlay_snapshot` when its walk reaches the pruning point.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PruningPointOverlaySnapshot {
    pub pruning_point: BlockHash,
    pub snapshot: OverlaySnapshot,
}

impl MemSizeEstimator for PruningPointOverlaySnapshot {}

/// Every decodable [`ComputeCertificatePayload`] among `txs`, paired with its transaction id (the
/// id a [`ComputeChallengePayload`] names when challenging it).
pub fn compute_certificates_from_accepted_txs(txs: &[Transaction]) -> Vec<(TransactionId, ComputeCertificatePayload)> {
    let mut out = Vec::new();
    for tx in txs {
        if palw_tx_kind(&tx.subnetwork_id) == Some(PalwTxKind::ComputeCertificate)
            && let Ok(cert) = borsh::from_slice::<ComputeCertificatePayload>(&tx.payload)
        {
            out.push((tx.id(), cert));
        }
    }
    out
}

/// Every decodable [`ComputeChallengePayload`] among `txs`.
pub fn compute_challenges_from_accepted_txs(txs: &[Transaction]) -> Vec<ComputeChallengePayload> {
    compute_challenges_with_ids(txs).into_iter().map(|(_, c)| c).collect()
}

/// As [`compute_challenges_from_accepted_txs`], but paired with each challenge's own transaction
/// id — what the block-validity rule needs in order to name the offending transaction when it
/// rejects a block.
pub fn compute_challenges_with_ids(txs: &[Transaction]) -> Vec<(TransactionId, ComputeChallengePayload)> {
    let mut out = Vec::new();
    for tx in txs {
        if palw_tx_kind(&tx.subnetwork_id) == Some(PalwTxKind::ComputeChallenge)
            && let Ok(c) = borsh::from_slice::<ComputeChallengePayload>(&tx.payload)
        {
            out.push((tx.id(), c));
        }
    }
    out
}

/// Stateless validation of a [`ComputeCertificatePayload`].*?'s bytes.
///
/// Structure only — the caller still checks signatures, sortition membership, and the model table.
/// Rejects, in order: undecodable/trailing bytes, wrong version (payload or any nested part), a
/// non-eligible verification scheme, an unknown quantization discriminant, wrong ML-DSA-87 key/sig
/// lengths, an empty or over-large verifier set, a duplicated verifier, and an executor that
/// appears among its own verifiers (§6 requires the two roles be separated).
pub fn validate_compute_certificate_payload(payload: &[u8]) -> Result<(), PalwTxError> {
    let cert: ComputeCertificatePayload = borsh::from_slice(payload).map_err(|_| PalwTxError::Decode)?;
    if cert.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(cert.version));
    }
    if cert.spec.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(cert.spec.version));
    }
    if cert.receipt.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(cert.receipt.version));
    }
    if !cert.spec.verification_scheme.is_consensus_eligible() {
        return Err(PalwTxError::IneligibleVerificationScheme);
    }
    if !cert.spec.quantization.is_known() {
        return Err(PalwTxError::UnknownQuantizationProfile);
    }
    if cert.executor_signature.len() != MLDSA87_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(cert.executor_signature.len()));
    }
    Ok(())
}

/// Stateless validation of a [`ComputeVerdictPayload`]'s bytes.
///
/// The verdict must be **self-consistent**: `Confirmed` requires the verifier's replay hash to
/// equal the executor's, `Refuted` requires it to differ. A verdict that contradicts its own two
/// hashes is a lie on its face and is rejected here rather than counted and adjudicated later.
///
/// The verifier's committee membership, its bond, and its signature are stateful and are checked
/// by the credit walk when it collects the verdicts belonging to a certificate.
pub fn validate_compute_verdict_payload(payload: &[u8]) -> Result<(), PalwTxError> {
    let v: ComputeVerdictPayload = borsh::from_slice(payload).map_err(|_| PalwTxError::Decode)?;
    if v.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(v.version));
    }
    if v.signature.len() != MLDSA87_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(v.signature.len()));
    }
    if !v.is_self_consistent() {
        return Err(PalwTxError::SelfContradictoryVerdict);
    }
    Ok(())
}

/// Stateless shape of a [`FutureAnchorCommitmentPayload`] (subnetwork `FutureAnchorCommitmentPayload`).
///
/// Everything checkable without a chain: version, signature width, and the lock's internal
/// consistency. Whether the declared lock is the *true* one is a question about history and is
/// answered by the credit walk, which counts a future-anchor commitment only if its lock matches what the chain
/// shows.

pub fn compute_verdicts_from_accepted_txs(txs: &[Transaction]) -> Vec<ComputeVerdictPayload> {
    let mut out = Vec::new();
    for tx in txs {
        if palw_tx_kind(&tx.subnetwork_id) == Some(PalwTxKind::ComputeVerdict)
            && let Ok(v) = borsh::from_slice::<ComputeVerdictPayload>(&tx.payload)
        {
            out.push(v);
        }
    }
    out
}

/// A signature- and bond-verified verdict, as the credit walk collects it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeVerdictRecord {
    pub certificate_tx_id: TransactionId,
    pub payload: ComputeVerdictPayload,
    /// DAA score of the chain block that accepted the verdict. A verdict accepted BEFORE its
    /// certificate is nonsense and is dropped.
    pub accepted_daa_score: u64,
}

/// Reduce the verdicts collected for one certificate to the panel verifier set
/// [`crate::palw::compute::verify_compute_certificate`] consumes.
///
/// Enforces, in order:
/// * the verdict was accepted at or after its certificate (a verdict cannot precede the claim it
///   judges);
/// * it agrees with the executor's claimed receipt hash as the thing being judged;
/// * one verdict per verifier — the FIRST by acceptance order wins, so a verifier cannot flood
///   the chain with verdicts and have consensus pick the convenient one. A later disagreeing
///   verdict is left for the `ContradictoryVerification` challenge to slash, not silently used;
/// * the verifier is in the sortitioned committee;
/// * it names this certificate's job, so a verdict cannot be lifted between jobs that happen to
///   share a receipt hash;
/// * its [`crate::palw::compute::ReplayProof`] attests — both verdicts must show an execution that actually
///   produces the hash they report. See [`crate::palw::compute::ReplayProof`] for why a refutation needs
///   this every bit as much as a confirmation.
///
/// Bounded by [`MAX_PANEL_ATTESTATIONS`]: beyond that the extra verdicts are ignored rather
/// than growing the per-certificate verification cost without limit.
pub fn verdicts_for_certificate(
    records: &[ComputeVerdictRecord],
    certificate_tx_id: TransactionId,
    job_id: Hash64,
    executor_receipt_hash: Hash64,
    certificate_daa_score: u64,
    committee: &HashSet<Hash64>,
) -> Vec<VerifierAttestation> {
    let mut ordered: Vec<&ComputeVerdictRecord> = records
        .iter()
        .filter(|r| {
            r.certificate_tx_id == certificate_tx_id
                && r.accepted_daa_score >= certificate_daa_score
                && r.payload.job_id == job_id
                && r.payload.executor_receipt_hash == executor_receipt_hash
                && committee.contains(&r.payload.verifier_id)
                && r.payload.replay_proof.attests(r.payload.job_id, r.payload.replay_receipt_hash)
        })
        .collect();
    // Deterministic order: acceptance depth first, then verifier id as the tie-break within a
    // block, so every node picks the same "first" verdict per verifier.
    ordered.sort_by(|a, b| a.accepted_daa_score.cmp(&b.accepted_daa_score).then(a.payload.verifier_id.cmp(&b.payload.verifier_id)));
    let mut seen: HashSet<Hash64> = HashSet::new();
    let mut out = Vec::new();
    for r in ordered {
        if !seen.insert(r.payload.verifier_id) {
            continue;
        }
        out.push(r.payload.as_attestation());
        if out.len() == MAX_PANEL_ATTESTATIONS {
            break;
        }
    }
    out
}

/// Stateless validation of a [`ComputeChallengePayload`]'s bytes.
///
/// A [`ComputeFraudKind::ContradictoryVerification`] challenge must carry exactly the two
/// conflicting verdicts, from one verifier over one job, that actually contradict each other — the
/// offence is provable from the payload alone, so a challenge that does not carry the proof is
/// rejected outright rather than deferred to an adjudication step.
pub fn validate_compute_challenge_payload(payload: &[u8]) -> Result<(), PalwTxError> {
    let c: ComputeChallengePayload = borsh::from_slice(payload).map_err(|_| PalwTxError::Decode)?;
    if c.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(c.version));
    }
    if c.signature.len() != MLDSA87_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(c.signature.len()));
    }
    match c.kind {
        ComputeFraudKind::ContradictoryVerification => {
            if c.contradictory_verdicts.len() != 2 {
                return Err(PalwTxError::MalformedContradictionProof);
            }
            let (a, b) = (&c.contradictory_verdicts[0], &c.contradictory_verdicts[1]);
            if a.verifier_id != b.verifier_id {
                return Err(PalwTxError::MalformedContradictionProof);
            }
            // The slashed bond must be the contradicting verifier's own — provable here without
            // touching the certificate, so a contradiction proof can never be aimed at a third party.
            if c.target_bond_outpoint != a.bond_outpoint || a.bond_outpoint != b.bond_outpoint {
                return Err(PalwTxError::MalformedContradictionProof);
            }
            // Identical verdicts are not a contradiction; two signatures over the same claim are
            // just a duplicate.
            if a.verdict == b.verdict && a.replay_receipt_hash == b.replay_receipt_hash {
                return Err(PalwTxError::MalformedContradictionProof);
            }
            for v in &c.contradictory_verdicts {
                if v.signature.len() != MLDSA87_SIG_LEN {
                    return Err(PalwTxError::InvalidSignatureLen(v.signature.len()));
                }
            }
        }
        _ => {
            if !c.contradictory_verdicts.is_empty() {
                return Err(PalwTxError::MalformedContradictionProof);
            }
        }
    }
    Ok(())
}

/// Stateless validation of a [`ComputeCommitmentPayload`]'s bytes.
///
/// The job input is size-capped here, at the isolation stage, because it is the one unbounded
/// field in the compute overlay: every other payload is fixed-shape. An uncapped input would let
/// a commitment carry arbitrary data at overlay-transaction prices.
pub fn validate_compute_commitment_payload(payload: &[u8]) -> Result<(), PalwTxError> {
    let c: ComputeCommitmentPayload = borsh::from_slice(payload).map_err(|_| PalwTxError::Decode)?;
    if c.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(c.version));
    }
    if c.input.is_empty() || c.input.len() > MAX_JOB_INPUT_BYTES {
        return Err(PalwTxError::JobInputSize(c.input.len()));
    }
    if c.signature.len() != MLDSA87_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(c.signature.len()));
    }
    Ok(())
}

/// A signature- and bond-verified phase-1 commitment, as the credit walk collects it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeCommitmentRecord {
    pub job_id: Hash64,
    pub executor_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    /// The job's published input. Carried through so a node that was sortitioned onto this job's
    /// committee can replay it from chain data alone, and so the walk can check the certificate's
    /// spec commits to exactly these bytes.
    pub input: Vec<u8>,
    /// blue_score of the chain block that accepted the commitment — this is what fixes the
    /// future_anchor epoch, and therefore the verifier committee.
    pub accepted_blue_score: u64,
    pub accepted_daa_score: u64,
}

/// The epoch whose canonical anchor is a commitment's sortition future_anchor: the epoch **after** the
/// one that accepted the commitment.
///
/// `+1` is the whole security property. The anchor of the commitment's own epoch may already be
/// determined when the commitment is published (epochs are lagged and backed off, so the anchor
/// is an older block), which would let an executor grind `sampling_seed` against a future_anchor it can
/// already see. The next epoch's anchor cannot exist yet.
pub fn commitment_future_anchor_epoch(accepted_blue_score: u64, epoch_len_blue_score: u64) -> u64 {
    accepted_blue_score / epoch_len_blue_score.max(1) + 1
}

/// Every decodable [`ComputeCommitmentPayload`] among `txs`.
pub fn compute_commitments_from_accepted_txs(txs: &[Transaction]) -> Vec<(TransactionId, ComputeCommitmentPayload)> {
    let mut out = Vec::new();
    for tx in txs {
        if palw_tx_kind(&tx.subnetwork_id) == Some(PalwTxKind::ComputeCommitment)
            && let Ok(c) = borsh::from_slice::<ComputeCommitmentPayload>(&tx.payload)
        {
            out.push((tx.id(), c));
        }
    }
    out
}

/// Accepted capability declarations with the transaction that carried each, so a store can key by
/// it and a reorg can delete exactly what it added.
pub fn compute_capabilities_with_ids_from_accepted_txs(txs: &[Transaction]) -> Vec<(TransactionId, ComputeCapabilityPayload)> {
    let mut out = Vec::new();
    for tx in txs {
        if palw_tx_kind(&tx.subnetwork_id) == Some(PalwTxKind::ComputeCapability)
            && let Ok(c) = borsh::from_slice::<ComputeCapabilityPayload>(&tx.payload)
        {
            out.push((tx.id(), c));
        }
    }
    out
}

/// Stateless validation of a [`ComputeCapabilityPayload`]'s bytes. The signature, the bond
/// binding, the model-table membership and the expiry cap are stateful and are checked by the
/// credit walk when it builds a job's candidate pool.
pub fn validate_compute_capability_payload(payload: &[u8]) -> Result<(), PalwTxError> {
    let c: ComputeCapabilityPayload = borsh::from_slice(payload).map_err(|_| PalwTxError::Decode)?;
    if c.version != PALW_PAYLOAD_VERSION_V1 {
        return Err(PalwTxError::UnsupportedVersion(c.version));
    }
    if c.signature.len() != MLDSA87_SIG_LEN {
        return Err(PalwTxError::InvalidSignatureLen(c.signature.len()));
    }
    Ok(())
}

/// Every decodable [`ComputeCapabilityPayload`] among `txs`.
pub fn compute_capabilities_from_accepted_txs(txs: &[Transaction]) -> Vec<ComputeCapabilityPayload> {
    let mut out = Vec::new();
    for tx in txs {
        if palw_tx_kind(&tx.subnetwork_id) == Some(PalwTxKind::ComputeCapability)
            && let Ok(c) = borsh::from_slice::<ComputeCapabilityPayload>(&tx.payload)
        {
            out.push(c);
        }
    }
    out
}

/// A signature- and bond-verified capability declaration.
///
/// Persisted (`DatabaseStorePrefixes::ComputeCapabilities`) rather than re-collected per walk: it
/// outlives the credit window by orders of magnitude, so a walk-scoped copy disappears while the
/// declaration is still perfectly valid.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct ComputeCapabilityRecord {
    /// The chain block that accepted the declaration.
    ///
    /// DAA is a number like a clock, not a proof of ancestry: two branches can carry blocks at the
    /// same score, so `accepted_daa_score <= pov` admits a declaration that the point of view's own
    /// history does not contain. A candidate branch scored against the selected chain's store would
    /// then borrow the other branch's committee. This field is what the ancestry check uses.
    pub declaration_block: BlockHash,
    pub validator_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    pub model_weights_hash: Hash64,
    pub runtime_hash: Hash64,
    pub runtime_class_id: Hash64,
    /// DAA score of the chain block that accepted the declaration.
    ///
    /// Without it a declaration has no beginning, only an end, and `is_live_at` would answer "yes"
    /// for every point of view before it existed. A validator could then watch a certificate land
    /// and declare afterwards to insert itself into that certificate's already-drawn committee —
    /// or merely to enlarge the candidate pool and re-roll somebody else out of it, which silently
    /// invalidates the verdicts the real committee already published.
    pub accepted_daa_score: u64,
    /// Effective expiry: `min(declared, accepted + max_capability_validity_blocks)`.
    pub expiry_daa_score: u64,
}

impl ComputeCapabilityRecord {
    /// Whether this declaration stands at `pov_daa_score` — bounded at **both** ends, so a
    /// point of view before the declaration existed does not see it.
    pub fn is_live_at(&self, pov_daa_score: u64) -> bool {
        self.accepted_daa_score <= pov_daa_score && pov_daa_score < self.expiry_daa_score
    }

    /// Whether it covers the `(model, runtime)` profile a job names.
    pub fn covers(&self, model_weights_hash: Hash64, runtime_hash: Hash64) -> bool {
        self.model_weights_hash == model_weights_hash && self.runtime_hash == runtime_hash
    }
}

/// Fixed-size: six 64-byte digests and two scores, no owned collections, so the derived estimate
/// is exact and the store may be size-tracked.
impl MemSizeEstimator for ComputeCapabilityRecord {}

/// Reduce collected declarations to the `(validator_id, runtime_class_id)` candidate pool
/// [`crate::palw::compute::select_panel_verifiers`] draws from for one job profile.
///
/// Keeps only declarations that are live at `pov_daa_score` and cover the job's exact
/// `(h_M, h_R)`. Deduplicated by `validator_id` keeping the **latest** expiry, so a validator
/// that renewed its declaration appears once — otherwise one operator would occupy several
/// committee slots and a k-of-n audit would degrade to k-of-1.
pub fn capability_candidate_pool(
    records: &[ComputeCapabilityRecord],
    model_weights_hash: Hash64,
    runtime_hash: Hash64,
    pov_daa_score: u64,
) -> Vec<(Hash64, Hash64)> {
    let mut best: BTreeMap<Hash64, (u64, Hash64)> = BTreeMap::new();
    for r in records {
        if !r.covers(model_weights_hash, runtime_hash) || !r.is_live_at(pov_daa_score) {
            continue;
        }
        let slot = best.entry(r.validator_id).or_insert((0, r.runtime_class_id));
        if r.expiry_daa_score >= slot.0 {
            *slot = (r.expiry_daa_score, r.runtime_class_id);
        }
    }
    best.into_iter().map(|(id, (_, class))| (id, class)).collect()
}

/// One accepted certificate this node was sortitioned to audit and has not yet judged.
///
/// Everything needed to form the verdict is here, and all of it came off the chain: the spec and
/// the executor's claim from the certificate, the input from the phase-1 commitment. That is the
/// point — a verifier that had to fetch anything from the executor would be auditing a job the
/// party under audit chose what to reveal about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingComputeVerdict {
    pub certificate_tx_id: TransactionId,
    pub job_id: Hash64,
    /// The job to re-execute, exactly as the executor signed it.
    pub spec: crate::palw::compute::LlmJobSpec,
    /// The job's input bytes, from the commitment. `job_input_commitment(input)` equals
    /// `spec.input_commitment` — the walk refuses the certificate otherwise.
    pub input: Vec<u8>,
    pub executor_id: Hash64,
    /// `R_j` as the executor claimed it. The verdict is the comparison of this against what this
    /// node's own replay produces; it is never an input to that replay.
    pub executor_receipt_hash: Hash64,
    pub executor_bond_outpoint: TransactionOutpoint,
    /// DAA score of the chain block that accepted the certificate. A verdict must be accepted at
    /// or after it to count.
    pub certificate_daa_score: u64,
}

/// This node's own standing in the compute overlay — what the validator service needs to decide
/// whether to declare a capability, commit to a job, or certify one it already committed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputeStatusView {
    /// Whether the compute overlay is **running** at the sink (the shadow fence). Everything else
    /// here is idle when it is not — an operator's node has nothing to execute, audit or certify
    /// below this fence.
    pub shadow_active: bool,
    /// Whether verified compute is the **voting weight** at the sink (the weight fence). False
    /// with `shadow_active` true is the soak: the overlay is producing and policing real compute,
    /// and finality is still running on bonded stake.
    pub palw_active: bool,
    pub sink_daa_score: u64,
    /// Epoch at the sink — what a certificate must declare to be creditable in the block that
    /// accepts it.
    pub epoch: u64,
    /// Expiry of this validator's live capability declaration, if it has one. `None` means it is
    /// not in any committee draw for the profile and cannot be audited into one.
    pub capability_expiry_daa_score: Option<u64>,
    /// In-class validators (excluding this one) that declared the same profile and are
    /// active-bonded. A job needs `min_panel_confirmations` of them to mint, so an executor
    /// with fewer is doing work that cannot be credited.
    pub in_class_peer_count: usize,
    /// This validator's commitments that no certificate of its own has completed yet.
    pub open_commitments: Vec<OpenComputeCommitment>,
}

/// A phase-1 commitment of this node's that is still waiting to be certified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenComputeCommitment {
    pub commitment_tx_id: TransactionId,
    pub job_id: Hash64,
    /// The input this commitment published — the executor re-derives its spec from these bytes
    /// rather than persisting one across the commit→certify gap.
    pub input: Vec<u8>,
    pub accepted_daa_score: u64,
    /// The epoch whose canonical anchor is this job's sortition future_anchor.
    pub future_anchor_epoch: u64,
    /// Whether that epoch's anchor exists yet. Executing before it does is not wrong, but
    /// certifying before it does is: a certificate may not predate its own future_anchor.
    pub future_anchor_ready: bool,
    /// Whether the commitment has aged past `max_commitment_age_blocks` — no certificate naming
    /// it can be credited any more, so the job has to be re-committed.
    pub expired: bool,
}

/// ADR-0026 shape rule, applied to compute challenges: like a
/// [`ComputeFraudProofPayload`] tx, a challenge is a pure evidence carrier and must declare **no**
/// outputs, because consensus mints the reporter reward as a side-effect at `(challenge_tx_id, 0)`
/// and a declared output would collide with that mint.
pub fn validate_compute_challenge_tx(payload: &[u8], outputs: &[TransactionOutput]) -> Result<(), PalwTxError> {
    validate_compute_challenge_payload(payload)?;
    if !outputs.is_empty() {
        return Err(PalwTxError::ComputeChallengeHasOutputs(outputs.len()));
    }
    Ok(())
}
