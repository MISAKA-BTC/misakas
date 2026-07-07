//! kaspa-pq Phase 10/11/12: DNS Probabilistic Finality Overlay type
//! surface.
//!
//! See [ADR-0009](../../docs/adr/0009-dns-probabilistic-finality.md)
//! for the consensus design,
//! [ADR-0010](../../docs/adr/0010-validator-node-architecture.md) for
//! the in-process validator architecture, and
//! [ADR-0011](../../docs/adr/0011-validator-deployment-and-equivocation-safety.md)
//! for the single-host deployment + equivocation-safety operating
//! model. This module carries the **type surface only** that Phase
//! 10 follow-up PRs (10.4 — 10.14) will reference; consensus rule
//! implementations panic with explicit `unimplemented!()` so the
//! missing surface is loud rather than silently-zero.
//!
//! Categories:
//!
//! - **Wire payloads** (`StakeBondPayload`, `StakeAttestation`,
//!   `StakeAttestationShardPayload`, `SlashingEvidencePayload`) —
//!   what nodes commit on-chain. Bounded by `MAX_ATTESTATIONS_PER_SHARD`.
//! - **Consensus state** (`StakeBondRecord`, `ValidatorRecord`,
//!   `ValidatorSetSnapshot`, `DnsState`) — what nodes derive from the
//!   wire payloads and persist in the consensus stores defined by
//!   ADR-0010 §"Subsystem file layout".
//! - **Node-side policy** (`BlockTemplatePolicy`, `DnsParams`) —
//!   per-network knobs read at startup.
//! - **Validator-local state** (`ValidatorStatus`, `SignedEpochRecord`,
//!   `SignedEpochCheckOutcome`) — node-local surface every validator
//!   service needs (in-process or sidecar). Never on the wire; never a
//!   consensus input. See ADR-0011.
//! - **RPC view** (`DnsConfirmation`) — surface returned by the
//!   `getDnsConfirmation` method (lands in PR-10.14).
//! - **Helpers** (`validator_set_commitment`, `stake_attestation_message`,
//!   `check_signed_epoch_record`) — byte-deterministic derivations
//!   and pure-function safety checks every node / validator must
//!   agree on. Panic-stub-free; consumed by validator + verifier
//!   alike.
//!
//! Hash widths follow [ADR-0008](../../docs/adr/0008-hash64-consensus-identity.md)
//! and [ADR-0010](../../docs/adr/0010-validator-node-architecture.md)
//! §"Validator-set commitment derivation": `validator_id`,
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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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

use crate::subnets::{
    SUBNETWORK_ID_SLASHING_EVIDENCE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, SUBNETWORK_ID_STAKE_BOND, SUBNETWORK_ID_STAKE_UNBOND,
    SubnetworkId,
};
use crate::{
    BlockHash, BlueWorkType, TransactionId,
    tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionOutpoint, TransactionOutput},
};

/// 2592 bytes — matches `kaspa_txscript::MLDSA87_PK_LEN`. Repeated
/// here so this module does not have to depend on `kaspa-txscript`;
/// asserted-equal by [`tests::dns_constants_have_expected_values`].
pub const STAKE_VALIDATOR_PUBKEY_LEN: usize = 2592;

/// 4627 bytes — matches `kaspa_txscript::MLDSA87_SIG_LEN`. Same
/// re-export rationale as [`STAKE_VALIDATOR_PUBKEY_LEN`].
pub const STAKE_ATTESTATION_SIG_LEN: usize = 4627;

/// Per-block upper bound on the number of attestations a single
/// [`StakeAttestationShardPayload`] may carry. See ADR-0009 §"Why
/// partial certificates" for the mass-budget arithmetic that drove
/// this cap (a 64-validator full certificate would be ~216 KB and
/// blow out `max_block_mass`).
pub const MAX_ATTESTATIONS_PER_SHARD: usize = 16;

/// Fixed-point scale for [`StakeScore`] / [`DnsConfirmation`] integer
/// arithmetic. Always 10^9, so a "full one-vote epoch" contributes
/// exactly `STAKE_SCORE_SCALE` to the score. Keeps consensus arithmetic
/// integer-only.
pub const STAKE_SCORE_SCALE: u128 = 1_000_000_000;

/// Default DAA distance a bridge/finality-dependent producer policy may tolerate
/// from the last DNS-confirmed anchor. Each network carries the active value in
/// [`DnsParams::bridge_finality_max_staleness_daa_score`].
pub const DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE: u64 = 1_500;

/// kaspa-pq Phase 10 wire-format version of every payload struct.
/// Bumped only by a hard-fork ADR; consumers reject foreign versions.
pub const DNS_PAYLOAD_VERSION_V1: u16 = 1;

/// kaspa-pq Phase 10 ML-DSA-87 attestation signing context. Distinct from the
/// transaction context (`b"kaspa-pq-v2/tx/mldsa87"`), the address context, and
/// the sighash domain, so an attestation signature can never be replayed as any
/// of those (and vice versa). NOTE (audit L-1): the leading `-v1`/`-v2` digit is
/// a *per-domain* separation tag, not a global scheme version — the overlay
/// domains (att/takeover) keep `-v1` while the tx/address/sighash domains use
/// `-v2` (the md2 context bump). Each string is consensus-fixed; changing any of
/// them is a hard-fork (re-genesis) change.
pub const ATTESTATION_MLDSA87_CONTEXT: &[u8] = b"kaspa-pq-v1/att/mldsa87";

/// kaspa-pq Phase 10 BLAKE2b-256 domain key used when constructing
/// the attestation message that ML-DSA-87 signs over. Consumed by
/// [`stake_attestation_message`]. See ADR-0009 §"Attestation target".
pub const ATTESTATION_MESSAGE_DOMAIN: &[u8] = b"kaspa-pq-v1/stake-attestation";

/// kaspa-pq H-05 (ADR-0010 "Unbonding"): ML-DSA-87 signing context for a
/// `StakeUnbondRequest`. Distinct from the tx / attestation / takeover contexts
/// so an unbond authorization can never be replayed as any of those.
pub const UNBOND_REQUEST_CONTEXT: &[u8] = b"kaspa-pq-v1/unbond/mldsa87";

/// kaspa-pq H-05: BLAKE2b-256 domain key for the unbond-request message the
/// owner signs over (binds the authorization to the specific bond outpoint).
pub const UNBOND_REQUEST_MESSAGE_DOMAIN: &[u8] = b"kaspa-pq-v1/unbond-request";

/// kaspa-pq Phase 11 BLAKE2b-512 domain key used by
/// [`validator_set_commitment`]. Consensus-fixed and bumped only by
/// a hard-fork ADR (the `-v1` suffix is the contract). See
/// ADR-0010 §"Validator-set commitment derivation".
pub const VALIDATOR_SET_COMMITMENT_KEY: &[u8] = b"kaspa-pq-validator-set-v1";

/// kaspa-pq Phase 13 coordinated-failover domain keys (ADR-0014).
///
/// - `HOST_ID_KEY` — keys the BLAKE2b-256 over `hostname ||
///   host_boot_nonce` that produces a stable, rebuild-resistant
///   `HostId` for each validator host.
/// - `TAKEOVER_TOKEN_MESSAGE_DOMAIN` — keys the BLAKE2b-256 over
///   the takeover-token signing material (see
///   [`takeover_token_message`]).
/// - `TAKEOVER_TOKEN_CONTEXT` — ML-DSA-87 `ctx` parameter for the
///   `sign_ctx` call that produces the
///   [`TakeoverToken::signature`]. Distinct from both the
///   transaction context (`b"kaspa-pq-v2/tx/mldsa87"`) and the
///   attestation context (`b"kaspa-pq-v1/att/mldsa87"`,
///   ADR-0009 §"Attestation target") so a takeover-token
///   signature can never be replayed as a transaction or
///   attestation signature, and vice versa.
///
/// These three are consensus-irrelevant (the entire coordinated-
/// failover protocol is node-local; no on-chain surface), but
/// the `-v1` suffix is the contract — renaming auditable.
pub const HOST_ID_KEY: &[u8] = b"kaspa-pq-validator-host-id-v1";
pub const TAKEOVER_TOKEN_MESSAGE_DOMAIN: &[u8] = b"kaspa-pq-takeover-token-v1";
pub const TAKEOVER_TOKEN_CONTEXT: &[u8] = b"kaspa-pq-v1/takeover/mldsa87";

/// kaspa-pq Phase 13 remote-signer protocol (ADR-0015) — node-
/// local wire format between a validator client and a separate
/// signer process over a Unix domain socket. Versioning is
/// protocol-level (not consensus); bumped on incompatible wire
/// changes, not on type-level additions.
pub const SIGNER_PROTOCOL_VERSION: u16 = 1;

/// kaspa-pq Phase 13 BLAKE2b-512 domain key for the
/// remote-signer audit log chain (ADR-0015 §"Audit log"). Used
/// to chain `SignerAuditRecord` entries by feeding the prior
/// chain hash + the new record's Borsh bytes through this
/// keyed hasher. Tamper-detection is the cryptographic
/// guarantee — any insertion or deletion shifts the chain and
/// is detectable by a verifier walking from a known-good entry.
pub const AUDIT_LOG_CHAIN_KEY: &[u8] = b"kaspa-pq-signer-audit-v1";

/// kaspa-pq audit M-04 — ML-DSA-87 `ctx` domain separator for a
/// signer's *audit-log checkpoint* signature. The hash chain in
/// [`AUDIT_LOG_CHAIN_KEY`] is keyed with a PUBLIC key, so a host
/// with write access to `audit.log` can rewrite history and
/// recompute a consistent chain — the chain alone is not
/// tamper-EVIDENT against the host itself. A checkpoint periodically
/// SIGNS the current chain head with the validator's ML-DSA-87 key
/// (held in-process / HSM, NOT on disk); an attacker who rewrites the
/// log to a different head cannot forge a checkpoint signature over
/// that head, so the divergence is detectable by anyone holding the
/// validator public key (from the on-chain bond). Domain-separated
/// from att/unbond/takeover/tx so a checkpoint signature can never be
/// replayed as any overlay or transaction signature, and vice versa.
pub const AUDIT_CHECKPOINT_MLDSA87_CONTEXT: &[u8] = b"kaspa-pq-v1/audit-ckpt/mldsa87";

/// Capability bitflags for the [`SignerHello`] / [`SignerHelloAck`]
/// handshake (ADR-0015 §"Protocol versioning + handshake").
/// Additive — new flags can land without bumping
/// `SIGNER_PROTOCOL_VERSION`. Each constant pins a single bit
/// position.
pub const CAP_SIGN_TRANSACTION: u32 = 0x01;
pub const CAP_SIGN_ATTESTATION: u32 = 0x02;
pub const CAP_SIGN_TAKEOVER_TOKEN: u32 = 0x04;
pub const CAP_POLICY_STRICT: u32 = 0x08;
pub const CAP_AUDIT_LOG: u32 = 0x10;
pub const CAP_HSM_BACKED: u32 = 0x20;

/// Fixed-point scaled stake score. Wrapper for documentation /
/// arithmetic clarity; the underlying `u128` is the same number of
/// "stake-score units" used throughout the overlay.
#[derive(
    Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize,
)]
pub struct StakeScore(pub u128);

impl Display for StakeScore {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Pretty-print as fixed-point: `STAKE_SCORE_SCALE` units per
        // "1.0" so `1_500_000_000` displays as `1.5`.
        let whole = self.0 / STAKE_SCORE_SCALE;
        let frac = self.0 % STAKE_SCORE_SCALE;
        write!(f, "{whole}.{frac:09}")
    }
}

/// Three-stage rollout token (ADR-0009 §"Three-stage rollout").
///
/// Never appears explicitly in tx payloads; it is a view of network
/// state used by the RPC layer and by node-internal activation
/// gating. Persisted as a single byte.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum DnsRolloutStage {
    /// Phases 1–9 only; bond / shard / evidence txs are rejected.
    #[default]
    Launch = 0,
    /// StakeBond and StakeAttestationShard txs are accepted; the
    /// reorg gate is **not** enforced. Visibility-only.
    Bootstrap = 1,
    /// `total_active_stake ≥ MIN_ACTIVE_STAKE`,
    /// `active_validators ≥ MIN_ACTIVE_VALIDATORS`,
    /// `daa_score ≥ dns_activation_daa_score`. Reorg gate engages.
    Active = 2,
}

/// kaspa-pq Phase 13 (ADR-0018 §C): a read-only DNS-finality **health** signal,
/// orthogonal to [`DnsRolloutStage`]. It **never** invalidates a block — when degraded,
/// PoW/GHOSTDAG, normal txs, and PoW-confirmation all continue; only the DNS-confirmed
/// anchor stops advancing. Derived from the per-epoch included-stake fractions by
/// [`derive_dns_health`] and surfaced (later) via `getDnsConfirmation`.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum DnsHealth {
    /// Overlay not yet `Active` — there is no DNS finality to judge.
    #[default]
    DisabledBeforeActivation = 0,
    /// StakeScore advancing normally (a recent epoch met the quality floor φS).
    Active = 1,
    /// The last `degraded_stake_quality_epochs` epochs all fell below φS — sustained
    /// sub-quality inclusion. Finality stalls; the base ledger is unaffected.
    DegradedStakeQualityLow = 2,
    /// The last `degraded_stake_quality_epochs` epochs all fell below the censorship
    /// floor (near-zero inclusion — the signature of Worker attestation censorship).
    DegradedCertificateCensored = 3,
}

/// Per-bond lifecycle state stored alongside the registry entry.
/// ADR-0010 §"Validator service runtime" specifies the eligibility
/// predicate as `bond ∈ active_bonds ∧ bond ∉ unbonding_bonds ∧
/// bond ∉ slashed_bonds`; the four-state enum below makes that
/// predicate a single field comparison.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum BondStatus {
    /// Bond has been committed but `activation_daa_score` has not
    /// been reached yet; the validator cannot attest.
    #[default]
    Pending = 0,
    /// Bond is active and the validator may attest.
    Active = 1,
    /// Owner has submitted an unbond request; bond will be released
    /// after `unbonding_period_blocks`. No new attestations
    /// accepted.
    Unbonding = 2,
    /// Slashed by a `SlashingEvidencePayload`; bond is burned and
    /// the validator is removed from the active validator set.
    Slashed = 3,
}

// ---------------------------------------------------------------------
// Wire payloads (transaction-level).
// ---------------------------------------------------------------------

/// kaspa-pq Phase 10 stake-bond payload.
///
/// Carried inside a transaction with subnetwork id
/// `SUBNETWORK_ID_STAKE_BOND` (consensus rule to be added in PR-10.4).
/// The bond locks an amount of coins to a validator ML-DSA-87 key for
/// at least `unbonding_period_blocks` blocks past any later withdraw
/// request. ADR-0009 §"Long-range bound" requires
/// `unbonding_period_blocks ≥ max_reorg_horizon + evidence_window`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeBondPayload {
    pub version: u16,

    /// `BLAKE2b-512(owner_public_key)` (ADR-0008 widening). The
    /// matching address surface is the kaspa-pq P2PKH (ADR-0002)
    /// after its own Phase 9 widening.
    pub owner_pubkey_hash: Hash64,
    /// `BLAKE2b-512(validator_public_key)`.
    pub validator_pubkey_hash: Hash64,

    /// Raw 2592-byte ML-DSA-87 public key for the validator. Stored
    /// in full so attestations can be verified by any node without an
    /// out-of-band registry lookup. Validated against
    /// `validator_pubkey_hash` at consensus time.
    pub validator_pubkey: Vec<u8>,

    /// Bonded amount in sompi.
    pub amount: u64,

    /// First DAA score at which this bond's attestations contribute
    /// to `StakeScore`. Lets a freshly-bonded validator observe the
    /// network before issuing attestations.
    pub activation_daa_score: u64,

    /// Per-bond unbonding window in blocks. Consensus-validated
    /// against the network-wide `DnsParams::unbonding_period_blocks`
    /// floor.
    pub unbonding_period_blocks: u64,

    /// The owner's **declared** ML-DSA P2PKH spend payload —
    /// `BLAKE2b-512(owner_public_key)`, i.e. the 64-byte
    /// `Address { version: PubKeyHashMlDsa87 }` payload (ADR-0019 §8;
    /// widened from the former 32-byte BLAKE2b-256 form).
    /// This is the **only** field validator rewards (ADR-0013
    /// coinbase fan-out) are paid to. `owner_pubkey_hash` above is the
    /// separately-keyed 64-byte BLAKE2b-512 *identity* hash (ADR-0008)
    /// and is **not** a payable target — distinct domain-separated
    /// hashes, not interchangeable. See ADR-0013 Addendum B. A malformed
    /// value only misdirects the owner's own rewards (self-griefing), so
    /// consensus imposes no check beyond the fixed 64-byte width
    /// guaranteed by the type. Appended last to keep the borsh layout
    /// change localized (pre-activation wire change — no live bond exists).
    pub owner_reward_spk_payload: [u8; 64],
}

/// One validator attestation over a selected-chain anchor.
///
/// Many `StakeAttestation`s are batched into
/// `StakeAttestationShardPayload` for on-chain commitment. A raw
/// attestation is ~4600+100 bytes (the ML-DSA-87 signature
/// dominates).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeAttestation {
    pub version: u16,

    /// 64-byte validator identifier (ADR-0010 §"Validator-set
    /// commitment derivation"). Conventionally equal to the
    /// `validator_pubkey_hash` from the corresponding `StakeBond`.
    pub validator_id: Hash64,

    /// Refers to the transaction outpoint that created the bond. The
    /// outpoint's `transaction_id` widens to `Hash64` in the PR-9.5
    /// cascade.
    pub bond_outpoint: TransactionOutpoint,

    /// `daa_score / epoch_length_blocks`.
    pub epoch: u64,

    /// Selected-chain anchor this attestation approves.
    pub target_hash: Hash64,

    /// `daa_score` of the anchor; redundant with `target_hash` but
    /// included so an attestation can be partially-verified without a
    /// header lookup.
    pub target_daa_score: u64,

    /// Hash64 of the committee snapshot the attestation is bound to.
    /// Lets a verifier reject attestations issued under a stale
    /// validator set. Derived via [`validator_set_commitment`].
    pub validator_set_commitment: Hash64,

    /// 4627-byte ML-DSA-87 signature over the BLAKE2b-256
    /// attestation message produced by [`stake_attestation_message`]
    /// with `ATTESTATION_MLDSA87_CONTEXT` as the libcrux `ctx`
    /// parameter.
    pub signature: Vec<u8>,
}

/// Phase 10 transaction payload that commits up to
/// `MAX_ATTESTATIONS_PER_SHARD` attestations on-chain.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeAttestationShardPayload {
    pub version: u16,
    pub epoch: u64,
    pub target_hash: Hash64,
    pub target_daa_score: u64,
    pub validator_set_commitment: Hash64,

    /// All attestations in a single shard must share the
    /// `(epoch, target_hash, validator_set_commitment)` tuple above
    /// (consensus rule in PR-10.4). Bounded by
    /// `MAX_ATTESTATIONS_PER_SHARD`.
    pub attestations: Vec<StakeAttestation>,
}

/// Wrap a single [`StakeAttestation`] into a one-element
/// [`StakeAttestationShardPayload`], copying the shard-level
/// `(epoch, target_hash, target_daa_score, validator_set_commitment)` from the
/// attestation so the PR-10.4 single-anchor-per-shard invariant holds by
/// construction. This is the common in-process-validator case (one validator
/// emitting one attestation per epoch); batching multiple validators' attestations
/// into a fuller shard is an aggregator concern, not the signer's.
pub fn single_attestation_shard(attestation: StakeAttestation) -> StakeAttestationShardPayload {
    StakeAttestationShardPayload {
        version: DNS_PAYLOAD_VERSION_V1,
        epoch: attestation.epoch,
        target_hash: attestation.target_hash,
        target_daa_score: attestation.target_daa_score,
        validator_set_commitment: attestation.validator_set_commitment,
        attestations: vec![attestation],
    }
}

/// Build the subnetwork [`Transaction`] carrying a borsh-encoded
/// [`StakeAttestationShardPayload`] on `SUBNETWORK_ID_STAKE_ATTESTATION_SHARD`.
///
/// **TEST HELPER — NOT for production (audit M-02).** The transaction has no inputs/outputs, so
/// the stock `NoTxInputs` isolation rule rejects it at mempool ingestion and in blocks. It exists
/// only to exercise payload/eligibility logic in unit tests. Production validators MUST build the
/// fee-funded, signed shard via `kaspa_pq_validator_core::ValidatorKey::build_funded_shard_tx`
/// (ADR-0010 §"Validator service runtime" step 9). `#[doc(hidden)]` to keep it off the public API.
#[doc(hidden)]
pub fn stake_attestation_shard_tx(shard: &StakeAttestationShardPayload) -> Transaction {
    let payload = borsh::to_vec(shard).expect("borsh serialization of a well-formed shard is infallible");
    Transaction::new(crate::constants::TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, 0, payload)
}

/// Phase 10 transaction payload that burns a validator's bond by
/// presenting two incompatible attestations from the same
/// `(bond_outpoint, validator_id, epoch)` triple. Must be submitted
/// within `DnsParams::evidence_window_blocks` of the latest of the
/// two cited attestations.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SlashingEvidencePayload {
    pub version: u16,
    pub bond_outpoint: TransactionOutpoint,
    pub attestation_a: StakeAttestation,
    pub attestation_b: StakeAttestation,

    /// The reporter's declared 64-byte ML-DSA P2PKH spend payload
    /// (`BLAKE2b-512(reporter_public_key)`, ADR-0019 §8; widened from the
    /// former 32-byte BLAKE2b-256 form) that the slashing reporter reward
    /// is paid to (ADR-0013 Addendum C). A malformed value only misdirects
    /// the reporter's own reward, so consensus imposes no check beyond the
    /// fixed 64-byte width. Appended last (pre-activation wire change — no
    /// live evidence tx).
    pub reporter_reward_spk_payload: [u8; 64],
}

// ---------------------------------------------------------------------
// Consensus-state types (derived from wire payloads, persisted in
// the stores defined by ADR-0010 §"Subsystem file layout").
// ---------------------------------------------------------------------

/// Registry entry derived from a confirmed [`StakeBondPayload`].
///
/// Lives in `database/src/stores/stake_registry.rs` (created in
/// PR-10.5) keyed by `bond_outpoint`. Carries all fields the
/// validator-service eligibility check (ADR-0010 §"Validator service
/// runtime") needs without re-loading the original payload.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct StakeBondRecord {
    pub version: u16,

    /// Identifies the bond uniquely; matches the outpoint of the
    /// transaction that created it.
    pub bond_outpoint: TransactionOutpoint,

    pub owner_pubkey_hash: Hash64,
    pub validator_pubkey_hash: Hash64,
    pub validator_pubkey: Vec<u8>,

    pub amount: u64,
    pub activation_daa_score: u64,
    /// kaspa-pq ADR-0022: the DAA score at which the bond's creating tx was
    /// accepted on the selected chain. Unlike `activation_daa_score` (which is
    /// `max(declared, accepted)` and may be future-dated), this is the exact
    /// creation point, so a pruned node can reconstruct the bond set "as-of the
    /// pruning point" (`created_daa_score ≤ pp_daa`) without a per-block revert.
    pub created_daa_score: u64,
    pub unbonding_period_blocks: u64,

    /// Copied verbatim from [`StakeBondPayload::owner_reward_spk_payload`]:
    /// the owner's declared 64-byte ML-DSA P2PKH spend payload that
    /// ADR-0013 validator rewards are paid to (ADR-0019 §8). See ADR-0013
    /// Addendum B. `serde` needs an explicit 64-byte helper (the derive only
    /// covers `[u8; N <= 32]`); borsh derives a 64-byte array natively.
    #[serde(with = "serde_reward_payload64")]
    pub owner_reward_spk_payload: [u8; 64],

    /// DAA score at which an `Unbonding` request was submitted, or
    /// `None` if still bondable / active / slashed. Combined with
    /// `unbonding_period_blocks` it gives the release height.
    pub unbond_request_daa_score: Option<u64>,
    /// DAA score at which a `SlashingEvidencePayload` was accepted,
    /// or `None` if not slashed.
    pub slashed_at_daa_score: Option<u64>,

    pub status: BondStatus,
}

/// PR-10.4-db: the `StakeBonds` consensus store (`CachedDbAccess`) requires
/// its value to estimate its memory footprint. The store uses an
/// item-capped (`untracked`) cache policy, so the default `size_of::<Self>()`
/// estimate is unused for eviction — an empty impl mirrors `UtxoEntry`.
impl MemSizeEstimator for StakeBondRecord {}

/// kaspa-pq: filter for [`ConsensusApi::get_stake_bonds`](crate::api::ConsensusApi::get_stake_bonds),
/// the paged enumeration behind the `GetStakeBonds` RPC. The `StakeBonds` store
/// is keyed only by outpoint (no secondary owner index), so owner/status
/// filtering is a full scan + in-memory filter; the result is always bounded by
/// `limit` and walked with an outpoint `cursor`, so an unbounded set never
/// crosses the RPC boundary.
#[derive(Clone, Debug, Default)]
pub struct StakeBondQuery {
    /// Restrict to bonds whose `owner_pubkey_hash` equals this value; `None` = any owner.
    pub owner_pubkey_hash: Option<Hash64>,
    /// Restrict to bonds whose *effective* status (evaluated at the sink) is in
    /// this set; `None`/empty = any status.
    pub status_in: Option<Vec<BondStatus>>,
    /// Return only bonds ordered strictly after this outpoint (exclusive).
    /// Bonds are ordered by `(transaction_id, index)`.
    pub cursor: Option<TransactionOutpoint>,
    /// Maximum entries to return; `0` selects the server default and values
    /// above the server cap are clamped.
    pub limit: usize,
    /// Point-of-view DAA score for effective-status evaluation; `None` uses the
    /// live sink. Pin it across a `status_in`-filtered multi-page walk so the
    /// effective-status set is a consistent snapshot (a mid-walk status change
    /// would otherwise skip a bond that sorts before the cursor). Resolved in the
    /// consensus wrapper, then passed to [`paginate_stake_bonds`].
    pub pov_daa_score: Option<u64>,
}

/// kaspa-pq: one page of [`ConsensusApi::get_stake_bonds`](crate::api::ConsensusApi::get_stake_bonds).
/// Records carry their own `bond_outpoint`; `pov_daa_score` is the sink DAA
/// score the effective status was evaluated at, so the RPC layer can recompute
/// effective status consistently with the consensus-side status filter without
/// a second sink lookup.
#[derive(Clone, Debug, Default)]
pub struct StakeBondPage {
    pub bonds: Vec<StakeBondRecord>,
    /// Outpoint to pass as the next `cursor`, or `None` when this is the last page.
    pub next_cursor: Option<TransactionOutpoint>,
    pub pov_daa_score: u64,
}

/// Per-validator entry inside a [`ValidatorSetSnapshot`]. Carries the
/// minimal fields fed into [`validator_set_commitment`]:
/// `validator_id || stake_amount || activation_daa_score`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorRecord {
    pub validator_id: Hash64,
    pub stake_amount: u64,
    pub activation_daa_score: u64,
}

/// Snapshot of the active validator set at a given epoch.
///
/// The `validators` vector **must** be sorted ascending
/// by `validator_id` for [`validator_set_commitment`] to be
/// byte-deterministic across nodes; the helper sorts a clone before
/// hashing, so callers can pass in any order, but persistence stores
/// the sorted form.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorSetSnapshot {
    pub epoch: u64,
    pub validators: Vec<ValidatorRecord>,
}

/// Per-anchor DNS state surfaced by the consensus pipeline to the
/// RPC layer and to the validator service. Lives in
/// `database/src/stores/stake_score.rs` (PR-10.5) keyed by
/// `selected_chain_anchor`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct DnsState {
    pub selected_chain_anchor: Hash64,
    pub anchor_daa_score: u64,

    /// audit H-02 (true WorkDepth): the blue work accumulated SINCE the confirmable canonical
    /// anchor (`blue_work(sink) − blue_work(anchor)`), i.e. an anchor-relative confirmation DEPTH —
    /// NOT cumulative-from-genesis work. Compared against `required_work_depth` in `is_dns_confirmed`.
    pub work_depth: BlueWorkType,
    pub stake_depth: StakeScore,

    /// Latest anchor that satisfies both `work_depth >=
    /// required_work_depth` and `stake_depth >= required_stake_depth`.
    /// Equal to `selected_chain_anchor` when the anchor itself is
    /// DNS-confirmed.
    pub last_dns_confirmed_anchor: Hash64,
    pub last_dns_confirmed_anchor_daa_score: u64,

    pub rollout_stage: DnsRolloutStage,
    /// Hash64 of the validator-set snapshot at this anchor's epoch.
    /// Mirrors the `validator_set_commitment` field on attestations
    /// so the RPC layer can echo it back to clients without
    /// recomputing.
    pub validator_set_commitment: Hash64,

    /// kaspa-pq Phase 13 (ADR-0018 §C): read-only DNS-finality health for this anchor,
    /// derived once per epoch by [`derive_dns_health`] over the same bounded StakeScore
    /// window (`DisabledBeforeActivation` until the overlay reaches `Active`). Carried
    /// here so `getDnsConfirmation` can surface it without re-walking the window. **Never**
    /// a block-validity input — degraded health stalls the DNS-confirmed anchor only; PoW
    /// confirmation and the base ledger are unaffected. Appended last to keep the borsh
    /// layout change localized.
    pub health: DnsHealth,
}

// ---------------------------------------------------------------------
// Node-side policy.
// ---------------------------------------------------------------------

/// Per-network DNS consensus parameters. Stored alongside the
/// existing `consensus/core::config::params::Params` and consumed by
/// the PR-10.5 / PR-10.7 / PR-10.8 / PR-10.9 implementations.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DnsParams {
    /// DAA score at which the [`DnsRolloutStage`] gate flips from
    /// `Bootstrap` to `Active`. The other two activation conditions
    /// (`MIN_ACTIVE_STAKE`, `MIN_ACTIVE_VALIDATORS`) are checked at
    /// the activation tick.
    pub dns_activation_daa_score: u64,

    pub min_active_stake_sompi: u64,
    pub min_active_validators: u32,

    /// Per-bond minimum stake (sompi). A `StakeBond` whose `amount` is below this is **not**
    /// admitted to the bond registry at acceptance (`bond_mutations_from_accepted_txs`), so it
    /// can never become `Active` or attest — i.e. the network's minimum stake-per-validator.
    /// Distinct from `min_active_stake_sompi` (the network-wide total needed to reach the
    /// `Active` rollout stage). `0` = no per-bond floor (devnet/simnet).
    pub min_bond_amount_sompi: u64,

    pub epoch_length_blocks: u64,

    /// `cW` — minimum work-depth for history confirmation. **Intentionally `ZERO`
    /// in both current presets (audit H-03):** the two-dimensional (work × stake)
    /// finality safety lives in the v3 reorg gate (`TwoDimensionalDominance` plus
    /// `emergency_work_margin`/`emergency_stake_margin` below) — a heavier but
    /// stake-less branch cannot pass it. The confirmation predicate's WorkDepth
    /// term is retained as an optional extra buffer; a deployment that wants the
    /// confirmation itself to also require buried PoW can raise this above 0.
    pub required_work_depth: BlueWorkType,
    /// `cS` — minimum stake-depth (in [`STAKE_SCORE_SCALE`] units)
    /// for history confirmation.
    pub required_stake_depth: StakeScore,

    /// Mainnet-only: extra margin a candidate must clear on
    /// `WorkScore` to pass the two-dimensional dominance rule. PoC /
    /// testnet hard-checkpoint mode ignores this; mainnet enforces.
    pub emergency_work_margin: BlueWorkType,
    /// Mainnet-only: matching emergency margin on `StakeScore`.
    pub emergency_stake_margin: StakeScore,

    pub max_reorg_horizon_blocks: u64,
    pub evidence_window_blocks: u64,
    pub unbonding_period_blocks: u64,

    pub max_attestations_per_block: u16,
    pub max_attestation_shard_mass: u64,

    /// DAA-distance window (ADR-0009 Addendum B §B.3(c)) bounding both
    /// validator-reward *recency* and *cross-block uniqueness*: an
    /// attestation is rewardable only if `including_block.daa_score −
    /// target_daa_score ≤ reward_uniqueness_window_blocks`, and the
    /// coinbase fan-out checks `(bond, epoch)` uniqueness only against
    /// selected-chain ancestors within this same window. Because two
    /// rewardable inclusions of one `(bond, epoch)` are then both within
    /// the window of the attestation's target, they are within the window
    /// of each other — so the bounded ancestor walk is guaranteed to see
    /// the earlier reward. Keeps the per-block walk bounded (a stale
    /// attestation beyond the window is simply unrewarded, never
    /// double-rewarded).
    pub reward_uniqueness_window_blocks: u64,

    /// kaspa-pq Phase 13 (ADR-0018 §B): the **stake-event quality floor** `φS` (basis
    /// points). An epoch whose included-attestation stake fraction
    /// `included_stake / expected_active_stake` is below `φS` earns **zero** StakeScore;
    /// above it the credit is the smooth `(fraction − φS)/(1 − φS)`
    /// ([`epoch_stake_credit`]). NOT a BFT 2/3 supermajority — set `φS >` the modelled
    /// adversarial stake-inclusion rate so a minority cannot accumulate StakeScore.
    /// `φS == 0` reproduces the pre-ADR-0018 linear credit exactly.
    pub stake_event_quality_floor_bps: u16,

    /// kaspa-pq Phase 13 (ADR-0018 §C): consecutive recent epochs that must all fall below
    /// φS before [`DnsHealth`] reports a sustained degradation (shorter = reacts faster;
    /// longer = tolerates transient dips).
    pub degraded_stake_quality_epochs: u32,
    /// kaspa-pq Phase 13 (ADR-0018 §C): the near-zero included-stake fraction (basis
    /// points, `< φS`) below which sustained degradation is read as **censorship**
    /// (`DegradedCertificateCensored`) rather than low participation
    /// (`DegradedStakeQualityLow`).
    pub stake_censorship_floor_bps: u16,

    /// Validator reward-distribution parameters (ADR-0013). Consumed
    /// by the PR-10.5′-b coinbase fan-out and the PR-10.12′ slashing
    /// split. Carried here (rather than as a separate `Params` field)
    /// so it inherits `DnsParams`'s `Option` gating — the whole
    /// validator-reward track is inert wherever `dns_params` is `None`
    /// or `daa_score < dns_activation_daa_score`.
    pub reward_params: RewardParams,

    /// kaspa-pq Phase 13 (ADR-0018 §H): which reorg-dominance rule the gate enforces in
    /// the `Active` stage. Mainnet selects [`DnsReorgMode::TwoDimensionalDominance`] (a
    /// candidate that exits the DNS-confirmed prefix must out-Work **and** out-Stake
    /// canonical since their common ancestor); PoC/testnet/devnet stay
    /// [`DnsReorgMode::HardCheckpoint`] (reject any such exit — the loud testing
    /// convenience). Read by the processor's reorg gate (`dns_reorg_allows`); genesis-active
    /// (`dns_activation_daa_score` = 0 everywhere) like the rest of the overlay.
    /// Appended last to keep the borsh layout change localized.
    pub reorg_mode: DnsReorgMode,

    /// kaspa-pq Phase 13 (ADR-0018 §F staged rollout): DAA score at which the
    /// reward split reaches its full Stage-3 ratios (Worker 75 / Validator 25 /
    /// Node 0). Between [`Self::dns_activation_daa_score`] (Stage 2 bootstrap,
    /// smaller validator share) and this score the bootstrap split applies; below
    /// activation there is no carve (Stage 1: the miner takes the whole reward).
    /// Keyed on DAA (not the pov-dependent `DnsState.rollout_stage`) so the
    /// construction and validation coinbase paths pick the same stage. Appended
    /// last to keep the borsh layout change localized. `0` on every current net
    /// (GENESIS_ACTIVE + PRODUCTION) — the Stage-3 full split applies from genesis.
    pub full_reward_split_daa_score: u64,

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2 economics): the master activation fence for the
    /// post-launch validator economics — the φS-gated **quality-bonus** payout, the 4-way
    /// **slashing** distribution (reserve + victim-epoch shares), and the **security-reserve**
    /// drip. Independent of [`Self::dns_activation_daa_score`] (which already activates the base
    /// participation reward + 2-way slashing on devnet), so the v2 economics stay byte-identical
    /// until explicitly switched on. `u64::MAX` on devnet/simnet (`GENESIS_ACTIVE_DNS_PARAMS`, inert);
    /// `0` on mainnet/testnet (`PRODUCTION_DNS_PARAMS`) — the v2 economics are ACTIVE from genesis
    /// there (no genesis-block change; the per-net fence is not a genesis input). Appended last to
    /// keep the borsh layout change localized.
    pub pos_v2_activation_daa_score: u64,

    // ---- kaspa-pq DNS v3: Canonical Lagged Anchor (blue_score-coordinated epochs) ----
    // DNS attestation epochs are coordinated by header-committed `blue_score`, NOT the
    // selected-chain *index*. The index is store-local: an archival node numbers from genesis,
    // an IBD node from its pruning point, so the same block gets different indices and
    // `index / L` would split StakeScore permanently between archival and IBD-synced nodes.
    // blue_score is consensus-validated, strictly monotonic on the selected-parent chain, and
    // pruning-invariant, so all nodes agree on a block's epoch.
    /// DNS attestation epoch length in blue_score units. `epoch(b) = blue_score(b) / L`.
    pub attestation_epoch_length_blue_score: u64,
    /// blue_score the tip must advance past an epoch's end before that epoch is "ready" to
    /// attest (absorbs selected-chain churn so honest validators converge on one lagged anchor).
    pub attestation_lag_blue_score: u64,
    /// blue_score backoff below an epoch's end for the canonical-anchor cutoff:
    /// `anchor(E) = latest selected-chain ancestor with blue_score <= epoch_end(E) - backoff`.
    pub attestation_anchor_backoff_blue_score: u64,
    /// How far back (blue_score) the StakeScore / canonical-anchor walk scans from the tip; must
    /// cover `required_stake_depth` epochs + lag + grace (replaces reusing
    /// `max_reorg_horizon_blocks` for the stake walk).
    pub stake_score_window_blue_score: u64,

    /// kaspa-pq ADR-0018 §F bridge wiring: DAA score at/after which an accepted L1 tx that
    /// CREATES ≥1 `EVM_DEPOSIT_LOCK` output (ADR-0020 §9.2 — recognised by the same
    /// `parse_evm_deposit_lock` check the claim path uses) has its whole fee classified as a
    /// **DNS-finality fee** (validator-primary [`split_finality_fees`], 75/25) instead of a
    /// normal-tx fee (90/10). Bridge txs are the L1 point where EVM-lane value most depends on
    /// the validators' `finalized` head, so they fund the §E pool at the finality ratio. The
    /// classification is DOUBLY gated at fee ACCUMULATION (`calculate_utxo_state` — shared by
    /// the coinbase construction and validation paths, so c==v holds structurally): this fence
    /// AND the net's `evm_activation_daa_score` (lock OUTPUTS are consensus-legal everywhere,
    /// but the bridge only exists on an EVM-active net — without the second gate a miner on an
    /// EVM-inert net could self-include a never-claimable lock tx to reroute fees). Below
    /// either fence, `BlockRewardData::finality_fees` stays 0 and every split is byte-identical
    /// to the pre-wiring math. `0` on every current preset (live wherever the EVM lane is —
    /// testnet/devnet today; mainnet/simnet stay inert via their EVM gate). Appended last to
    /// keep the borsh layout change localized.
    pub finality_fee_activation_daa_score: u64,

    /// kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): DAA score at which the bond
    /// spend-gate moves from the legacy own-body REJECT check to the acceptance-time SKIP check.
    ///
    /// The legacy [`crate::...bond_spend_gate`] scans only a chain block's OWN body against the
    /// selected-parent bond view, so it misses a spend of a Pending/Active bond's locked output-0
    /// that rides in a MERGE-BLUE block of the chain block's mergeset (those txs are accepted by
    /// `calculate_utxo_state` with no bond check). Below this fence that legacy gate runs unchanged.
    /// At/above it, per-tx UTXO validation rejects (so the acceptance loop SKIPS, not rejects-the-
    /// block — avoiding a liveness wedge) any accepted mergeset tx that spends a non-releasable bond,
    /// evaluated against the post-acceptance bond view (selected parent + this mergeset's fresh bond
    /// inserts). Construction == validation because `calculate_utxo_state` is the single shared site.
    ///
    /// `u64::MAX` (inert) on every current preset, so today's behavior is byte-identical (the legacy
    /// own-body gate). Activation is a coordinated, hard-forking tightening (it makes some currently-
    /// valid blocks' merge-blue bond-spends un-accepted) and must be deployed in lockstep across the
    /// mesh. NOT a genesis-block input; appended last to keep the borsh layout change append-only.
    pub bond_spend_gate_mergeset_activation_daa_score: u64,

    /// kaspa-pq DNS-finality optional hard inclusion: DAA score at which ready, under-certified
    /// attestation epochs become block-mandatory. At/after this fence, a block whose selected parent
    /// already has an active validator set and a ready canonical epoch may not advance the selected
    /// chain unless the selected-parent chain plus this block's body reaches
    /// `stake_event_quality_floor_bps` for the oldest deficient ready epoch.
    ///
    /// This is a hard-forking liveness trade-off: it fully blocks miner attestation censorship at
    /// consensus level, but if validators do not produce enough signatures the base ledger stops
    /// instead of merely degrading DNS finality. Shipped presets keep this at `u64::MAX` so
    /// attestation is a finality / reward / health signal rather than a base-ledger validity gate;
    /// tests or private/research nets can lower it deliberately to exercise hard inclusion.
    pub mandatory_attestation_inclusion_daa_score: u64,

    /// Maximum DAA distance a bridge/finality-dependent producer policy may tolerate from the
    /// last DNS-confirmed anchor. This is a per-network knob because using it as a block-validity
    /// rule would be a hard-forking consensus decision; current shipped code uses it only to pause
    /// local finality-dependent production/RPC flows while the base ledger keeps advancing.
    pub bridge_finality_max_staleness_daa_score: u64,
}

/// kaspa-pq DNS v3 — the canonical, lagged, blue_score-coordinated epoch anchor that the
/// signer, verifier, reward path, and reorg gate all derive identically. `anchor_hash` is a
/// block hash (`Hash64`); the wire attestation's `target_hash` must equal it, and the verifier
/// must additionally confirm `header(target_hash).blue_score == anchor_blue_score` and
/// `header(target_hash).daa_score == anchor_daa_score`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalLaggedEpochAnchor {
    pub epoch: u64,
    pub epoch_start_blue_score: u64,
    pub epoch_end_blue_score: u64,
    pub cutoff_blue_score: u64,
    pub anchor_hash: Hash64,
    pub anchor_blue_score: u64,
    pub anchor_daa_score: u64,
    /// `anchor(E) == anchor(E-1)` — a sparse / jumpy selected chain made this epoch reuse the
    /// previous epoch's anchor. Such an epoch earns NO new StakeScore / reward credit (a
    /// consensus rule), so the same history state cannot be credited under multiple epoch ids.
    pub duplicate_of_previous_anchor: bool,
}

/// blue_score at which epoch `E` begins: `E * L`.
pub fn epoch_start_blue_score(epoch: u64, epoch_len_blue_score: u64) -> u64 {
    epoch.saturating_mul(epoch_len_blue_score.max(1))
}

/// blue_score at which epoch `E` ends (inclusive): `(E + 1) * L - 1`.
pub fn epoch_end_blue_score(epoch: u64, epoch_len_blue_score: u64) -> u64 {
    epoch_start_blue_score(epoch.saturating_add(1), epoch_len_blue_score).saturating_sub(1)
}

/// Canonical-anchor cutoff for epoch `E`: `epoch_end(E) - backoff`.
pub fn anchor_cutoff_blue_score(epoch: u64, epoch_len_blue_score: u64, backoff_blue_score: u64) -> u64 {
    epoch_end_blue_score(epoch, epoch_len_blue_score).saturating_sub(backoff_blue_score)
}

/// The latest epoch "ready" to attest given the current tip blue_score, or `None` if no epoch
/// has buried by `lag` yet. Epoch `E` is ready iff `tip_blue_score >= epoch_end(E) + lag`.
pub fn ready_epoch_from_tip_blue_score(tip_blue_score: u64, epoch_len_blue_score: u64, lag_blue_score: u64) -> Option<u64> {
    let epoch_len = epoch_len_blue_score.max(1);
    let safe = tip_blue_score.checked_sub(lag_blue_score)?;
    let completed = safe.checked_add(1)? / epoch_len;
    if completed == 0 { None } else { Some(completed - 1) }
}

/// Pure core of canonical-anchor selection (testable without a store). `ancestors` is the tip's
/// selected-parent chain, tip-first, each `(hash, blue_score, daa_score)` with blue_score
/// strictly decreasing; it must reach down to at least `anchor_cutoff(E-1)`. Returns the
/// canonical anchor for `epoch` — the most-recent ancestor with `blue_score <= anchor_cutoff(E)`
/// (NOT an in-epoch block, so a sparse / jumpy selected chain never leaves an epoch without an
/// anchor) — and flags `duplicate_of_previous_anchor` when that same block is also `anchor(E-1)`.
/// `None` only if the window doesn't reach the cutoff (epoch not yet buried, or walk too short).
pub fn canonical_lagged_epoch_anchor(
    epoch: u64,
    epoch_len_blue_score: u64,
    backoff_blue_score: u64,
    ancestors: &[(Hash64, u64, u64)],
) -> Option<CanonicalLaggedEpochAnchor> {
    let epoch_len = epoch_len_blue_score.max(1);
    let cutoff = anchor_cutoff_blue_score(epoch, epoch_len, backoff_blue_score);
    let (anchor_hash, anchor_blue_score, anchor_daa_score) = *ancestors.iter().find(|(_, bs, _)| *bs <= cutoff)?;
    let duplicate_of_previous_anchor = if epoch == 0 {
        false
    } else {
        let prev_cutoff = anchor_cutoff_blue_score(epoch - 1, epoch_len, backoff_blue_score);
        ancestors.iter().find(|(_, bs, _)| *bs <= prev_cutoff).map(|(h, _, _)| *h) == Some(anchor_hash)
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

impl DnsParams {
    /// kaspa-pq DNS v3: are the blue_score canonical-anchor parameters self-consistent?
    /// The reorg gate only engages in the `Active` stage, where finality depends entirely on
    /// these; a misconfiguration (e.g. a zero epoch length, or a stake-score window too short
    /// to cover the creditable epochs + their lag) would make the canonical anchor / StakeScore
    /// ill-defined. `update_dns_state` refuses to enter `Active` unless this holds, so an
    /// invalid v3 config fails safe (the gate stays dormant) rather than splitting finality.
    ///
    /// Invariants:
    /// - `attestation_epoch_length_blue_score >= 1` (the epoch divisor).
    /// - `attestation_lag_blue_score >= 1` (an epoch must bury before it is attestable, else
    ///   honest validators never converge off the churning tip).
    /// - `attestation_anchor_backoff_blue_score < attestation_epoch_length_blue_score` (the
    ///   cutoff `epoch_end - backoff` stays within the epoch's own span).
    /// - `stake_score_window_blue_score >= max(2, required_stake_depth/SCALE)·L + lag + backoff`
    ///   — the walk must reach back over every creditable epoch (`required_stake_depth` epochs;
    ///   StakeScore accrues `STAKE_SCORE_SCALE` units per fully-participated epoch) plus the lag
    ///   and backoff, and over at least the PREVIOUS ready epoch's cutoff so the duplicate-anchor
    ///   flag stays decidable (audit M-05 — the depth term was previously only documented, not
    ///   enforced).
    pub fn dns_v3_params_consistent(&self) -> bool {
        let l = self.attestation_epoch_length_blue_score;
        let lag = self.attestation_lag_blue_score;
        let backoff = self.attestation_anchor_backoff_blue_score;
        let window = self.stake_score_window_blue_score;
        // audit M-05: cover the full creditable horizon, not just 2 epochs.
        let depth_epochs = ((self.required_stake_depth.0 / STAKE_SCORE_SCALE) as u64).max(2);
        let needed = depth_epochs.saturating_mul(l).saturating_add(lag).saturating_add(backoff);
        l >= 1 && lag >= 1 && backoff < l && window >= needed
    }

    /// ADR-0018 §F staged reward rollout — the effective fee/subsidy split for a
    /// block at `daa_score`, selected deterministically from the score (NOT the
    /// node-local `DnsState.rollout_stage`, which is pov-dependent and would split
    /// the chain). `None` below [`Self::dns_activation_daa_score`] (Stage 1: no
    /// carve — the miner takes the whole subsidy + fees, the pre-overlay behavior).
    /// Between activation and [`Self::full_reward_split_daa_score`] it is the
    /// bootstrap split (Stage 2, smaller validator share); at/after it, the full
    /// split (Stage 3). Both the coinbase carve and `coinbase_validator_pool`
    /// consume the returned params, so construction and validation agree. Active on
    /// every current network (`dns_activation_daa_score = 0`).
    pub fn reward_fee_split(&self, daa_score: u64) -> Option<&FeeSplitParams> {
        if daa_score < self.dns_activation_daa_score {
            None
        } else if daa_score >= self.full_reward_split_daa_score {
            Some(&self.reward_params.fee_split)
        } else {
            Some(&self.reward_params.fee_split_bootstrap)
        }
    }
}

/// Validator reward-distribution parameters (ADR-0013).
///
/// Three fields: the per-attestation flat reward paid into a
/// new validator-side inflation track, the basis-points fraction
/// of any slashed bond that goes to the reporter, and a defensive
/// per-block cap on the total validator-side coinbase outflow.
///
/// Lives alongside [`DnsParams`] and is consumed by
/// `consensus/src/processes/coinbase.rs` (PR-10.5′) for the
/// coinbase fan-out, and by the slashing side-effect (ADR-0016 §D.4)
/// for the equivocation-slashing reporter reward. The distribution
/// helper takes an optional reward floor as a separate argument
/// (`min`-cap rule per ADR-0013 §"Slashing distribution").
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RewardParams {
    /// Flat per-included-attestation reward. ADR-0013 makes this
    /// flat (not stake-proportional) on purpose: every active
    /// validator attests each epoch (ADR-0017), so a flat reward
    /// gives every staked sompi a uniform expected APY regardless
    /// of validator size.
    pub per_attestation_reward_sompi: u64,

    /// Basis points (10000 = 100%) of any slashed bond paid to
    /// the slashing reporter. Mainnet recommendation: 1000 bps
    /// = 10%. Applies to equivocation slashes
    /// ([`SlashingEvidencePayload`]); a distribution helper may
    /// additionally `min`-cap the reward at a caller-supplied floor
    /// (ADR-0013 §"Slashing distribution").
    pub slashing_reporter_reward_bps: u16,

    /// Hard cap on the per-block validator-side coinbase outflow.
    /// Defensive — `per_attestation_reward_sompi ×
    /// max_attestations_per_block` should never exceed this; if
    /// it does, the consensus rule prefers the cap and refunds
    /// the difference rather than overflowing into the coinbase
    /// accumulator. See ADR-0013 §"Inflation cap".
    pub max_validator_inflation_per_block_sompi: u64,

    /// kaspa-pq Phase 13 (ADR-0018 §E): basis-points share of the per-epoch validator
    /// pool routed to the **participation** sub-pool (paid proportionally to every
    /// included validator via [`validator_participation_reward`]). The remainder funds
    /// the **quality-bonus** sub-pool. Recommended `7000` (70%). Appended last to keep
    /// the borsh layout change localized (pre-activation; no live reward exists).
    pub validator_participation_bps: u16,
    /// kaspa-pq Phase 13 (ADR-0018 §E): basis-points share of the per-epoch validator
    /// pool routed to the **quality-bonus** sub-pool (paid via [`validator_quality_bonus`]
    /// only when the epoch's included fraction met φS). Recommended `3000` (30%);
    /// `validator_participation_bps + validator_quality_bonus_bps` should equal `10_000`.
    /// [`split_validator_pool`] takes the remainder, so any rounding dust lands in the
    /// bonus pool rather than being lost.
    pub validator_quality_bonus_bps: u16,
    /// kaspa-pq Phase 13 (ADR-0018 §D): fixed sompi bonus paid to the Worker on the block
    /// that first pushes an epoch's included fraction from `< φS` to `≥ φS` (the
    /// "quality-gate bonus" — an economic nudge to complete an epoch, **not** a
    /// certificate). Consumed by [`worker_inclusion_bounty`]. Placeholder; calibrated
    /// pre-mainnet.
    pub quality_gate_bonus_sompi: u64,
    /// kaspa-pq Phase 13 (ADR-0018 §D): Worker inclusion-bounty urgency multiplier in
    /// [`STAKE_SCORE_SCALE`] fixed-point (`STAKE_SCORE_SCALE` = 1.0×, no boost). Lets a
    /// later policy scale the bounty up as an epoch ages without inclusion; the inert
    /// default is `STAKE_SCORE_SCALE`. Consumed by [`worker_inclusion_bounty`].
    pub worker_urgency_multiplier_scaled: u64,

    /// kaspa-pq Phase 13 (ADR-0018 §F): the Worker / Validator / Service basis-point
    /// split ratios for the block subsidy and the two fee classes. These **size** the
    /// §D/§E pools ([`SubsidySplit::worker_inclusion_sompi`] is the §D pool,
    /// [`SubsidySplit::validator_sompi`] the §E pool) and the Service reserve. Appended
    /// last to keep the borsh layout change localized (pre-activation; no live reward).
    pub fee_split: FeeSplitParams,

    /// kaspa-pq Phase 13 (ADR-0018 §F staged rollout): the Stage-2 *bootstrap*
    /// split (smaller validator share, e.g. subsidy 90/10/0) applied between
    /// `DnsParams::dns_activation_daa_score` and
    /// `DnsParams::full_reward_split_daa_score`; [`Self::fee_split`] is the final
    /// Stage-3 split. Selected by [`DnsParams::reward_fee_split`]. Appended last to
    /// keep the borsh layout change localized (pre-activation; no live reward).
    pub fee_split_bootstrap: FeeSplitParams,

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): basis-points share of a slashed bond routed to the
    /// **security-reserve** pool (the long-term validator-security budget 原資). Gated by
    /// [`DnsParams::pos_v2_activation_daa_score`]; `0` (inert) until activation. Together with
    /// the reporter + victim shares, `reporter + reserve + victim ≤ 10_000`; the remainder burns.
    pub security_reserve_bps: u16,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): basis-points share of a slashed bond routed to the
    /// **victim-epoch pool** — compensation distributed to the honest (non-slashed) validators
    /// who participated in the slashed epoch. Gated by the v2 fence; `0` (inert) until activation.
    pub victim_epoch_pool_bps: u16,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): per-epoch cap (sompi) on the **security-reserve drip**
    /// released into the validator participation pool at epoch finalization. Gated by the v2
    /// fence; `0` (inert) until activation.
    pub reserve_drip_per_epoch_cap_sompi: u64,
}

/// Outcome of [`compute_attestation_reward_payouts`] — the per-block
/// validator-side coinbase payout pair.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AttestationRewardPayout {
    /// Total sompi that flows into validator-side coinbase outputs
    /// for this block (capped at
    /// `RewardParams::max_validator_inflation_per_block_sompi`).
    pub total_payout_sompi: u64,
    /// Sompi withheld by the per-block cap. Non-zero only when
    /// `per_attestation_reward × count` exceeded the cap. Should
    /// never happen under correct parameterisation; the field is
    /// surfaced so a future audit / monitor can flag the
    /// misconfiguration.
    pub refunded_sompi: u64,
}

/// Outcome of [`compute_slashing_distribution`] — how a slashed bond splits across
/// the ADR-0018 "本格版" (PoS-v2) **4 ways** (priority reporter → reserve → victim →
/// burn; the four fields sum to `slashed_amount_sompi` exactly). The reserve + victim
/// shares are `0` until the v2 fence opens (`security_reserve_bps = victim_epoch_pool_bps
/// = 0`), so the split degenerates to the pre-v2 2-way reporter + burn and is byte-identical.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SlashingDistribution {
    /// Sompi paid to whoever submitted the slashing evidence (minted at `(slashing_tx_id, 0)`).
    pub reporter_reward_sompi: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): sompi routed to the **security-reserve** pool (the
    /// long-term validator-security budget 原資). Accrued to the reserve pool (Phase 4); `0` until
    /// `pos_v2_activation`. Until the pool exists it is unminted (≡ burn).
    pub security_reserve_sompi: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): sompi routed to the **victim-epoch pool** — compensation
    /// minted to the honest (non-slashed) validators who participated in the slashed validator's
    /// epoch (minted at `(slashing_tx_id, 2..)`). `0` until `pos_v2_activation`.
    pub victim_epoch_pool_sompi: u64,
    /// Sompi removed from active supply. Mechanism is a PR-10.12
    /// implementation detail (zero-script_public_key sink or
    /// inflation-accumulator decrement); either way the value
    /// leaves circulation.
    pub burned_sompi: u64,
}

/// Miner-side block-template policy. See ADR-0010 §"Block template
/// policy" for the reservation algorithm. Consumed by the
/// block-template builder once PR-10.11 lands.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockTemplatePolicy {
    /// Upper bound on attestations the miner is willing to package
    /// into a single block. ADR-0009 fixes the consensus ceiling at
    /// [`MAX_ATTESTATIONS_PER_SHARD`]; this field lets a miner choose
    /// a stricter local cap (for benchmarking or staged rollout)
    /// without changing consensus.
    pub max_attestations_per_block: u16,
    /// Mass budget reserved for `StakeAttestationShardPayload` txs.
    pub max_attestation_shard_mass: u64,
    /// Mass budget that must remain available for normal user txs,
    /// to guarantee a high-attestation epoch cannot starve them.
    pub reserve_mass_for_normal_txs: u64,
}

// ---------------------------------------------------------------------
// RPC view.
// ---------------------------------------------------------------------

/// RPC view returned by the `getDnsConfirmation` method (added in
/// PR-10.14). Surfaces both the PoW-only confirmation level and the
/// DNS-augmented one so callers can choose which to trust.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DnsConfirmation {
    pub block_hash: Hash64,

    pub work_depth: BlueWorkType,
    pub required_work_depth: BlueWorkType,

    pub stake_depth: StakeScore,
    pub required_stake_depth: StakeScore,

    pub pow_confirmed: bool,
    pub dns_confirmed: bool,

    pub rollout_stage: DnsRolloutStage,
    pub expected_dns_confirmation_seconds: u64,

    /// Free-text fields for risk-bound notes. Per ADR-0009
    /// §"Public-claim discipline", consumers must read these
    /// alongside the boolean confirmation flags rather than
    /// interpreting reorg probability as a joint product.
    pub work_reorg_risk_upper_bound: String,
    pub stake_reorg_risk_upper_bound: String,
    pub dns_reorg_risk_conservative_bound: String,
    pub note: String,

    /// kaspa-pq Phase 13 (ADR-0018 §C): the DNS-finality health signal for `block_hash`
    /// (mirrors [`DnsState::health`]). Read-only liveness — `DegradedStakeQualityLow` /
    /// `DegradedCertificateCensored` mean the DNS-confirmed anchor has stopped advancing,
    /// **not** that any block is invalid; `pow_confirmed` is unaffected. Appended last.
    pub health: DnsHealth,

    /// audit M-01: the LAST DNS-confirmed canonical lagged anchor — the actual, stable finality
    /// point — and its DAA score. Distinct from `block_hash`, which is the (pov-dependent, every-block)
    /// selected-chain anchor (the sink). Explorers/exchanges MUST treat THIS as the DNS-final point,
    /// not `block_hash`. `Hash64::default()` (and score 0) until an anchor is first confirmed.
    pub last_dns_confirmed_anchor: Hash64,
    pub last_dns_confirmed_anchor_daa_score: u64,
}

/// Per-epoch active-validator-set view surfaced by the consensus pipeline to the
/// in-process validator service (and the `getValidatorStatus` RPC).
///
/// Computed deterministically from the current sink DAA score and the stake-bond
/// store: under ADR-0017 every active-bond validator attests, so `members` is the
/// full active set for `epoch`, canonical (`validator_id`-sorted). A validator is
/// eligible to attest iff its `validator_id` appears in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveValidatorSet {
    /// Epoch this set governs (`= pov_daa_score / epoch_length_blocks`).
    pub epoch: u64,
    /// Sink DAA score the active set was evaluated at (point of view).
    pub pov_daa_score: u64,
    /// Number of active validators at `pov_daa_score` (`== members.len()`).
    pub active_validator_count: usize,
    /// The active validators, sorted ascending by `validator_id`.
    pub members: Vec<Hash64>,
}

/// One active validator that can still contribute toward a mandatory attestation
/// deficit for a specific canonical epoch/anchor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MandatoryAttestationValidator {
    pub bond_outpoint: TransactionOutpoint,
    pub validator_id: Hash64,
    pub stake_sompi: u64,
}

/// A `(bond, validator, epoch)` key already credited by the selected-parent
/// chain for a mandatory attestation deficit.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MandatoryAttestationContributionKey {
    pub bond_outpoint: TransactionOutpoint,
    pub validator_id: Hash64,
    pub epoch: u64,
}

/// Consensus snapshot used by mining to prioritize attestation shards that can
/// actually clear the hard mandatory floor for the current template snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MandatoryAttestationDeficit {
    pub epoch: u64,
    pub target_hash: Hash64,
    pub target_daa_score: u64,
    pub validator_set_commitment: Hash64,
    /// Stake already credited before this template's body selection. In the legacy diagnostic API
    /// this is selected-parent-chain stake only; in the template-exact selector snapshot it also
    /// includes candidate accepted transactions from the virtual state.
    pub pre_body_included_stake: u64,
    pub expected_stake: u64,
    pub required_stake: u64,
    pub required_stake_delta: u64,
    pub quality_floor_bps: u16,
    pub already_contributed: Vec<MandatoryAttestationContributionKey>,
    pub active_validators: Vec<MandatoryAttestationValidator>,
}

/// Read-only liveness/monitoring view for liveness-first networks where mandatory attestation
/// inclusion is not a base-ledger validity rule. Unlike [`MandatoryAttestationDeficit`], this is
/// returned even when the hard mandatory fence is inert, so operators can see which ready epochs
/// are below the StakeScore quality floor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttestationQualityDeficit {
    pub epoch: u64,
    pub target_hash: Hash64,
    pub target_daa_score: u64,
    pub included_stake: u64,
    pub expected_stake: u64,
    pub required_stake: u64,
    pub required_stake_delta: u64,
    pub quality_floor_bps: u16,
    pub health: DnsHealth,
}

/// Mass-capacity liveness check for activating the hard mandatory attestation gate. It answers:
/// "Given the active stake distribution and quality floor, can a single block
/// carry enough attestation shards to reach the floor?" A `false` result keeps the gate dormant;
/// block validation must not reject an otherwise valid block solely because this invariant is false.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MandatoryAttestationMassCapacity {
    pub expected_stake: u64,
    pub included_stake: u64,
    pub required_stake: u64,
    pub required_stake_delta: u64,
    pub required_validator_count: usize,
    pub required_shard_count: u64,
    pub max_shard_count_by_mass: u64,
    pub required_mass: u64,
    pub max_block_mass: u64,
    pub fits: bool,
}

/// Everything the in-process validator service needs to issue one stake
/// attestation for the current epoch, assembled by the consensus pipeline so the
/// network-, active-set-, and target-binding match the verifier (`virtual_processor`)
/// byte-for-byte. The service's only remaining job is to sign [`Self::message`]
/// under [`ATTESTATION_MLDSA87_CONTEXT`] with its ML-DSA-87 key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorAttestationTarget {
    pub epoch: u64,
    /// Selected-chain anchor (sink) the attestation approves.
    pub target_hash: Hash64,
    pub target_daa_score: u64,
    /// Commitment over the epoch committee — the snapshot the attestation binds to.
    pub validator_set_commitment: Hash64,
    /// Ready-to-sign 32-byte digest: `stake_attestation_message(genesis_hash, epoch,
    /// target_hash, target_daa_score, validator_set_commitment, bond_outpoint)`.
    pub message: Hash,
}

// ---------------------------------------------------------------------
// Byte-deterministic derivations.
// ---------------------------------------------------------------------

/// Derive a validator's overlay identity (`validator_id`, equal to its
/// `validator_pubkey_hash`) from its ML-DSA-87 public key, per ADR-0008
/// §"Hash64 consensus identity" and ADR-0012 (`validator_id ==
/// BLAKE2b-512(validator_pubkey)`):
///
/// ```text
/// validator_id = BLAKE2b-512(validator_pubkey)   // unkeyed, 64-byte output
/// ```
///
/// This is the **canonical** derivation and the single source of truth for
/// the overlay: the in-process validator service uses it to advertise its
/// own identity, and the stateful `StakeBond` validation rule uses it to
/// enforce `validator_pubkey_hash == validator_id_from_pubkey(validator_pubkey)`
/// (the `owner_pubkey_hash` is derived identically from the owner key). It is
/// intentionally distinct from the 32-byte BLAKE2b-256 P2PKH *spend* address
/// payload: the overlay identity is the full 64-byte digest that the `Hash64`
/// registry fields require. Unkeyed (no domain separator) to match the ADR
/// text byte-for-byte; domain separation is unnecessary because the input is a
/// fixed-length public key, not a multi-field structure.
pub fn validator_id_from_pubkey(validator_pubkey: &[u8]) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(Blake2bParams::new().hash_length(64).to_state().update(validator_pubkey).finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Local-only fingerprint of an ML-DSA-87 signature: unkeyed `BLAKE2b-512` of the
/// signature bytes, stored in [`SignedEpochRecord::signature_fingerprint`] so a
/// validator can recognise a re-broadcast of its own in-flight attestation across
/// restarts without persisting the full ~3.3 KB signature. It is **not** part of
/// the equivocation predicate (see [`check_signed_epoch_record`]) — two valid hedged
/// signatures over the same message differ, so only `(target_hash, target_daa_score)`
/// equality decides equivocation.
pub fn signature_fingerprint(signature: &[u8]) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(Blake2bParams::new().hash_length(64).to_state().update(signature).finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Compute the validator-set commitment for `epoch` over the
/// `validators` set, per ADR-0010 §"Validator-set commitment
/// derivation":
///
/// ```text
/// snapshot_bytes = epoch.to_le_bytes()
///               || (sorted_validators.len() as u32).to_le_bytes()
///               || for each v in sorted_validators (by validator_id asc):
///                      v.validator_id.as_bytes()           (64 B)
///                   || v.stake_amount.to_le_bytes()        (8  B)
///                   || v.activation_daa_score.to_le_bytes()(8  B)
///
/// validator_set_commitment = BLAKE2b-512(
///     key   = VALIDATOR_SET_COMMITMENT_KEY,
///     input = snapshot_bytes,
/// )
/// ```
///
/// The function clones `validators` before sorting, so caller order
/// does not matter; this keeps the helper safe to call on a
/// borrowed slice from any store iteration without forcing a
/// pre-sort up the stack. Consensus stores are nonetheless required
/// to **persist** the sorted form so on-disk snapshots are
/// canonical.
pub fn validator_set_commitment(epoch: u64, validators: &[ValidatorRecord]) -> Hash64 {
    let mut sorted: Vec<ValidatorRecord> = validators.to_vec();
    sorted.sort_by(|a, b| a.validator_id.cmp(&b.validator_id));

    let mut hasher = Blake2bParams::new().hash_length(64).key(VALIDATOR_SET_COMMITMENT_KEY).to_state();
    hasher.update(&epoch.to_le_bytes());
    // len-as-u32 to match the ADR text byte-for-byte; consensus will
    // reject any snapshot whose actual length exceeds u32::MAX, but
    // that check lives in the validation rule (PR-10.5), not here.
    hasher.update(&(sorted.len() as u32).to_le_bytes());
    for v in &sorted {
        hasher.update(v.validator_id.as_byte_slice());
        hasher.update(&v.stake_amount.to_le_bytes());
        hasher.update(&v.activation_daa_score.to_le_bytes());
    }

    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Compute the BLAKE2b-256 attestation message that ML-DSA-87 signs
/// over, per ADR-0009 §"Attestation target" as pinned by **Addendum
/// A.3**:
///
/// ```text
/// attestation_message = BLAKE2b-256(
///     key   = ATTESTATION_MESSAGE_DOMAIN,
///     input = network_id
///          || epoch.to_le_bytes()
///          || target_hash.as_bytes()              (64 B)
///          || target_daa_score.to_le_bytes()
///          || validator_set_commitment.as_bytes() (64 B)
///          || bond_outpoint.transaction_id        (64 B)
///          || bond_outpoint.index.to_le_bytes()   (4 B),
/// )
/// ```
///
/// `network_id` and `bond_outpoint` are **required** (Addendum A.3): they
/// bind the attestation to a specific network and to the specific bond
/// whose stake it pledges, so a signature cannot be replayed across
/// networks or re-associated with a different bond. `network_id` is the
/// caller-supplied canonical network discriminator bytes; passing it as
/// `&[u8]` keeps this module decoupled from `NetworkId`.
///
/// The 32-byte digest is returned as the upstream [`Hash`] (alias for
/// `Hash32`) so it composes directly with the libcrux ML-DSA-87 `sign_ctx`
/// API. The signing context (`ATTESTATION_MLDSA87_CONTEXT`) is applied at
/// the ML-DSA-87 layer, not inside this hasher — keeping the two domain
/// separators independent.
pub fn stake_attestation_message(
    network_id: &[u8],
    epoch: u64,
    target_hash: Hash64,
    target_daa_score: u64,
    validator_set_commitment: Hash64,
    bond_outpoint: TransactionOutpoint,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(ATTESTATION_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(&epoch.to_le_bytes());
    hasher.update(target_hash.as_byte_slice());
    hasher.update(&target_daa_score.to_le_bytes());
    hasher.update(validator_set_commitment.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());

    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// kaspa-pq H-05 (audit, ADR-0010 "Unbonding"): an owner-authorized request to
/// begin unbonding a `StakeBond`, carried on `SUBNETWORK_ID_STAKE_UNBOND`.
/// Accepting it stamps the bond's `unbond_request_daa_score` (→ `Unbonding`);
/// the staked output-0 then becomes spendable once `unbond_request_daa_score +
/// unbonding_period_blocks` is reached (the `bond_spend_gate`). Authorization is
/// the owner's ML-DSA-87 signature over [`unbond_request_message`] under
/// [`UNBOND_REQUEST_CONTEXT`]: without it an attacker could force every honest
/// validator's bond into `Unbonding` and grief them out of the active set (a
/// liveness attack), so the *request* — not just the eventual spend — must be
/// owner-authorized.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StakeUnbondRequestPayload {
    pub version: u16,
    /// The bond being unbonded (its creating tx's output-0 outpoint).
    pub bond_outpoint: TransactionOutpoint,
    /// The bond owner's 2592-byte ML-DSA-87 public key. Bound to the bond at
    /// acceptance: `validator_id_from_pubkey(owner_pubkey) == bond.owner_pubkey_hash`.
    pub owner_pubkey: Vec<u8>,
    /// 4627-byte ML-DSA-87 signature over [`unbond_request_message`].
    pub signature: Vec<u8>,
}

/// kaspa-pq H-05 / audit M-04: the BLAKE2b-256 digest the bond owner signs to
/// authorize unbonding `bond_outpoint`. Keyed by [`UNBOND_REQUEST_MESSAGE_DOMAIN`]
/// (purpose separation from attestations/slashing) and bound to BOTH the
/// `network_id` (audit M-04 — prevents cross-network replay of an unbond
/// authorization, mirroring [`stake_attestation_message`]) AND the bond outpoint
/// (so the authorization cannot be reused for another bond). `network_id` is the
/// chain's genesis hash (ADR-0009 Addendum A.3).
pub fn unbond_request_message(network_id: &[u8], bond_outpoint: TransactionOutpoint) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(UNBOND_REQUEST_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// Stateless validation of a [`StakeUnbondRequestPayload`] (subnetwork
/// `SUBNETWORK_ID_STAKE_UNBOND`): decodability, version, and the owner key /
/// signature lengths. Like the attestation check, the ML-DSA-87 signature is
/// verified in the stateful block-validity rule (`unbond_request_authorized`),
/// which also binds the key to the bond's `owner_pubkey_hash`.
pub fn validate_stake_unbond_payload(payload: &[u8]) -> Result<(), DnsTxError> {
    let req: StakeUnbondRequestPayload = decode_dns_payload(payload)?;
    if req.version != DNS_PAYLOAD_VERSION_V1 {
        return Err(DnsTxError::UnsupportedVersion(req.version));
    }
    if req.owner_pubkey.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(DnsTxError::InvalidPubKeyLen(req.owner_pubkey.len()));
    }
    if req.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(DnsTxError::InvalidSignatureLen(req.signature.len()));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Validator-local state (ADR-0011 §"Decision").
//
// These types are *never* on the wire and are *not* consensus
// inputs — they describe the local view a validator service
// (in-process or sidecar) maintains across restarts so honest
// operators cannot accidentally double-sign across a restart, and
// so an operator can answer "is my bond healthy?" without reading
// the source code.
// ---------------------------------------------------------------------

/// Operator-visible status of a running validator service.
/// Returned by `kaspa-pq-cli validator status` and by the future
/// `getValidatorStatus` RPC (lands in PR-10.14′). Nine variants;
/// default is `NodeNotSynced` (a freshly-started validator is
/// "not yet sure if the node it just connected to is at tip").
///
/// See ADR-0011 §"Validator status enum" for the meaning of each
/// variant. The variant ordering / discriminant values are
/// API-stable: persisted to JSON / Borsh by RPC clients, so any
/// future reorder is a wire-format break.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ValidatorStatus {
    /// Local node has not yet reached `is_synced()`. Validator
    /// service stays idle.
    #[default]
    NodeNotSynced = 0,
    /// `--stake-bond` outpoint does not exist in the stake
    /// registry yet (the bond tx may still be propagating).
    BondNotFound = 1,
    /// Bond exists, `daa_score < activation_daa_score`.
    BondPending = 2,
    /// Bond is active but not yet reflected in the current epoch's active validator
    /// set (a transient; under ADR-0017 every active bond attests).
    ActiveIdle = 3,
    /// Bond is active, validator is in the current epoch's set,
    /// and `signed_epoch_db` shows no prior signature for this
    /// epoch.
    ActiveEligible = 4,
    /// Already signed the current epoch — recorded in
    /// `signed_epoch_db`.
    SignedThisEpoch = 5,
    /// Bond is in the unbonding window. No new attestations.
    Unbonding = 6,
    /// Bond has been burned by a `SlashingEvidencePayload`. The
    /// validator service exits with a non-zero status.
    Slashed = 7,
    /// `--dry-run` set; per-epoch computation runs, signing is
    /// skipped.
    DryRun = 8,
    /// ADR-0014: standby host has booted with `--enable-validator`
    /// and `--stake-bond …` but has not yet received a valid
    /// `TakeoverToken` for any future epoch. Variant **appended**
    /// per ADR-0014 §"`ValidatorStatus` extension" so existing RPC
    /// clients parsing variants 0..8 are unaffected.
    AwaitingTakeoverToken = 9,
}

/// Per-(epoch, validator, bond) signing record persisted in the
/// validator's local `signed_epoch_db` (ADR-0011 §"Signed-epoch
/// persistence"). Loaded at startup so a restart cannot trigger
/// honest equivocation across the same epoch.
///
/// Note: the DB key is the triple `(bond_outpoint, validator_id,
/// epoch)` and is *not* stored inside the record — those three
/// fields uniquely identify the slot the record occupies, and
/// storing them again would invite drift. The record carries only
/// the per-attestation content the equivocation check compares
/// against.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct SignedEpochRecord {
    /// Epoch this attestation is bound to. Matches
    /// `StakeAttestation::epoch`.
    pub epoch: u64,
    /// Selected-chain anchor the attestation approved. Two records
    /// with the same epoch but a differing `target_hash` are
    /// slashable evidence under ADR-0009 §"`SlashingEvidencePayload`";
    /// the local guard exists to stop the second one before it
    /// leaves the host.
    pub target_hash: Hash64,
    /// DAA score of the anchor. Redundant with `target_hash` for
    /// safety purposes (a hash collision would be required to
    /// fool both fields), but kept independent so the equivocation
    /// rule catches the rare case of a node bug producing the same
    /// `target_hash` at different DAA scores.
    pub target_daa_score: u64,
    /// `BLAKE2b-512` of the 4627-byte ML-DSA-87 signature bytes.
    /// Pinned so the validator can recognise a re-broadcast of an
    /// in-flight attestation across restarts without re-storing
    /// the full ~3.3 KB signature. **Not** part of the
    /// equivocation predicate — ML-DSA-87 is hedged by default and
    /// two valid signatures over the same message differ on the
    /// `rnd` parameter, so bit-equality would be too strict.
    pub signature_fingerprint: Hash64,
}

/// Outcome of the equivocation-safety check performed before a
/// validator signs a new attestation (ADR-0011
/// §"Signed-epoch persistence"). The validator service uses this
/// to decide whether to call libcrux's `sign_ctx`.
///
/// API-stable discriminant; persisted to JSON / Borsh by RPC
/// clients.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SignedEpochCheckOutcome {
    /// No prior record for this `(bond_outpoint, validator_id,
    /// epoch)` triple. Safe to sign and gossip.
    #[default]
    Allow = 0,
    /// A prior record exists with the **same** `target_hash` and
    /// `target_daa_score`. Re-sending the same attestation is not
    /// equivocation; the validator service may re-gossip but is
    /// not required to. Critical for restart-during-gossip
    /// scenarios.
    AllowRebroadcast = 1,
    /// A prior record exists with a **different** `(target_hash |
    /// target_daa_score)`. Signing the candidate would produce
    /// slashable evidence. The validator service must refuse to
    /// sign and surface the conflict in logs + the status RPC.
    Block = 2,
}

/// Pure-function equivocation guard.
///
/// Returns one of [`SignedEpochCheckOutcome::Allow`],
/// [`SignedEpochCheckOutcome::AllowRebroadcast`], or
/// [`SignedEpochCheckOutcome::Block`] given the prior signing
/// record (if any) for the same `(bond_outpoint, validator_id,
/// epoch)` triple and the candidate the validator service is
/// about to sign. See ADR-0011 §"Signed-epoch persistence" for
/// the decision table.
///
/// The function deliberately does **not** validate the
/// `signature_fingerprint`: two valid hedged ML-DSA-87 signatures
/// over the same message will have different fingerprints, so the
/// predicate that matters is target-hash + target-daa-score
/// equality.
///
/// Both arguments come from the same trust domain (the validator's
/// own DB and its own in-flight candidate), so this function does
/// no cryptographic verification — it is a pure comparison.
pub fn check_signed_epoch_record(prev: Option<&SignedEpochRecord>, candidate: &SignedEpochRecord) -> SignedEpochCheckOutcome {
    match prev {
        None => SignedEpochCheckOutcome::Allow,
        Some(p) if p.target_hash == candidate.target_hash && p.target_daa_score == candidate.target_daa_score => {
            SignedEpochCheckOutcome::AllowRebroadcast
        }
        Some(_) => SignedEpochCheckOutcome::Block,
    }
}

// ---------------------------------------------------------------------
// Coordinated-failover protocol (ADR-0014).
//
// Node-local artefacts only — no on-chain surface, no consensus
// input. The TakeoverToken transfers signing authority between
// two same-host validator processes at a specific future epoch
// so an honest operator cannot accidentally double-sign across
// a planned handoff. ADR-0009 SlashingEvidencePayload remains
// the consensus-side safety net for malicious operators.
// ---------------------------------------------------------------------

/// Per-host stable identifier (ADR-0014 §"`host_id` derivation").
///
/// The 32-byte `Hash` is the natural fit — `HostId` never enters
/// consensus state, so the wider `Hash64` is unnecessary. Bound
/// by the local-only protocol surface; aliasing rather than
/// newtyping keeps interop simple at the cost of letting `HostId`
/// values mix with generic 32-byte hashes by accident at the
/// type level (acceptable trade because the protocol is
/// node-local and the few call sites are concentrated).
pub type HostId = Hash;

/// Coordinated-failover takeover token (ADR-0014
/// §"`TakeoverToken`"). Carries an ML-DSA-87 signature by the
/// validator key transferring signing authority from
/// `yielding_host_id` to `taking_over_host_id` at
/// `valid_from_epoch`. Stored locally on both hosts in
/// `~/.kaspa-pq/takeover-tokens/`; never on-chain.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TakeoverToken {
    pub version: u16,

    /// `host_id` of the validator currently signing (the yielding
    /// side). Must match the host that generated the token.
    pub yielding_host_id: HostId,

    /// `host_id` of the validator about to start signing. The
    /// receiving host MUST refuse to honor a token whose
    /// `taking_over_host_id ≠ its own host_id` (ADR-0014
    /// §"Handoff protocol" step 3.b).
    pub taking_over_host_id: HostId,

    /// Validator identity both hosts share. Must match the
    /// receiving host's `--stake-bond → validator_id`.
    pub validator_id: Hash64,

    /// First epoch at which the taking-over host may sign. The
    /// yielding host MUST NOT sign any epoch
    /// `≥ valid_from_epoch` after issuing this token.
    pub valid_from_epoch: u64,

    /// Number of epochs of grace overlap during which neither
    /// host signs (defensive against in-flight gossip). Typically
    /// 1; max 8 (one epoch ≈ minutes, anything longer is a
    /// configuration error). The taking-over host starts signing
    /// at `valid_from_epoch + grace_epochs`.
    pub grace_epochs: u8,

    /// Wall-clock issuance timestamp (informational; **not** part
    /// of the signed material — clocks drift, so the protocol
    /// does not rely on it).
    pub issued_at_unix_secs: u64,

    /// 4627-byte ML-DSA-87 signature by the validator key over
    /// [`takeover_token_message`] with `TAKEOVER_TOKEN_CONTEXT`
    /// as the libcrux `ctx` parameter.
    pub signature: Vec<u8>,
}

/// Compute the per-host `HostId` (ADR-0014 §"`host_id`
/// derivation"):
///
/// ```text
/// host_id = BLAKE2b-256(
///     key   = HOST_ID_KEY,
///     input = hostname || host_boot_nonce (32 B),
/// )
/// ```
///
/// `host_boot_nonce` is a fresh 32-byte random generated by
/// `kaspa-pq-cli validator host-id init` and persisted at
/// `/etc/kaspa-pq/host-nonce`. The nonce makes `HostId`
/// rebuild-stable but resistant to spoofing — an operator who
/// rebuilds the secondary host gets a new `HostId` unless they
/// explicitly re-use the nonce file.
pub fn compute_host_id(hostname: &[u8], boot_nonce: &[u8; 32]) -> HostId {
    let mut hasher = Blake2bParams::new().hash_length(32).key(HOST_ID_KEY).to_state();
    hasher.update(hostname);
    hasher.update(boot_nonce);
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// Compute the BLAKE2b-256 message that the validator key signs
/// to produce [`TakeoverToken::signature`] (ADR-0014
/// §"`TakeoverToken`"):
///
/// ```text
/// takeover_token_message = BLAKE2b-256(
///     key   = TAKEOVER_TOKEN_MESSAGE_DOMAIN,
///     input = yielding_host_id.as_bytes()       (32 B)
///          || taking_over_host_id.as_bytes()    (32 B)
///          || validator_id.as_bytes()           (64 B)
///          || valid_from_epoch.to_le_bytes()
///          || [grace_epochs],
/// )
/// ```
///
/// The 32-byte digest is returned as the upstream [`Hash`] so it
/// composes directly with the libcrux ML-DSA-87 `sign_ctx` /
/// `verify_ctx` APIs. The ML-DSA-87 signing context
/// (`TAKEOVER_TOKEN_CONTEXT`) is applied at the ML-DSA-87 layer,
/// not inside this hasher — keeping the two domain separators
/// independent and distinct from every other ML-DSA-87 use site
/// in the protocol (ADR-0014 §"Public-claim discipline" replay
/// safety claim).
pub fn takeover_token_message(
    yielding_host_id: HostId,
    taking_over_host_id: HostId,
    validator_id: Hash64,
    valid_from_epoch: u64,
    grace_epochs: u8,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(TAKEOVER_TOKEN_MESSAGE_DOMAIN).to_state();
    hasher.update(&yielding_host_id.as_bytes());
    hasher.update(&taking_over_host_id.as_bytes());
    hasher.update(validator_id.as_byte_slice());
    hasher.update(&valid_from_epoch.to_le_bytes());
    hasher.update(&[grace_epochs]);
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

// ---------------------------------------------------------------------
// Remote-signer protocol (ADR-0015).
//
// Node-local wire format between a validator client and a
// `kaspa-pq-signer` process over a Unix domain socket. None of
// these types enter consensus state; they describe the bytes
// flowing across the local socket only.
// ---------------------------------------------------------------------

/// Per-purpose tag carried in a [`SignerRequest`] (ADR-0015
/// §"Request / response cycle"). Wire-format discriminants are
/// API-stable; reordering is a hard fork of the protocol.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SigningPurpose {
    /// Standard transaction signing — message digest is whatever
    /// the tx-script ML-DSA-87 sign path produces; context is
    /// `b"kaspa-pq-v1/tx/mldsa87"`.
    #[default]
    Transaction = 0,
    /// DNS overlay attestation — message digest is from
    /// [`stake_attestation_message`]; context is
    /// `ATTESTATION_MLDSA87_CONTEXT`.
    Attestation = 1,
    /// Coordinated-failover takeover token — message digest is
    /// from [`takeover_token_message`]; context is
    /// `TAKEOVER_TOKEN_CONTEXT`.
    TakeoverToken = 2,
    /// DNS overlay unbond request — message digest is from
    /// [`unbond_request_message`]; context is `UNBOND_REQUEST_CONTEXT`
    /// (audit H-03; appended, so discriminants 0-2 are unchanged).
    Unbond = 3,
}

/// The digest the signer will ML-DSA-87-sign, **typed by purpose** (audit H-03). This makes the
/// digest size a compile-time property of the request: a [`SigningPurpose::Transaction`] carries a
/// 64-byte [`Hash64`] transaction sighash, while the overlay digests (attestation / unbond /
/// takeover) are the 32-byte BLAKE2b-256 [`Hash`] their `*_message` helpers produce. The previous
/// fixed `message_digest: Hash` (32 bytes) could not represent a transaction sighash at all — so
/// passing a 32-byte value for a tx-signing request was a silent protocol break; it is now
/// unrepresentable.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SignerMessageDigest {
    /// 64-byte ML-DSA-87 transaction sighash (`calc_mldsa87_signature_hash`).
    Transaction(Hash64),
    /// 32-byte attestation message digest ([`stake_attestation_message`]).
    Attestation(Hash),
    /// 32-byte unbond-request message digest ([`unbond_request_message`]).
    Unbond(Hash),
    /// 32-byte takeover-token message digest ([`takeover_token_message`]).
    TakeoverToken(Hash),
}

impl SignerMessageDigest {
    /// The [`SigningPurpose`] this digest is for. A [`SignerRequest`] is well-formed only when its
    /// `purpose` equals this (see [`SignerRequest::purpose_matches_digest`]).
    pub fn purpose(&self) -> SigningPurpose {
        match self {
            SignerMessageDigest::Transaction(_) => SigningPurpose::Transaction,
            SignerMessageDigest::Attestation(_) => SigningPurpose::Attestation,
            SignerMessageDigest::Unbond(_) => SigningPurpose::Unbond,
            SignerMessageDigest::TakeoverToken(_) => SigningPurpose::TakeoverToken,
        }
    }
}

/// Per-purpose structured metadata attached to a
/// [`SignerRequest`] (ADR-0015 §"Request / response cycle").
/// **Not** part of the signed message — in-band hints for the
/// signer's policy engine. Operators using
/// [`SignerPolicy::Permissive`] can pass [`SignerMetadata::None`]
/// for any purpose.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SignerMetadata {
    None,
    Attestation { epoch: u64, target_hash: Hash64, target_daa_score: u64 },
    TakeoverToken { yielding_host_id: Hash, taking_over_host_id: Hash, valid_from_epoch: u64, grace_epochs: u8 },
}

/// Failure modes for a [`SignerRequest`]. Tuple-variant data is
/// carried explicitly (not `Option<String>`) so the wire format
/// stays compact.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SignerError {
    ProtocolVersionMismatch,
    KeyNotFound,
    UnknownPurpose,
    /// Free-text reason — typically equivocation evidence
    /// summary for `SignerPolicy::Strict` rejections.
    PolicyViolation(String),
    /// Vendor-specific HSM error: `(code, message)`. `code` is
    /// the raw PKCS#11 / vendor-SDK return value; `message` is
    /// the corresponding human string.
    HsmError(u32, String),
    RateLimit,
    InternalError(String),
}

/// Per-validator signer policy mode (ADR-0015 §"Policy model").
/// Wire-format discriminant is API-stable.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SignerPolicy {
    /// Sign every well-formed request; no equivocation guard.
    /// Closest to the ADR-0010 local-key-file behaviour.
    #[default]
    Permissive = 0,
    /// Sign every well-formed request but log policy
    /// violations as warnings. Migration path from `Permissive`
    /// to `Strict`.
    AuditOnly = 1,
    /// Enforce the ADR-0011 equivocation guard at the signer.
    /// Refuse `Attestation` requests whose
    /// `(validator_id, epoch)` already has a recorded differing
    /// target. **Moves the authoritative `SignedEpochRecord`
    /// store from the validator client to the signer.**
    Strict = 2,
}

/// Client → server handshake frame (ADR-0015 §"Protocol
/// versioning + handshake"). Sent immediately upon connection.
/// `client_identity` is the [`HostId`] from ADR-0014 so the
/// signer's audit log can attribute requests to a specific
/// validator client.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignerHello {
    pub protocol_version: u16,
    pub capabilities: u32,
    pub client_identity: HostId,
}

/// Server → client handshake response. Mismatched
/// `protocol_version` closes the connection with one
/// [`SignerError::ProtocolVersionMismatch`] frame and no
/// further traffic.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignerHelloAck {
    pub protocol_version: u16,
    pub capabilities: u32,
    pub server_identity: HostId,
}

/// Length-prefixed Borsh request frame (ADR-0015 §"Request /
/// response cycle"). One request per signature; the server
/// dedupes by `request_id` for the lifetime of the connection.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignerRequest {
    /// Monotonic per-client request id. Wraps at `u64::MAX`
    /// (practically: never in any reasonable validator lifetime).
    pub request_id: u64,
    /// Which key the signer should use. The signer may hold
    /// more than one validator key (multi-tenant signer); the
    /// request selects via this field.
    pub validator_id: Hash64,
    pub purpose: SigningPurpose,
    /// libcrux ML-DSA-87 `sign_ctx` ctx parameter. Caller
    /// provides; the signer does not infer the context from
    /// the purpose tag because future protocol extensions may
    /// need a non-standard context for the same purpose.
    pub context: Vec<u8>,
    /// The digest to sign, typed by purpose (audit H-03). Must agree with `purpose` — see
    /// [`SignerRequest::purpose_matches_digest`]. A `Transaction` request carries a 64-byte sighash;
    /// the others carry a 32-byte BLAKE2b-256 digest.
    pub message_digest: SignerMessageDigest,
    pub metadata: SignerMetadata,
}

impl SignerRequest {
    /// Audit H-03: the request is well-formed only when its `purpose` tag agrees with the typed
    /// `message_digest` variant. A signer MUST reject a request where they disagree (the typed
    /// digest already makes a wrong *size* unrepresentable; this catches a wrong *tag*).
    pub fn purpose_matches_digest(&self) -> bool {
        self.purpose == self.message_digest.purpose()
    }
}

/// Server → client response. The `result` payload is either the
/// 4627-byte ML-DSA-87 signature or a structured failure.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignerResponse {
    pub request_id: u64,
    pub result: Result<Vec<u8>, SignerError>,
}

/// One outcome of a request in the audit log (ADR-0015 §"Audit
/// log"). Tuple variant for the `Refused` case carries the same
/// [`SignerError`] sent over the wire, so the audit log records
/// what the client was told.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SignerOutcome {
    Signed,
    Refused(SignerError),
}

/// One row in the signer's append-only audit log (ADR-0015
/// §"Audit log"). Records the request content plus the
/// signature fingerprint (not the full signature blob — pinned
/// for tamper detection without ballooning log size).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignerAuditRecord {
    pub timestamp_unix_secs: u64,
    pub client_identity: HostId,
    pub request_id: u64,
    pub validator_id: Hash64,
    pub purpose: SigningPurpose,
    pub metadata: SignerMetadata,
    pub message_digest: SignerMessageDigest,
    /// `BLAKE2b-512` of the 4627-byte signature bytes (zero
    /// `Hash64` if the request was refused). Pinned so the audit
    /// record stays small while still witnessing what was
    /// signed.
    pub signature_fingerprint: Hash64,
    pub outcome: SignerOutcome,
}

/// kaspa-pq audit M-04 — a signed checkpoint of the signer's
/// append-only audit log. Periodically the signer signs the current
/// hash-chain head ([`compute_signer_audit_chain_entry`]) with a held
/// validator ML-DSA-87 key under [`AUDIT_CHECKPOINT_MLDSA87_CONTEXT`],
/// and appends one of these to `audit.checkpoints`. Because the chain
/// key is public, a host that can rewrite `audit.log` can produce a
/// self-consistent chain — but it cannot forge `signature` over a
/// rewritten `chain_head`. Exporting these records off-box (syslog /
/// transparency log / second host) gives unforgeable tamper-evidence:
/// anyone holding the validator public key (derivable from the
/// on-chain bond as `BLAKE2b-512(pubkey) == validator_id`) can verify
/// `signature` against `chain_head`, and recompute the chain head at
/// `record_index` from a copy of the log to confirm they match.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignerAuditCheckpoint {
    /// Number of audit records that had been appended when this
    /// checkpoint was taken (`chain_head` is the head AFTER that many
    /// records). Lets a verifier locate the exact prefix to recompute.
    pub record_index: u64,
    pub timestamp_unix_secs: u64,
    /// The validator key that signed this checkpoint; its public key
    /// (hence `validator_id == BLAKE2b-512(pubkey)`) is the verifier's
    /// anchor.
    pub validator_id: Hash64,
    /// The audit-log chain head being attested.
    pub chain_head: Hash64,
    /// ML-DSA-87 signature over `chain_head.as_byte_slice()` under
    /// [`AUDIT_CHECKPOINT_MLDSA87_CONTEXT`]. Full signature (not a
    /// fingerprint) so it is externally verifiable without the signer.
    pub signature: Vec<u8>,
}

/// Compute the next entry in the signer's audit-log chain
/// (ADR-0015 §"Audit log"):
///
/// ```text
/// next_chain_hash = BLAKE2b-512(
///     key   = AUDIT_LOG_CHAIN_KEY,
///     input = prev_chain_hash.as_bytes()       (64 B)
///          || borsh::to_vec(&record),
/// )
/// ```
///
/// Walking the log from a known-good `prev_chain_hash` and
/// recomputing every successor lets a verifier detect any
/// post-hoc tampering — an inserted, deleted, or modified
/// record shifts every subsequent chain hash.
///
/// The genesis case (first record after log rotation) uses
/// `prev_chain_hash = ZERO_HASH64` or the terminal hash of the
/// previous log file; either is the verifier's known-good
/// starting point.
pub fn compute_signer_audit_chain_entry(prev_chain_hash: Hash64, record: &SignerAuditRecord) -> Hash64 {
    let record_bytes = borsh::to_vec(record).expect("SignerAuditRecord Borsh-serialise is infallible");
    let mut hasher = Blake2bParams::new().hash_length(64).key(AUDIT_LOG_CHAIN_KEY).to_state();
    hasher.update(prev_chain_hash.as_byte_slice());
    hasher.update(&record_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------
// Reward / slashing distribution (ADR-0013).
// ---------------------------------------------------------------------

/// Compute the per-block validator-side coinbase payout (ADR-0013
/// §"Per-attestation flat reward" + §"Inflation cap").
///
/// Returns the total sompi that flows into validator-side
/// coinbase outputs for the block, plus any sompi withheld by
/// the per-block cap. Under correct parameterisation the
/// refund is always zero — the cap is a defensive guard
/// against a misconfigured
/// `per_attestation_reward_sompi × max_attestations_per_block`
/// product overflowing.
///
/// This helper is intentionally arithmetic-only — actual
/// coinbase output construction (one
/// `Output { value: per_attestation_reward, script_public_key:
/// owner_address }` per included attestation) is a
/// `consensus/src/processes/coinbase.rs` (PR-10.5′) concern
/// that consumes this helper's output as a sanity bound.
pub fn compute_attestation_reward_payouts(
    per_attestation_reward_sompi: u64,
    included_attestation_count: usize,
    max_validator_inflation_per_block_sompi: u64,
) -> AttestationRewardPayout {
    // Saturating arithmetic so a bogus `(u64::MAX, usize::MAX)`
    // input produces a defined value rather than panicking. Real
    // inputs are bounded by `MAX_ATTESTATIONS_PER_SHARD` and the
    // mainnet per-attestation parameter, but defence-in-depth.
    let uncapped = (per_attestation_reward_sompi as u128).saturating_mul(included_attestation_count as u128);
    let capped = uncapped.min(max_validator_inflation_per_block_sompi as u128);

    // Both fit in u64 by construction (capped ≤ u64::MAX).
    let total_payout_sompi = capped as u64;
    let refunded_sompi = (uncapped - capped) as u64;
    AttestationRewardPayout { total_payout_sompi, refunded_sompi }
}

// ---------------------------------------------------------------------
// kaspa-pq Phase 13 inclusion economics (ADR-0018 §D/§E).
//
// These pure functions replace the ADR-0013 *flat* per-attestation reward
// ([`compute_attestation_reward_payouts`]) with **proportional, anti-capture**
// payouts: every payout is a share of a per-epoch *pool*, normalised by the
// epoch's **expected** active stake (`total_active_stake_by_epoch`), never by the
// included count. So including a few attestations earns only a small slice — a
// minority can never drain a pool (the rejected `pool / included_count` design).
// The unspent remainder of each pool rolls over (a caller concern); these helpers
// only compute the per-recipient share. The pool amounts are produced by the §F
// fee/subsidy split; here they are inputs. The split and the coinbase fan-out are
// wired, and the section is active from genesis (gated on `dns_activation_daa_score`
// = 0 everywhere today).
// ---------------------------------------------------------------------

/// Shared anti-capture proportional share: `pool × min(stake, expected_stake) /
/// expected_stake` (`0` when `expected_stake == 0`). Integer `u128`, multiply-
/// before-divide (no lossy `unit` intermediate, mirroring [`epoch_stake_credit`]).
/// The `min` clamp is the anti-capture invariant — a single share can never exceed
/// the whole `pool`, so over-counting (duplicate inclusion) cannot over-drain it. In
/// the valid domain (`stake ≤ expected_stake`) the clamp is a no-op, so the result
/// equals the ADR-0018 §D/§E formulas byte-for-byte.
#[inline]
fn proportional_share(pool: u128, stake: u128, expected_stake: u128) -> u128 {
    if expected_stake == 0 {
        return 0;
    }
    pool.saturating_mul(stake.min(expected_stake)) / expected_stake
}

/// ADR-0018 §D — the Worker inclusion bounty for one block.
///
/// The Worker that includes attestation shards is paid for **quality contribution**,
/// not for the act of inclusion: the base bounty is a [`proportional_share`] of the
/// epoch's `worker_inclusion_pool` against the epoch's **expected** stake, scaled by
/// `urgency_multiplier_scaled` ([`STAKE_SCORE_SCALE`] fixed-point, `STAKE_SCORE_SCALE`
/// = 1.0×). When this block is the one that first lifts the epoch's included fraction
/// from `< φS` to `≥ φS` (`crossed_quality_floor`), the fixed `quality_gate_bonus` is
/// added — an economic nudge to complete an epoch, **not** a certificate.
///
/// `newly_included_stake` is the stake of attestations *first* included by this block
/// (valid signature + bond, correct epoch/target, within the reward window); duplicate
/// / invalid / expired / already-included stake is excluded by the caller. The unspent
/// remainder of the pool is the caller's rollover concern.
pub fn worker_inclusion_bounty(
    pool: u128,
    newly_included_stake: u128,
    expected_stake: u128,
    urgency_multiplier_scaled: u128,
    crossed_quality_floor: bool,
    quality_gate_bonus: u128,
) -> u128 {
    let base = proportional_share(pool, newly_included_stake, expected_stake);
    let urgent = base.saturating_mul(urgency_multiplier_scaled) / STAKE_SCORE_SCALE;
    if crossed_quality_floor { urgent.saturating_add(quality_gate_bonus) } else { urgent }
}

/// ADR-0018 §E — split a per-epoch validator pool into its `(participation,
/// quality_bonus)` sub-pools by `participation_bps`. The quality-bonus sub-pool takes
/// the **remainder** (`pool − participation`), so the two sum to `pool` exactly and no
/// sompi is lost to rounding (any dust lands in the bonus pool). `participation_bps` is
/// clamped to `10_000`.
pub fn split_validator_pool(validator_pool: u128, participation_bps: u16) -> (u128, u128) {
    let bps = (participation_bps as u128).min(10_000);
    let participation = validator_pool.saturating_mul(bps) / 10_000;
    let quality_bonus = validator_pool.saturating_sub(participation);
    (participation, quality_bonus)
}

/// ADR-0018 §E — one validator's **participation** reward: a [`proportional_share`] of
/// the participation sub-pool against the epoch's expected stake. Paid to every included
/// validator regardless of whether the epoch met φS. Minority inclusion earns only its
/// proportional slice, never the whole pool.
pub fn validator_participation_reward(participation_pool: u128, included_valid_stake: u128, expected_stake: u128) -> u128 {
    proportional_share(participation_pool, included_valid_stake, expected_stake)
}

/// ADR-0018 §E — one validator's **quality bonus**: a [`proportional_share`] of the
/// quality-bonus sub-pool, **but only when the epoch's included fraction met φS**
/// (`epoch_meets_quality_floor`). Below the floor the bonus pool pays nothing and rolls
/// over entirely.
pub fn validator_quality_bonus(
    quality_bonus_pool: u128,
    included_valid_stake: u128,
    expected_stake: u128,
    epoch_meets_quality_floor: bool,
) -> u128 {
    if !epoch_meets_quality_floor {
        return 0;
    }
    proportional_share(quality_bonus_pool, included_valid_stake, expected_stake)
}

// ---------------------------------------------------------------------
// kaspa-pq Phase 13 fee / subsidy split (ADR-0018 §F).
//
// Three independent, **dust-free** splits that size the §D/§E pools and the
// Service reserve from a block's subsidy and fees. All basis-points; the
// *primary* recipient of each split takes the remainder, so the parts sum to
// the input exactly — no value is minted or lost to rounding regardless of
// whether the configured bps sum to 10_000 (a misconfiguration only mis-weights
// the split, never breaks supply). Pure; consumed by the coinbase fan-out, and
// active from genesis — every current net runs the whole overlay from
// `dns_activation_daa_score` = 0.
//
// The §D `worker_inclusion_pool` is `SubsidySplit::worker_inclusion_sompi`; the
// §E validator pool is `SubsidySplit::validator_sompi` (plus the validator share
// of normal-tx and finality fees). The normal-tx year-10 ramp (85/10/5 →
// 80/15/5) is a documented follow-up; the fixed 85/10/5 is ADR-acceptable.
// ---------------------------------------------------------------------

/// Per-network §F split ratios (basis points). Each group is intended to sum to
/// `10_000`; the split helpers route any rounding remainder to the group's
/// primary recipient, so the stored "primary" bps is documentary (asserted in
/// tests). Nested in [`RewardParams`].
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FeeSplitParams {
    /// Block-subsidy split (DNS Active, Stage 3). Worker total (75%) = `base`
    /// (67%, the primary — takes the remainder) + `inclusion` (8%, the §D
    /// `worker_inclusion_pool`); `validator` (25%) is the §E pool. `service` is
    /// **0** — the Node reward was dropped (Sybil-prone; node duty is enforced via
    /// the validator role instead). The field is retained at 0 for borsh stability
    /// / future re-activation.
    pub subsidy_worker_base_bps: u16,
    pub subsidy_worker_inclusion_bps: u16,
    pub subsidy_validator_bps: u16,
    pub subsidy_service_bps: u16,
    /// Normal-tx-fee split (a permanent validator share so the layer outlives the
    /// subsidy). Worker (90%, primary) / Validator (10%) / Service (0, retained).
    pub normal_fee_worker_bps: u16,
    pub normal_fee_validator_bps: u16,
    pub normal_fee_service_bps: u16,
    /// DNS-finality-fee split (validators directly provide finality). Validator
    /// (75%, primary) / Worker (25%) / Service (0, retained). WIRED (ADR-0018 §F
    /// bridge wiring): an accepted tx creating ≥1 `EVM_DEPOSIT_LOCK` output is
    /// finality-class — classified at fee accumulation in `calculate_utxo_state`,
    /// doubly gated on [`DnsParams::finality_fee_activation_daa_score`] AND the
    /// net's EVM activation; split via [`split_finality_fees`] inside
    /// [`split_block_reward`].
    pub finality_fee_validator_bps: u16,
    pub finality_fee_worker_bps: u16,
    pub finality_fee_service_bps: u16,
}

/// ADR-0018 §F block-subsidy split outcome. `worker_base + worker_inclusion` is
/// the Worker's 70%; `validator` is the §E validator pool; `service` is the
/// protocol reserve. The four fields sum to the input subsidy exactly.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubsidySplit {
    pub worker_base_sompi: u64,
    pub worker_inclusion_sompi: u64,
    pub validator_sompi: u64,
    pub service_sompi: u64,
}

/// ADR-0018 §F three-way fee split outcome (normal-tx or DNS-finality). The three
/// fields sum to the input fee exactly.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FeeSplit {
    pub worker_sompi: u64,
    pub validator_sompi: u64,
    pub service_sompi: u64,
}

/// `amount × min(bps, 10_000) / 10_000` as u64 (u128 intermediate, saturating).
/// Shared by the §F split helpers, which derive the primary recipient as the
/// remainder so each split is dust-free.
#[inline]
fn bps_share(amount: u64, bps: u16) -> u64 {
    let bps = (bps as u128).min(10_000);
    ((amount as u128).saturating_mul(bps) / 10_000) as u64
}

/// ADR-0018 §F — split a block subsidy four ways (Worker base + inclusion,
/// Validator, Service). `worker_base` is the primary recipient and takes the
/// remainder, so the four parts sum to `subsidy_sompi` exactly.
///
/// Conservation holds for ANY bps configuration (defensive): each non-primary
/// share is capped at the amount still unassigned, so a (mis)configured group
/// summing past 10_000 bps cannot over-mint — for every well-formed config
/// (group sum ≤ 10_000, all current presets) the caps are exact no-ops
/// (Σ floor(amount·bpsᵢ/10⁴) ≤ amount), keeping the math byte-identical.
pub fn split_block_subsidy(subsidy_sompi: u64, p: &FeeSplitParams) -> SubsidySplit {
    let mut remaining = subsidy_sompi;
    let worker_inclusion_sompi = bps_share(subsidy_sompi, p.subsidy_worker_inclusion_bps).min(remaining);
    remaining -= worker_inclusion_sompi;
    let validator_sompi = bps_share(subsidy_sompi, p.subsidy_validator_bps).min(remaining);
    remaining -= validator_sompi;
    let service_sompi = bps_share(subsidy_sompi, p.subsidy_service_bps).min(remaining);
    let worker_base_sompi = remaining - service_sompi;
    SubsidySplit { worker_base_sompi, worker_inclusion_sompi, validator_sompi, service_sompi }
}

/// ADR-0018 §F — split normal-tx fees three ways. Worker is primary and takes the
/// remainder, so the three parts sum to `fees_sompi` exactly. Conservation holds
/// for ANY bps configuration (see [`split_block_subsidy`] — same defensive caps,
/// no-ops for every well-formed config).
pub fn split_normal_tx_fees(fees_sompi: u64, p: &FeeSplitParams) -> FeeSplit {
    let mut remaining = fees_sompi;
    let validator_sompi = bps_share(fees_sompi, p.normal_fee_validator_bps).min(remaining);
    remaining -= validator_sompi;
    let service_sompi = bps_share(fees_sompi, p.normal_fee_service_bps).min(remaining);
    let worker_sompi = remaining - service_sompi;
    FeeSplit { worker_sompi, validator_sompi, service_sompi }
}

/// ADR-0018 §F — split DNS-finality fees three ways. Validator is primary and
/// takes the remainder, so the three parts sum to `fees_sompi` exactly.
/// Conservation holds for ANY bps configuration (see [`split_block_subsidy`] —
/// same defensive caps, no-ops for every well-formed config).
pub fn split_finality_fees(fees_sompi: u64, p: &FeeSplitParams) -> FeeSplit {
    let mut remaining = fees_sompi;
    let worker_sompi = bps_share(fees_sompi, p.finality_fee_worker_bps).min(remaining);
    remaining -= worker_sompi;
    let service_sompi = bps_share(fees_sompi, p.finality_fee_service_bps).min(remaining);
    let validator_sompi = remaining - service_sompi;
    FeeSplit { worker_sompi, validator_sompi, service_sompi }
}

/// ADR-0018 §F — the combined Worker / Validator / Service split of ONE block's coinbase
/// reward: its `subsidy` under the subsidy ratios ([`split_block_subsidy`]), its normal-tx
/// fees under the normal ratios ([`split_normal_tx_fees`]), and its DNS-finality fees under
/// the finality ratios ([`split_finality_fees`], validator-primary). The fee arguments mirror
/// [`crate::coinbase::BlockRewardData`] directly: `total_fees_sompi` is the block's WHOLE fee
/// take and `finality_fees_sompi` is its finality-class subset (bridge txs creating
/// `EVM_DEPOSIT_LOCK` outputs, gated on `finality_fee_activation_daa_score`); the normal-class
/// part is derived as `total − finality` HERE so callers cannot mis-pair the two. The Worker
/// total goes to that block's miner; the Validator total funds the §E pool; the Service total
/// is the reserve (don't-mint in the first coinbase slice). Sums to `subsidy + total_fees`
/// exactly (each sub-split is dust-free). The §D worker-inclusion sub-pool stays inside the
/// Worker total here — its conditional bounty is a later slice. `finality_fees_sompi = 0`
/// (every pre-wiring caller / below the fence) reproduces the historical math byte-identically.
pub fn split_block_reward(
    subsidy_sompi: u64,
    total_fees_sompi: u64,
    finality_fees_sompi: u64,
    fee_split: &FeeSplitParams,
) -> FeeSplit {
    // Defensive clamp: finality fees are a subset of the total by construction
    // (`calculate_utxo_state` accumulates them from the same accepted-tx walk).
    let finality = finality_fees_sompi.min(total_fees_sompi);
    let s = split_block_subsidy(subsidy_sompi, fee_split);
    let f = split_normal_tx_fees(total_fees_sompi - finality, fee_split);
    let d = split_finality_fees(finality, fee_split);
    FeeSplit {
        worker_sompi: s
            .worker_base_sompi
            .saturating_add(s.worker_inclusion_sompi)
            .saturating_add(f.worker_sompi)
            .saturating_add(d.worker_sompi),
        validator_sompi: s.validator_sompi.saturating_add(f.validator_sompi).saturating_add(d.validator_sompi),
        service_sompi: s.service_sompi.saturating_add(f.service_sompi).saturating_add(d.service_sompi),
    }
}

/// ADR-0018 §E — the validator-side coinbase outputs distributing `participation_pool`
/// proportionally among the included validators. Each validator is paid
/// [`validator_participation_reward`]`(pool, its_stake, expected_stake)` to its
/// `owner_reward_spk_payload`; zero-value shares emit no output. `included` is
/// `(owner_reward_spk_payload, included_stake)` per first-included validator, in the
/// caller's canonical order.
///
/// The outputs sum to **≤ `participation_pool`** (Σ included stake ≤ `expected_stake`); the
/// unspent remainder is **not minted** (the ADR-0018 §E rollover, realised as don't-mint —
/// so a censoring miner cannot keep it, preserving anti-capture). Pure and DAG-free, so the
/// coinbase **construction** and **validation** paths build byte-identical outputs. The §E
/// quality-bonus and §D worker-bounty are later slices.
pub fn validator_participation_outputs(
    participation_pool: u128,
    included: &[([u8; 64], u64)],
    expected_stake: u128,
) -> Vec<TransactionOutput> {
    included
        .iter()
        .filter_map(|(payload, stake)| {
            let reward =
                validator_participation_reward(participation_pool, *stake as u128, expected_stake).min(u64::MAX as u128) as u64;
            (reward > 0).then(|| TransactionOutput::new(reward, p2pkh_mldsa87_spk(payload)))
        })
        .collect()
}

/// ADR-0018 "本格版" (PoS-v2) §E/§B — does an epoch's **included** (rewarded) stake meet the
/// stake-event quality floor φS? `true` iff `included_stake / expected_stake ≥ φS_bps / 10_000`,
/// evaluated as the overflow-safe integer cross-multiply `included_stake · 10_000 ≥ φS_bps ·
/// expected_stake`. This is the **whole-epoch** gate (ADR-0018 locked decision): at/above φS the
/// epoch's quality-bonus pool is distributed to its included validators; below it the pool rolls
/// over (don't-mint). `expected_stake == 0` ⇒ vacuously meets (there is no stake to include, and
/// every bonus share is then 0 anyway). `φS_bps == 0` ⇒ always meets (the pre-ADR-0018 behavior).
pub fn epoch_meets_quality_floor(included_stake: u128, expected_stake: u128, phi_s_bps: u16) -> bool {
    included_stake.saturating_mul(10_000) >= (phi_s_bps as u128).saturating_mul(expected_stake)
}

/// Minimum stake needed to satisfy a quality floor, rounded up so "one sompi
/// below the floor" remains below.
pub fn required_stake_for_quality_floor(expected_stake: u64, quality_floor_bps: u16) -> u64 {
    ((expected_stake as u128).saturating_mul(quality_floor_bps as u128).saturating_add(9_999) / 10_000).min(u64::MAX as u128) as u64
}

/// Computes the one-block mass-capacity invariant for hard mandatory
/// attestation inclusion.
///
/// `remaining_stakes` must contain only validators that have not already been
/// credited by the selected-parent chain or candidate accepted transactions for
/// this epoch. The stakes are sorted descending because the best-case packing
/// for reaching the remaining floor delta uses the largest still-uncredited
/// validators first. Current validator/miner production emits one attestation
/// shard transaction per validator, so this check deliberately treats each
/// remaining validator as one shard transaction. If aggregate shard production
/// is added later, the production, relay, replacement, and template tests should
/// land before relaxing this conservative invariant.
pub fn mandatory_attestation_mass_capacity(
    remaining_stakes: impl IntoIterator<Item = u64>,
    expected_stake: u64,
    included_stake: u64,
    quality_floor_bps: u16,
    max_block_mass: u64,
    max_attestation_shard_mass: u64,
) -> MandatoryAttestationMassCapacity {
    let mut stakes: Vec<u64> = remaining_stakes.into_iter().filter(|stake| *stake > 0).collect();
    stakes.sort_by(|a, b| b.cmp(a));

    let required_stake = required_stake_for_quality_floor(expected_stake, quality_floor_bps);
    let required_stake_delta = required_stake.saturating_sub(included_stake);

    let mut accumulated = 0u64;
    let mut required_validator_count = 0usize;
    if required_stake_delta > 0 {
        for stake in &stakes {
            accumulated = accumulated.saturating_add(*stake);
            required_validator_count += 1;
            if accumulated >= required_stake_delta {
                break;
            }
        }
    }

    let required_shard_count = required_validator_count as u64;
    let required_mass = required_shard_count.saturating_mul(max_attestation_shard_mass);
    let max_shard_count_by_mass = if max_attestation_shard_mass == 0 { 0 } else { max_block_mass / max_attestation_shard_mass };
    let has_enough_stake = required_stake_delta == 0 || accumulated >= required_stake_delta;
    let fits = has_enough_stake
        && (required_shard_count == 0
            || (max_attestation_shard_mass > 0 && required_shard_count <= max_shard_count_by_mass && required_mass <= max_block_mass));

    MandatoryAttestationMassCapacity {
        expected_stake,
        included_stake,
        required_stake,
        required_stake_delta,
        required_validator_count,
        required_shard_count,
        max_shard_count_by_mass,
        required_mass,
        max_block_mass,
        fits,
    }
}

/// ADR-0018 "本格版" (PoS-v2) §E — the **deferred quality-bonus** coinbase outputs for a finalized
/// epoch, the §E analogue of [`validator_participation_outputs`]. Pays each included validator
/// [`validator_quality_bonus`]`(quality_pool, stake, expected_stake, meets_floor)` to its
/// `owner_reward_spk_payload`. When the epoch did **not** meet φS (`meets_floor == false`) every
/// share is 0 → no outputs (the whole pool rolls over, realised as don't-mint). Zero-value shares
/// emit no output.
///
/// `included` is the finalized [`EpochTally::included`] — `(owner_reward_spk_payload, stake)` per
/// rewarded validator, in its stored (chain-deterministic) order, with the 64-byte payload as a
/// [`Hash64`]. The outputs sum to **≤ `quality_pool`** (Σ included stake ≤ `expected_stake`); the
/// unspent remainder is **not minted** (anti-capture rollover). Pure and DAG-free → the coinbase
/// construction and validation paths build byte-identical outputs from the same finalized tally.
pub fn validator_quality_bonus_outputs(
    quality_pool: u128,
    included: &[(Hash64, u64)],
    expected_stake: u128,
    meets_floor: bool,
) -> Vec<TransactionOutput> {
    if !meets_floor {
        return Vec::new();
    }
    included
        .iter()
        .filter_map(|(payload, stake)| {
            let reward = validator_quality_bonus(quality_pool, *stake as u128, expected_stake, true).min(u64::MAX as u128) as u64;
            (reward > 0).then(|| TransactionOutput::new(reward, p2pkh_mldsa87_spk(&payload.as_bytes())))
        })
        .collect()
}

/// ADR-0018 §E — build a block's validator participation-reward outputs from its included
/// attestations, the §E analogue of the flat [`validator_reward_outputs_from_attestations`]
/// it replaces in the coinbase fan-out. `attestations` is `(bond_outpoint, epoch,
/// owner_reward_spk_payload, included_stake)` per included, eligibility-checked attestation,
/// in canonical order. Applies the same uniqueness as the flat path — **within-block**
/// `(bond_outpoint, epoch)` dedup (first occurrence wins) and **cross-block** dedup against
/// `already_rewarded` (§B.3(c)) — then pays each surviving validator
/// [`validator_participation_reward`]`(participation_pool, stake, expected_stake)`.
///
/// `expected_stake` is the per-block expected-stake denominator
/// ([`ActiveBondView::total_active_stake_at`]); using *expected* (not included) stake is the
/// §E anti-capture property. A **whole-output pool cap** guarantees the outputs sum to ≤
/// `participation_pool` even under multi-epoch multiplicities (the canonical-order tail that
/// would exceed the pool is dropped — left unrewarded, so a later block may pay it). The
/// unspent remainder is **not minted** (don't-mint rollover). Pure and DAG-free → the
/// coinbase construction and validation paths produce byte-identical outputs. Returns the
/// outputs plus the rewarded `(bond_outpoint, epoch)` keys for `rewarded_epochs_store`.
pub fn validator_participation_reward_outputs(
    participation_pool: u128,
    expected_stake: u128,
    attestations: &[(TransactionOutpoint, u64, [u8; 64], u64)],
    already_rewarded: &RewardedEpochSet,
) -> (Vec<TransactionOutput>, Vec<(TransactionOutpoint, u64)>) {
    let mut seen_in_block: HashSet<(TransactionOutpoint, u64)> = HashSet::new();
    let mut outputs: Vec<TransactionOutput> = Vec::new();
    let mut rewarded_keys: Vec<(TransactionOutpoint, u64)> = Vec::new();
    let mut spent: u128 = 0;
    for (bond_outpoint, epoch, payload, stake) in attestations {
        let key = (*bond_outpoint, *epoch);
        // Cross-block uniqueness (§B.3(c)): skip a (bond, epoch) already rewarded on the
        // selected-chain prefix. Within-block uniqueness: first occurrence wins.
        if already_rewarded.contains(bond_outpoint, *epoch) || !seen_in_block.insert(key) {
            continue;
        }
        let reward = validator_participation_reward(participation_pool, *stake as u128, expected_stake);
        if reward == 0 {
            continue; // zero stake / zero pool / degenerate denominator → no output, still allowed later
        }
        // Whole-output pool cap (value-conserving): stop at the first reward that would push
        // the total past the pool; the canonical-order tail is dropped (not marked rewarded,
        // so a later block may pay it).
        if spent.saturating_add(reward) > participation_pool {
            break;
        }
        spent += reward;
        outputs.push(TransactionOutput::new(reward as u64, p2pkh_mldsa87_spk(payload)));
        rewarded_keys.push(key);
    }
    (outputs, rewarded_keys)
}

/// Build the canonical kaspa-pq ML-DSA-87 P2PKH `scriptPublicKey`
/// for a 32-byte spend payload (ADR-0002 / ADR-0013 Addendum B).
///
/// The 37-byte script is
/// `OpDup ‖ OpBlake2b512 ‖ OpData64 ‖ <payload64> ‖ OpEqualVerify ‖ OpCheckSigMlDsa87`
/// at `ScriptPublicKey` version 0 (ADR-0019 §8 — widened from the former
/// 32-byte BLAKE2b-256 / `OpBlake2b`+`OpData32` form). The opcode bytes are
/// pinned as literals here because `consensus-core` does not depend on full
/// `kaspa-txscript` (only `kaspa-txscript-errors`); the output is
/// **byte-identical** to
/// `kaspa_txscript::pay_to_address_script(&Address::new(_, Version::PubKeyHashMlDsa87, payload))`
/// and a parity test in the `consensus` crate
/// (`processes::coinbase`) pins that equality. The `ScriptPublicKey`
/// bytes are prefix-independent, so coinbase construction and
/// validation need agree only on the 64-byte payload.
pub fn p2pkh_mldsa87_spk(owner_reward_spk_payload: &[u8; 64]) -> ScriptPublicKey {
    // ADR-0019 §8 "Script template" opcode bytes (see
    // crypto/txscript/src/opcodes/mod.rs): OP_BLAKE2B_512 == 0xc4
    // (repurposed reserved opcode `OpUnknown196`), OP_DATA64 == 0x40.
    const OP_DUP: u8 = 0x76;
    const OP_BLAKE2B_512: u8 = 0xc4;
    const OP_DATA64: u8 = 0x40;
    const OP_EQUAL_VERIFY: u8 = 0x88;
    const OP_CHECKSIG_MLDSA87: u8 = 0xa6;

    let mut script = Vec::with_capacity(69);
    script.push(OP_DUP);
    script.push(OP_BLAKE2B_512);
    script.push(OP_DATA64);
    script.extend_from_slice(owner_reward_spk_payload);
    script.push(OP_EQUAL_VERIFY);
    script.push(OP_CHECKSIG_MLDSA87);
    // P2PKH-ML-DSA spk version is `MAX_SCRIPT_PUBLIC_KEY_VERSION` == 0.
    ScriptPublicKey::new(0, ScriptVec::from_slice(&script))
}

/// Build the validator-side coinbase outputs for a block (ADR-0013
/// §"Coinbase fan-out", as amended by Addendum B): one
/// `TransactionOutput { value: per_attestation_reward_sompi,
/// script_public_key: p2pkh_mldsa87_spk(payload) }` per included
/// attestation, in the **canonical order the caller supplies**
/// (ADR-0013 fixes that order as `(shard_index, attestation_index)`;
/// resolving each attestation → its bond's `owner_reward_spk_payload`
/// is the caller's job).
///
/// The per-block inflation cap is applied as a **whole-output**
/// truncation: at most `max_validator_inflation_per_block_sompi /
/// per_attestation_reward_sompi` outputs are emitted, dropping the
/// canonical-order tail rather than ever minting a partial reward.
/// Under correct parameterisation
/// (`per_attestation_reward_sompi × max_attestations_per_block ≤ cap`)
/// the cap never bites and every supplied payload is paid. This is
/// the binding output-construction rule;
/// [`compute_attestation_reward_payouts`] is the matching arithmetic
/// *bound* on the total (the two agree exactly whenever the cap does
/// not truncate, which is the only correctly-parameterised regime).
///
/// Pure and DAG-free so it can be unit-tested in isolation and called
/// identically from the coinbase **construction** and **validation**
/// paths (PR-10.5′-b). With `per_attestation_reward_sompi == 0` or an
/// empty payload slice it returns no outputs — so on every current
/// network (where the overlay is dormant and no attestation is
/// included) the validator side of the coinbase is empty and the
/// coinbase is byte-for-byte unchanged.
pub fn validator_reward_outputs(
    per_attestation_reward_sompi: u64,
    max_validator_inflation_per_block_sompi: u64,
    reward_spk_payloads: &[[u8; 64]],
) -> Vec<TransactionOutput> {
    if per_attestation_reward_sompi == 0 {
        return Vec::new();
    }
    // Whole-output cap: never emit a partial per-attestation reward.
    let max_payable =
        (max_validator_inflation_per_block_sompi / per_attestation_reward_sompi).min(reward_spk_payloads.len() as u64) as usize;
    reward_spk_payloads[..max_payable]
        .iter()
        .map(|payload| TransactionOutput::new(per_attestation_reward_sompi, p2pkh_mldsa87_spk(payload)))
        .collect()
}

/// Build a block's validator reward outputs from its included attestations
/// (ADR-0009 Addendum B §B.5 / ADR-0013 §"Coinbase fan-out").
///
/// `attestations` is `(bond_outpoint, epoch, owner_reward_spk_payload)` for
/// each included, already-eligibility-checked attestation, in the canonical
/// `(shard_tx_index, attestation_index)` order the caller supplies. Applies
/// **within-block** `(bond_outpoint, epoch)` uniqueness — the first
/// occurrence is rewarded, later duplicates earn nothing (§B.4) — then the
/// whole-output per-block cap via [`validator_reward_outputs`].
///
/// Pure and DAG-free so the coinbase **construction** (block-template) and
/// **validation** paths run it identically and produce byte-identical
/// outputs. With no attestations or a zero reward it returns no outputs, so
/// the coinbase is unchanged on every current network.
///
/// NOTE — cross-block (selected-chain-prefix) `(bond, epoch)` uniqueness
/// (§B.3(c)) is **not** applied here; it is the caller's responsibility via a
/// composed [`RewardedEpochSet`] and is wired in PR-10.5′-b3b. Until then only
/// within-block dedup is enforced (immaterial while the overlay is dormant).
pub fn validator_reward_outputs_from_attestations(
    per_attestation_reward_sompi: u64,
    max_validator_inflation_per_block_sompi: u64,
    attestations: &[(TransactionOutpoint, u64, [u8; 64])],
    already_rewarded: &RewardedEpochSet,
) -> (Vec<TransactionOutput>, Vec<(TransactionOutpoint, u64)>) {
    if per_attestation_reward_sompi == 0 {
        return (Vec::new(), Vec::new());
    }
    // Whole-output per-block cap (never emit a partial reward).
    let max_payable = (max_validator_inflation_per_block_sompi / per_attestation_reward_sompi) as usize;
    let mut seen_in_block: HashSet<(TransactionOutpoint, u64)> = HashSet::new();
    let mut outputs: Vec<TransactionOutput> = Vec::new();
    let mut rewarded_keys: Vec<(TransactionOutpoint, u64)> = Vec::new();
    for (bond_outpoint, epoch, payload) in attestations {
        if outputs.len() >= max_payable {
            break; // cap reached — remaining attestations earn nothing this block
        }
        let key = (*bond_outpoint, *epoch);
        // Cross-block uniqueness (§B.3(c)): skip a (bond, epoch) already
        // rewarded on the selected-chain prefix.
        if already_rewarded.contains(bond_outpoint, *epoch) {
            continue;
        }
        // Within-block uniqueness: first occurrence wins.
        if !seen_in_block.insert(key) {
            continue;
        }
        outputs.push(TransactionOutput::new(per_attestation_reward_sompi, p2pkh_mldsa87_spk(payload)));
        rewarded_keys.push(key);
    }
    (outputs, rewarded_keys)
}

/// Compute the slashing distribution for a slashed bond
/// (ADR-0013 §"Slashing distribution"). Sums exactly to
/// `slashed_amount_sompi` — no value created or destroyed by
/// rounding.
///
/// Used for the equivocation case ([`SlashingEvidencePayload`]);
/// callers that need a reward floor `min`-cap the result through
/// [`apply_unreveal_reporter_min_cap`] (separate helper for
/// clarity).
pub fn compute_slashing_distribution(
    slashed_amount_sompi: u64,
    slashing_reporter_reward_bps: u16,
    // kaspa-pq ADR-0018 "本格版" (PoS-v2): the reserve + victim shares. `0` (the pre-v2 2-way split:
    // reporter + burn, byte-identical) until the v2 fence opens.
    security_reserve_bps: u16,
    victim_epoch_pool_bps: u16,
) -> SlashingDistribution {
    // Promote to u128 for the multiplication so a max-bond × bps product cannot overflow
    // (`u64::MAX × 10000 ≈ 1.8e23`, well within u128). `bps_of` floors each share.
    let bps_of = |bps: u16| (slashed_amount_sompi as u128).saturating_mul(bps as u128) / 10000;
    // Priority reporter → reserve → victim → burn: clamp each share to what remains so a
    // misconfiguration (Σbps > 10_000) can never push `burned` negative. Under correct params
    // (Σbps ≤ 10_000) no clamp bites and the four shares sum to `slashed_amount_sompi`.
    let reporter_reward_sompi = (bps_of(slashing_reporter_reward_bps) as u64).min(slashed_amount_sompi);
    let security_reserve_sompi = (bps_of(security_reserve_bps) as u64).min(slashed_amount_sompi - reporter_reward_sompi);
    let victim_epoch_pool_sompi =
        (bps_of(victim_epoch_pool_bps) as u64).min(slashed_amount_sompi - reporter_reward_sompi - security_reserve_sompi);
    let burned_sompi = slashed_amount_sompi - reporter_reward_sompi - security_reserve_sompi - victim_epoch_pool_sompi;
    SlashingDistribution { reporter_reward_sompi, security_reserve_sompi, victim_epoch_pool_sompi, burned_sompi }
}

/// Apply the ADR-0013 `min`-cap to a reporter reward: the reporter
/// receives `min(bps_reward, unreveal_reporter_reward_sompi_floor)`,
/// and the burn share grows by whatever the reporter no longer
/// collects. A generic, caller-supplied floor; the equivocation
/// side-effect passes no floor today (the commit-reveal unreveal
/// path that originally motivated it was removed by ADR-0017).
pub fn apply_unreveal_reporter_min_cap(
    distribution: SlashingDistribution,
    unreveal_reporter_reward_sompi_floor: u64,
) -> SlashingDistribution {
    let capped_reporter = distribution.reporter_reward_sompi.min(unreveal_reporter_reward_sompi_floor);
    let extra_burn = distribution.reporter_reward_sompi - capped_reporter;
    SlashingDistribution {
        reporter_reward_sompi: capped_reporter,
        // The reserve / victim shares are unaffected by the reporter floor; the reporter's
        // uncollected remainder rolls into burn.
        security_reserve_sompi: distribution.security_reserve_sompi,
        victim_epoch_pool_sompi: distribution.victim_epoch_pool_sompi,
        burned_sompi: distribution.burned_sompi + extra_burn,
    }
}

/// Build the consensus-emitted slashing distribution (ADR-0013 Addendum C, as
/// amended by Addendum C.2): given the slashed amount `S` and the reporter's
/// declared spend payload, returns the single reporter-reward
/// [`TransactionOutput`] consensus mints as a side-effect at
/// `(slashing_tx_id, 0)` — `value =
/// compute_slashing_distribution(S, bps).reporter_reward_sompi`, `script_public_key
/// = p2pkh_mldsa87_spk(reporter_reward_spk_payload)` — and the burned amount
/// (`S − reporter_reward_sompi`, which leaves the active supply implicitly
/// when the bond's locked UTXO is removed and only the reporter reward is
/// re-minted). Pass `unreveal_floor = Some(floor)` to apply the
/// §"Slashing distribution" `min`-cap; equivocation passes `None`.
///
/// Returns `None` for the output when the reporter reward is zero (e.g.
/// `bps == 0` — everything burns), so no zero-value output is emitted. Pure
/// and DAG-free, so the slashing-tx **construction** and **validation** paths
/// produce byte-identical results.
pub fn slashing_distribution_output(
    slashed_amount_sompi: u64,
    slashing_reporter_reward_bps: u16,
    // kaspa-pq ADR-0018 "本格版" (PoS-v2): the reserve + victim shares (0 until the v2 fence).
    security_reserve_bps: u16,
    victim_epoch_pool_bps: u16,
    reporter_reward_spk_payload: &[u8; 64],
    unreveal_floor: Option<u64>,
) -> (Option<TransactionOutput>, SlashingDistribution) {
    let mut dist =
        compute_slashing_distribution(slashed_amount_sompi, slashing_reporter_reward_bps, security_reserve_bps, victim_epoch_pool_bps);
    if let Some(floor) = unreveal_floor {
        dist = apply_unreveal_reporter_min_cap(dist, floor);
    }
    let output = (dist.reporter_reward_sompi > 0)
        .then(|| TransactionOutput::new(dist.reporter_reward_sompi, p2pkh_mldsa87_spk(reporter_reward_spk_payload)));
    // Returns the full 4-way split so the caller can carry the reserve + victim shares into the
    // side-effect (the victim *outputs* are built later from the slashed epoch's accumulator).
    (output, dist)
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) §slashing — the **victim-epoch compensation** outputs: the
/// `victim_pool` (the slashed bond's victim share) distributed stake-proportionally among the
/// **honest** validators who participated in the slashed validator's epoch. `honest_included` is
/// `(owner_reward_spk_payload, included_stake)` per honest validator — the slashed epoch's
/// accumulator `included` set **with the slashed validator removed** (the caller excludes it) — and
/// `total_honest_stake` is the sum of those stakes (the denominator). Each is paid
/// [`proportional_share`]`(victim_pool, stake, total_honest_stake)` to its ML-DSA P2PKH script;
/// zero-value shares emit no output. The outputs sum to ≤ `victim_pool` (Σ honest stake =
/// `total_honest_stake`); the unspent remainder is not minted. Pure and DAG-free, so the slashing
/// **validation** and **virtual-recompute** paths build byte-identical victim outputs.
pub fn victim_compensation_outputs(
    victim_pool: u64,
    honest_included: &[(Hash64, u64)],
    total_honest_stake: u128,
) -> Vec<TransactionOutput> {
    honest_included
        .iter()
        .filter_map(|(payload, stake)| {
            let comp = proportional_share(victim_pool as u128, *stake as u128, total_honest_stake).min(u64::MAX as u128) as u64;
            (comp > 0).then(|| TransactionOutput::new(comp, p2pkh_mldsa87_spk(&payload.as_bytes())))
        })
        .collect()
}

/// The deterministic consensus side-effect of slashing one bond
/// (ADR-0013 Addendum C / ADR-0016 §D.4). Computed per genuine evidence so
/// the slashing-tx **construction** and **validation** paths agree byte-for-
/// byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashingSideEffect {
    /// The slashed bond's outpoint. Its locked output-0 UTXO (value
    /// `slashed_amount_sompi`, ADR-0016 §D.1) is removed from the UTXO set.
    pub bond_outpoint: TransactionOutpoint,
    /// `S` = the slashed amount (the bond's `amount`) that leaves the supply
    /// when the output-0 UTXO is removed.
    pub slashed_amount_sompi: u64,
    /// The reporter-reward UTXO consensus mints as a side-effect at
    /// `(slashing_tx_id, 0)` (ADR-0013 Addendum C.2), or `None` when the reward
    /// rounds to zero (everything burns). Under C.2 this is **not** an output
    /// on the slashing transaction — the transaction declares no outputs and
    /// the reward is minted atomically with the output-0 removal.
    pub reporter_output: Option<TransactionOutput>,
    /// The implicitly-burned remainder (`S − reporter_reward`): removed with
    /// the output-0 UTXO and never re-minted.
    pub burned_sompi: u64,
    /// The id of the **first** (canonical mergeset order) effective slashing
    /// transaction for this bond. The reporter reward is minted at outpoint
    /// `(slashing_tx_id, 0)`; a slashing transaction declares no outputs
    /// (ADR-0013 Addendum C.2), so index 0 is always free.
    pub slashing_tx_id: TransactionId,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): the **security-reserve** share of `S`. Accrued to the
    /// reserve pool (Phase 4); until that pool exists it stays unminted (≡ burn). `0` until the v2 fence.
    pub security_reserve_sompi: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): the **victim-epoch** pool — the share to compensate the
    /// slashed validator's honest epoch peers. `0` until the v2 fence.
    pub victim_epoch_pool_sompi: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): the slashed validator's equivocation epoch (from the
    /// evidence's attestations), used to recompute that epoch's honest included set for the victim
    /// payout.
    pub slashed_epoch: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2): the victim-compensation outputs, minted at
    /// `(slashing_tx_id, 2..)`. EMPTY from the pure resolver — filled by the processor's
    /// selected-parent-window recompute of `slashed_epoch`'s honest set (construction == validation,
    /// reorg-safe). Inert (empty) while the v2 fence is closed.
    pub victim_outputs: Vec<TransactionOutput>,
}

/// Build a block's slashing side-effects from its genuine, accepted slashing
/// evidence (ADR-0013 Addendum C / ADR-0016 §D.4).
///
/// `evidence` is `(bond_outpoint, S, reporter_reward_spk_payload)` for each
/// accepted, genuineness-checked evidence — in the canonical block order the
/// caller supplies. `S` (the slashed amount) and the reporter payload are
/// resolved by the caller against the block's selected-parent bond view (the
/// caller feeds only bonds with a live, removable output-0). Applies
/// **within-block** `bond_outpoint` uniqueness — a bond is slashed at most
/// once per block (two evidences against the same bond in one block collapse
/// to a single removal + reporter payout) — then computes each reporter output
/// + burn via [`slashing_distribution_output`].
///
/// `unreveal_floor` is `None` for equivocation evidence (a generic reward-floor
/// hook; the commit-reveal unreveal case it was built for was removed by
/// ADR-0017). Pure and DAG-free, so it is run identically by construction and
/// validation. With no evidence it returns no side-effects, so on every
/// current network (overlay dormant) it is a no-op.
pub fn slashing_side_effects_from_evidence(
    // `(bond_outpoint, S, reporter_reward_spk_payload, slashing_tx_id, slashed_epoch)`.
    evidence: &[(TransactionOutpoint, u64, [u8; 64], TransactionId, u64)],
    slashing_reporter_reward_bps: u16,
    // kaspa-pq ADR-0018 "本格版" (PoS-v2): reserve + victim shares (0 until the v2 fence ⇒ 2-way).
    security_reserve_bps: u16,
    victim_epoch_pool_bps: u16,
    unreveal_floor: Option<u64>,
) -> Vec<SlashingSideEffect> {
    let mut seen: HashSet<TransactionOutpoint> = HashSet::new();
    let mut effects = Vec::new();
    for (bond_outpoint, slashed_amount, reporter_payload, slashing_tx_id, slashed_epoch) in evidence {
        // Within-block dedup: a bond's locked UTXO can be removed only once, so
        // the **first** slashing tx targeting it (canonical order) wins both the
        // removal and the reporter mint at `(slashing_tx_id, 0)`.
        if !seen.insert(*bond_outpoint) {
            continue;
        }
        let (reporter_output, dist) = slashing_distribution_output(
            *slashed_amount,
            slashing_reporter_reward_bps,
            security_reserve_bps,
            victim_epoch_pool_bps,
            reporter_payload,
            unreveal_floor,
        );
        effects.push(SlashingSideEffect {
            bond_outpoint: *bond_outpoint,
            slashed_amount_sompi: *slashed_amount,
            reporter_output,
            burned_sompi: dist.burned_sompi,
            slashing_tx_id: *slashing_tx_id,
            security_reserve_sompi: dist.security_reserve_sompi,
            victim_epoch_pool_sompi: dist.victim_epoch_pool_sompi,
            slashed_epoch: *slashed_epoch,
            // Filled by the processor's selected-parent-window recompute of `slashed_epoch`.
            victim_outputs: Vec::new(),
        });
    }
    effects
}

/// Resolve a block's equivocation-slashing side-effects from its accepted
/// slashing evidence and the block's selected-parent active-bond view
/// (ADR-0013 Addendum C / ADR-0016 §D.4).
///
/// Each evidence's *genuineness* (its bond resolves and both equivocating
/// attestations ML-DSA-verify) is a **separate** block-validity rule
/// (`slashing_evidence_genuine` in the virtual processor) that rejects the
/// block before any side-effect is applied, so resolution may assume
/// genuineness and only needs to fix `S` and the reporter payload. For each
/// accepted evidence whose bond resolves to `Active` or `Unbonding` (per
/// [`effective_bond_status`] at the block's `daa_score`) — the only states
/// still holding a removable locked output-0 — `S` is the bond's `amount` and
/// the reporter payload is taken from the evidence. A bond that is `Pending`,
/// already `Slashed`, or unknown in the view yields no side-effect, so a stake
/// is never removed twice. The resolved `(bond_outpoint, S, payload)` triples
/// (in canonical block order) are handed to
/// [`slashing_side_effects_from_evidence`], which applies within-block
/// `bond_outpoint` uniqueness and computes each reporter output + burn.
///
/// Equivocation only — `unreveal_floor` is `None`. (The commit-reveal unreveal
/// slash path was removed by ADR-0017.) Pure and DAG-free, so the slashing-tx
/// **construction**
/// and **validation** paths produce identical side-effects. With no accepted
/// evidence it returns nothing, so on every current network (overlay dormant)
/// it is a no-op.
pub fn resolve_slashing_side_effects(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    daa_score: u64,
    slashing_reporter_reward_bps: u16,
    // kaspa-pq ADR-0018 "本格版" (PoS-v2): reserve + victim shares (0 until the v2 fence ⇒ 2-way).
    security_reserve_bps: u16,
    victim_epoch_pool_bps: u16,
) -> Vec<SlashingSideEffect> {
    // `(bond_outpoint, S, reporter_reward_spk_payload, slashing_tx_id, slashed_epoch)`.
    let mut resolved: Vec<(TransactionOutpoint, u64, [u8; 64], TransactionId, u64)> = Vec::new();
    // Iterate the txs directly (rather than via `slashing_evidence_from_accepted_txs`)
    // so each evidence keeps its **slashing tx id** — the mint outpoint
    // `(slashing_tx_id, 0)` under Addendum C.2.
    for tx in txs {
        if dns_tx_kind(&tx.subnetwork_id) != Some(DnsTxKind::SlashingEvidence) {
            continue;
        }
        let Ok(ev) = borsh::from_slice::<SlashingEvidencePayload>(&tx.payload) else {
            continue;
        };
        let Some(bond) = bond_view.get(&ev.bond_outpoint) else {
            continue;
        };
        // Only a bond whose stake is still locked (Active/Unbonding) has a
        // removable output-0 UTXO; Pending/Slashed/unknown contribute nothing.
        if matches!(effective_bond_status(bond, daa_score), BondStatus::Active | BondStatus::Unbonding) {
            // The slashed (equivocation) epoch is the attestations' shared epoch (both attestations
            // are for the same epoch — that is what makes them equivocating).
            resolved.push((ev.bond_outpoint, bond.amount, ev.reporter_reward_spk_payload, tx.id(), ev.attestation_a.epoch));
        }
    }
    slashing_side_effects_from_evidence(&resolved, slashing_reporter_reward_bps, security_reserve_bps, victim_epoch_pool_bps, None)
}

// ---------------------------------------------------------------------
// Consensus rule implementations (PR-10.5).
//
// `compute_stake_score` and `check_dns_reorg_rule` below replace the
// PR-10.3 `*_stub` panics with the real deterministic logic from
// ADR-0009. They are pure functions: the DAG-dependent facts (per-epoch
// signed/total stake, common-ancestor work/stake split, whether the
// candidate keeps the confirmed anchor) are computed by the consensus
// pipeline in a later PR and fed in here, so the rule itself stays
// unit-testable in isolation and free of any `RuleError` dependency.
// ---------------------------------------------------------------------

/// Per-epoch stake tally fed into [`compute_stake_score`] (ADR-0009
/// §"StakeScore mechanics"). The caller enforces the
/// `(bond_outpoint, validator_id, epoch)` uniqueness rule, so
/// `signed_stake_sompi` already excludes any validator double-counted
/// across attestation shards.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EpochStakeTally {
    pub epoch: u64,
    /// Deduplicated active stake whose validators signed this epoch's
    /// selected-chain anchor.
    pub signed_stake_sompi: u64,
    /// Total active stake at this epoch (the normalisation denominator).
    pub total_active_stake_sompi: u64,
}

/// Per-epoch **quality-gated** StakeScore credit (ADR-0018 §B; refines ADR-0009
/// §"StakeScore mechanics"):
///
/// ```text
/// f = included_stake / expected_stake                        (clamped to 1.0)
/// credit = 0,                                      if f < φS
///        = (f − φS) / (1 − φS) × STAKE_SCORE_SCALE, otherwise
/// ```
///
/// `φS` (`quality_floor_bps`, basis points) is the **stake-event quality floor** — an
/// epoch below it earns nothing, so a minority included-stake fraction can never
/// accumulate StakeScore (the attack the prior linear credit allowed). `φS == 0`
/// reproduces that linear credit **exactly** (the `10_000` factors cancel). The numerator
/// is clamped to the denominator (an over-count cannot inflate the score) and the
/// arithmetic is integer `u128` throughout (no floats; `saturating_mul` is defensive —
/// the real products are far below `u128::MAX`).
pub fn epoch_stake_credit(included_stake: u128, expected_stake: u128, quality_floor_bps: u16) -> u128 {
    if expected_stake == 0 {
        return 0;
    }
    let expected = expected_stake;
    let included = included_stake.min(expected);
    let floor = (quality_floor_bps as u128).min(10_000);
    // f < φS  ⟺  included × 10_000 < expected × floor.
    if included.saturating_mul(10_000) < expected.saturating_mul(floor) {
        return 0;
    }
    let denom = 10_000 - floor; // (1 − φS) in bps units
    if denom == 0 {
        // φS == 100%: only full inclusion reaches here, and earns the full scale.
        return STAKE_SCORE_SCALE;
    }
    // (included×10_000 − expected×floor) × SCALE / (expected × denom).
    let numerator = included * 10_000 - expected * floor;
    numerator.saturating_mul(STAKE_SCORE_SCALE) / expected.saturating_mul(denom)
}

/// Deterministic `StakeScore(H)` aggregation over the epochs whose anchors lie on the
/// selected chain ending at the target (ADR-0009 §"StakeScore mechanics", quality-gated
/// per ADR-0018 §B). Each epoch's credit passes through the `quality_floor_bps` φS gate
/// ([`epoch_stake_credit`]); every node observing the same on-chain shard set + the same
/// `φS` reaches the same number — integer `u128` throughout, no floats.
pub fn compute_stake_score(per_epoch: &[EpochStakeTally], quality_floor_bps: u16) -> StakeScore {
    let mut acc: u128 = 0;
    for e in per_epoch {
        acc = acc.saturating_add(epoch_stake_credit(
            e.signed_stake_sompi as u128,
            e.total_active_stake_sompi as u128,
            quality_floor_bps,
        ));
    }
    StakeScore(acc)
}

/// Derive the read-only [`DnsHealth`] signal (ADR-0018 §C) from the per-epoch tallies of
/// the bounded StakeScore window (sorted ascending by epoch, as produced by
/// [`aggregate_epoch_tallies`]). **Not** a consensus gate — purely a liveness signal that
/// never affects block validity.
///
/// - `overlay_active == false` → `DisabledBeforeActivation`.
/// - Healthy (`Active`) if any of the last `degraded_epochs` epochs meets φS
///   (`quality_floor_bps`), or if there is less than `degraded_epochs` epochs of history.
/// - Otherwise the last `degraded_epochs` epochs are all below φS → degraded:
///   `DegradedCertificateCensored` when they are **all** below the near-zero
///   `censorship_floor_bps` (the censorship signature), else `DegradedStakeQualityLow`.
pub fn derive_dns_health(
    per_epoch: &[EpochStakeTally],
    quality_floor_bps: u16,
    censorship_floor_bps: u16,
    degraded_epochs: u32,
    overlay_active: bool,
) -> DnsHealth {
    if !overlay_active {
        return DnsHealth::DisabledBeforeActivation;
    }
    let m = (degraded_epochs as usize).max(1);
    if per_epoch.len() < m {
        return DnsHealth::Active; // insufficient history for a sustained-degradation signal
    }
    let window = &per_epoch[per_epoch.len() - m..];
    let frac_bps = |e: &EpochStakeTally| -> u128 {
        if e.total_active_stake_sompi == 0 {
            return 0;
        }
        (e.signed_stake_sompi.min(e.total_active_stake_sompi) as u128) * 10_000 / (e.total_active_stake_sompi as u128)
    };
    if window.iter().any(|e| frac_bps(e) >= quality_floor_bps as u128) {
        return DnsHealth::Active; // a recent epoch met φS
    }
    if window.iter().all(|e| frac_bps(e) < censorship_floor_bps as u128) {
        DnsHealth::DegradedCertificateCensored
    } else {
        DnsHealth::DegradedStakeQualityLow
    }
}

/// History-confirmation predicate — `WorkDepth(B) ≥ cW ∧ StakeDepth(B) ≥ cS`. An anchor is
/// DNS-confirmed iff it clears **both** thresholds. Used by the consensus pipeline to advance
/// [`DnsState::last_dns_confirmed_anchor`].
///
/// audit H-02 — what this is and is NOT: on the production presets `cW = required_work_depth =
/// BlueWorkType::ZERO`, so the work term is satisfied trivially and confirmation is effectively
/// **stake-depth only** (a stake-confirmed canonical lagged anchor). This is NOT the "Double
/// Nakamoto" claim of an independent PoW *and* PoS confirmation count — the PoW dimension does NOT
/// gate confirmation here. The two-dimensional **finality safety** (non-substitutability: a heavier
/// PoW chain cannot rewrite a stake-confirmed anchor) is enforced separately by the reorg gate
/// [`check_dns_reorg_rule`], which requires BOTH a WorkScore and a StakeScore dominance margin over
/// canonical SINCE THE COMMON ANCESTOR (deltas, not the absolute cumulative `work_depth` read here).
/// Set `cW > 0` only if you also switch `work_depth` to an anchor-relative delta (see the audit note).
pub fn is_dns_confirmed(
    work_depth: BlueWorkType,
    stake_depth: StakeScore,
    required_work_depth: BlueWorkType,
    required_stake_depth: StakeScore,
) -> bool {
    work_depth >= required_work_depth && stake_depth >= required_stake_depth
}

/// Reorg-gate mode (ADR-0009 §"Phase-specific behaviour").
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum DnsReorgMode {
    /// PoC / testnet: a candidate that exits the latest DNS-confirmed
    /// anchor is rejected outright. Loud and easy to test; **not** DNS
    /// finality — a testing convenience per ADR-0009 §"Public-claim
    /// discipline".
    #[default]
    HardCheckpoint = 0,
    /// Mainnet: the two-dimensional `WorkScore × StakeScore`
    /// non-substitutability gate.
    TwoDimensionalDominance = 1,
}

/// Inputs to [`check_dns_reorg_rule`]. The DAG-dependent facts are
/// computed by the consensus pipeline (later PR) and passed in, keeping
/// the decision a pure, unit-testable function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsReorgInputs {
    pub rollout_stage: DnsRolloutStage,
    pub mode: DnsReorgMode,
    /// `true` iff the candidate chain still contains the latest
    /// DNS-confirmed anchor (it does not rewrite confirmed history).
    pub candidate_includes_confirmed_anchor: bool,
    /// `WorkScore` accumulated by each chain *after* the common
    /// ancestor `I = common_ancestor(candidate, canonical_tip)`.
    pub candidate_work_after: BlueWorkType,
    pub canonical_work_after: BlueWorkType,
    /// `StakeScore` accumulated after `I`.
    pub candidate_stake_after: StakeScore,
    pub canonical_stake_after: StakeScore,
    pub emergency_work_margin: BlueWorkType,
    pub emergency_stake_margin: StakeScore,
}

/// Outcome of the DNS reorg gate. The consensus pipeline maps the
/// reject variants to a `RuleError`; surfacing a rich enum keeps
/// consensus-core free of that dependency and mirrors the
/// [`SignedEpochCheckOutcome`] style.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DnsReorgOutcome {
    /// Not in `Active` rollout stage — the gate is dormant and
    /// PoW/GHOSTDAG decides alone (Phases 1–9 behaviour).
    GateInactive,
    /// The candidate keeps the latest DNS-confirmed anchor; confirmed
    /// history is not rewritten, so the gate does not trigger.
    IncludesConfirmedAnchor,
    /// Mainnet path: the candidate exits the confirmed prefix and beats
    /// canonical on **both** `WorkScore` and `StakeScore` by the
    /// emergency margins. The rare legitimate deep-reorg path.
    DominanceSatisfied,
    /// PoC / testnet hard-checkpoint reject: candidate exits the
    /// confirmed prefix.
    HardCheckpointReject,
    /// Mainnet reject: candidate exits the confirmed prefix but fails
    /// the two-dimensional dominance test (a PoW-only or stake-only
    /// attacker lands here — non-substitutability).
    DominanceViolation,
}

impl DnsReorgOutcome {
    /// `true` for the accept variants (gate dormant, anchor retained,
    /// or dominance satisfied).
    pub fn is_accept(self) -> bool {
        matches!(self, DnsReorgOutcome::GateInactive | DnsReorgOutcome::IncludesConfirmedAnchor | DnsReorgOutcome::DominanceSatisfied)
    }
}

/// Pure decision for the DNS reorg gate (ADR-0009 §"Decision" +
/// §"Phase-specific behaviour"). Replaces the PR-10.3
/// `check_dns_reorg_rule_stub`.
///
/// Two-dimensional **non-substitutability**: in mainnet mode a
/// candidate that exits the DNS-confirmed prefix must *strictly* beat
/// canonical on `WorkScore` **and** `StakeScore`, each by its emergency
/// margin. A PoW-only surplus or a stake-only surplus is rejected —
/// "PoW surplus does not substitute for PoS deficit and vice versa".
pub fn check_dns_reorg_rule(inputs: &DnsReorgInputs) -> DnsReorgOutcome {
    // The gate engages only in the Active rollout stage (ADR-0009
    // §"Three-stage rollout"); Launch/Bootstrap run pure PoW/GHOSTDAG.
    if inputs.rollout_stage != DnsRolloutStage::Active {
        return DnsReorgOutcome::GateInactive;
    }
    // A candidate that still contains the confirmed anchor does not
    // rewrite confirmed history — the gate does not trigger.
    if inputs.candidate_includes_confirmed_anchor {
        return DnsReorgOutcome::IncludesConfirmedAnchor;
    }
    // The candidate exits the DNS-confirmed prefix.
    match inputs.mode {
        DnsReorgMode::HardCheckpoint => DnsReorgOutcome::HardCheckpointReject,
        DnsReorgMode::TwoDimensionalDominance => {
            // `saturating_add` so an (astronomically unlikely) margin
            // overflow conservatively makes the bound un-beatable.
            let work_bound = inputs.canonical_work_after.saturating_add(inputs.emergency_work_margin);
            let stake_bound = inputs.canonical_stake_after.0.saturating_add(inputs.emergency_stake_margin.0);
            let work_ok = inputs.candidate_work_after > work_bound;
            let stake_ok = inputs.candidate_stake_after.0 > stake_bound;
            if work_ok && stake_ok { DnsReorgOutcome::DominanceSatisfied } else { DnsReorgOutcome::DominanceViolation }
        }
    }
}

/// ADR-0018 §H — assemble [`DnsReorgInputs`] from the raw per-chain facts the
/// consensus pipeline gathers, computing the `*_work_after` fields as the
/// **WorkScore accumulated since the common ancestor** `I = common_ancestor(
/// candidate, canonical_tip)`: `blue_work(tip) − blue_work(I)`. Blue work is a
/// cumulative GHOSTDAG quantity, so this delta is an exact `saturating_sub` (a
/// `tip` that is itself `I` contributes zero, the floor).
///
/// `candidate_stake_after` / `canonical_stake_after` are passed in already
/// reduced to the since-`I` delta — StakeScore is a *windowed*, non-cumulative
/// quantity (see [`compute_stake_score`]), so its since-ancestor value is **not**
/// a subtraction and must be computed per branch by the caller (the heavier
/// DAG walk wired in a follow-up). Keeping that out of this helper leaves the
/// Work-dimension derivation pure and unit-testable, and makes the two-input
/// shape explicit for the gate.
#[allow(clippy::too_many_arguments)]
pub fn reorg_inputs_since_common_ancestor(
    rollout_stage: DnsRolloutStage,
    mode: DnsReorgMode,
    candidate_includes_confirmed_anchor: bool,
    candidate_blue_work: BlueWorkType,
    canonical_blue_work: BlueWorkType,
    common_ancestor_blue_work: BlueWorkType,
    candidate_stake_after: StakeScore,
    canonical_stake_after: StakeScore,
    emergency_work_margin: BlueWorkType,
    emergency_stake_margin: StakeScore,
) -> DnsReorgInputs {
    DnsReorgInputs {
        rollout_stage,
        mode,
        candidate_includes_confirmed_anchor,
        candidate_work_after: candidate_blue_work.saturating_sub(common_ancestor_blue_work),
        canonical_work_after: canonical_blue_work.saturating_sub(common_ancestor_blue_work),
        candidate_stake_after,
        canonical_stake_after,
        emergency_work_margin,
        emergency_stake_margin,
    }
}

// =====================================================================
// PR-10.4: DNS finality overlay transaction kinds + stateless payload
// validation (ADR-0009 §"On-chain artefacts").
//
// `dns_tx_kind` maps a routed subnetwork id to its payload kind; the
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

/// Payload kind carried by a DNS finality overlay transaction, keyed by
/// its routed subnetwork id (`SubnetworkId::is_dns_overlay`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DnsTxKind {
    /// `SUBNETWORK_ID_STAKE_BOND` — [`StakeBondPayload`].
    StakeBond,
    /// `SUBNETWORK_ID_STAKE_ATTESTATION_SHARD` — [`StakeAttestationShardPayload`].
    StakeAttestationShard,
    /// `SUBNETWORK_ID_SLASHING_EVIDENCE` — [`SlashingEvidencePayload`].
    SlashingEvidence,
    /// `SUBNETWORK_ID_STAKE_UNBOND` — [`StakeUnbondRequestPayload`].
    StakeUnbond,
}

/// Maps a subnetwork id to its DNS overlay payload kind, or `None` for a
/// non-overlay subnetwork (native / coinbase / registry / unknown). The
/// mirror of [`SubnetworkId::is_dns_overlay`] that also names the kind.
pub fn dns_tx_kind(subnetwork_id: &SubnetworkId) -> Option<DnsTxKind> {
    if *subnetwork_id == SUBNETWORK_ID_STAKE_BOND {
        Some(DnsTxKind::StakeBond)
    } else if *subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD {
        Some(DnsTxKind::StakeAttestationShard)
    } else if *subnetwork_id == SUBNETWORK_ID_SLASHING_EVIDENCE {
        Some(DnsTxKind::SlashingEvidence)
    } else if *subnetwork_id == SUBNETWORK_ID_STAKE_UNBOND {
        Some(DnsTxKind::StakeUnbond)
    } else {
        None
    }
}

/// Stateless validation failure for a DNS overlay transaction payload.
/// The consensus tx-validation layer wraps this in
/// `TxRuleError::InvalidDnsOverlayPayload`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DnsTxError {
    /// Payload bytes did not borsh-decode into the expected type (also
    /// fires on trailing bytes after an otherwise-valid prefix).
    Decode,
    /// The `version` field is not `DNS_PAYLOAD_VERSION_V1`.
    UnsupportedVersion(u16),
    /// Stake-bond amount is zero (a bond must lock non-zero stake).
    ZeroBondAmount,
    /// Validator public key is not exactly `STAKE_VALIDATOR_PUBKEY_LEN`.
    InvalidPubKeyLen(usize),
    /// The declared `validator_pubkey_hash` is not the canonical
    /// `validator_id_from_pubkey(validator_pubkey)` (audit H-04): the overlay
    /// identity must be the unkeyed BLAKE2b-512 of the bond's own key.
    ValidatorPubkeyHashMismatch,
    /// An attestation signature is not exactly `STAKE_ATTESTATION_SIG_LEN`.
    InvalidSignatureLen(usize),
    /// An attestation shard carries no attestations.
    EmptyShard,
    /// An attestation shard exceeds `MAX_ATTESTATIONS_PER_SHARD`.
    ShardTooLarge(usize),
    /// An attestation in a shard does not match the shard's
    /// `(epoch, target_hash, validator_set_commitment)` tuple.
    ShardTupleMismatch,
    /// An attestation declares a non-zero `validator_set_commitment` (audit #4):
    /// ADR-0017 dropped the sortition committee, so the VSC is a fixed-zero wire
    /// invariant; a non-zero value is rejected at the stateless layer.
    NonZeroValidatorSetCommitment,
    /// The two attestations in slashing evidence do not share the same
    /// `(bond_outpoint, validator_id, epoch)` triple.
    EvidenceTripleMismatch,
    /// The two attestations approve the same anchor — not equivocation,
    /// so they are not slashable evidence.
    EvidenceNotIncompatible,
    /// ADR-0016 D.1: a `StakeBond` tx has no output-0 to lock the stake in.
    MissingBondOutput,
    /// ADR-0016 D.1: the bond output-0 value does not equal `amount`
    /// (`expected` = payload `amount`, `got` = output-0 value).
    BondOutputValueMismatch { expected: u64, got: u64 },
    /// ADR-0016 D.1: the bond output-0 script is not the owner's
    /// canonical P2PKH-ML-DSA script over `owner_reward_spk_payload`.
    BondOutputScriptMismatch,
    /// ADR-0013 Addendum C.2: a `SlashingEvidence` tx must declare **no**
    /// outputs — it is a pure evidence carrier whose reporter reward is
    /// minted by consensus as a side-effect at `(slashing_tx_id, 0)`, so any
    /// declared output (even a zero-value one) would collide with that mint.
    /// `n` is the offending output count.
    SlashingEvidenceHasOutputs(usize),
}

impl Display for DnsTxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DnsTxError::Decode => write!(f, "DNS overlay payload failed to decode"),
            DnsTxError::UnsupportedVersion(v) => write!(f, "unsupported DNS overlay payload version {v}"),
            DnsTxError::ZeroBondAmount => write!(f, "stake-bond amount must be non-zero"),
            DnsTxError::InvalidPubKeyLen(n) => write!(f, "validator public key length {n} is invalid"),
            DnsTxError::ValidatorPubkeyHashMismatch => {
                write!(f, "stake-bond validator_pubkey_hash is not BLAKE2b-512(validator_pubkey)")
            }
            DnsTxError::InvalidSignatureLen(n) => write!(f, "attestation signature length {n} is invalid"),
            DnsTxError::EmptyShard => write!(f, "attestation shard is empty"),
            DnsTxError::ShardTooLarge(n) => write!(f, "attestation shard has {n} attestations, above the maximum"),
            DnsTxError::ShardTupleMismatch => write!(f, "attestation does not match the shard's anchor tuple"),
            DnsTxError::NonZeroValidatorSetCommitment => write!(f, "attestation validator_set_commitment must be zero (ADR-0017)"),
            DnsTxError::EvidenceTripleMismatch => {
                write!(f, "slashing evidence attestations are not from the same (bond, validator, epoch) triple")
            }
            DnsTxError::EvidenceNotIncompatible => write!(f, "slashing evidence attestations approve the same anchor"),
            DnsTxError::MissingBondOutput => write!(f, "stake-bond tx has no output-0 to lock the stake"),
            DnsTxError::BondOutputValueMismatch { expected, got } => {
                write!(f, "stake-bond output-0 value {got} does not equal the bonded amount {expected}")
            }
            DnsTxError::BondOutputScriptMismatch => write!(f, "stake-bond output-0 is not the owner's P2PKH-ML-DSA script"),
            DnsTxError::SlashingEvidenceHasOutputs(n) => {
                write!(f, "slashing-evidence tx must declare no outputs but has {n}")
            }
        }
    }
}

/// Borsh-decode a DNS overlay payload, mapping any decode failure (bad
/// bytes *or* trailing data — `borsh::from_slice` rejects both) to
/// [`DnsTxError::Decode`].
fn decode_dns_payload<T: BorshDeserialize>(payload: &[u8]) -> Result<T, DnsTxError> {
    borsh::from_slice::<T>(payload).map_err(|_| DnsTxError::Decode)
}

/// Per-attestation version + ML-DSA signature-length invariants, shared
/// by the shard and the slashing-evidence validators.
fn check_attestation_wellformed(att: &StakeAttestation) -> Result<(), DnsTxError> {
    if att.version != DNS_PAYLOAD_VERSION_V1 {
        return Err(DnsTxError::UnsupportedVersion(att.version));
    }
    if att.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(DnsTxError::InvalidSignatureLen(att.signature.len()));
    }
    // audit #4: the validator_set_commitment is a fixed-zero wire invariant (ADR-0017 dropped the
    // sortition committee). Enforce it at the single per-attestation gate that BOTH the shard
    // (`validate_stake_attestation_shard_payload`) and the slashing-evidence
    // (`validate_slashing_evidence_payload`) paths funnel through, so no attestation with a
    // non-zero VSC ever reaches a block and the downstream eligibility / StakeScore paths only
    // ever see VSC == 0 (the signed digest's VSC field is then always zero too).
    if att.validator_set_commitment != Hash64::default() {
        return Err(DnsTxError::NonZeroValidatorSetCommitment);
    }
    Ok(())
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
pub fn validate_stake_bond_payload(payload: &[u8]) -> Result<(), DnsTxError> {
    decode_and_check_stake_bond_payload(payload).map(|_| ())
}

fn decode_and_check_stake_bond_payload(payload: &[u8]) -> Result<StakeBondPayload, DnsTxError> {
    let bond: StakeBondPayload = decode_dns_payload(payload)?;
    if bond.version != DNS_PAYLOAD_VERSION_V1 {
        return Err(DnsTxError::UnsupportedVersion(bond.version));
    }
    if bond.amount == 0 {
        return Err(DnsTxError::ZeroBondAmount);
    }
    if bond.validator_pubkey.len() != STAKE_VALIDATOR_PUBKEY_LEN {
        return Err(DnsTxError::InvalidPubKeyLen(bond.validator_pubkey.len()));
    }
    // audit H-04 / ADR-0008/0012: bind the declared overlay identity to the key.
    // `validator_id` is the canonical unkeyed BLAKE2b-512(validator_pubkey); a bond
    // must not declare a `validator_pubkey_hash` that is not derived from its own
    // key, or validator identity stops being uniquely key-derived (breaking
    // dup-detection, the validator-set commitment, and external key->id monitoring).
    // Purely intra-payload, so enforced statelessly here.
    if bond.validator_pubkey_hash != validator_id_from_pubkey(&bond.validator_pubkey) {
        return Err(DnsTxError::ValidatorPubkeyHashMismatch);
    }
    Ok(bond)
}

/// Stateless validation of a whole `StakeBond` **transaction** (ADR-0016
/// D.1 stake-lock rule, on top of [`validate_stake_bond_payload`]): its
/// output-0 (the bond outpoint, ADR-0009 Addendum A.1) must lock the
/// declared stake — `value == amount` and `script_public_key ==
/// p2pkh_mldsa87_spk(owner_reward_spk_payload)` (the owner's P2PKH-ML-DSA
/// address). This makes `amount` real, owner-controlled coins parked at
/// output-0 rather than a self-declared number, closing the fake-stake
/// hole and giving slashing (ADR-0013 Addendum C) something to consume.
///
/// Stateless (it inspects only the transaction's own output-0), so it runs
/// in the PR-10.4 isolation validator alongside the payload checks. Like
/// all DNS-overlay stateless validation it is "always-on" for
/// `StakeBond`-subnetwork txs, but inert in practice: no `StakeBond` tx is
/// submitted on any network while the overlay is dormant.
pub fn validate_stake_bond_tx(payload: &[u8], outputs: &[TransactionOutput]) -> Result<(), DnsTxError> {
    let bond = decode_and_check_stake_bond_payload(payload)?;
    let output0 = outputs.first().ok_or(DnsTxError::MissingBondOutput)?;
    if output0.value != bond.amount {
        return Err(DnsTxError::BondOutputValueMismatch { expected: bond.amount, got: output0.value });
    }
    if output0.script_public_key != p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload) {
        return Err(DnsTxError::BondOutputScriptMismatch);
    }
    Ok(())
}

/// Stateless validation of a [`StakeAttestationShardPayload`] (subnetwork
/// `SUBNETWORK_ID_STAKE_ATTESTATION_SHARD`): decodability, payload
/// version, shard cardinality (`1..=MAX_ATTESTATIONS_PER_SHARD`), and
/// that every attestation is well-formed **and** shares the shard's
/// `(epoch, target_hash, validator_set_commitment)` tuple — the PR-10.4
/// single-anchor-per-shard rule. Signature verification and the
/// `(bond_outpoint, validator_id, epoch)` on-chain uniqueness rule are
/// deferred to the stateful PR.
pub fn validate_stake_attestation_shard_payload(payload: &[u8]) -> Result<(), DnsTxError> {
    let shard: StakeAttestationShardPayload = decode_dns_payload(payload)?;
    if shard.version != DNS_PAYLOAD_VERSION_V1 {
        return Err(DnsTxError::UnsupportedVersion(shard.version));
    }
    if shard.attestations.is_empty() {
        return Err(DnsTxError::EmptyShard);
    }
    if shard.attestations.len() > MAX_ATTESTATIONS_PER_SHARD {
        return Err(DnsTxError::ShardTooLarge(shard.attestations.len()));
    }
    for att in &shard.attestations {
        check_attestation_wellformed(att)?;
        if att.epoch != shard.epoch
            || att.target_hash != shard.target_hash
            || att.target_daa_score != shard.target_daa_score
            || att.validator_set_commitment != shard.validator_set_commitment
        {
            return Err(DnsTxError::ShardTupleMismatch);
        }
    }
    Ok(())
}

/// Stateless validation of a [`SlashingEvidencePayload`] (subnetwork
/// `SUBNETWORK_ID_SLASHING_EVIDENCE`): decodability, payload version,
/// both attestations well-formed and sharing the same
/// `(bond_outpoint, validator_id, epoch)` triple (bound to the payload's
/// own `bond_outpoint`), and *incompatible* — approving different anchors
/// (`target_hash` differs), which is the equivocation being punished.
/// Signature verification and the `evidence_window_blocks` recency check
/// are deferred to the stateful PR.
pub fn validate_slashing_evidence_payload(payload: &[u8]) -> Result<(), DnsTxError> {
    let ev: SlashingEvidencePayload = decode_dns_payload(payload)?;
    if ev.version != DNS_PAYLOAD_VERSION_V1 {
        return Err(DnsTxError::UnsupportedVersion(ev.version));
    }
    check_attestation_wellformed(&ev.attestation_a)?;
    check_attestation_wellformed(&ev.attestation_b)?;
    let (a, b) = (&ev.attestation_a, &ev.attestation_b);
    // Same (bond_outpoint, validator_id, epoch) triple, both bound to the
    // payload's own bond_outpoint.
    if a.bond_outpoint != ev.bond_outpoint
        || b.bond_outpoint != ev.bond_outpoint
        || a.validator_id != b.validator_id
        || a.epoch != b.epoch
    {
        return Err(DnsTxError::EvidenceTripleMismatch);
    }
    // Incompatible == different anchors at the same epoch (equivocation).
    if a.target_hash == b.target_hash {
        return Err(DnsTxError::EvidenceNotIncompatible);
    }
    Ok(())
}

/// Stateless validation of a `SUBNETWORK_ID_SLASHING_EVIDENCE` **transaction**
/// (ADR-0013 Addendum C.2): its payload is valid equivocation evidence
/// ([`validate_slashing_evidence_payload`]) **and** it declares **no outputs**.
///
/// A slashing transaction is a pure evidence carrier; its reporter reward is
/// minted by consensus as a side-effect at `(slashing_tx_id, 0)` (Addendum
/// C.2), so any declared output — even a zero-value one, which would pass the
/// `Σ out ≤ Σ in` rule — would create a UTXO at that outpoint and collide with
/// the mint. Enforcing "no outputs" here in the isolation validator (body
/// processing, which runs for **every** block) guarantees the mint outpoint is
/// always free, including for slashing txs in non-chain merged-blue blocks that
/// `verify_expected_utxo_state` never sees. Mirrors the §D.1
/// [`validate_stake_bond_tx`] output rule.
pub fn validate_slashing_evidence_tx(payload: &[u8], outputs: &[TransactionOutput]) -> Result<(), DnsTxError> {
    validate_slashing_evidence_payload(payload)?;
    if !outputs.is_empty() {
        return Err(DnsTxError::SlashingEvidenceHasOutputs(outputs.len()));
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
//     `effective_bond_status` to gate attestation/slashing txs against an
//     existing, active bond at the point-of-view DAA score).
//
// `effective_bond_status` derives the bond's status purely from its
// DAA-stamped fields (activation / unbond-request / slash height) rather
// than trusting the cached `status` field, so a single source of truth
// governs eligibility regardless of when the cached field was last
// written. ADR-0009 §"Stake bonds" + ADR-0010 §"Validator service
// runtime".
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
///    attestations accepted, per ADR-0010).
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
/// predicate the PR-10.9c attestation/slashing stateful checks apply to a
/// referenced bond (`bond ∈ active_bonds`, ADR-0010).
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
/// DNS-overlay transactions on the selected chain (ADR-0009 Addendum A.4).
/// The virtual processor **applies** these for a block joining the selected
/// chain and **reverts** them for a block leaving it (reorg), exactly like
/// the UTXO set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BondMutation {
    /// A new bond created by a `StakeBondPayload` tx, keyed by its output-0
    /// outpoint (Addendum A.1). Apply = insert; revert = delete.
    Insert(TransactionOutpoint, StakeBondRecord),
    /// A bond burned by accepted `SlashingEvidencePayload`, stamped with the
    /// accepting block's DAA score. Apply = set `slashed_at_daa_score`;
    /// revert = clear it.
    Slash(TransactionOutpoint, u64),
    /// A bond moved to `Unbonding` by an accepted `StakeUnbondRequest` (audit
    /// H-05), stamped with the accepting block's DAA score. Apply = set
    /// `unbond_request_daa_score`; revert = clear it. The stateful rule rejects
    /// unbond on a non-`Pending`/`Active` bond, so at most one applies per bond
    /// per chain (clean revert).
    Unbond(TransactionOutpoint, u64),
}

/// Derives the ordered [`BondMutation`]s implied by a chain block's
/// **accepted** transactions (ADR-0009 Addendum A.4 / A.1).
///
/// Pure: decodes the DNS-overlay payloads (defensively skipping any that
/// fail to decode — a committed block's txs already passed PR-10.4 stateless
/// validation, so this is belt-and-suspenders) and maps them to store
/// mutations. The caller decides which txs count as "accepted"; this
/// function is agnostic to that selection so it stays unit-testable.
///
/// - `StakeBond` → `Insert` at `bond_outpoint = (tx.id(), 0)`.
/// - `SlashingEvidence` → `Slash(payload.bond_outpoint, accepted_daa_score)`.
/// - `StakeAttestationShard` → nothing (it feeds the StakeScore aggregation
///   in A.5, not the bond set).
///
/// Bond *activation* is **not** stamped here; it is derived at read time
/// from the payload's `activation_daa_score` (see [`effective_bond_status`]).
pub fn bond_mutations_from_accepted_txs(
    txs: &[Transaction],
    accepted_daa_score: u64,
    min_bond_amount_sompi: u64,
    unbonding_floor_blocks: u64,
) -> Vec<BondMutation> {
    let mut muts = Vec::new();
    for tx in txs {
        match dns_tx_kind(&tx.subnetwork_id) {
            Some(DnsTxKind::StakeBond) => {
                if let Ok(payload) = borsh::from_slice::<StakeBondPayload>(&tx.payload) {
                    // Per-bond minimum stake: a bond below the network floor is NOT admitted to
                    // the registry (so it can never become Active / attest). Deterministic, so
                    // the coinbase-construction and validation passes agree.
                    if payload.amount < min_bond_amount_sompi {
                        continue;
                    }
                    let outpoint = TransactionOutpoint::new(tx.id(), 0);
                    let mut record = stake_bond_record_from_payload(&payload, outpoint);
                    // kaspa-pq DNS v2 (P-1B): activation is NON-RETROACTIVE. A bond may not declare a
                    // past `activation_daa_score` and thereby insert itself into a historical epoch's
                    // active set, which would retroactively change that epoch's StakeScore / reward
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
            Some(DnsTxKind::SlashingEvidence) => {
                if let Ok(payload) = borsh::from_slice::<SlashingEvidencePayload>(&tx.payload) {
                    muts.push(BondMutation::Slash(payload.bond_outpoint, accepted_daa_score));
                }
            }
            Some(DnsTxKind::StakeUnbond) => {
                // audit H-05: an accepted, owner-authorized unbond request stamps the bond's
                // unbond clock. Authorization (owner-key binding + signature) and the
                // Pending/Active precondition are enforced by `unbond_request_authorized` as a
                // block-validity rule, so any unbond reaching here is valid and applies once.
                if let Ok(req) = borsh::from_slice::<StakeUnbondRequestPayload>(&tx.payload) {
                    muts.push(BondMutation::Unbond(req.bond_outpoint, accepted_daa_score));
                }
            }
            Some(DnsTxKind::StakeAttestationShard) | None => {}
        }
    }
    muts
}

/// Per-block **active-bond view** (ADR-0009 Addendum B §B.1).
///
/// An in-memory snapshot of the `StakeBonds` set as-of a specific block,
/// built by composing per-block [`BondMutation`] diffs along the block's
/// selected-chain prefix — the bond analogue of the per-block UTXO view
/// (`selected_parent_utxo_view.compose(&mergeset_diff)`). Pure and
/// deterministic, so the per-block validator-reward coinbase fan-out
/// (ADR-0013) and the Model-B block-validity rule can resolve
/// `bond_outpoint → active bond record` **identically on every node**,
/// rather than reading the point-of-view-dependent virtual-commit-time
/// global store (which would chain-split — see Addendum B §B.0).
///
/// [`Self::apply`] / [`Self::revert`] mirror the virtual processor's
/// `stage_dns_bond_mutations` byte-for-byte (the persisted-store path),
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
    /// `ChainPath.added` branch of `stage_dns_bond_mutations`.
    pub fn apply(&mut self, mutations: &[BondMutation]) {
        for mutation in mutations {
            match mutation {
                BondMutation::Insert(outpoint, record) => {
                    self.bonds.insert(*outpoint, record.clone());
                }
                BondMutation::Slash(outpoint, daa) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        record.slashed_at_daa_score = Some(*daa);
                        record.status = BondStatus::Slashed;
                    }
                }
                BondMutation::Unbond(outpoint, daa) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        record.unbond_request_daa_score = Some(*daa);
                    }
                }
            }
        }
    }

    /// Revert one block's `bond_diff` (mutations in **reverse** order, so a
    /// `Slash` whose `Insert` is reverted in the same diff is handled
    /// gracefully). Mirrors the `ChainPath.removed` branch of
    /// `stage_dns_bond_mutations`.
    pub fn revert(&mut self, mutations: &[BondMutation]) {
        for mutation in mutations.iter().rev() {
            match mutation {
                BondMutation::Insert(outpoint, _) => {
                    self.bonds.remove(outpoint);
                }
                BondMutation::Slash(outpoint, _) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        record.slashed_at_daa_score = None;
                        record.status = BondStatus::Active;
                    }
                }
                BondMutation::Unbond(outpoint, _) => {
                    if let Some(record) = self.bonds.get_mut(outpoint) {
                        record.unbond_request_daa_score = None;
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

    /// Snapshot every bond record in the view (any status). Used by the ADR-0018 §H
    /// reorg gate to score a *candidate* branch's StakeScore-since-common-ancestor under
    /// that branch's OWN bond set (the in-loop view advanced to the candidate) — scoring a
    /// branch under the wrong view would mis-credit it and risk wrongly accepting a reorg
    /// that abandons confirmed history. Callers gate each record by [`is_bond_active_at`].
    pub fn records(&self) -> Vec<StakeBondRecord> {
        self.bonds.values().cloned().collect()
    }

    /// Total stake of all bonds that are `Active` at `pov_daa_score`. The ADR-0018 §E
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

/// The set of `(bond_outpoint, epoch)` pairs already rewarded on a block's
/// selected-chain prefix (ADR-0009 Addendum B §B.3(c) reward uniqueness).
///
/// Composed/reverted alongside [`ActiveBondView`] so that each
/// `(bond, epoch)` earns at most one coinbase reward across the selected
/// chain, deterministically and reorg-safely — the reward analogue of the
/// §A.5 `(bond_outpoint, validator_id, epoch)` StakeScore dedup, narrowed
/// to `(bond_outpoint, epoch)` because the reward is per bond-epoch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RewardedEpochSet {
    rewarded: HashSet<(TransactionOutpoint, u64)>,
}

impl RewardedEpochSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` iff `(outpoint, epoch)` was already rewarded on this prefix.
    pub fn contains(&self, outpoint: &TransactionOutpoint, epoch: u64) -> bool {
        self.rewarded.contains(&(*outpoint, epoch))
    }

    /// Record a reward. Returns `true` if newly inserted, `false` if it was
    /// already present (a duplicate, which per §B.4 is *not* rewarded again
    /// and does *not* invalidate the block).
    pub fn insert(&mut self, outpoint: TransactionOutpoint, epoch: u64) -> bool {
        self.rewarded.insert((outpoint, epoch))
    }

    /// Reverse an `insert` on reorg.
    pub fn remove(&mut self, outpoint: &TransactionOutpoint, epoch: u64) -> bool {
        self.rewarded.remove(&(*outpoint, epoch))
    }

    pub fn len(&self) -> usize {
        self.rewarded.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rewarded.is_empty()
    }
}

// =====================================================================
// PR-10.5 / A.5: deterministic StakeScore aggregation -> DnsState.
//
// Pure, store-free helpers. The consensus crate (which can call into
// kaspa-txscript for ML-DSA verification) is responsible for walking the
// selected chain, verifying each attestation's signature against its
// bond's `validator_pubkey` under `ATTESTATION_MLDSA87_CONTEXT`, and
// gating by `is_bond_active_at` — then it passes the surviving
// contributions here. Keeping the aggregation pure makes the
// dedup + normalisation deterministic and unit-testable.
// =====================================================================

/// One signature-verified, bond-active attestation contribution fed into
/// [`aggregate_epoch_tallies`]. The caller (consensus aggregation pass)
/// has already (a) confirmed the referenced bond exists and is `Active` at
/// the attestation's `target_daa_score`, and (b) verified the ML-DSA-87
/// signature — so only the dedup key and the bond's stake remain.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AttestationContribution {
    pub epoch: u64,
    pub validator_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    /// The contributing bond's stake in sompi.
    pub signed_stake_sompi: u64,
}

/// Aggregate validated attestation contributions into per-epoch
/// [`EpochStakeTally`]s (ADR-0009 §"StakeScore mechanics" / Addendum A.5).
///
/// Enforces the `(bond_outpoint, validator_id, epoch)` uniqueness rule —
/// each triple contributes its stake at most once, even across multiple
/// shards. `total_active_stake_by_epoch` supplies each epoch's
/// normalisation denominator (total `Active` stake at that epoch); an
/// epoch with a denominator but no signed stake yields a `0` tally, and
/// signed contributions for an epoch absent from the denominator map are
/// ignored (no denominator ⇒ the epoch is not yet normalisable). Tallies
/// are returned ascending by epoch for deterministic downstream hashing.
pub fn aggregate_epoch_tallies(
    contributions: &[AttestationContribution],
    total_active_stake_by_epoch: &BTreeMap<u64, u64>,
) -> Vec<EpochStakeTally> {
    let mut seen: HashSet<(TransactionOutpoint, Hash64, u64)> = HashSet::new();
    let mut signed_by_epoch: BTreeMap<u64, u64> = BTreeMap::new();
    for c in contributions {
        // Dedup the (bond, validator, epoch) triple; count its stake once.
        if seen.insert((c.bond_outpoint, c.validator_id, c.epoch)) {
            let entry = signed_by_epoch.entry(c.epoch).or_insert(0);
            *entry = entry.saturating_add(c.signed_stake_sompi);
        }
    }
    total_active_stake_by_epoch
        .iter()
        .map(|(&epoch, &total)| EpochStakeTally {
            epoch,
            // `signed` is clamped to `total` inside `epoch_stake_credit`,
            // so an over-count cannot inflate the score.
            signed_stake_sompi: signed_by_epoch.get(&epoch).copied().unwrap_or(0),
            total_active_stake_sompi: total,
        })
        .collect()
}

/// Build the new [`DnsState`] for `anchor`, advancing the last
/// DNS-confirmed anchor when `anchor` clears **both** depth thresholds
/// (ADR-0009 Addendum A.5; via [`is_dns_confirmed`]).
///
/// `prev` is the previous `DnsState` (the singleton store's current value,
/// or `None` before the overlay's first write).
///
/// **audit #3 (consensus-split fix):** `anchor` is the POV-dependent `sink` and is stored
/// as `selected_chain_anchor` ONLY for the recompute throttle — it is NOT what gets confirmed.
/// The confirmed point is `confirmable_anchor`: the canonical lagged anchor of the latest ready
/// epoch (a fixed, blue_score-coordinated selected-chain block every node derives identically).
/// A candidate is confirmed only when the depth predicate holds AND a canonical anchor exists;
/// then `last_dns_confirmed_anchor` is that canonical anchor, so all nodes protect the identical
/// anchor in the reorg gate. (Confirming the `sink` instead would let nodes that recompute at
/// different boundary sinks protect different anchors and split the gate.) When not confirmed the
/// previously-confirmed anchor is carried forward; with no previous confirmation it defaults to
/// the zero `Hash64` ("nothing confirmed yet"), which the reorg gate treats as dormant.
///
/// `health` is the per-epoch [`DnsHealth`] signal the caller derived for this anchor
/// (via [`derive_dns_health`]); it is stored verbatim — a pure liveness annotation that
/// never influences whether the anchor confirms.
#[allow(clippy::too_many_arguments)]
pub fn advance_dns_confirmation(
    prev: Option<&DnsState>,
    anchor: Hash64,
    anchor_daa_score: u64,
    confirmable_anchor: Option<(Hash64, u64)>,
    work_depth: BlueWorkType,
    stake_depth: StakeScore,
    rollout_stage: DnsRolloutStage,
    validator_set_commitment: Hash64,
    health: DnsHealth,
    required_work_depth: BlueWorkType,
    required_stake_depth: StakeScore,
) -> DnsState {
    // Confirm the CANONICAL anchor (deterministic across nodes), never the POV-dependent sink.
    let confirmed =
        confirmable_anchor.is_some() && is_dns_confirmed(work_depth, stake_depth, required_work_depth, required_stake_depth);
    let (last_dns_confirmed_anchor, last_dns_confirmed_anchor_daa_score) = match (confirmed, confirmable_anchor, prev) {
        (true, Some(canonical), _) => canonical,
        (_, _, Some(p)) => (p.last_dns_confirmed_anchor, p.last_dns_confirmed_anchor_daa_score),
        _ => (Hash64::default(), 0),
    };
    DnsState {
        selected_chain_anchor: anchor,
        anchor_daa_score,
        work_depth,
        stake_depth,
        last_dns_confirmed_anchor,
        last_dns_confirmed_anchor_daa_score,
        rollout_stage,
        validator_set_commitment,
        health,
    }
}

/// Per-epoch normalisation denominator for StakeScore: for each epoch in
/// `epoch_anchor_daa` (epoch → that epoch's selected-chain anchor DAA
/// score), the total stake of bonds that are `Active` at that anchor's DAA
/// score (ADR-0009 §"StakeScore mechanics" / Addendum A.5).
///
/// Pure: the caller supplies the bonds in the (bounded) window and each
/// epoch's anchor DAA score; activation / slash / unbond are evaluated via
/// [`is_bond_active_at`] (DAA-stamped, so this is reorg-safe with no
/// incremental state). Pairs with [`aggregate_epoch_tallies`] to feed
/// [`compute_stake_score`].
pub fn total_active_stake_by_epoch(bonds: &[StakeBondRecord], epoch_anchor_daa: &BTreeMap<u64, u64>) -> BTreeMap<u64, u64> {
    epoch_anchor_daa
        .iter()
        .map(|(&epoch, &anchor_daa)| {
            let total = bonds.iter().filter(|b| is_bond_active_at(b, anchor_daa)).fold(0u64, |acc, b| acc.saturating_add(b.amount));
            (epoch, total)
        })
        .collect()
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — the per-epoch accumulator tally (Phase 1).
///
/// One entry per epoch in the consensus `DbEpochAccumulatorStore`, recomputed
/// deterministically from the selected-chain bounded window at each virtual-state commit
/// (the `update_dns_state` recompute precedent). It records everything the deferred §E
/// quality-bonus payout (Phase 2) needs once an epoch is buried and final:
///
/// * `expected_stake` — the epoch's total `Active` stake at its anchor (`epoch · epoch_length`),
///   the §E anti-capture **denominator** (mirrors [`total_active_stake_by_epoch`]).
/// * `included` — the validators whose attestations for this epoch were **rewarded** (included
///   in a coinbase fan-out) on the selected chain, as `(owner_reward_spk_payload, included_stake)`.
///   Addendum B §B.3(c) cross-block `(bond, epoch)` uniqueness guarantees each validator appears
///   at most once, so this is a union with no dedup. The §E φS gate's numerator is `Σ` of these
///   stakes (Phase 2).
/// * `quality_pool_accrued` — `Σ` of the per-block validator quality sub-pools
///   ([`split_validator_pool`]`.1`) of the blocks **in** this epoch (`block_daa / epoch_length`).
///   Paid out stake-proportionally to `included` iff the epoch met φS (Phase 2); else it rolls
///   over (don't-mint).
/// * `finalized` — `true` once the epoch is buried beyond `finalization_depth` past its end, so
///   neither its included set nor its pool can change under any future block or reorg; the
///   deferred payout reads only finalized epochs.
///
/// `included` stores the 64-byte payload as a [`Hash64`] (serde-stable; `[u8; 64]` has no derive),
/// converted back via [`Hash64::as_bytes`] for [`p2pkh_mldsa87_spk`] at payout.
///
/// Gated by `pos_v2_activation_daa_score`: inert on devnet/simnet (`GENESIS_ACTIVE_DNS_PARAMS`,
/// fence `u64::MAX` — no tally is ever written); written from block 1 on mainnet/testnet
/// (`PRODUCTION_DNS_PARAMS`, fence `0` — the v2 economics are active).
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct EpochTally {
    pub expected_stake: u128,
    pub included: Vec<(Hash64, u64)>,
    pub quality_pool_accrued: u128,
    pub finalized: bool,
}

// The accumulator store uses an untracked (`Count`) cache policy, so this estimate is never
// consulted for eviction — an empty impl mirrors [`StakeBondRecord`] / `UtxoEntry`.
impl MemSizeEstimator for EpochTally {}

/// kaspa-pq ADR-0022 — the keyed BLAKE2b-512 domain for the L1 header
/// `overlay_commitment_root` (distinct from `EvmCommitment64` / `EvmPayload64`).
pub const MISAKA_OVERLAY_COMMITMENT_CONTEXT: &[u8] = b"OverlayCommit64";

/// kaspa-pq ADR-0022 — the complete DNS/PoS-v2 **overlay** state as-of a block
/// `B`, i.e. the minimal set of overlay rows required to validate `B`'s
/// selected-chain descendants during pruned-IBD **without** access to `past(B)`.
/// Its keyed BLAKE2b-512 digest is committed in `Header::overlay_commitment_root`
/// (under [`MISAKA_OVERLAY_COMMITMENT_CONTEXT`]); the block-template builder fills
/// the header field from the virtual overlay state and the virtual processor
/// re-derives + checks it (`c == v`). A pruning-point import verifies the
/// peer-supplied snapshot against the committed root before persisting it.
///
/// The snapshot commits **raw inputs**, not derived tallies: every overlay store a
/// pruned-IBD node needs (`stake_bonds_store`, `reserve_balance_store`,
/// `rewarded_epochs_store`, `block_quality_pool_store`) is reconstructable from
/// these, and the per-epoch [`EpochTally`] accumulator is then *derived* via
/// [`recompute_epoch_tallies`] — so there is no stale-store / recompute-anchor
/// ambiguity in the commitment (template and verify both gather the same per-block
/// rows for the same `selected_parent`).
///
/// Components (the snapshot is taken **as-of `B`'s selected parent** — exactly the
/// inputs `verify_expected_utxo_state` and the template builder already hold):
///
/// * `bonds` — the live bond set (cumulative; from the walked `ActiveBondView`,
///   seed for `initial_active_bond_view`).
/// * `reserve_balance` — `reserve_balance_store[selected_parent]` (cumulative
///   security-reserve balance; drives the §F drip of the finalizing child).
/// * `window` — every selected-chain block in
///   `(selected_parent − walk_bound, selected_parent]` with its per-block overlay
///   contribution (rewarded `(outpoint, epoch)` keys + validator quality sub-pool),
///   where `walk_bound = reward_uniqueness_window + max_reorg_horizon + 2·epoch_length`
///   covers both the reward-uniqueness dedup and the epoch-accumulator recompute.
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
    /// Cumulative security-reserve balance after the selected parent.
    pub reserve_balance: u64,
    /// Per-block overlay contributions for the window; canonical order = ascending by block hash.
    pub window: Vec<BlockOverlayContribution>,
}

/// kaspa-pq ADR-0022 — one selected-chain block's overlay contribution carried in
/// an [`OverlaySnapshot::window`]. Mirrors [`BlockEpochContribution`] (the input to
/// [`recompute_epoch_tallies`]) plus the `block_hash` so a pruned-IBD import can
/// rebuild the per-block `rewarded_epochs_store` / `block_quality_pool_store` rows.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct BlockOverlayContribution {
    pub block_hash: BlockHash,
    pub block_daa_score: u64,
    /// The `(bond_outpoint, epoch)` keys this block's coinbase rewarded.
    pub rewarded_keys: Vec<(TransactionOutpoint, u64)>,
    /// The block's validator quality sub-pool (`block_quality_pool_store`).
    pub quality_subpool: u64,
}

impl OverlaySnapshot {
    /// Sorts every component into the FROZEN canonical order, in place. Idempotent.
    /// All sort keys use `Hash64`'s derived `Ord` (`TransactionId`/`BlockHash` are
    /// `Hash64`) so the order is byte-deterministic across nodes.
    pub fn canonicalize(&mut self) {
        self.bonds.sort_by(|a, b| {
            (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
        });
        for c in self.window.iter_mut() {
            c.rewarded_keys.sort_by(|a, b| (a.0.transaction_id, a.0.index, a.1).cmp(&(b.0.transaction_id, b.0.index, b.1)));
        }
        self.window.sort_by(|a, b| a.block_hash.cmp(&b.block_hash));
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

    /// Reconstruct the per-block epoch-accumulator inputs (oldest → newest by
    /// `block_daa_score`) from the window, for [`recompute_epoch_tallies`].
    pub fn epoch_contributions(&self) -> Vec<BlockEpochContribution> {
        let mut v: Vec<BlockEpochContribution> = self
            .window
            .iter()
            .map(|c| BlockEpochContribution {
                block_daa_score: c.block_daa_score,
                rewarded_keys: c.rewarded_keys.clone(),
                quality_subpool: c.quality_subpool,
            })
            .collect();
        v.sort_by_key(|c| c.block_daa_score);
        v
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

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — one selected-chain block's contribution to the epoch
/// accumulator recompute (Phase 1): its DAA score (→ its block-epoch), the `(bond_outpoint, epoch)`
/// keys its coinbase rewarded (from the per-block `rewarded_epochs_store`), and the per-block
/// validator quality sub-pool (from the per-block quality-pool store). Pure input to
/// [`recompute_epoch_tallies`]; the processor gathers it from the bounded-window walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockEpochContribution {
    pub block_daa_score: u64,
    pub rewarded_keys: Vec<(TransactionOutpoint, u64)>,
    pub quality_subpool: u64,
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — recompute the [`EpochTally`] of every epoch touched by the
/// selected-chain window `contributions`, deterministically and reorg-safely (Phase 1).
///
/// Mirrors the `update_dns_state` design: the accumulator is a **pure function** of the selected
/// chain (the per-block rewarded keys + quality sub-pools in `contributions`, all keyed by block
/// hash and therefore reorg-safe) and the current `bonds` snapshot — so a reorg simply re-derives
/// the live epochs from the new chain, with no incremental delta to revert.
///
/// For each touched epoch E:
/// * `included` collects, in `contributions` order, `(owner_reward_spk_payload, amount)` for every
///   rewarded `(bond, E)` key whose bond resolves in `bonds` (raw lookup — a *later* slash/unbond
///   does not retroactively un-include a past participation). §B.3(c) makes each `(bond, E)` appear
///   once across the chain, so no dedup is needed.
/// * `quality_pool_accrued` sums the quality sub-pool of every block whose own epoch is E.
/// * `expected_stake` is the total stake of bonds `Active` at E's anchor (`E · epoch_length`).
/// * `finalized` is set once `sink_daa_score ≥ (E+1)·epoch_length + finalization_depth`. The caller
///   passes `finalization_depth = reward_uniqueness_window_blocks + max_reorg_horizon_blocks`: the
///   epoch's blocks (pool, `≤ (E+1)·L`) and its attestation-rewarding blocks (included set, `≤ E·L +
///   window`) are then both buried beyond the reorg horizon, so the tally is immutable. The caller
///   never re-derives an already-finalized epoch (its blocks may lie partly outside the window), so
///   finalization is one-way.
///
/// Returns the tallies ascending by epoch (deterministic). The caller supplies `contributions` in
/// selected-chain order so the `included` ordering is chain-deterministic.
pub fn recompute_epoch_tallies(
    sink_daa_score: u64,
    epoch_length_blocks: u64,
    finalization_depth: u64,
    contributions: &[BlockEpochContribution],
    bonds: &[StakeBondRecord],
) -> Vec<(u64, EpochTally)> {
    let epoch_len = epoch_length_blocks.max(1);
    let bond_by_outpoint: HashMap<TransactionOutpoint, &StakeBondRecord> = bonds.iter().map(|b| (b.bond_outpoint, b)).collect();

    let mut quality_by_epoch: BTreeMap<u64, u128> = BTreeMap::new();
    let mut included_by_epoch: BTreeMap<u64, Vec<(Hash64, u64)>> = BTreeMap::new();

    for c in contributions {
        // Every block marks its own epoch present (even with a 0 pool), so an epoch with blocks
        // but no rewarded attestations still gets a tally + finalized flag.
        let block_epoch = c.block_daa_score / epoch_len;
        let q = quality_by_epoch.entry(block_epoch).or_insert(0);
        *q = q.saturating_add(c.quality_subpool as u128);
        for (outpoint, epoch) in &c.rewarded_keys {
            if let Some(bond) = bond_by_outpoint.get(outpoint) {
                included_by_epoch.entry(*epoch).or_default().push((Hash64::from_bytes(bond.owner_reward_spk_payload), bond.amount));
            }
        }
    }

    // Union of epochs touched by either a block (quality) or a rewarded attestation (included).
    let epochs: BTreeSet<u64> = quality_by_epoch.keys().copied().chain(included_by_epoch.keys().copied()).collect();
    epochs
        .into_iter()
        .map(|epoch| {
            let anchor_daa = epoch.saturating_mul(epoch_len);
            let expected_stake =
                bonds.iter().filter(|b| is_bond_active_at(b, anchor_daa)).fold(0u128, |acc, b| acc.saturating_add(b.amount as u128));
            let finalized = sink_daa_score >= epoch.saturating_add(1).saturating_mul(epoch_len).saturating_add(finalization_depth);
            let included = included_by_epoch.get(&epoch).cloned().unwrap_or_default();
            let quality_pool_accrued = quality_by_epoch.get(&epoch).copied().unwrap_or(0);
            (epoch, EpochTally { expected_stake, included, quality_pool_accrued, finalized })
        })
        .collect()
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 2) §E — the inclusive epoch range `[E_min, E_max]`
/// a block at `daa_score` (selected parent at `parent_daa`) **newly finalizes**, i.e. the epochs
/// whose finalization threshold `(E+1)·epoch_length + finalization_depth` first falls in
/// `(parent_daa, daa_score]`. Because DAA is monotonic on a chain, each epoch's threshold is
/// crossed by exactly one block, so this is the **once-per-epoch** guard for the deferred
/// quality-bonus payout (no extra store needed; reorg-safe because a competing chain's crossing
/// block re-pays the identical immutable tally). `None` when the block crosses no threshold — the
/// common case (a block advances DAA by far less than an epoch length, so most blocks finalize
/// nothing). Pure; usually a single epoch.
pub fn epochs_finalized_at(parent_daa: u64, daa_score: u64, epoch_length_blocks: u64, finalization_depth: u64) -> Option<(u64, u64)> {
    let epoch_len = epoch_length_blocks.max(1);
    // threshold(E) = (E+1)·L + fd ∈ (parent_daa, daa_score].  With m = E+1 (≥ 1):
    //   parent_daa − fd < m·L ≤ daa_score − fd.
    let hi = daa_score.checked_sub(finalization_depth)?; // need daa_score ≥ fd, else nothing finalized
    let m_max = hi / epoch_len; // ⌊(daa_score − fd)/L⌋  (largest m with m·L ≤ daa_score − fd)
    if m_max == 0 {
        return None; // not even epoch 0 is buried yet
    }
    let m_min = match parent_daa.checked_sub(finalization_depth) {
        None => 1,                                      // parent below the first threshold ⇒ from epoch 0
        Some(lo) => (lo / epoch_len).saturating_add(1), // smallest m with m·L > lo
    }
    .max(1);
    if m_min > m_max {
        return None; // the parent already crossed every threshold ≤ daa_score
    }
    Some((m_min - 1, m_max - 1)) // E = m − 1
}

/// Flattens every [`StakeAttestation`] from the `StakeAttestationShard`
/// payloads among `txs` (the decode-only first half of the A.5 aggregation
/// input). Pure; defensively skips undecodable shard payloads. Signature
/// verification + bond lookup happen in the consensus crate (which can call
/// `kaspa-txscript`), keeping the borsh decode here and out of the
/// virtual processor.
pub fn attestations_from_accepted_txs(txs: &[Transaction]) -> Vec<StakeAttestation> {
    let mut out = Vec::new();
    for tx in txs {
        if dns_tx_kind(&tx.subnetwork_id) == Some(DnsTxKind::StakeAttestationShard)
            && let Ok(shard) = borsh::from_slice::<StakeAttestationShardPayload>(&tx.payload)
        {
            out.extend(shard.attestations);
        }
    }
    out
}

/// kaspa-pq DNS-finality (E1/§6.1): decode ONE tx's `StakeAttestationShardPayload`.
/// `Some(shard)` iff the tx is on `SUBNETWORK_ID_STAKE_ATTESTATION_SHARD` AND its
/// payload borsh-decodes; `None` for a non-shard tx OR a shard-subnetwork tx whose
/// payload is malformed. Lets the consensus-crate template classifier distinguish
/// "not a shard" (keep) from "malformed shard" (drop) without taking a direct borsh
/// dependency — the decode lives here in core alongside [`attestations_from_accepted_txs`].
pub fn decode_attestation_shard(tx: &Transaction) -> Option<StakeAttestationShardPayload> {
    if dns_tx_kind(&tx.subnetwork_id) != Some(DnsTxKind::StakeAttestationShard) {
        return None;
    }
    borsh::from_slice::<StakeAttestationShardPayload>(&tx.payload).ok()
}

/// kaspa-pq H-05: the `(tx_id, StakeUnbondRequestPayload)` of every decodable
/// `StakeUnbondRequest` among `txs` (mirrors [`attestations_from_accepted_txs`];
/// the tx id is carried so the block-validity rule can name the offending tx).
/// Pure; defensively skips undecodable payloads (the stateless tx check already
/// rejected them).
pub fn unbond_requests_from_accepted_txs(txs: &[Transaction]) -> Vec<(TransactionId, StakeUnbondRequestPayload)> {
    let mut out = Vec::new();
    for tx in txs {
        if dns_tx_kind(&tx.subnetwork_id) == Some(DnsTxKind::StakeUnbond)
            && let Ok(req) = borsh::from_slice::<StakeUnbondRequestPayload>(&tx.payload)
        {
            out.push((tx.id(), req));
        }
    }
    out
}

/// Flattens every [`SlashingEvidencePayload`] from the
/// `SUBNETWORK_ID_SLASHING_EVIDENCE` txs among `txs`. Pure; defensively skips
/// undecodable payloads (a committed block's txs already passed the PR-10.4
/// stateless check). The consensus crate consumes this to apply the stateful
/// genuineness rule (both attestations sign-verify against the bond's
/// `validator_pubkey`) before the bond is mutated to `Slashed`.
pub fn slashing_evidence_from_accepted_txs(txs: &[Transaction]) -> Vec<SlashingEvidencePayload> {
    let mut out = Vec::new();
    for tx in txs {
        if dns_tx_kind(&tx.subnetwork_id) != Some(DnsTxKind::SlashingEvidence) {
            continue;
        }
        if let Ok(ev) = borsh::from_slice::<SlashingEvidencePayload>(&tx.payload) {
            out.push(ev);
        }
    }
    out
}

/// Builds the [`DnsConfirmation`] RPC view from the current [`DnsState`] and
/// the network's confirmation thresholds (ADR-0009; the `getDnsConfirmation`
/// RPC, PR-10.14). Pure. `pow_confirmed` is the work-depth threshold alone;
/// `dns_confirmed` requires **both** depths (via [`is_dns_confirmed`]).
///
/// Per ADR-0009 §"Public-claim discipline", the three `*_risk_*` strings are
/// deliberately descriptive (not a single joint probability) and must be read
/// alongside the boolean flags. `expected_dns_confirmation_seconds` is left 0
/// (a calibrated estimate is a follow-up).
pub fn dns_confirmation_from_state(
    state: &DnsState,
    required_work_depth: BlueWorkType,
    required_stake_depth: StakeScore,
) -> DnsConfirmation {
    let pow_confirmed = state.work_depth >= required_work_depth;
    let dns_confirmed = is_dns_confirmed(state.work_depth, state.stake_depth, required_work_depth, required_stake_depth);
    DnsConfirmation {
        block_hash: state.selected_chain_anchor,
        work_depth: state.work_depth,
        required_work_depth,
        stake_depth: state.stake_depth,
        required_stake_depth,
        pow_confirmed,
        dns_confirmed,
        rollout_stage: state.rollout_stage,
        expected_dns_confirmation_seconds: 0,
        work_reorg_risk_upper_bound: "see ADR-0009 §Public-claim discipline".to_string(),
        stake_reorg_risk_upper_bound: "see ADR-0009 §Public-claim discipline".to_string(),
        dns_reorg_risk_conservative_bound: "see ADR-0009 §Public-claim discipline".to_string(),
        note: format!(
            "rollout_stage={:?}; health={:?}; pow_confirmed={pow_confirmed}; dns_confirmed={dns_confirmed}",
            state.rollout_stage, state.health
        ),
        health: state.health,
        // audit M-01: surface the stable DNS-confirmed anchor (≠ the pov-dependent sink block_hash).
        last_dns_confirmed_anchor: state.last_dns_confirmed_anchor,
        last_dns_confirmed_anchor_daa_score: state.last_dns_confirmed_anchor_daa_score,
    }
}

/// Freshness predicate for bridge/finality-dependent operations.
///
/// Missing attestations are not a base-ledger validity failure, but consumers that
/// depend on DNS finality must require a currently confirmed, non-stale anchor.
pub fn dns_finality_fresh_for_bridge(
    dns_confirmed: bool,
    last_dns_confirmed_anchor: Hash64,
    last_dns_confirmed_anchor_daa_score: u64,
    current_daa_score: u64,
    max_staleness_daa_score: u64,
) -> bool {
    dns_confirmed
        && last_dns_confirmed_anchor != Hash64::default()
        && current_daa_score >= last_dns_confirmed_anchor_daa_score
        && current_daa_score - last_dns_confirmed_anchor_daa_score <= max_staleness_daa_score
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ADR-0022: OverlaySnapshot commitment ----

    fn mk_bond(seed: u64, amount: u64) -> StakeBondRecord {
        StakeBondRecord {
            version: 2,
            bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(seed), (seed % 7) as u32),
            owner_pubkey_hash: Hash64::from_u64_word(seed + 1),
            validator_pubkey_hash: Hash64::from_u64_word(seed + 2),
            validator_pubkey: vec![(seed % 251) as u8; 8],
            amount,
            activation_daa_score: seed,
            created_daa_score: seed,
            unbonding_period_blocks: 700,
            owner_reward_spk_payload: [(seed % 256) as u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: BondStatus::Active,
        }
    }

    fn mk_contrib(seed: u64) -> BlockOverlayContribution {
        let op = |s: u64| TransactionOutpoint::new(Hash64::from_u64_word(s), 0);
        BlockOverlayContribution {
            block_hash: Hash64::from_u64_word(seed),
            block_daa_score: seed * 100,
            rewarded_keys: vec![(op(seed + 11), 3u64), (op(seed + 10), 2u64)],
            quality_subpool: seed,
        }
    }

    /// kaspa-pq GetStakeBonds: owner filtering, deterministic outpoint ordering,
    /// and cursor paging reproduce the full ordered set exactly (no gaps/dupes).
    #[test]
    fn paginate_filters_orders_and_pages() {
        let owner_a = Hash64::from_u64_word(1000);
        let owner_b = Hash64::from_u64_word(2000);
        let mut records = Vec::new();
        for seed in [5u64, 1, 9, 3, 7] {
            let mut b = mk_bond(seed, seed * 10);
            b.owner_pubkey_hash = owner_a;
            records.push(b);
        }
        for seed in [2u64, 8] {
            let mut b = mk_bond(seed, seed * 10);
            b.owner_pubkey_hash = owner_b;
            records.push(b);
        }
        let pov = 100;
        let query =
            |cursor, limit| StakeBondQuery { owner_pubkey_hash: Some(owner_a), status_in: None, cursor, limit, pov_daa_score: None };

        // Owner filter isolates A's 5 bonds; a fits-in-one-page result has no cursor.
        let all_a = paginate_stake_bonds(records.clone(), &query(None, 0), pov);
        assert_eq!(all_a.bonds.len(), 5);
        assert!(all_a.bonds.iter().all(|b| b.owner_pubkey_hash == owner_a));
        assert!(all_a.next_cursor.is_none());
        assert_eq!(all_a.pov_daa_score, pov);

        // Deterministic ascending order by (txid, index).
        let mut sorted = all_a.bonds.clone();
        sorted.sort_by(|a, b| {
            (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
        });
        assert_eq!(all_a.bonds, sorted, "page must be ordered by outpoint");

        // Paging with limit=2 walks the same sequence via next_cursor — exclusive,
        // so no gaps and no duplicates.
        let mut paged = Vec::new();
        let mut cursor = None;
        loop {
            let page = paginate_stake_bonds(records.clone(), &query(cursor, 2), pov);
            assert!(page.bonds.len() <= 2);
            paged.extend(page.bonds.iter().cloned());
            match page.next_cursor {
                Some(c) => {
                    assert_eq!(page.bonds.len(), 2, "a non-final page is full");
                    cursor = Some(c);
                }
                None => break,
            }
        }
        assert_eq!(paged, all_a.bonds, "paged walk must reproduce the full ordered set exactly");
    }

    /// kaspa-pq GetStakeBonds: the status filter uses *effective* status at the
    /// pov DAA score (not the stored field); an empty list means "any status".
    #[test]
    fn paginate_filters_by_effective_status() {
        let pov = 50;
        let mut pending = mk_bond(200, 1); // activation 200 > pov → Pending
        pending.status = BondStatus::Pending;
        let active = mk_bond(10, 2); // activation 10 ≤ pov, no unbond/slash → Active
        let mut unbonding = mk_bond(20, 3);
        unbonding.unbond_request_daa_score = Some(30); // ≤ pov → Unbonding
        let mut slashed = mk_bond(31, 4);
        slashed.slashed_at_daa_score = Some(40); // ≤ pov → Slashed
        let records = vec![pending, active.clone(), unbonding.clone(), slashed];

        let q = |statuses| StakeBondQuery {
            owner_pubkey_hash: None,
            status_in: Some(statuses),
            cursor: None,
            limit: 0,
            pov_daa_score: None,
        };

        let only_active = paginate_stake_bonds(records.clone(), &q(vec![BondStatus::Active]), pov);
        assert_eq!(only_active.bonds.len(), 1);
        assert_eq!(only_active.bonds[0].bond_outpoint, active.bond_outpoint);

        let two = paginate_stake_bonds(records.clone(), &q(vec![BondStatus::Active, BondStatus::Unbonding]), pov);
        assert_eq!(two.bonds.len(), 2);
        assert!(two.bonds.iter().any(|b| b.bond_outpoint == unbonding.bond_outpoint));

        // Empty status set is treated as no status filter (all four records).
        let any = paginate_stake_bonds(records, &q(vec![]), pov);
        assert_eq!(any.bonds.len(), 4);
    }

    /// kaspa-pq GetStakeBonds: under a FIXED pov (the pinning the request's
    /// `pov_daa_score` provides), a `status_in`-filtered multi-page walk is
    /// gap-free — the property that prevents the mid-walk skip when the sink
    /// would otherwise advance between page fetches.
    #[test]
    fn paginate_status_filtered_walk_is_complete_under_fixed_pov() {
        let pov = 100;
        let owner = Hash64::from_u64_word(4242);
        let mut records = Vec::new();
        for seed in [5u64, 1, 9, 3, 7] {
            // activation = seed ≤ pov → Active
            let mut b = mk_bond(seed, seed);
            b.owner_pubkey_hash = owner;
            records.push(b);
        }
        for seed in [200u64, 150] {
            // activation > pov → Pending (must be excluded by status=Active)
            let mut b = mk_bond(seed, seed);
            b.owner_pubkey_hash = owner;
            records.push(b);
        }
        let q = |cursor, limit| StakeBondQuery {
            owner_pubkey_hash: Some(owner),
            status_in: Some(vec![BondStatus::Active]),
            cursor,
            limit,
            pov_daa_score: Some(pov),
        };

        let full = paginate_stake_bonds(records.clone(), &q(None, 0), pov);
        assert_eq!(full.bonds.len(), 5, "5 active bonds match at the fixed pov");

        let mut paged = Vec::new();
        let mut cursor = None;
        loop {
            let page = paginate_stake_bonds(records.clone(), &q(cursor, 2), pov);
            paged.extend(page.bonds.iter().cloned());
            match page.next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(paged, full.bonds, "fixed-pov status-filtered walk reproduces the full set with no gap");
    }

    /// ADR-0022: the overlay commitment is order-independent (canonicalized) and
    /// sensitive to every component — the load-bearing property for verifying a
    /// peer-supplied snapshot against `header.overlay_commitment_root`.
    #[test]
    fn overlay_commitment_is_canonical_and_sensitive() {
        let bonds = vec![mk_bond(9, 100), mk_bond(2, 200), mk_bond(5, 300)];
        let window = vec![mk_contrib(40), mk_contrib(20), mk_contrib(31)];
        let a = OverlaySnapshot { bonds: bonds.clone(), reserve_balance: 12345, window: window.clone() };
        // Same content, every vector reversed → same commitment after canonicalization.
        let b = OverlaySnapshot {
            bonds: bonds.iter().rev().cloned().collect(),
            reserve_balance: 12345,
            window: window
                .iter()
                .rev()
                .map(|c| {
                    let mut c = c.clone();
                    c.rewarded_keys.reverse();
                    c
                })
                .collect(),
        };
        assert_eq!(a.commitment_root(), b.commitment_root(), "commitment must be order-independent");

        // Every component is committed: changing any one flips the digest.
        let mut c = a.clone();
        c.reserve_balance += 1;
        assert_ne!(a.commitment_root(), c.commitment_root(), "reserve_balance must be committed");
        let mut d = a.clone();
        d.bonds[0].amount += 1;
        assert_ne!(a.commitment_root(), d.commitment_root(), "bond amount must be committed");
        let mut e = a.clone();
        e.window[0].quality_subpool += 1;
        assert_ne!(a.commitment_root(), e.commitment_root(), "quality sub-pool must be committed");
        let mut f = a.clone();
        f.window[0].rewarded_keys[0].1 += 1;
        assert_ne!(a.commitment_root(), f.commitment_root(), "rewarded keys must be committed");
        let mut g = a.clone();
        g.window[0].block_daa_score += 1;
        assert_ne!(a.commitment_root(), g.commitment_root(), "block daa score must be committed");

        // epoch_contributions is daa-ordered regardless of window order.
        let contribs = a.epoch_contributions();
        assert!(contribs.windows(2).all(|w| w[0].block_daa_score <= w[1].block_daa_score));

        // The empty snapshot has a stable, non-zero digest (the genesis/pre-bond value).
        assert_ne!(OverlaySnapshot::default().commitment_root(), Hash64::default());
    }

    // ---- PR-10.5: StakeScore + DNS reorg gate ----

    #[test]
    fn epoch_stake_credit_quality_floor() {
        // φS = 0 reproduces the prior linear credit EXACTLY (the 10_000 factors cancel).
        assert_eq!(epoch_stake_credit(1, 3, 0), 333_333_333); // floor(1e9/3)
        assert_eq!(epoch_stake_credit(5, 10, 0), STAKE_SCORE_SCALE / 2); // 0.5
        assert_eq!(epoch_stake_credit(50, 50, 0), STAKE_SCORE_SCALE); // 1.0
        // φS = 0.60: below → 0, exactly-at → 0 (continuous), above → smooth (f−φS)/(1−φS).
        let floor = 6000u16;
        assert_eq!(epoch_stake_credit(5, 10, floor), 0); // 0.50 < 0.60
        assert_eq!(epoch_stake_credit(6, 10, floor), 0); // exactly at the floor → 0
        assert_eq!(epoch_stake_credit(8, 10, floor), STAKE_SCORE_SCALE / 2); // (0.80−0.60)/0.40 = 0.5
        assert_eq!(epoch_stake_credit(10, 10, floor), STAKE_SCORE_SCALE); // full inclusion → 1.0
        // Edges.
        assert_eq!(epoch_stake_credit(7, 0, floor), 0); // no active stake
        assert_eq!(epoch_stake_credit(999, 50, floor), STAKE_SCORE_SCALE); // numerator clamped to expected
        assert_eq!(epoch_stake_credit(10, 10, 10_000), STAKE_SCORE_SCALE); // φS = 100%: full inclusion only
        assert_eq!(epoch_stake_credit(9, 10, 10_000), 0); // φS = 100%: < 100% → 0
    }

    #[test]
    fn compute_stake_score_sums_credits_deterministically() {
        let epochs = vec![
            EpochStakeTally { epoch: 1, signed_stake_sompi: 10, total_active_stake_sompi: 10 }, // 1.0
            EpochStakeTally { epoch: 2, signed_stake_sompi: 5, total_active_stake_sompi: 10 },  // 0.5
            EpochStakeTally { epoch: 3, signed_stake_sompi: 0, total_active_stake_sompi: 10 },  // 0.0
        ];
        // φS = 0 (no floor): linear sum 1.0 + 0.5 + 0.0.
        let s = compute_stake_score(&epochs, 0);
        assert_eq!(s, StakeScore(STAKE_SCORE_SCALE + STAKE_SCORE_SCALE / 2));
        assert_eq!(compute_stake_score(&epochs, 0), s); // deterministic
        assert_eq!(compute_stake_score(&[], 0), StakeScore(0));
        // φS = 0.60: epoch 2 (0.50) and 3 (0.0) drop to 0; only epoch 1 (1.0) credits.
        assert_eq!(compute_stake_score(&epochs, 6000), StakeScore(STAKE_SCORE_SCALE));
    }

    #[test]
    fn derive_dns_health_signal() {
        let tally =
            |signed: u64, total: u64| EpochStakeTally { epoch: 0, signed_stake_sompi: signed, total_active_stake_sompi: total };
        let (floor, censor, m) = (6000u16, 1000u16, 3u32);
        // Not active → DisabledBeforeActivation regardless of tallies.
        assert_eq!(derive_dns_health(&[tally(0, 10)], floor, censor, m, false), DnsHealth::DisabledBeforeActivation);
        // Fewer than M epochs of history → Active (no sustained signal yet).
        assert_eq!(derive_dns_health(&[tally(0, 10), tally(0, 10)], floor, censor, m, true), DnsHealth::Active);
        // A recent epoch meets φS → Active even after a dip (health recovers immediately).
        let recovered = vec![tally(0, 10), tally(0, 10), tally(7, 10)]; // last = 0.70 ≥ 0.60
        assert_eq!(derive_dns_health(&recovered, floor, censor, m, true), DnsHealth::Active);
        // Last M all below φS but above the censorship floor → StakeQualityLow.
        let low = vec![tally(3, 10), tally(2, 10), tally(4, 10)]; // 0.30 / 0.20 / 0.40, in [0.10, 0.60)
        assert_eq!(derive_dns_health(&low, floor, censor, m, true), DnsHealth::DegradedStakeQualityLow);
        // Last M all near-zero (below the censorship floor) → CertificateCensored.
        assert_eq!(
            derive_dns_health(&[tally(0, 10), tally(0, 10), tally(0, 10)], floor, censor, m, true),
            DnsHealth::DegradedCertificateCensored
        );
        // Not ALL below the censorship floor within the window → StakeQualityLow.
        let mixed = vec![tally(0, 10), tally(0, 10), tally(3, 10)]; // last 0.30 > 0.10 censorship floor
        assert_eq!(derive_dns_health(&mixed, floor, censor, m, true), DnsHealth::DegradedStakeQualityLow);
    }

    #[test]
    fn is_dns_confirmed_requires_both_thresholds() {
        let w = BlueWorkType::from_u64;
        let (cw, cs) = (w(100), StakeScore(STAKE_SCORE_SCALE));
        assert!(is_dns_confirmed(w(100), StakeScore(STAKE_SCORE_SCALE), cw, cs)); // both met
        assert!(is_dns_confirmed(w(200), StakeScore(STAKE_SCORE_SCALE * 2), cw, cs));
        assert!(!is_dns_confirmed(w(99), StakeScore(STAKE_SCORE_SCALE), cw, cs)); // work short
        assert!(!is_dns_confirmed(w(100), StakeScore(STAKE_SCORE_SCALE - 1), cw, cs)); // stake short
    }

    fn reorg_inputs(
        stage: DnsRolloutStage,
        mode: DnsReorgMode,
        includes: bool,
        cw: u64,
        kw: u64,
        cs: u128,
        ks: u128,
    ) -> DnsReorgInputs {
        DnsReorgInputs {
            rollout_stage: stage,
            mode,
            candidate_includes_confirmed_anchor: includes,
            candidate_work_after: BlueWorkType::from_u64(cw),
            canonical_work_after: BlueWorkType::from_u64(kw),
            candidate_stake_after: StakeScore(cs),
            canonical_stake_after: StakeScore(ks),
            emergency_work_margin: BlueWorkType::from_u64(0),
            emergency_stake_margin: StakeScore(0),
        }
    }

    #[test]
    fn dns_reorg_gate_dormant_before_active() {
        let i = reorg_inputs(DnsRolloutStage::Bootstrap, DnsReorgMode::TwoDimensionalDominance, false, 1, 100, 1, 100);
        assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::GateInactive);
        assert!(check_dns_reorg_rule(&i).is_accept());
    }

    #[test]
    fn dns_reorg_includes_confirmed_anchor_ok() {
        let i = reorg_inputs(DnsRolloutStage::Active, DnsReorgMode::TwoDimensionalDominance, true, 0, 999, 0, 999);
        assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::IncludesConfirmedAnchor);
    }

    #[test]
    fn dns_reorg_hard_checkpoint_rejects_any_exit() {
        // Even a candidate that dominates on both axes is rejected under hard-checkpoint.
        let i = reorg_inputs(DnsRolloutStage::Active, DnsReorgMode::HardCheckpoint, false, 9999, 1, 9999, 1);
        assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::HardCheckpointReject);
        assert!(!check_dns_reorg_rule(&i).is_accept());
    }

    #[test]
    fn dns_reorg_two_dimensional_non_substitutability() {
        let (m, a) = (DnsReorgMode::TwoDimensionalDominance, DnsRolloutStage::Active);
        // beats BOTH → accepted
        assert_eq!(check_dns_reorg_rule(&reorg_inputs(a, m, false, 200, 100, 200, 100)), DnsReorgOutcome::DominanceSatisfied);
        // beats WORK only (stake equal) → rejected (non-substitutability)
        assert_eq!(check_dns_reorg_rule(&reorg_inputs(a, m, false, 200, 100, 100, 100)), DnsReorgOutcome::DominanceViolation);
        // beats STAKE only (work equal) → rejected
        assert_eq!(check_dns_reorg_rule(&reorg_inputs(a, m, false, 100, 100, 200, 100)), DnsReorgOutcome::DominanceViolation);
        // ties on both (must STRICTLY beat) → rejected
        assert_eq!(check_dns_reorg_rule(&reorg_inputs(a, m, false, 100, 100, 100, 100)), DnsReorgOutcome::DominanceViolation);
    }

    #[test]
    fn dns_reorg_dominance_respects_margins() {
        let (m, a) = (DnsReorgMode::TwoDimensionalDominance, DnsRolloutStage::Active);
        let mut i = reorg_inputs(a, m, false, 150, 100, 150, 100);
        i.emergency_work_margin = BlueWorkType::from_u64(60); // need cand_W > 160; 150 fails
        i.emergency_stake_margin = StakeScore(10);
        assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceViolation);
        i.candidate_work_after = BlueWorkType::from_u64(161); // clears work margin; stake 150 > 110 ok
        assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceSatisfied);
    }

    #[test]
    fn reorg_inputs_since_common_ancestor_subtracts_work() {
        let w = BlueWorkType::from_u64;
        // work_after = blue_work(tip) − blue_work(common ancestor); cumulative → exact subtraction.
        // Stake is windowed (not cumulative) so it is passed through verbatim, not subtracted.
        let i = reorg_inputs_since_common_ancestor(
            DnsRolloutStage::Active,
            DnsReorgMode::TwoDimensionalDominance,
            false,
            w(1000), // candidate tip cumulative work
            w(900),  // canonical tip cumulative work
            w(500),  // common ancestor cumulative work
            StakeScore(7),
            StakeScore(3),
            w(0),
            StakeScore(0),
        );
        assert_eq!(i.candidate_work_after, w(500)); // 1000 − 500
        assert_eq!(i.canonical_work_after, w(400)); // 900 − 500
        assert_eq!((i.candidate_stake_after, i.canonical_stake_after), (StakeScore(7), StakeScore(3))); // passed through
        // Candidate out-works (500 > 400) AND out-stakes (7 > 3) → dominance satisfied.
        assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceSatisfied);

        // A tip that IS the common ancestor contributes zero since-ancestor work (saturating
        // floor) → it cannot dominate, so the gate rejects.
        let i = reorg_inputs_since_common_ancestor(
            DnsRolloutStage::Active,
            DnsReorgMode::TwoDimensionalDominance,
            false,
            w(500), // candidate tip == common ancestor
            w(900),
            w(500),
            StakeScore(9),
            StakeScore(1),
            w(0),
            StakeScore(0),
        );
        assert_eq!(i.candidate_work_after, w(0)); // 500 − 500, floored
        assert_eq!(i.canonical_work_after, w(400));
        assert_eq!(check_dns_reorg_rule(&i), DnsReorgOutcome::DominanceViolation); // zero work cannot dominate
    }

    #[test]
    fn dns_constants_have_expected_values() {
        // Cross-check against the consensus-core kaspa-pq constant
        // values (the kaspa-txscript crate is downstream of
        // consensus-core, so we cannot pull MLDSA87_PK_LEN /
        // MLDSA87_SIG_LEN from there directly without creating a
        // dependency cycle; the values are duplicated here and the
        // assertion is the contract).
        assert_eq!(STAKE_VALIDATOR_PUBKEY_LEN, 2592);
        assert_eq!(STAKE_ATTESTATION_SIG_LEN, 4627);

        // ADR-0009 / ADR-0010 / ADR-0012 / ADR-0014 domain-
        // separator strings. All consensus-fixed (or consensus-
        // adjacent, for the node-local failover keys) and bumped
        // only by a hard-fork ADR — pin the bytes so any
        // accidental rename trips this test.
        assert_eq!(ATTESTATION_MLDSA87_CONTEXT, b"kaspa-pq-v1/att/mldsa87");
        assert_eq!(ATTESTATION_MESSAGE_DOMAIN, b"kaspa-pq-v1/stake-attestation");
        assert_eq!(VALIDATOR_SET_COMMITMENT_KEY, b"kaspa-pq-validator-set-v1");
        // ADR-0014 — node-local failover protocol keys.
        assert_eq!(HOST_ID_KEY, b"kaspa-pq-validator-host-id-v1");
        assert_eq!(TAKEOVER_TOKEN_MESSAGE_DOMAIN, b"kaspa-pq-takeover-token-v1");
        assert_eq!(TAKEOVER_TOKEN_CONTEXT, b"kaspa-pq-v1/takeover/mldsa87");
        // ADR-0015 — node-local remote-signer audit chain key
        // and protocol-version pin.
        assert_eq!(AUDIT_LOG_CHAIN_KEY, b"kaspa-pq-signer-audit-v1");
        // ADR-0015 / audit M-04 — audit-checkpoint signing context.
        assert_eq!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, b"kaspa-pq-v1/audit-ckpt/mldsa87");
        assert_eq!(SIGNER_PROTOCOL_VERSION, 1);
        // ADR-0015 capability bitflags must be single-bit and
        // pairwise distinct so they compose correctly under
        // bitwise OR.
        let caps =
            [CAP_SIGN_TRANSACTION, CAP_SIGN_ATTESTATION, CAP_SIGN_TAKEOVER_TOKEN, CAP_POLICY_STRICT, CAP_AUDIT_LOG, CAP_HSM_BACKED];
        for c in caps {
            assert!(c.count_ones() == 1, "capability {c:#x} is not a single bit");
        }
        for i in 0..caps.len() {
            for j in (i + 1)..caps.len() {
                assert_eq!(caps[i] & caps[j], 0, "capabilities {i} and {j} overlap");
            }
        }

        // Replay safety: tx vs attestation vs takeover contexts
        // must all differ (ADR-0002 / ADR-0009 §"Attestation
        // target" / ADR-0014 §"Public-claim discipline").
        assert_ne!(ATTESTATION_MLDSA87_CONTEXT, b"kaspa-pq-v1/tx/mldsa87");
        assert_ne!(TAKEOVER_TOKEN_CONTEXT, b"kaspa-pq-v1/tx/mldsa87");
        assert_ne!(TAKEOVER_TOKEN_CONTEXT, ATTESTATION_MLDSA87_CONTEXT);
        // audit M-04 — a checkpoint signature must not collide with
        // any overlay/tx domain, else it could be replayed as one.
        assert_ne!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, b"kaspa-pq-v1/tx/mldsa87");
        assert_ne!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, ATTESTATION_MLDSA87_CONTEXT);
        assert_ne!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, UNBOND_REQUEST_CONTEXT);
        assert_ne!(AUDIT_CHECKPOINT_MLDSA87_CONTEXT, TAKEOVER_TOKEN_CONTEXT);
    }

    #[test]
    fn stake_score_display() {
        assert_eq!(StakeScore(0).to_string(), "0.000000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE).to_string(), "1.000000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE + 500_000_000).to_string(), "1.500000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE * 3 / 4).to_string(), "0.750000000");
    }

    fn fixture_outpoint() -> TransactionOutpoint {
        // PR-9.5c: `TransactionOutpoint.transaction_id` widened to
        // `TransactionId` (= Hash64).
        TransactionOutpoint::new(Hash64::from_bytes([0x77u8; 64]), 42)
    }

    /// ADR-0018 §F canonical splits (Node share = 0): Stage-3 subsidy 67/8/25/0,
    /// normal-tx 90/10/0, finality 75/25/0 (each group sums to 100%).
    fn fixture_fee_split() -> FeeSplitParams {
        FeeSplitParams {
            subsidy_worker_base_bps: 6700,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 2500,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        }
    }

    /// ADR-0018 §F Stage-2 bootstrap split (smaller validator share): subsidy
    /// 82/8/10/0, normal-tx 90/10/0, finality 75/25/0.
    fn fixture_fee_split_bootstrap() -> FeeSplitParams {
        FeeSplitParams {
            subsidy_worker_base_bps: 8200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 1000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        }
    }

    fn fixture_attestation() -> StakeAttestation {
        StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: Hash64::from_bytes([0xa5u8; 64]),
            bond_outpoint: fixture_outpoint(),
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::default(), // audit #4: VSC is a fixed-zero invariant
            signature: vec![0x33u8; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn fixture_validators() -> Vec<ValidatorRecord> {
        vec![
            ValidatorRecord { validator_id: Hash64::from_bytes([0xcc; 64]), stake_amount: 30, activation_daa_score: 300 },
            ValidatorRecord { validator_id: Hash64::from_bytes([0xaa; 64]), stake_amount: 10, activation_daa_score: 100 },
            ValidatorRecord { validator_id: Hash64::from_bytes([0xbb; 64]), stake_amount: 20, activation_daa_score: 200 },
        ]
    }

    #[test]
    fn stake_bond_payload_borsh_roundtrip() {
        let bond = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
            validator_pubkey_hash: Hash64::from_bytes([0xbbu8; 64]),
            validator_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
            amount: 100_000_000_000,
            activation_daa_score: 5_000,
            unbonding_period_blocks: 100_000,
            owner_reward_spk_payload: [0xddu8; 64],
        };
        let bytes = borsh::to_vec(&bond).unwrap();
        let back: StakeBondPayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, bond);
    }

    #[test]
    fn stake_attestation_borsh_roundtrip() {
        let att = fixture_attestation();
        let bytes = borsh::to_vec(&att).unwrap();
        let back: StakeAttestation = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, att);
        // Spot-check the dominant size component: the ML-DSA-87
        // signature plus borsh framing. The Vec<u8> Borsh layout is
        // 4-byte length prefix + N data bytes, so a 4627-byte sig
        // contributes 4 + 4627 = 4631 bytes plus the other fixed
        // fields.
        assert!(bytes.len() >= STAKE_ATTESTATION_SIG_LEN);
    }

    #[test]
    fn stake_attestation_shard_borsh_roundtrip() {
        let shard = StakeAttestationShardPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::from_bytes([0x22u8; 64]),
            attestations: vec![fixture_attestation(); 8],
        };
        assert!(shard.attestations.len() <= MAX_ATTESTATIONS_PER_SHARD);
        let bytes = borsh::to_vec(&shard).unwrap();
        let back: StakeAttestationShardPayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, shard);
    }

    #[test]
    fn slashing_evidence_borsh_roundtrip() {
        let evidence = SlashingEvidencePayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint: fixture_outpoint(),
            attestation_a: fixture_attestation(),
            attestation_b: {
                let mut b = fixture_attestation();
                b.target_hash = Hash64::from_bytes([0x33u8; 64]);
                b
            },
            reporter_reward_spk_payload: [0xeeu8; 64],
        };
        let bytes = borsh::to_vec(&evidence).unwrap();
        let back: SlashingEvidencePayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, evidence);
    }

    // ---- PR-10.4: DNS overlay tx kinds + stateless payload validation ----

    fn fixture_bond() -> StakeBondPayload {
        let validator_pubkey = vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN];
        StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
            // audit H-04: declare the canonical key-derived overlay identity.
            validator_pubkey_hash: validator_id_from_pubkey(&validator_pubkey),
            validator_pubkey,
            amount: 100_000_000_000,
            activation_daa_score: 5_000,
            unbonding_period_blocks: 100_000,
            owner_reward_spk_payload: [0xddu8; 64],
        }
    }

    fn fixture_shard(n: usize) -> StakeAttestationShardPayload {
        StakeAttestationShardPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::default(), // audit #4: VSC is a fixed-zero invariant
            attestations: vec![fixture_attestation(); n],
        }
    }

    fn fixture_evidence() -> SlashingEvidencePayload {
        SlashingEvidencePayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint: fixture_outpoint(),
            attestation_a: fixture_attestation(),
            attestation_b: {
                let mut b = fixture_attestation();
                b.target_hash = Hash64::from_bytes([0x33u8; 64]); // different anchor → equivocation
                b
            },
            reporter_reward_spk_payload: [0xeeu8; 64],
        }
    }

    #[test]
    fn single_attestation_shard_tx_builds_a_valid_overlay_tx() {
        let att = fixture_attestation();
        let shard = single_attestation_shard(att.clone());
        assert_eq!(shard.attestations, vec![att.clone()]);
        // Shard-level tuple is copied from the attestation.
        assert_eq!(
            (shard.epoch, shard.target_hash, shard.target_daa_score, shard.validator_set_commitment),
            (att.epoch, att.target_hash, att.target_daa_score, att.validator_set_commitment)
        );

        let tx = stake_attestation_shard_tx(&shard);
        assert!(tx.inputs.is_empty() && tx.outputs.is_empty());
        assert_eq!(dns_tx_kind(&tx.subnetwork_id), Some(DnsTxKind::StakeAttestationShard));
        // The built payload must pass the stateless shard validator and decode back.
        assert!(validate_stake_attestation_shard_payload(&tx.payload).is_ok());
        let decoded: StakeAttestationShardPayload = borsh::from_slice(&tx.payload).unwrap();
        assert_eq!(decoded, shard);
    }

    #[test]
    fn dns_tx_kind_maps_overlay_subnetworks() {
        assert_eq!(dns_tx_kind(&SUBNETWORK_ID_STAKE_BOND), Some(DnsTxKind::StakeBond));
        assert_eq!(dns_tx_kind(&SUBNETWORK_ID_STAKE_ATTESTATION_SHARD), Some(DnsTxKind::StakeAttestationShard));
        assert_eq!(dns_tx_kind(&SUBNETWORK_ID_SLASHING_EVIDENCE), Some(DnsTxKind::SlashingEvidence));
        // Non-overlay subnetworks (native=0, coinbase=1, registry=2, unknown=3) → None.
        for b in [0u8, 1, 2, 3] {
            assert_eq!(dns_tx_kind(&SubnetworkId::from_byte(b)), None);
        }
        // dns_tx_kind agrees with the SubnetworkId::is_dns_overlay predicate.
        assert!(SUBNETWORK_ID_STAKE_BOND.is_dns_overlay());
        assert!(!SubnetworkId::from_byte(0).is_dns_overlay());
    }

    #[test]
    fn validate_stake_bond_payload_accepts_wellformed() {
        let bytes = borsh::to_vec(&fixture_bond()).unwrap();
        assert_eq!(validate_stake_bond_payload(&bytes), Ok(()));
    }

    #[test]
    fn validate_stake_bond_payload_rejects_malformed() {
        // Undecodable bytes.
        assert_eq!(validate_stake_bond_payload(&[0xff, 0x00, 0x12]), Err(DnsTxError::Decode));
        // Bad version.
        let mut bad = fixture_bond();
        bad.version = 2;
        assert_eq!(validate_stake_bond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::UnsupportedVersion(2)));
        // Zero bonded amount.
        let mut bad = fixture_bond();
        bad.amount = 0;
        assert_eq!(validate_stake_bond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ZeroBondAmount));
        // Wrong validator pubkey length.
        let mut bad = fixture_bond();
        bad.validator_pubkey = vec![0u8; STAKE_VALIDATOR_PUBKEY_LEN - 1];
        assert_eq!(
            validate_stake_bond_payload(&borsh::to_vec(&bad).unwrap()),
            Err(DnsTxError::InvalidPubKeyLen(STAKE_VALIDATOR_PUBKEY_LEN - 1))
        );
        // audit H-04: validator_pubkey_hash not derived from the validator key.
        let mut bad = fixture_bond();
        bad.validator_pubkey_hash = Hash64::from_bytes([0x01u8; 64]);
        assert_eq!(validate_stake_bond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ValidatorPubkeyHashMismatch));
    }

    // ---- kaspa-pq H-05: StakeUnbondRequest (audit) ----

    fn fixture_unbond(bond_outpoint: TransactionOutpoint) -> StakeUnbondRequestPayload {
        StakeUnbondRequestPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint,
            owner_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
            signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    #[test]
    fn validate_stake_unbond_payload_accepts_and_rejects() {
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x11u8; 64]), 0);
        assert_eq!(validate_stake_unbond_payload(&borsh::to_vec(&fixture_unbond(op)).unwrap()), Ok(()));
        assert_eq!(validate_stake_unbond_payload(&[0xff, 0x00]), Err(DnsTxError::Decode));
        let mut bad = fixture_unbond(op);
        bad.version = 2;
        assert_eq!(validate_stake_unbond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::UnsupportedVersion(2)));
        let mut bad = fixture_unbond(op);
        bad.owner_pubkey = vec![0u8; 10];
        assert_eq!(validate_stake_unbond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::InvalidPubKeyLen(10)));
        let mut bad = fixture_unbond(op);
        bad.signature = vec![0u8; 10];
        assert_eq!(validate_stake_unbond_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::InvalidSignatureLen(10)));
    }

    #[test]
    fn dns_tx_kind_maps_stake_unbond() {
        assert_eq!(dns_tx_kind(&SUBNETWORK_ID_STAKE_UNBOND), Some(DnsTxKind::StakeUnbond));
    }

    #[test]
    fn unbond_mutation_and_view_apply_revert() {
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x22u8; 64]), 0);
        let payload = borsh::to_vec(&fixture_unbond(op)).unwrap();
        let tx = Transaction::new(crate::constants::TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_UNBOND, 0, payload);
        // bond_mutations derives exactly one Unbond stamped at the accepting DAA.
        let muts = bond_mutations_from_accepted_txs(&[tx], 5_000, 0, 0);
        assert_eq!(muts, vec![BondMutation::Unbond(op, 5_000)]);
        // apply → unbond clock set → effective status Unbonding (precedence over activation).
        let mut view = ActiveBondView::from_records([(op, stake_bond_record_from_payload(&fixture_bond(), op))]);
        view.apply(&muts);
        assert_eq!(view.get(&op).unwrap().unbond_request_daa_score, Some(5_000));
        assert_eq!(effective_bond_status(view.get(&op).unwrap(), 5_000), BondStatus::Unbonding);
        // revert → cleared (clean, because the rule admits at most one unbond per bond).
        view.revert(&muts);
        assert_eq!(view.get(&op).unwrap().unbond_request_daa_score, None);
    }

    // ---- ADR-0016 D.1: validate_stake_bond_tx (stake-lock rule) ----

    #[test]
    fn validate_stake_bond_tx_accepts_locked_output() {
        let bond = fixture_bond();
        let bytes = borsh::to_vec(&bond).unwrap();
        let outputs = vec![TransactionOutput::new(bond.amount, p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload))];
        assert_eq!(validate_stake_bond_tx(&bytes, &outputs), Ok(()));
    }

    #[test]
    fn validate_stake_bond_tx_rejects_unlocked_or_misdirected() {
        let bond = fixture_bond();
        let bytes = borsh::to_vec(&bond).unwrap();
        // No output-0 to lock the stake in.
        assert_eq!(validate_stake_bond_tx(&bytes, &[]), Err(DnsTxError::MissingBondOutput));
        // Output-0 value != amount.
        let wrong_value = vec![TransactionOutput::new(bond.amount - 1, p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload))];
        assert_eq!(
            validate_stake_bond_tx(&bytes, &wrong_value),
            Err(DnsTxError::BondOutputValueMismatch { expected: bond.amount, got: bond.amount - 1 })
        );
        // Correct value but the output pays a different (non-owner) payload.
        let wrong_spk = vec![TransactionOutput::new(bond.amount, p2pkh_mldsa87_spk(&[0x00u8; 64]))];
        assert_eq!(validate_stake_bond_tx(&bytes, &wrong_spk), Err(DnsTxError::BondOutputScriptMismatch));
        // Payload checks still fire first (zero amount) regardless of outputs.
        let mut zero = bond.clone();
        zero.amount = 0;
        assert_eq!(validate_stake_bond_tx(&borsh::to_vec(&zero).unwrap(), &[]), Err(DnsTxError::ZeroBondAmount));
    }

    #[test]
    fn validate_attestation_shard_accepts_wellformed() {
        // The MAX-sized shard and the single-attestation lower bound both pass.
        assert_eq!(
            validate_stake_attestation_shard_payload(&borsh::to_vec(&fixture_shard(MAX_ATTESTATIONS_PER_SHARD)).unwrap()),
            Ok(())
        );
        assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&fixture_shard(1)).unwrap()), Ok(()));
    }

    #[test]
    fn validate_attestation_shard_rejects_malformed() {
        // Undecodable.
        assert_eq!(validate_stake_attestation_shard_payload(&[0x00]), Err(DnsTxError::Decode));
        // Empty shard.
        assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&fixture_shard(0)).unwrap()), Err(DnsTxError::EmptyShard));
        // Over the cardinality cap.
        let over = MAX_ATTESTATIONS_PER_SHARD + 1;
        assert_eq!(
            validate_stake_attestation_shard_payload(&borsh::to_vec(&fixture_shard(over)).unwrap()),
            Err(DnsTxError::ShardTooLarge(over))
        );
        // Bad shard version.
        let mut bad = fixture_shard(2);
        bad.version = 9;
        assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::UnsupportedVersion(9)));
        // Member attestation with wrong signature length.
        let mut bad = fixture_shard(2);
        bad.attestations[1].signature = vec![0u8; STAKE_ATTESTATION_SIG_LEN + 1];
        assert_eq!(
            validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()),
            Err(DnsTxError::InvalidSignatureLen(STAKE_ATTESTATION_SIG_LEN + 1))
        );
        // Member attestation that disagrees with the shard's anchor hash.
        let mut bad = fixture_shard(2);
        bad.attestations[1].target_hash = Hash64::from_bytes([0xee; 64]);
        assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ShardTupleMismatch));
        // Member attestation whose epoch disagrees.
        let mut bad = fixture_shard(2);
        bad.attestations[0].epoch = 999;
        assert_eq!(validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::ShardTupleMismatch));
        // audit #4: a non-zero validator_set_commitment is rejected (fixed-zero invariant).
        let mut bad = fixture_shard(1);
        bad.attestations[0].validator_set_commitment = Hash64::from_bytes([0x01; 64]);
        assert_eq!(
            validate_stake_attestation_shard_payload(&borsh::to_vec(&bad).unwrap()),
            Err(DnsTxError::NonZeroValidatorSetCommitment)
        );
    }

    #[test]
    fn validate_slashing_evidence_accepts_wellformed() {
        assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&fixture_evidence()).unwrap()), Ok(()));
    }

    #[test]
    fn validate_slashing_evidence_rejects_malformed() {
        // Undecodable.
        assert_eq!(validate_slashing_evidence_payload(&[0x01, 0x02]), Err(DnsTxError::Decode));
        // Bad version.
        let mut bad = fixture_evidence();
        bad.version = 5;
        assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::UnsupportedVersion(5)));
        // Same anchor → not equivocation.
        let mut bad = fixture_evidence();
        bad.attestation_b.target_hash = bad.attestation_a.target_hash;
        assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::EvidenceNotIncompatible));
        // Different validator_id → not the same triple.
        let mut bad = fixture_evidence();
        bad.attestation_b.validator_id = Hash64::from_bytes([0x5a; 64]);
        assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::EvidenceTripleMismatch));
        // Different epoch → not the same triple.
        let mut bad = fixture_evidence();
        bad.attestation_b.epoch += 1;
        assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::EvidenceTripleMismatch));
        // Payload bond_outpoint that does not match the cited attestations.
        let mut bad = fixture_evidence();
        bad.bond_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x01; 64]), 0);
        assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::EvidenceTripleMismatch));
        // Bad signature length in the first attestation.
        let mut bad = fixture_evidence();
        bad.attestation_a.signature = vec![0u8; 10];
        assert_eq!(validate_slashing_evidence_payload(&borsh::to_vec(&bad).unwrap()), Err(DnsTxError::InvalidSignatureLen(10)));
    }

    #[test]
    fn dns_tx_error_display_is_nonempty() {
        for e in [
            DnsTxError::Decode,
            DnsTxError::UnsupportedVersion(2),
            DnsTxError::ZeroBondAmount,
            DnsTxError::InvalidPubKeyLen(3),
            DnsTxError::InvalidSignatureLen(4),
            DnsTxError::EmptyShard,
            DnsTxError::ShardTooLarge(99),
            DnsTxError::ShardTupleMismatch,
            DnsTxError::EvidenceTripleMismatch,
            DnsTxError::EvidenceNotIncompatible,
        ] {
            assert!(!e.to_string().is_empty());
        }
    }

    // ---- PR-10.9 foundation: stake-bond lifecycle helpers ----

    #[test]
    fn stake_bond_record_from_payload_initializes_pending() {
        let payload = fixture_bond(); // amount 100e9, activation 5_000, unbonding 100_000
        let op = fixture_outpoint();
        let rec = stake_bond_record_from_payload(&payload, op);
        assert_eq!(rec.bond_outpoint, op);
        assert_eq!(rec.version, payload.version);
        assert_eq!(rec.owner_pubkey_hash, payload.owner_pubkey_hash);
        assert_eq!(rec.validator_pubkey_hash, payload.validator_pubkey_hash);
        assert_eq!(rec.validator_pubkey, payload.validator_pubkey);
        assert_eq!(rec.amount, payload.amount);
        assert_eq!(rec.activation_daa_score, payload.activation_daa_score);
        assert_eq!(rec.unbonding_period_blocks, payload.unbonding_period_blocks);
        assert_eq!(rec.unbond_request_daa_score, None);
        assert_eq!(rec.slashed_at_daa_score, None);
        assert_eq!(rec.status, BondStatus::Pending);
    }

    #[test]
    fn effective_bond_status_activation_transition() {
        let rec = stake_bond_record_from_payload(&fixture_bond(), fixture_outpoint()); // activation 5_000
        assert_eq!(effective_bond_status(&rec, 0), BondStatus::Pending);
        assert_eq!(effective_bond_status(&rec, 4_999), BondStatus::Pending);
        assert_eq!(effective_bond_status(&rec, 5_000), BondStatus::Active); // inclusive at activation
        assert_eq!(effective_bond_status(&rec, 1_000_000), BondStatus::Active);
        assert!(!is_bond_active_at(&rec, 4_999));
        assert!(is_bond_active_at(&rec, 5_000));
    }

    #[test]
    fn effective_bond_status_unbonding_then_slashed_precedence() {
        let mut rec = stake_bond_record_from_payload(&fixture_bond(), fixture_outpoint()); // activation 5_000, unbonding_period 100_000
        // Active before any unbond/slash.
        assert_eq!(effective_bond_status(&rec, 10_000), BondStatus::Active);

        // Unbond requested at 20_000 -> Unbonding from that height (not active).
        rec.unbond_request_daa_score = Some(20_000);
        assert_eq!(effective_bond_status(&rec, 19_999), BondStatus::Active);
        assert_eq!(effective_bond_status(&rec, 20_000), BondStatus::Unbonding);
        assert!(!is_bond_active_at(&rec, 20_000));
        assert_eq!(bond_release_daa_score(&rec), Some(120_000)); // 20_000 + 100_000

        // A slash at 25_000 takes precedence over the unbond from its height on.
        rec.slashed_at_daa_score = Some(25_000);
        assert_eq!(effective_bond_status(&rec, 24_999), BondStatus::Unbonding);
        assert_eq!(effective_bond_status(&rec, 25_000), BondStatus::Slashed);
        assert_eq!(effective_bond_status(&rec, u64::MAX), BondStatus::Slashed);
    }

    #[test]
    fn bond_release_daa_score_none_without_unbond_and_saturates() {
        let mut rec = stake_bond_record_from_payload(&fixture_bond(), fixture_outpoint());
        assert_eq!(bond_release_daa_score(&rec), None);
        // saturating_add: a pathological u64::MAX request height never wraps early.
        rec.unbond_request_daa_score = Some(u64::MAX);
        assert_eq!(bond_release_daa_score(&rec), Some(u64::MAX));
    }

    fn dns_overlay_tx(subnetwork_id: SubnetworkId, payload: Vec<u8>) -> Transaction {
        Transaction::new(0, vec![], vec![], 0, subnetwork_id, 0, payload)
    }

    #[test]
    fn bond_mutations_extracts_insert_and_slash() {
        let bond_payload = fixture_bond();
        let bond_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&bond_payload).unwrap());
        let expected_outpoint = TransactionOutpoint::new(bond_tx.id(), 0); // A.1: output 0

        let evidence = fixture_evidence(); // references fixture_outpoint() as its bond
        let slash_tx = dns_overlay_tx(SUBNETWORK_ID_SLASHING_EVIDENCE, borsh::to_vec(&evidence).unwrap());

        // Attestation-shard + a native tx contribute no bond mutations.
        let shard_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, borsh::to_vec(&fixture_shard(2)).unwrap());
        let native_tx = dns_overlay_tx(SubnetworkId::from_byte(0), vec![1, 2, 3]);

        let muts = bond_mutations_from_accepted_txs(&[bond_tx, slash_tx, shard_tx, native_tx], 12_345, 0, 0);
        assert_eq!(muts.len(), 2);
        // P-1B: the inserted record's activation is clamped up to the acceptance DAA (12_345 here),
        // so a bond cannot back-date itself into a past epoch's active set / StakeScore denominator.
        let mut expected_record = stake_bond_record_from_payload(&bond_payload, expected_outpoint);
        expected_record.activation_daa_score = expected_record.activation_daa_score.max(12_345);
        // ADR-0022: `created_daa_score` is stamped with the acceptance DAA (the bond's creation point).
        expected_record.created_daa_score = 12_345;
        assert_eq!(muts[0], BondMutation::Insert(expected_outpoint, expected_record));
        assert_eq!(muts[1], BondMutation::Slash(evidence.bond_outpoint, 12_345));
    }

    #[test]
    fn bond_activation_clamped_to_acceptance_daa() {
        // P-1B (DNS v2/v3): a bond declaring a past (or zero) activation cannot back-date itself
        // into a historical epoch's active set — the inserted record's activation is
        // max(declared, acceptance DAA). A future-dated activation is left as declared.
        let mut past = fixture_bond();
        past.activation_daa_score = 5;
        let past_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&past).unwrap());
        match &bond_mutations_from_accepted_txs(&[past_tx], 10_000, 0, 0)[0] {
            BondMutation::Insert(_, record) => {
                assert_eq!(record.activation_daa_score, 10_000, "past activation is clamped up to acceptance DAA");
            }
            _ => panic!("expected an Insert mutation"),
        }

        let mut future = fixture_bond();
        future.activation_daa_score = 50_000;
        let future_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&future).unwrap());
        match &bond_mutations_from_accepted_txs(&[future_tx], 10_000, 0, 0)[0] {
            BondMutation::Insert(_, record) => {
                assert_eq!(record.activation_daa_score, 50_000, "future activation is left as declared (no down-clamp)");
            }
            _ => panic!("expected an Insert mutation"),
        }
    }

    #[test]
    fn shard_tuple_rejects_target_daa_score_mismatch() {
        // P-1C (DNS v2/v3): the shard's (epoch, target_hash, target_daa_score, vsc) tuple must
        // match every contained attestation — including target_daa_score (previously unchecked).
        let att = fixture_attestation();
        let mut shard = single_attestation_shard(att);
        // Desync ONLY target_daa_score (the attestation keeps its own value).
        shard.target_daa_score = shard.target_daa_score.wrapping_add(1);
        let payload = borsh::to_vec(&shard).unwrap();
        assert!(
            matches!(validate_stake_attestation_shard_payload(&payload), Err(DnsTxError::ShardTupleMismatch)),
            "a shard whose target_daa_score disagrees with its attestation must be rejected"
        );
    }

    // ---- DNS v3: Canonical Lagged Anchor (blue_score-coordinated) ----

    #[test]
    fn ready_epoch_from_tip_blue_score_off_by_one() {
        // L=100, lag=20 -> epoch E is ready iff tip_blue >= epoch_end(E)+lag = (E+1)*100-1+20.
        assert_eq!(ready_epoch_from_tip_blue_score(118, 100, 20), None); // epoch_end(0)+lag = 119
        assert_eq!(ready_epoch_from_tip_blue_score(119, 100, 20), Some(0));
        assert_eq!(ready_epoch_from_tip_blue_score(219, 100, 20), Some(1));
        assert_eq!(ready_epoch_from_tip_blue_score(0, 100, 20), None);
    }

    #[test]
    fn canonical_anchor_most_recent_at_or_below_cutoff() {
        // L=100, backoff=10 -> cutoff(1) = epoch_end(1)-10 = 199-10 = 189.
        let h = |n: u8| Hash64::from_bytes([n; 64]);
        let ancestors = vec![(h(9), 250, 2500), (h(8), 190, 1900), (h(7), 185, 1850), (h(6), 100, 1000), (h(5), 50, 500)];
        let a = canonical_lagged_epoch_anchor(1, 100, 10, &ancestors).unwrap();
        assert_eq!(a.anchor_hash, h(7), "most-recent ancestor with blue_score <= 189 is h7@185");
        assert_eq!(a.anchor_blue_score, 185);
        assert_eq!(a.anchor_daa_score, 1850);
        assert_eq!(a.cutoff_blue_score, 189);
        assert_eq!(a.epoch_end_blue_score, 199);
        assert!(!a.duplicate_of_previous_anchor);
    }

    #[test]
    fn canonical_anchor_no_hole_on_blue_score_jump() {
        // High-parallel: blue_score jumps 300 -> 80 across one selected-chain step, so NO ancestor
        // lands inside epoch 1's [100,199] band — yet the anchor still exists (most-recent at/below
        // the cutoff). The "in-epoch block" requirement is what created the original hole; the
        // cutoff (<=) formulation removes it without falling back to the (split-prone) chain index.
        let h = |n: u8| Hash64::from_bytes([n; 64]);
        let ancestors = vec![(h(9), 300, 3000), (h(8), 80, 800), (h(7), 10, 100)];
        let a = canonical_lagged_epoch_anchor(1, 100, 10, &ancestors).unwrap(); // cutoff = 189
        assert_eq!(a.anchor_hash, h(8), "anchor exists even with no in-epoch block");
        assert_eq!(a.anchor_blue_score, 80);
    }

    #[test]
    fn canonical_anchor_duplicate_flagged_and_pruning_base_invariant() {
        // The anchor depends ONLY on (hash, blue_score) — there is no index input — so the
        // selected-chain index ORIGIN (archival genesis=0 vs IBD pruning_point=0) is irrelevant:
        // identical blue_score data => identical anchor. That is the pruning-base invariance that
        // the retracted chain-index design lacked.
        let h = |n: u8| Hash64::from_bytes([n; 64]);
        // Sparse chain: nothing in (189,289] or (289,389], so epochs 2 and 3 reuse anchors.
        let ancestors = vec![(h(9), 400, 4000), (h(8), 250, 2500), (h(7), 150, 1500), (h(6), 50, 500)];
        let e1 = canonical_lagged_epoch_anchor(1, 100, 10, &ancestors).unwrap(); // cutoff 189 -> h7@150
        let e2 = canonical_lagged_epoch_anchor(2, 100, 10, &ancestors).unwrap(); // cutoff 289 -> h8@250
        let e3 = canonical_lagged_epoch_anchor(3, 100, 10, &ancestors).unwrap(); // cutoff 389 -> h8@250
        assert_eq!(e1.anchor_hash, h(7));
        assert!(!e1.duplicate_of_previous_anchor);
        assert_eq!(e2.anchor_hash, h(8));
        assert!(!e2.duplicate_of_previous_anchor, "e2 anchor (h8) differs from e1 anchor (h7)");
        assert_eq!(e3.anchor_hash, h(8));
        assert!(e3.duplicate_of_previous_anchor, "epoch 3 reuses epoch 2's anchor -> not creditable");
    }

    #[test]
    fn dns_v3_params_consistency_gate() {
        use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS};
        // The SHIPPED presets must be v3-consistent — else the reorg gate could never enter
        // Active on the very networks that use them.
        assert!(GENESIS_ACTIVE_DNS_PARAMS.dns_v3_params_consistent(), "devnet/simnet preset is v3-consistent");
        assert!(PRODUCTION_DNS_PARAMS.dns_v3_params_consistent(), "mainnet/testnet preset is v3-consistent");

        // Each invariant violation flips the gate false (fail-safe: update_dns_state then never
        // enters Active, so finality stays dormant rather than ill-defined).
        let mut p = GENESIS_ACTIVE_DNS_PARAMS;
        p.attestation_epoch_length_blue_score = 0;
        assert!(!p.dns_v3_params_consistent(), "a zero epoch length is rejected");

        let mut p = GENESIS_ACTIVE_DNS_PARAMS;
        p.attestation_lag_blue_score = 0;
        assert!(!p.dns_v3_params_consistent(), "a zero lag is rejected");

        let mut p = GENESIS_ACTIVE_DNS_PARAMS;
        p.attestation_anchor_backoff_blue_score = p.attestation_epoch_length_blue_score;
        assert!(!p.dns_v3_params_consistent(), "backoff >= epoch length is rejected");

        let mut p = GENESIS_ACTIVE_DNS_PARAMS;
        p.stake_score_window_blue_score = p.attestation_epoch_length_blue_score; // < depth·L + lag + backoff
        assert!(!p.dns_v3_params_consistent(), "a window too short to cover the creditable horizon is rejected");
        // audit M-05: covering only 2L (not required_stake_depth epochs) must be rejected.
        let mut p = GENESIS_ACTIVE_DNS_PARAMS;
        let depth_epochs = (p.required_stake_depth.0 / STAKE_SCORE_SCALE) as u64;
        assert!(depth_epochs > 2, "fixture assumes a deep stake horizon");
        p.stake_score_window_blue_score =
            2 * p.attestation_epoch_length_blue_score + p.attestation_lag_blue_score + p.attestation_anchor_backoff_blue_score;
        assert!(!p.dns_v3_params_consistent(), "covering only 2L is insufficient when required_stake_depth > 2 epochs");
    }

    #[test]
    fn shipped_dns_presets_keep_attestation_out_of_base_chain_validity() {
        use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};

        for (name, params) in [
            ("genesis-active/dev-sim", GENESIS_ACTIVE_DNS_PARAMS),
            ("production/mainnet", PRODUCTION_DNS_PARAMS),
            ("testnet", TESTNET_DNS_PARAMS),
        ] {
            assert_eq!(
                params.mandatory_attestation_inclusion_daa_score,
                u64::MAX,
                "{name} must be liveness-first: missing attestations degrade finality, not block validity"
            );
            assert_eq!(params.stake_event_quality_floor_bps, 6000, "{name} must retain the 60% StakeScore/reward quality floor");
        }
    }

    #[test]
    fn bridge_finality_freshness_requires_confirmed_non_stale_dns_anchor() {
        let anchor = Hash64::from_bytes([0x42; 64]);
        let anchor_daa = 10_000;

        assert!(dns_finality_fresh_for_bridge(
            true,
            anchor,
            anchor_daa,
            anchor_daa + DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE,
            DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE
        ));
        assert!(!dns_finality_fresh_for_bridge(
            false,
            anchor,
            anchor_daa,
            anchor_daa,
            DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE
        ));
        assert!(!dns_finality_fresh_for_bridge(true, Hash64::default(), 0, 0, DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE));
        assert!(!dns_finality_fresh_for_bridge(
            true,
            anchor,
            anchor_daa,
            anchor_daa - 1,
            DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE
        ));
        assert!(!dns_finality_fresh_for_bridge(
            true,
            anchor,
            anchor_daa,
            anchor_daa + DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE + 1,
            DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE
        ));
        assert!(dns_finality_fresh_for_bridge(true, anchor, anchor_daa, anchor_daa + 42, 42));
        assert!(!dns_finality_fresh_for_bridge(true, anchor, anchor_daa, anchor_daa + 43, 42));
    }

    #[test]
    fn bond_mutations_skips_undecodable_overlay_payload() {
        // A malformed stake-bond payload is defensively skipped, not panicked on.
        let bad = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, vec![0xff, 0x00, 0x12]);
        assert!(bond_mutations_from_accepted_txs(&[bad], 0, 0, 0).is_empty());
    }

    #[test]
    fn bond_mutations_empty_without_overlay_txs() {
        let native = dns_overlay_tx(SubnetworkId::from_byte(0), vec![]);
        let coinbase = dns_overlay_tx(SubnetworkId::from_byte(1), vec![]);
        assert!(bond_mutations_from_accepted_txs(&[native, coinbase], 100, 0, 0).is_empty());
    }

    #[test]
    fn bond_mutations_enforce_min_amount_and_unbonding_floor() {
        // fixture_bond: amount = 100_000_000_000, declared unbonding_period_blocks = 100_000.
        let bond_tx = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&fixture_bond()).unwrap());

        // (a) below the per-bond minimum → not admitted (no mutation).
        assert!(
            bond_mutations_from_accepted_txs(std::slice::from_ref(&bond_tx), 1, 100_000_000_001, 0).is_empty(),
            "a bond below min_bond_amount must be rejected"
        );
        // (b) at/above the minimum → admitted.
        let muts = bond_mutations_from_accepted_txs(std::slice::from_ref(&bond_tx), 1, 100_000_000_000, 0);
        assert_eq!(muts.len(), 1, "a bond at exactly the minimum is accepted");

        // (c) the stored unbonding period is clamped UP to the floor (declared 100_000 < 500_000).
        let muts = bond_mutations_from_accepted_txs(std::slice::from_ref(&bond_tx), 1, 0, 500_000);
        match &muts[0] {
            BondMutation::Insert(_, rec) => assert_eq!(rec.unbonding_period_blocks, 500_000, "unbonding clamped to floor"),
            other => panic!("expected Insert, got {other:?}"),
        }
        // (d) a floor below the declared value leaves the larger declared value intact.
        let muts = bond_mutations_from_accepted_txs(&[bond_tx], 1, 0, 50_000);
        match &muts[0] {
            BondMutation::Insert(_, rec) => assert_eq!(rec.unbonding_period_blocks, 100_000, "declared > floor is kept"),
            other => panic!("expected Insert, got {other:?}"),
        }
    }

    // ---- Addendum B §B.1: ActiveBondView + RewardedEpochSet ----

    fn fixture_bond_record(op: TransactionOutpoint) -> StakeBondRecord {
        // activation_daa_score = 5_000, status = Pending.
        stake_bond_record_from_payload(&fixture_bond(), op)
    }

    // ----- ADR-0018 "本格版" (PoS-v2) Phase 1: epoch accumulator pure core -----

    fn op(byte: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([byte; 64]), 0)
    }

    /// A bond with controllable amount / activation / reward payload (for accumulator tests).
    fn bond_rec(op: TransactionOutpoint, amount: u64, activation: u64, payload_byte: u8) -> StakeBondRecord {
        let mut r = fixture_bond_record(op);
        r.amount = amount;
        r.activation_daa_score = activation;
        r.owner_reward_spk_payload = [payload_byte; 64];
        r
    }

    fn contrib(daa: u64, keys: &[(TransactionOutpoint, u64)], quality: u64) -> BlockEpochContribution {
        BlockEpochContribution { block_daa_score: daa, rewarded_keys: keys.to_vec(), quality_subpool: quality }
    }

    /// Accumulation: `included` is keyed by **attestation** epoch (resolved against the bond
    /// snapshot, in contribution order) while `quality_pool_accrued` is keyed by **block** epoch
    /// — the two attributions are independent. Each block also makes its own epoch present.
    #[test]
    fn recompute_epoch_tallies_accumulates_included_and_quality() {
        let (a, b) = (op(0xA1), op(0xB2));
        let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 0, 0xB2)];
        // Two blocks in epoch 1 reward epoch-0 attestations (recency lag); no block in epoch 0.
        let contributions = vec![contrib(12, &[(a, 0)], 5), contrib(15, &[(b, 0)], 7)];

        let tallies = recompute_epoch_tallies(30, 10, 100, &contributions, &bonds);
        assert_eq!(
            tallies,
            vec![
                // epoch 0: included = both attesters (contribution order), no block of its own → pool 0.
                (
                    0,
                    EpochTally {
                        expected_stake: 300,
                        included: vec![(Hash64::from_bytes([0xA1; 64]), 100), (Hash64::from_bytes([0xB2; 64]), 200)],
                        quality_pool_accrued: 0,
                        finalized: false,
                    }
                ),
                // epoch 1: the two blocks live here → pool 5+7, but no epoch-1 attestation was rewarded.
                (1, EpochTally { expected_stake: 300, included: vec![], quality_pool_accrued: 12, finalized: false }),
            ]
        );
    }

    /// `expected_stake` is evaluated at each epoch's anchor (`epoch · epoch_length`): a bond that
    /// only activates later is excluded from earlier epochs' denominators.
    #[test]
    fn recompute_epoch_tallies_expected_stake_respects_activation() {
        let (a, b) = (op(0xA1), op(0xB2));
        // b activates at daa 100 (epoch 10) — absent from epoch 0/1 denominators.
        let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 100, 0xB2)];
        let contributions = vec![contrib(5, &[(a, 0)], 0), contrib(15, &[(a, 1)], 0)];
        let tallies = recompute_epoch_tallies(40, 10, 100, &contributions, &bonds);
        // epoch 0 anchor 0, epoch 1 anchor 10 — both before b's activation → expected 100.
        assert_eq!(tallies[0].1.expected_stake, 100);
        assert_eq!(tallies[1].1.expected_stake, 100);
    }

    /// Finalization is one-way at `sink_daa ≥ (E+1)·epoch_length + finalization_depth`.
    #[test]
    fn recompute_epoch_tallies_finalization_boundary() {
        let a = op(0xA1);
        let bonds = vec![bond_rec(a, 100, 0, 0xA1)];
        let contributions = vec![contrib(5, &[], 3)]; // one block in epoch 0
        // epoch 0 finalizes at (0+1)*10 + 100 = 110.
        assert!(!recompute_epoch_tallies(109, 10, 100, &contributions, &bonds)[0].1.finalized);
        assert!(recompute_epoch_tallies(110, 10, 100, &contributions, &bonds)[0].1.finalized);
    }

    /// No contributions ⇒ no tallies.
    #[test]
    fn recompute_epoch_tallies_empty() {
        assert!(recompute_epoch_tallies(1_000, 10, 100, &[], &[bond_rec(op(0xA1), 100, 0, 0xA1)]).is_empty());
    }

    /// A rewarded key whose bond is absent from the snapshot is skipped from `included` (the epoch
    /// is still emitted because the block lives in it).
    #[test]
    fn recompute_epoch_tallies_unresolvable_bond_skipped() {
        let (a, ghost) = (op(0xA1), op(0xEE));
        let bonds = vec![bond_rec(a, 100, 0, 0xA1)];
        let contributions = vec![contrib(3, &[(ghost, 0)], 4)]; // block in epoch 0 rewards an unknown bond
        let tallies = recompute_epoch_tallies(30, 10, 100, &contributions, &bonds);
        assert_eq!(tallies.len(), 1);
        assert_eq!(tallies[0].0, 0);
        assert!(tallies[0].1.included.is_empty());
        assert_eq!(tallies[0].1.quality_pool_accrued, 4);
    }

    /// One block may reward attestations for several distinct epochs; each lands in its own tally.
    #[test]
    fn recompute_epoch_tallies_multi_epoch_block() {
        let (a, b) = (op(0xA1), op(0xB2));
        let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 0, 0xB2)];
        let contributions = vec![contrib(25, &[(a, 0), (b, 1)], 9)]; // block in epoch 2
        let tallies = recompute_epoch_tallies(30, 10, 100, &contributions, &bonds);
        assert_eq!(tallies.len(), 3);
        assert_eq!(
            tallies[0],
            (
                0,
                EpochTally {
                    expected_stake: 300,
                    included: vec![(Hash64::from_bytes([0xA1; 64]), 100)],
                    quality_pool_accrued: 0,
                    finalized: false
                }
            )
        );
        assert_eq!(
            tallies[1],
            (
                1,
                EpochTally {
                    expected_stake: 300,
                    included: vec![(Hash64::from_bytes([0xB2; 64]), 200)],
                    quality_pool_accrued: 0,
                    finalized: false
                }
            )
        );
        assert_eq!(tallies[2], (2, EpochTally { expected_stake: 300, included: vec![], quality_pool_accrued: 9, finalized: false }));
    }

    /// Reorg-safety: the tally is a pure function of the supplied (selected-chain) contributions, so
    /// re-deriving an epoch from a different chain's blocks yields that chain's tally — no stale state.
    #[test]
    fn recompute_epoch_tallies_is_pure_per_chain() {
        let (a, b) = (op(0xA1), op(0xB2));
        let bonds = vec![bond_rec(a, 100, 0, 0xA1), bond_rec(b, 200, 0, 0xB2)];
        let chain_a = recompute_epoch_tallies(30, 10, 100, &[contrib(2, &[(a, 0)], 5)], &bonds);
        let chain_b = recompute_epoch_tallies(30, 10, 100, &[contrib(2, &[(b, 0)], 8)], &bonds);
        assert_eq!(chain_a[0].1.included, vec![(Hash64::from_bytes([0xA1; 64]), 100)]);
        assert_eq!(chain_a[0].1.quality_pool_accrued, 5);
        assert_eq!(chain_b[0].1.included, vec![(Hash64::from_bytes([0xB2; 64]), 200)]);
        assert_eq!(chain_b[0].1.quality_pool_accrued, 8);
    }

    // ----- ADR-0018 "本格版" (PoS-v2) Phase 2: φS gate + deferred quality-bonus outputs -----

    /// The φS gate is the integer cross-multiply `included·10_000 ≥ φS_bps·expected`, with the
    /// `expected == 0` and `φS_bps == 0` edge cases both meeting vacuously.
    #[test]
    fn epoch_meets_quality_floor_boundary() {
        // φS = 60%: included must be ≥ 600/1000.
        assert!(epoch_meets_quality_floor(600, 1000, 6000)); // exactly at the floor → meets
        assert!(!epoch_meets_quality_floor(599, 1000, 6000)); // one below → misses
        assert!(epoch_meets_quality_floor(601, 1000, 6000));
        // φS = 0 → always meets (pre-ADR-0018 behavior).
        assert!(epoch_meets_quality_floor(0, 1000, 0));
        // expected = 0 → vacuously meets.
        assert!(epoch_meets_quality_floor(0, 0, 6000));
    }

    #[test]
    fn mandatory_attestation_mass_capacity_detects_impossible_active_set() {
        let fits = mandatory_attestation_mass_capacity(std::iter::repeat_n(100u64, 10), 1_000, 0, 6000, 500_000, 50_000);
        assert_eq!(fits.expected_stake, 1_000);
        assert_eq!(fits.required_stake, 600);
        assert_eq!(fits.required_stake_delta, 600);
        assert_eq!(fits.required_validator_count, 6);
        assert_eq!(fits.required_shard_count, 6);
        assert!(fits.fits, "six single-validator shards fit in a 500k block at 50k per shard");

        let exact = mandatory_attestation_mass_capacity(std::iter::repeat_n(100u64, 17), 1_700, 700, 6000, 500_000, 50_000);
        assert_eq!(exact.required_stake, 1_020);
        assert_eq!(exact.required_stake_delta, 320);
        assert_eq!(exact.required_validator_count, 4);
        assert_eq!(exact.required_shard_count, 4);
        assert_eq!(exact.max_shard_count_by_mass, 10);
        assert!(exact.fits, "four remaining single-validator shards fit");

        let over = mandatory_attestation_mass_capacity(std::iter::repeat_n(100u64, 30), 3_000, 0, 6000, 500_000, 50_000);
        assert_eq!(over.required_validator_count, 18);
        assert_eq!(over.required_shard_count, 18);
        assert!(!over.fits, "eighteen single-validator shards cannot fit in one 500k block at 50k per shard");
    }

    #[test]
    fn mandatory_attestation_mass_capacity_uses_best_case_stake_packing() {
        // A single large validator can satisfy 60% even if many tiny validators are active.
        let mut stakes = vec![10_000u64];
        stakes.extend(std::iter::repeat_n(1u64, 200));
        let expected_stake = stakes.iter().sum();
        let cap = mandatory_attestation_mass_capacity(stakes, expected_stake, 0, 6000, 50_000, 50_000);
        assert_eq!(cap.required_validator_count, 1);
        assert_eq!(cap.required_shard_count, 1);
        assert!(cap.fits);
    }

    #[test]
    fn mandatory_attestation_mass_capacity_uses_remaining_delta() {
        let cap = mandatory_attestation_mass_capacity(std::iter::repeat_n(1u64, 401), 1_000, 599, 6000, 100, 100);
        assert_eq!(cap.required_stake, 600);
        assert_eq!(cap.required_stake_delta, 1);
        assert_eq!(cap.required_validator_count, 1);
        assert_eq!(cap.required_shard_count, 1);
        assert!(cap.fits, "one remaining sompi of stake needs only one single-validator shard");
    }

    #[test]
    fn mandatory_attestation_mass_capacity_50200_threshold_is_nine_shards() {
        let fits_nine = mandatory_attestation_mass_capacity(std::iter::repeat_n(100u64, 15), 1_500, 0, 6000, 500_000, 50_200);
        assert_eq!(fits_nine.required_validator_count, 9);
        assert_eq!(fits_nine.required_shard_count, 9);
        assert_eq!(fits_nine.max_shard_count_by_mass, 9);
        assert_eq!(fits_nine.required_mass, 451_800);
        assert!(fits_nine.fits, "nine 50,200-mass shards fit in a 500k block");

        let needs_ten = mandatory_attestation_mass_capacity(std::iter::repeat_n(100u64, 16), 1_600, 0, 6000, 500_000, 50_200);
        assert_eq!(needs_ten.required_validator_count, 10);
        assert_eq!(needs_ten.required_shard_count, 10);
        assert_eq!(needs_ten.max_shard_count_by_mass, 9);
        assert_eq!(needs_ten.required_mass, 502_000);
        assert!(!needs_ten.fits, "ten 50,200-mass shards exceed a 500k block");
    }

    /// Met epoch: each included validator is paid a proportional share of the quality pool, in
    /// stored order, to its ML-DSA P2PKH script.
    #[test]
    fn validator_quality_bonus_outputs_met_pays_proportional() {
        let included = vec![(Hash64::from_bytes([0xA1; 64]), 100u64), (Hash64::from_bytes([0xB2; 64]), 300u64)];
        let out = validator_quality_bonus_outputs(1000, &included, 400, true);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].value, 250); // 1000 · 100/400
        assert_eq!(out[0].script_public_key, p2pkh_mldsa87_spk(&[0xA1; 64]));
        assert_eq!(out[1].value, 750); // 1000 · 300/400
        assert_eq!(out[1].script_public_key, p2pkh_mldsa87_spk(&[0xB2; 64]));
        // Value-conserving: Σ ≤ pool.
        assert!(out.iter().map(|o| o.value as u128).sum::<u128>() <= 1000);
    }

    /// Unmet epoch ⇒ no outputs (the whole pool rolls over).
    #[test]
    fn validator_quality_bonus_outputs_unmet_is_empty() {
        let included = vec![(Hash64::from_bytes([0xA1; 64]), 100u64)];
        assert!(validator_quality_bonus_outputs(1000, &included, 400, false).is_empty());
    }

    /// A zero-stake (or zero-share) validator emits no output even when the epoch met φS.
    #[test]
    fn validator_quality_bonus_outputs_zero_share_skipped() {
        let included = vec![(Hash64::from_bytes([0xA1; 64]), 0u64), (Hash64::from_bytes([0xB2; 64]), 400u64)];
        let out = validator_quality_bonus_outputs(1000, &included, 400, true);
        assert_eq!(out.len(), 1); // only the 0xB2 validator
        assert_eq!(out[0].value, 1000);
        assert_eq!(out[0].script_public_key, p2pkh_mldsa87_spk(&[0xB2; 64]));
    }

    /// The deferred-payout once-per-epoch guard: a block finalizes exactly the epochs whose
    /// threshold `(E+1)·L + fd` is crossed in `(parent_daa, daa_score]`. With L=10, fd=100 the
    /// thresholds are 110 (E0), 120 (E1), 130 (E2), …
    #[test]
    fn epochs_finalized_at_crossing() {
        // Nothing buried yet (daa below epoch-0's threshold 110).
        assert_eq!(epochs_finalized_at(108, 109, 10, 100), None);
        assert_eq!(epochs_finalized_at(0, 50, 10, 100), None); // daa < fd
        // Exactly epoch 0 crosses at 110.
        assert_eq!(epochs_finalized_at(109, 110, 10, 100), Some((0, 0)));
        // Parent already finalized epoch 0 ⇒ this block finalizes nothing new.
        assert_eq!(epochs_finalized_at(110, 111, 10, 100), None);
        // Next block to reach 120 finalizes epoch 1.
        assert_eq!(epochs_finalized_at(110, 120, 10, 100), Some((1, 1)));
        // A DAA jump finalizes a contiguous range at once (epochs 0,1,2 at 110/120/130).
        assert_eq!(epochs_finalized_at(109, 130, 10, 100), Some((0, 2)));
        // Early chain: parent below the first threshold ⇒ range starts at epoch 0.
        assert_eq!(epochs_finalized_at(50, 115, 10, 100), Some((0, 0)));
        // epoch_length 0 is clamped to 1 (no panic / div-by-zero).
        assert_eq!(epochs_finalized_at(0, 105, 0, 100), Some((0, 4)));
    }

    #[test]
    fn active_bond_view_apply_insert_then_resolve() {
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let mut view = ActiveBondView::new();
        assert!(view.is_empty());
        view.apply(&[BondMutation::Insert(op, fixture_bond_record(op))]);
        assert_eq!(view.len(), 1);
        // Active well past activation; not active before it.
        assert!(view.active_bond_at(&op, 10_000).is_some());
        assert!(view.active_bond_at(&op, 0).is_none());
        // Unknown outpoint resolves to None.
        let other = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
        assert!(view.active_bond_at(&other, 10_000).is_none());
    }

    #[test]
    fn active_bond_view_from_records_seeds_verbatim() {
        // Seeding from the store snapshot must preserve each record's fields
        // (incl. an already-slashed one) and resolve them correctly.
        let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
        let mut slashed = fixture_bond_record(op2);
        slashed.slashed_at_daa_score = Some(1);
        slashed.status = BondStatus::Slashed;
        let view = ActiveBondView::from_records([(op1, fixture_bond_record(op1)), (op2, slashed)]);
        assert_eq!(view.len(), 2);
        assert!(view.active_bond_at(&op1, 10_000).is_some());
        assert!(view.active_bond_at(&op2, 10_000).is_none()); // seeded as slashed

        // ADR-0018 §H: `records()` snapshots every bond (any status) for the per-branch
        // reorg-gate StakeScore walk — both the active and the slashed one.
        let outpoints: Vec<_> = view.records().iter().map(|r| r.bond_outpoint).collect();
        assert_eq!(outpoints.len(), 2);
        assert!(outpoints.contains(&op1) && outpoints.contains(&op2));
    }

    #[test]
    fn active_bond_view_revert_insert_removes() {
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let muts = vec![BondMutation::Insert(op, fixture_bond_record(op))];
        let mut view = ActiveBondView::new();
        view.apply(&muts);
        view.revert(&muts);
        assert!(view.is_empty());
        assert!(view.get(&op).is_none());
    }

    #[test]
    fn active_bond_view_slash_then_revert_round_trips() {
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let mut view = ActiveBondView::new();
        view.apply(&[BondMutation::Insert(op, fixture_bond_record(op))]);
        assert!(view.active_bond_at(&op, 10_000).is_some());

        // Slash: record becomes Slashed → not active at any DAA score.
        let slash = vec![BondMutation::Slash(op, 8_000)];
        view.apply(&slash);
        assert_eq!(view.get(&op).unwrap().status, BondStatus::Slashed);
        assert_eq!(view.get(&op).unwrap().slashed_at_daa_score, Some(8_000));
        assert!(view.active_bond_at(&op, 10_000).is_none());

        // Revert slash (mirrors stage_dns_bond_mutations: clears slash,
        // status → Active); time-based activation makes it active again.
        view.revert(&slash);
        assert_eq!(view.get(&op).unwrap().slashed_at_daa_score, None);
        assert!(view.active_bond_at(&op, 10_000).is_some());
    }

    #[test]
    fn active_bond_view_multi_block_apply_then_reverse_revert_restores_consensus_state() {
        // Apply blocks b1, b2 then revert b2 (reverse chain order) → the
        // post-b1 *consensus state* is restored, exactly like a UTXO reorg.
        //
        // Note: equality is asserted over the consensus-relevant queries
        // (existence + `active_bond_at`), NOT full struct equality. A
        // Slash→revert leaves the cosmetic `status` enum at `Active` even
        // if the bond was `Pending` pre-slash — this faithfully mirrors
        // `stage_dns_bond_mutations` (the persisted store does the same),
        // and is consensus-invisible because every read goes through
        // `effective_bond_status`, which derives status purely from the
        // DAA-stamped fields and ignores the stored `status` enum.
        let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
        let b1 = vec![BondMutation::Insert(op1, fixture_bond_record(op1))];
        let b2 = vec![BondMutation::Insert(op2, fixture_bond_record(op2)), BondMutation::Slash(op1, 9_000)];

        let mut view = ActiveBondView::new();
        view.apply(&b1);
        view.apply(&b2);
        assert_eq!(view.len(), 2);
        assert!(view.active_bond_at(&op1, 10_000).is_none()); // slashed in b2
        assert!(view.active_bond_at(&op2, 10_000).is_some());

        // Revert b2 (most-recent first) → post-b1 consensus state.
        view.revert(&b2);
        assert_eq!(view.len(), 1);
        assert!(view.get(&op2).is_none()); // op2's Insert reverted
        assert!(view.active_bond_at(&op1, 10_000).is_some()); // slash cleared
        assert_eq!(view.get(&op1).unwrap().owner_reward_spk_payload, fixture_bond_record(op1).owner_reward_spk_payload);
    }

    #[test]
    fn rewarded_epoch_set_insert_contains_remove() {
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let mut set = RewardedEpochSet::new();
        assert!(!set.contains(&op, 7));
        assert!(set.insert(op, 7)); // newly inserted
        assert!(set.contains(&op, 7));
        assert!(!set.insert(op, 7)); // duplicate → false, not rewarded again
        assert_eq!(set.len(), 1);
        assert!(set.remove(&op, 7)); // reorg reverse
        assert!(!set.contains(&op, 7));
        assert!(set.is_empty());
    }

    #[test]
    fn rewarded_epoch_set_keys_on_both_outpoint_and_epoch() {
        let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
        let mut set = RewardedEpochSet::new();
        set.insert(op1, 1);
        // Same outpoint, different epoch → distinct (a later epoch is payable).
        assert!(!set.contains(&op1, 2));
        // Different outpoint, same epoch → distinct.
        assert!(!set.contains(&op2, 1));
        set.insert(op1, 2);
        set.insert(op2, 1);
        assert_eq!(set.len(), 3);
    }

    // ---- A.5: StakeScore aggregation -> DnsState ----

    #[test]
    fn aggregate_epoch_tallies_dedups_triple_and_normalises() {
        let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x01; 64]), 0);
        let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x02; 64]), 0);
        let v1 = Hash64::from_bytes([0xa1; 64]);
        let v2 = Hash64::from_bytes([0xa2; 64]);
        let contribs = vec![
            AttestationContribution { epoch: 1, validator_id: v1, bond_outpoint: op1, signed_stake_sompi: 30 },
            // Duplicate (op1, v1, epoch 1) — must NOT be double-counted.
            AttestationContribution { epoch: 1, validator_id: v1, bond_outpoint: op1, signed_stake_sompi: 30 },
            AttestationContribution { epoch: 1, validator_id: v2, bond_outpoint: op2, signed_stake_sompi: 20 },
            AttestationContribution { epoch: 2, validator_id: v1, bond_outpoint: op1, signed_stake_sompi: 30 },
        ];
        let totals = BTreeMap::from([(1u64, 100u64), (2u64, 100u64), (3u64, 100u64)]);
        let tallies = aggregate_epoch_tallies(&contribs, &totals);
        assert_eq!(tallies.len(), 3); // ascending by epoch
        assert_eq!(tallies[0], EpochStakeTally { epoch: 1, signed_stake_sompi: 50, total_active_stake_sompi: 100 });
        assert_eq!(tallies[1], EpochStakeTally { epoch: 2, signed_stake_sompi: 30, total_active_stake_sompi: 100 });
        assert_eq!(tallies[2], EpochStakeTally { epoch: 3, signed_stake_sompi: 0, total_active_stake_sompi: 100 });
        // End-to-end (φS = 0, linear): 0.5 + 0.3 + 0.0 = 0.8.
        assert_eq!(compute_stake_score(&tallies, 0), StakeScore(STAKE_SCORE_SCALE / 2 + STAKE_SCORE_SCALE * 3 / 10));
    }

    #[test]
    fn aggregate_epoch_tallies_ignores_epoch_without_denominator() {
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x01; 64]), 0);
        let contribs = vec![AttestationContribution {
            epoch: 9,
            validator_id: Hash64::from_bytes([0xa1; 64]),
            bond_outpoint: op,
            signed_stake_sompi: 50,
        }];
        assert!(aggregate_epoch_tallies(&contribs, &BTreeMap::new()).is_empty());
    }

    #[test]
    fn advance_dns_confirmation_confirms_canonical_anchor_not_sink() {
        let vsc = Hash64::from_bytes([0x22; 64]);
        let (cw, cs) = (BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE)); // work>=1000, stake>=1.0
        let stage = DnsRolloutStage::Active;

        // Stake below cS, no prev -> nothing confirmed (zero), even with a candidate anchor.
        let sink1 = Hash64::from_bytes([0x11; 64]);
        let canon1 = Hash64::from_bytes([0x91; 64]);
        let s1 = advance_dns_confirmation(
            None,
            sink1,
            500,
            Some((canon1, 480)),
            BlueWorkType::from_u64(2000),
            StakeScore(STAKE_SCORE_SCALE / 2),
            stage,
            vsc,
            DnsHealth::Active,
            cw,
            cs,
        );
        assert_eq!(s1.selected_chain_anchor, sink1);
        assert_eq!(s1.last_dns_confirmed_anchor, Hash64::default());

        // audit #3: thresholds met -> confirm the CANONICAL anchor, NOT the POV-dependent sink.
        let sink2 = Hash64::from_bytes([0x33; 64]);
        let canon2 = Hash64::from_bytes([0x99; 64]);
        let s2 = advance_dns_confirmation(
            Some(&s1),
            sink2,
            600,
            Some((canon2, 580)),
            BlueWorkType::from_u64(2000),
            StakeScore(STAKE_SCORE_SCALE),
            stage,
            vsc,
            DnsHealth::DegradedStakeQualityLow,
            cw,
            cs,
        );
        assert_eq!(s2.selected_chain_anchor, sink2, "selected_chain_anchor stays the sink (throttle only)");
        assert_eq!(s2.last_dns_confirmed_anchor, canon2, "confirmed anchor is the canonical anchor");
        assert_eq!(s2.last_dns_confirmed_anchor_daa_score, 580);
        assert_ne!(s2.last_dns_confirmed_anchor, sink2, "the sink is NOT what gets confirmed");
        assert_eq!(s2.health, DnsHealth::DegradedStakeQualityLow); // health never gates confirmation

        // No canonical anchor (None) -> cannot confirm even if depth passes; carry prev forward.
        let sink3 = Hash64::from_bytes([0x44; 64]);
        let s3 = advance_dns_confirmation(
            Some(&s2),
            sink3,
            700,
            None,
            BlueWorkType::from_u64(2000),
            StakeScore(STAKE_SCORE_SCALE),
            stage,
            vsc,
            DnsHealth::Active,
            cw,
            cs,
        );
        assert_eq!(s3.selected_chain_anchor, sink3);
        assert_eq!(s3.last_dns_confirmed_anchor, canon2, "no ready anchor -> keep prev confirmed");

        // Below-threshold stake (with a candidate anchor present) also carries prev forward.
        let s4 = advance_dns_confirmation(
            Some(&s2),
            sink3,
            700,
            Some((Hash64::from_bytes([0x77; 64]), 690)),
            BlueWorkType::from_u64(2000),
            StakeScore(0),
            stage,
            vsc,
            DnsHealth::Active,
            cw,
            cs,
        );
        assert_eq!(s4.last_dns_confirmed_anchor, canon2, "below-threshold -> keep prev confirmed");
        assert_eq!(s4.last_dns_confirmed_anchor_daa_score, 580);
    }

    /// audit H-02 (true WorkDepth, Option A): with `required_work_depth > 0`, confirmation is
    /// genuinely TWO-DIMENSIONAL — it requires BOTH `WorkDepth ≥ cW` AND `StakeDepth ≥ cS`. The
    /// decisive new property: a candidate with ENOUGH STAKE but SHALLOW work (`WorkDepth < cW`) is
    /// NOT confirmed (a stake-side adversary can no longer fast-finalize a low-PoW anchor). This is
    /// the behavior that `required_work_depth = 0` (devnet/simnet) collapses to "stake-only".
    #[test]
    fn true_workdepth_requires_both_work_and_stake() {
        let vsc = Hash64::from_bytes([0x22; 64]);
        let (cw, cs) = (BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE)); // cW=1000, cS=1.0
        let stage = DnsRolloutStage::Active;
        let canon = Hash64::from_bytes([0xC0; 64]);
        let confirm = |work: u64, stake: u128| {
            advance_dns_confirmation(
                None,
                Hash64::from_bytes([0x10; 64]),
                900,
                Some((canon, 880)),
                BlueWorkType::from_u64(work),
                StakeScore(stake),
                stage,
                vsc,
                DnsHealth::Active,
                cw,
                cs,
            )
            .last_dns_confirmed_anchor
        };
        // Both dimensions clear ⇒ confirmed.
        assert_eq!(confirm(2000, STAKE_SCORE_SCALE), canon, "work≥cW ∧ stake≥cS ⇒ confirmed");
        // Enough STAKE but SHALLOW work ⇒ NOT confirmed (the true-WorkDepth gate; stake-only would confirm).
        assert_eq!(confirm(999, STAKE_SCORE_SCALE), Hash64::default(), "stake≥cS but work<cW ⇒ NOT confirmed (true WorkDepth)");
        // Enough WORK but insufficient stake ⇒ NOT confirmed (the stake gate).
        assert_eq!(confirm(2000, STAKE_SCORE_SCALE / 2), Hash64::default(), "work≥cW but stake<cS ⇒ NOT confirmed");
        // Exactly at both thresholds ⇒ confirmed (inclusive ≥).
        assert_eq!(confirm(1000, STAKE_SCORE_SCALE), canon, "work==cW ∧ stake==cS ⇒ confirmed");
    }

    #[test]
    fn total_active_stake_by_epoch_sums_only_active_bonds() {
        // A: activation 100, stake 30. B: activation 500, stake 20.
        // C: activation 100, slashed at 300, stake 50.
        let mut a = stake_bond_record_from_payload(&fixture_bond(), fixture_outpoint());
        a.amount = 30;
        a.activation_daa_score = 100;
        let mut b = a.clone();
        b.amount = 20;
        b.activation_daa_score = 500;
        let mut c = a.clone();
        c.amount = 50;
        c.slashed_at_daa_score = Some(300);
        let bonds = vec![a, b, c];

        let epochs = BTreeMap::from([(1u64, 50u64), (2, 200), (3, 400), (4, 600)]);
        let totals = total_active_stake_by_epoch(&bonds, &epochs);
        assert_eq!(totals.get(&1), Some(&0)); // daa 50: all activate >= 100 -> Pending
        assert_eq!(totals.get(&2), Some(&80)); // daa 200: A(30) + C(50) active
        assert_eq!(totals.get(&3), Some(&30)); // daa 400: A(30); C slashed @300; B not yet
        assert_eq!(totals.get(&4), Some(&50)); // daa 600: A(30) + B(20); C slashed
    }

    #[test]
    fn attestations_from_accepted_txs_flattens_shards_only() {
        // Two shards (2 + 3 attestations) + a non-overlay tx -> 5 attestations.
        let shard_a = dns_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, borsh::to_vec(&fixture_shard(2)).unwrap());
        let shard_b = dns_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, borsh::to_vec(&fixture_shard(3)).unwrap());
        let native = dns_overlay_tx(SubnetworkId::from_byte(0), vec![1, 2, 3]);
        let bond = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, borsh::to_vec(&fixture_bond()).unwrap());
        let atts = attestations_from_accepted_txs(&[shard_a, native, shard_b, bond]);
        assert_eq!(atts.len(), 5);
        assert!(atts.iter().all(|a| a.signature.len() == STAKE_ATTESTATION_SIG_LEN));

        // Undecodable shard payload is skipped.
        let bad = dns_overlay_tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, vec![0xff]);
        assert!(attestations_from_accepted_txs(&[bad]).is_empty());
    }

    #[test]
    fn dns_confirmation_from_state_sets_flags() {
        let state = DnsState {
            selected_chain_anchor: Hash64::from_bytes([0x11; 64]),
            anchor_daa_score: 1000,
            work_depth: BlueWorkType::from_u64(5000),
            stake_depth: StakeScore(STAKE_SCORE_SCALE), // 1.0
            last_dns_confirmed_anchor: Hash64::default(),
            last_dns_confirmed_anchor_daa_score: 0,
            rollout_stage: DnsRolloutStage::Active,
            validator_set_commitment: Hash64::from_bytes([0x22; 64]),
            health: DnsHealth::DegradedCertificateCensored,
        };
        // Both thresholds met -> pow + dns confirmed.
        let c = dns_confirmation_from_state(&state, BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE / 2));
        assert_eq!(c.block_hash, state.selected_chain_anchor);
        assert!(c.pow_confirmed && c.dns_confirmed);
        assert_eq!(c.rollout_stage, DnsRolloutStage::Active);
        assert_eq!(c.health, DnsHealth::DegradedCertificateCensored); // surfaced verbatim from DnsState
        assert_eq!(c.required_work_depth, BlueWorkType::from_u64(1000));

        // Work met but stake below threshold -> pow only.
        let c = dns_confirmation_from_state(&state, BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE * 2));
        assert!(c.pow_confirmed && !c.dns_confirmed);

        // Work below threshold -> neither.
        let c = dns_confirmation_from_state(&state, BlueWorkType::from_u64(9999), StakeScore(0));
        assert!(!c.pow_confirmed && !c.dns_confirmed);
    }

    #[test]
    fn dns_params_borsh_roundtrip() {
        let params = DnsParams {
            dns_activation_daa_score: 1_000_000,
            min_active_stake_sompi: 10_000_000_000_000,
            min_active_validators: 32,
            min_bond_amount_sompi: 2_000_000_000_000,
            epoch_length_blocks: 600,
            required_work_depth: BlueWorkType::from_u64(1_000_000),
            required_stake_depth: StakeScore(10 * STAKE_SCORE_SCALE),
            emergency_work_margin: BlueWorkType::from_u64(10_000_000),
            emergency_stake_margin: StakeScore(100 * STAKE_SCORE_SCALE),
            max_reorg_horizon_blocks: 100_000,
            evidence_window_blocks: 200_000,
            unbonding_period_blocks: 350_000, // > R + E
            max_attestations_per_block: MAX_ATTESTATIONS_PER_SHARD as u16,
            max_attestation_shard_mass: 50_000,
            reward_uniqueness_window_blocks: 3_600, // ~6 epochs (epoch_length 600)
            stake_event_quality_floor_bps: 6000,    // ADR-0018 §B (φS = 0.60)
            degraded_stake_quality_epochs: 4,       // ADR-0018 §C
            stake_censorship_floor_bps: 1000,       // ADR-0018 §C (0.10)
            reward_params: RewardParams {
                per_attestation_reward_sompi: 100_000_000,
                slashing_reporter_reward_bps: 1000,
                max_validator_inflation_per_block_sompi: 100_000_000 * MAX_ATTESTATIONS_PER_SHARD as u64,
                validator_participation_bps: 10000,
                validator_quality_bonus_bps: 0,
                quality_gate_bonus_sompi: 50_000_000,
                worker_urgency_multiplier_scaled: STAKE_SCORE_SCALE as u64,
                fee_split: fixture_fee_split(),
                fee_split_bootstrap: fixture_fee_split_bootstrap(),
                security_reserve_bps: 2000,
                victim_epoch_pool_bps: 1000,
                reserve_drip_per_epoch_cap_sompi: 1_000_000,
            },
            reorg_mode: DnsReorgMode::TwoDimensionalDominance,
            full_reward_split_daa_score: 2_000_000,
            pos_v2_activation_daa_score: 3_000_000,
            attestation_epoch_length_blue_score: 4_000_000,
            attestation_lag_blue_score: 5_000_000,
            attestation_anchor_backoff_blue_score: 6_000_000,
            stake_score_window_blue_score: 7_000_000,
            finality_fee_activation_daa_score: 8_000_000,
            bond_spend_gate_mergeset_activation_daa_score: 9_000_000,
            mandatory_attestation_inclusion_daa_score: 10_000_000,
            bridge_finality_max_staleness_daa_score: 11_000_000,
        };
        let bytes = borsh::to_vec(&params).unwrap();
        let back: DnsParams = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, params);

        // ADR-0009 §"Long-range bound" requires U >= R + E.
        assert!(params.unbonding_period_blocks >= params.max_reorg_horizon_blocks + params.evidence_window_blocks);
        // ADR-0018 §E: the validator pool split is a partition (the two shares sum to 100%).
        assert_eq!(params.reward_params.validator_participation_bps + params.reward_params.validator_quality_bonus_bps, 10_000);
    }

    #[test]
    fn dns_confirmation_borsh_roundtrip() {
        let c = DnsConfirmation {
            block_hash: Hash64::from_bytes([0x99u8; 64]),
            work_depth: BlueWorkType::from_u64(42),
            required_work_depth: BlueWorkType::from_u64(10),
            stake_depth: StakeScore(500_000_000),
            required_stake_depth: StakeScore(STAKE_SCORE_SCALE),
            pow_confirmed: true,
            dns_confirmed: false,
            rollout_stage: DnsRolloutStage::Bootstrap,
            expected_dns_confirmation_seconds: 600,
            work_reorg_risk_upper_bound: "n/a".into(),
            stake_reorg_risk_upper_bound: "n/a".into(),
            dns_reorg_risk_conservative_bound: "n/a".into(),
            note: "Phase 10 stub".into(),
            health: DnsHealth::DegradedStakeQualityLow,
            last_dns_confirmed_anchor: Hash64::from_bytes([0x77u8; 64]),
            last_dns_confirmed_anchor_daa_score: 12345,
        };
        let bytes = borsh::to_vec(&c).unwrap();
        let back: DnsConfirmation = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn dns_rollout_stage_default_is_launch() {
        // The default rollout stage must be `Launch` so a node booting
        // before any DNS parameters are configured behaves as a pure
        // PoW kaspa-pq node. ADR-0009 §"Three-stage rollout".
        assert_eq!(DnsRolloutStage::default(), DnsRolloutStage::Launch);
    }

    #[test]
    fn bond_status_default_is_pending() {
        // A freshly-recorded bond is `Pending` until
        // `activation_daa_score` is crossed. Matches ADR-0010
        // §"Validator service runtime" predicate ordering.
        assert_eq!(BondStatus::default(), BondStatus::Pending);
    }

    #[test]
    fn stake_bond_record_borsh_roundtrip() {
        let rec = StakeBondRecord {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint: fixture_outpoint(),
            owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
            validator_pubkey_hash: Hash64::from_bytes([0xbbu8; 64]),
            validator_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
            amount: 100_000_000_000,
            activation_daa_score: 5_000,
            created_daa_score: 4_900,
            unbonding_period_blocks: 100_000,
            owner_reward_spk_payload: [0xddu8; 64],
            unbond_request_daa_score: Some(123_456),
            slashed_at_daa_score: None,
            status: BondStatus::Unbonding,
        };
        let bytes = borsh::to_vec(&rec).unwrap();
        let back: StakeBondRecord = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, rec);
    }

    #[test]
    fn validator_record_borsh_roundtrip() {
        let v = ValidatorRecord { validator_id: Hash64::from_bytes([0x42u8; 64]), stake_amount: 1_000_000, activation_daa_score: 99 };
        let bytes = borsh::to_vec(&v).unwrap();
        let back: ValidatorRecord = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn validator_set_snapshot_borsh_roundtrip() {
        let snap = ValidatorSetSnapshot { epoch: 7, validators: fixture_validators() };
        let bytes = borsh::to_vec(&snap).unwrap();
        let back: ValidatorSetSnapshot = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, snap);
    }

    #[test]
    fn dns_state_borsh_roundtrip() {
        let s = DnsState {
            selected_chain_anchor: Hash64::from_bytes([0x55u8; 64]),
            anchor_daa_score: 1_000,
            work_depth: BlueWorkType::from_u64(500),
            stake_depth: StakeScore(2 * STAKE_SCORE_SCALE),
            last_dns_confirmed_anchor: Hash64::from_bytes([0x66u8; 64]),
            last_dns_confirmed_anchor_daa_score: 900,
            rollout_stage: DnsRolloutStage::Active,
            validator_set_commitment: Hash64::from_bytes([0x77u8; 64]),
            health: DnsHealth::DegradedCertificateCensored,
        };
        let bytes = borsh::to_vec(&s).unwrap();
        let back: DnsState = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn block_template_policy_borsh_roundtrip() {
        let p = BlockTemplatePolicy {
            max_attestations_per_block: MAX_ATTESTATIONS_PER_SHARD as u16,
            max_attestation_shard_mass: 50_000,
            reserve_mass_for_normal_txs: 200_000,
        };
        let bytes = borsh::to_vec(&p).unwrap();
        let back: BlockTemplatePolicy = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }

    // ---- validator_set_commitment ---------------------------------

    #[test]
    fn validator_set_commitment_is_order_independent() {
        // Same set in three different input orders. ADR-0010
        // §"Validator-set commitment derivation" guarantees the
        // result is order-independent because the helper sorts a
        // clone before hashing.
        let a = fixture_validators();
        let mut b = a.clone();
        b.reverse();
        let mut c = a.clone();
        c.swap(0, 2);

        let ca = validator_set_commitment(11, &a);
        let cb = validator_set_commitment(11, &b);
        let cc = validator_set_commitment(11, &c);
        assert_eq!(ca, cb);
        assert_eq!(ca, cc);
    }

    #[test]
    fn validator_set_commitment_changes_with_epoch() {
        // Same validator set, different epochs => different
        // commitments. Guards against an attestation replay across
        // epoch boundaries.
        let v = fixture_validators();
        assert_ne!(validator_set_commitment(0, &v), validator_set_commitment(1, &v));
        assert_ne!(validator_set_commitment(11, &v), validator_set_commitment(12, &v));
    }

    #[test]
    fn validator_set_commitment_changes_with_membership() {
        // Removing a validator must change the commitment.
        let full = fixture_validators();
        let mut subset = full.clone();
        subset.pop();
        assert_ne!(validator_set_commitment(11, &full), validator_set_commitment(11, &subset));
    }

    #[test]
    fn validator_set_commitment_changes_with_stake_amount() {
        // Bumping any field a validator contributes (stake_amount,
        // activation_daa_score) must change the commitment.
        let baseline = fixture_validators();
        let mut bumped_stake = baseline.clone();
        bumped_stake[0].stake_amount += 1;
        assert_ne!(validator_set_commitment(11, &baseline), validator_set_commitment(11, &bumped_stake));

        let mut bumped_daa = baseline.clone();
        bumped_daa[0].activation_daa_score += 1;
        assert_ne!(validator_set_commitment(11, &baseline), validator_set_commitment(11, &bumped_daa));
    }

    #[test]
    fn validator_set_commitment_empty_is_well_defined() {
        // Empty validator set still produces a deterministic hash
        // (not all-zero): epoch || u32::LE(0).
        let c0 = validator_set_commitment(0, &[]);
        let c1 = validator_set_commitment(1, &[]);
        assert_ne!(c0, c1);
        // Must not collide with the all-zero `Hash64` sentinel.
        assert_ne!(c0, kaspa_hashes::ZERO_HASH64);
    }

    #[test]
    fn validator_set_commitment_matches_adr_byte_layout() {
        // Pin one full byte-layout to a known-good value. Any future
        // change to the ADR-0010 derivation (field order, length
        // prefix encoding, domain-separator key) trips this test
        // immediately — the value is consensus-stable and any drift
        // is a hard fork.
        let v = vec![
            ValidatorRecord { validator_id: Hash64::from_bytes([0x01u8; 64]), stake_amount: 1, activation_daa_score: 2 },
            ValidatorRecord { validator_id: Hash64::from_bytes([0x02u8; 64]), stake_amount: 3, activation_daa_score: 4 },
        ];
        // Re-derive the expected value with a hand-rolled hasher,
        // matching the ADR text byte-for-byte. Equality here is the
        // "two independent implementations agree" sanity check.
        let mut h = Blake2bParams::new().hash_length(64).key(VALIDATOR_SET_COMMITMENT_KEY).to_state();
        h.update(&5u64.to_le_bytes()); // epoch = 5
        h.update(&2u32.to_le_bytes()); // len   = 2
        // Sorted by validator_id ascending: [0x01..], [0x02..].
        h.update(&[0x01u8; 64]);
        h.update(&1u64.to_le_bytes());
        h.update(&2u64.to_le_bytes());
        h.update(&[0x02u8; 64]);
        h.update(&3u64.to_le_bytes());
        h.update(&4u64.to_le_bytes());
        let mut expected = [0u8; 64];
        expected.copy_from_slice(h.finalize().as_bytes());

        let actual = validator_set_commitment(5, &v);
        assert_eq!(actual.as_bytes(), expected);
    }

    #[test]
    fn validator_id_from_pubkey_is_unkeyed_blake2b_512() {
        // Canonical derivation = unkeyed BLAKE2b-512 of the public key
        // (ADR-0008/0012). Pinning it guards against accidental keying or a
        // switch to the 32-byte P2PKH address payload — either would be a hard fork.
        let pubkey = [0x42u8; 2592]; // MLDSA87_PK_LEN-sized sample
        let mut expected = [0u8; 64];
        expected.copy_from_slice(Blake2bParams::new().hash_length(64).to_state().update(&pubkey).finalize().as_bytes());
        let id = validator_id_from_pubkey(&pubkey);
        assert_eq!(id, Hash64::from_bytes(expected));
        // Deterministic and input-sensitive.
        assert_eq!(validator_id_from_pubkey(&pubkey), id);
        let mut other = pubkey;
        other[0] ^= 0x01;
        assert_ne!(validator_id_from_pubkey(&other), id);
    }

    #[test]
    fn signature_fingerprint_is_unkeyed_blake2b_512() {
        let sig = [0x7eu8; 4627]; // MLDSA87_SIG_LEN-sized sample
        let mut expected = [0u8; 64];
        expected.copy_from_slice(Blake2bParams::new().hash_length(64).to_state().update(&sig).finalize().as_bytes());
        assert_eq!(signature_fingerprint(&sig), Hash64::from_bytes(expected));
        // Input-sensitive (so a re-broadcast of a *different* signature is distinguishable).
        let mut other = sig;
        other[0] ^= 0x01;
        assert_ne!(signature_fingerprint(&other), signature_fingerprint(&sig));
    }

    // ---- stake_attestation_message --------------------------------

    #[test]
    fn stake_attestation_message_is_deterministic() {
        let target = Hash64::from_bytes([0x11u8; 64]);
        let vsc = Hash64::from_bytes([0x22u8; 64]);
        let op = fixture_outpoint();
        let a = stake_attestation_message(b"kaspa-pq-devnet", 7, target, 1_234_567, vsc, op);
        let b = stake_attestation_message(b"kaspa-pq-devnet", 7, target, 1_234_567, vsc, op);
        assert_eq!(a, b);
    }

    #[test]
    fn stake_attestation_message_changes_with_each_field() {
        // ADR-0009 Addendum A.3: every input — including network_id and
        // bond_outpoint — must perturb the digest.
        let net = b"kaspa-pq-devnet".as_slice();
        let th = Hash64::from_bytes([0x11u8; 64]);
        let vsc = Hash64::from_bytes([0x22u8; 64]);
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x77u8; 64]), 42);
        let base = stake_attestation_message(net, 7, th, 100, vsc, op);
        // network_id (A.3 — guards cross-network replay).
        assert_ne!(base, stake_attestation_message(b"kaspa-pq-testnet", 7, th, 100, vsc, op));
        // Epoch.
        assert_ne!(base, stake_attestation_message(net, 8, th, 100, vsc, op));
        // target_hash.
        assert_ne!(base, stake_attestation_message(net, 7, Hash64::from_bytes([0x12u8; 64]), 100, vsc, op));
        // target_daa_score.
        assert_ne!(base, stake_attestation_message(net, 7, th, 101, vsc, op));
        // validator_set_commitment.
        assert_ne!(base, stake_attestation_message(net, 7, th, 100, Hash64::from_bytes([0x23u8; 64]), op));
        // bond_outpoint transaction_id (A.3 — guards cross-bond replay).
        assert_ne!(
            base,
            stake_attestation_message(net, 7, th, 100, vsc, TransactionOutpoint::new(Hash64::from_bytes([0x78u8; 64]), 42))
        );
        // bond_outpoint index.
        assert_ne!(
            base,
            stake_attestation_message(net, 7, th, 100, vsc, TransactionOutpoint::new(Hash64::from_bytes([0x77u8; 64]), 43))
        );
    }

    #[test]
    fn stake_attestation_message_uses_attestation_domain_key_and_full_layout() {
        // Reconstruct the exact Addendum A.3 layout and verify (a) the
        // attestation domain key differs from the tx domain key, and (b)
        // `stake_attestation_message` matches the byte-for-byte layout.
        let net = b"kaspa-pq-devnet".as_slice();
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x77u8; 64]), 42);
        let inputs = |key: &[u8]| {
            let mut h = Blake2bParams::new().hash_length(32).key(key).to_state();
            h.update(net);
            h.update(&7u64.to_le_bytes());
            h.update(&[0x11u8; 64]);
            h.update(&100u64.to_le_bytes());
            h.update(&[0x22u8; 64]);
            h.update(&[0x77u8; 64]); // bond_outpoint.transaction_id
            h.update(&42u32.to_le_bytes()); // bond_outpoint.index
            h.finalize()
        };
        let with_att_key = inputs(ATTESTATION_MESSAGE_DOMAIN);
        let with_tx_key = inputs(b"kaspa-pq-v1/tx/mldsa87");
        assert_ne!(with_att_key.as_bytes(), with_tx_key.as_bytes());

        let actual = stake_attestation_message(net, 7, Hash64::from_bytes([0x11u8; 64]), 100, Hash64::from_bytes([0x22u8; 64]), op);
        assert_eq!(actual.as_bytes(), with_att_key.as_bytes());
    }

    // ---- Validator-local state (ADR-0011) -------------------------

    fn fixture_signed() -> SignedEpochRecord {
        SignedEpochRecord {
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            signature_fingerprint: Hash64::from_bytes([0xabu8; 64]),
        }
    }

    #[test]
    fn validator_status_default_is_node_not_synced() {
        // A freshly-started validator must default to
        // `NodeNotSynced` so it cannot take any sign-eligible
        // action before its runtime loop has confirmed the local
        // node is at tip. ADR-0011 §"Validator status enum".
        assert_eq!(ValidatorStatus::default(), ValidatorStatus::NodeNotSynced);
    }

    #[test]
    fn validator_status_discriminants_are_api_stable() {
        // The Borsh discriminant of each variant is API-stable —
        // RPC clients persist these to disk. Any reorder is a
        // wire-format break. Pin the integer values so the test
        // trips immediately on accidental drift. Variants 0..8
        // are pinned per ADR-0011; variant 9 is appended per
        // ADR-0014 §"`ValidatorStatus` extension".
        assert_eq!(ValidatorStatus::NodeNotSynced as u8, 0);
        assert_eq!(ValidatorStatus::BondNotFound as u8, 1);
        assert_eq!(ValidatorStatus::BondPending as u8, 2);
        assert_eq!(ValidatorStatus::ActiveIdle as u8, 3);
        assert_eq!(ValidatorStatus::ActiveEligible as u8, 4);
        assert_eq!(ValidatorStatus::SignedThisEpoch as u8, 5);
        assert_eq!(ValidatorStatus::Unbonding as u8, 6);
        assert_eq!(ValidatorStatus::Slashed as u8, 7);
        assert_eq!(ValidatorStatus::DryRun as u8, 8);
        assert_eq!(ValidatorStatus::AwaitingTakeoverToken as u8, 9);
    }

    #[test]
    fn validator_status_all_variants_borsh_roundtrip() {
        for v in [
            ValidatorStatus::NodeNotSynced,
            ValidatorStatus::BondNotFound,
            ValidatorStatus::BondPending,
            ValidatorStatus::ActiveIdle,
            ValidatorStatus::ActiveEligible,
            ValidatorStatus::SignedThisEpoch,
            ValidatorStatus::Unbonding,
            ValidatorStatus::Slashed,
            ValidatorStatus::DryRun,
            ValidatorStatus::AwaitingTakeoverToken,
        ] {
            let bytes = borsh::to_vec(&v).unwrap();
            let back: ValidatorStatus = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, v, "variant {v:?} did not round-trip through Borsh");
            assert_eq!(bytes.len(), 1, "ValidatorStatus must encode as a single byte");
        }
    }

    #[test]
    fn signed_epoch_record_borsh_roundtrip() {
        let r = fixture_signed();
        let bytes = borsh::to_vec(&r).unwrap();
        let back: SignedEpochRecord = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn signed_epoch_check_outcome_borsh_roundtrip() {
        for o in [SignedEpochCheckOutcome::Allow, SignedEpochCheckOutcome::AllowRebroadcast, SignedEpochCheckOutcome::Block] {
            let bytes = borsh::to_vec(&o).unwrap();
            let back: SignedEpochCheckOutcome = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, o);
        }
        assert_eq!(SignedEpochCheckOutcome::default(), SignedEpochCheckOutcome::Allow);
    }

    // ---- check_signed_epoch_record — full decision matrix -----------

    #[test]
    fn equivocation_check_allow_when_no_prior() {
        let candidate = fixture_signed();
        assert_eq!(check_signed_epoch_record(None, &candidate), SignedEpochCheckOutcome::Allow);
    }

    #[test]
    fn equivocation_check_allow_rebroadcast_when_exact_match() {
        let prev = fixture_signed();
        let candidate = prev.clone();
        assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::AllowRebroadcast);
    }

    #[test]
    fn equivocation_check_allow_rebroadcast_when_only_signature_fingerprint_differs() {
        // ML-DSA-87 is hedged by default (FIPS 204 §3.4); two
        // valid signatures over the same message differ on the
        // `rnd` parameter. Bit-equality on the fingerprint would
        // therefore be too strict, and would falsely block honest
        // re-signs after a restart. The predicate that matters is
        // (target_hash, target_daa_score) equality — this test
        // pins that.
        let prev = fixture_signed();
        let mut candidate = prev.clone();
        candidate.signature_fingerprint = Hash64::from_bytes([0xcdu8; 64]); // different fingerprint
        assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::AllowRebroadcast);
    }

    #[test]
    fn equivocation_check_block_when_target_hash_differs() {
        let prev = fixture_signed();
        let mut candidate = prev.clone();
        candidate.target_hash = Hash64::from_bytes([0x99u8; 64]); // different anchor — would be equivocation
        assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::Block);
    }

    #[test]
    fn equivocation_check_block_when_target_daa_score_differs() {
        let prev = fixture_signed();
        let mut candidate = prev.clone();
        // Same target_hash but different DAA score still counts —
        // ADR-0009 §"`SlashingEvidencePayload`" lists this as
        // evidence; the rare case of two attestations sharing
        // target_hash at different DAA scores is a node bug, and
        // signing both would still be slashable.
        candidate.target_daa_score = prev.target_daa_score + 1;
        assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::Block);
    }

    #[test]
    fn equivocation_check_block_when_both_target_fields_differ() {
        let prev = fixture_signed();
        let candidate = SignedEpochRecord {
            epoch: prev.epoch,
            target_hash: Hash64::from_bytes([0x99u8; 64]),
            target_daa_score: prev.target_daa_score + 1000,
            signature_fingerprint: Hash64::from_bytes([0xeeu8; 64]),
        };
        assert_eq!(check_signed_epoch_record(Some(&prev), &candidate), SignedEpochCheckOutcome::Block);
    }

    // ---- Reward / slashing distribution (ADR-0013) ----------------

    #[test]
    fn reward_params_borsh_roundtrip() {
        let p = RewardParams {
            per_attestation_reward_sompi: 100_000,
            slashing_reporter_reward_bps: 1000, // 10%
            max_validator_inflation_per_block_sompi: 5_000_000_000,
            validator_participation_bps: 10000,
            validator_quality_bonus_bps: 0,
            quality_gate_bonus_sompi: 25_000_000,
            worker_urgency_multiplier_scaled: STAKE_SCORE_SCALE as u64,
            fee_split: fixture_fee_split(),
            fee_split_bootstrap: fixture_fee_split_bootstrap(),
            security_reserve_bps: 2000,
            victim_epoch_pool_bps: 1000,
            reserve_drip_per_epoch_cap_sompi: 1_000_000,
        };
        let bytes = borsh::to_vec(&p).unwrap();
        let back: RewardParams = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn attestation_reward_payout_borsh_roundtrip() {
        let p = AttestationRewardPayout { total_payout_sompi: 1_600_000, refunded_sompi: 0 };
        let bytes = borsh::to_vec(&p).unwrap();
        let back: AttestationRewardPayout = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }

    // ---- ADR-0018 §D/§E inclusion economics --------------------------

    #[test]
    fn worker_inclusion_bounty_is_proportional_and_anti_capture() {
        let scale = STAKE_SCORE_SCALE; // 1.0× urgency
        // Degenerate epoch (no expected stake) → 0.
        assert_eq!(worker_inclusion_bounty(1000, 5, 0, scale, false, 0), 0);
        // Proportional: 5/10 of a 1000 pool at 1.0× urgency, no cross → 500.
        assert_eq!(worker_inclusion_bounty(1000, 5, 10, scale, false, 0), 500);
        // Urgency 2.0× doubles the base.
        assert_eq!(worker_inclusion_bounty(1000, 5, 10, 2 * scale, false, 0), 1000);
        // Crossing φS adds the fixed quality-gate bonus on top of the urgent base...
        assert_eq!(worker_inclusion_bounty(1000, 5, 10, scale, true, 123), 500 + 123);
        // ...but only when this block actually crosses the floor.
        assert_eq!(worker_inclusion_bounty(1000, 5, 10, scale, false, 123), 500);
        // Anti-capture: a few attestations earn only a tiny slice (1/1000 of the pool),
        // NOT the whole pool (the rejected `pool / included_count` design).
        assert_eq!(worker_inclusion_bounty(1000, 1, 1000, scale, false, 0), 1);
        // Anti-drain: over-counted stake (> expected) clamps to the whole pool, never more.
        assert_eq!(worker_inclusion_bounty(1000, 9999, 10, scale, false, 0), 1000);
    }

    #[test]
    fn split_validator_pool_is_dustfree() {
        // 70/30 split sums to the pool exactly.
        assert_eq!(split_validator_pool(1000, 7000), (700, 300));
        // Rounding dust lands in the bonus pool; the two still sum to the pool.
        let (p, q) = split_validator_pool(1001, 7000); // 1001 × 0.70 = 700.7 → 700
        assert_eq!((p, q), (700, 301));
        assert_eq!(p + q, 1001);
        // Edges: all-participation / all-bonus; bps clamps at 10_000.
        assert_eq!(split_validator_pool(1000, 10_000), (1000, 0));
        assert_eq!(split_validator_pool(1000, 0), (0, 1000));
        assert_eq!(split_validator_pool(1000, 20_000), (1000, 0)); // clamp
    }

    #[test]
    fn validator_two_pool_reward_is_proportional_and_gated() {
        // Participation: 10/100 of a 1000 pool → 100, paid regardless of the floor.
        assert_eq!(validator_participation_reward(1000, 10, 100), 100);
        assert_eq!(validator_participation_reward(1000, 10, 0), 0); // expected == 0
        // Anti-capture: a minority validator earns only its proportional slice, not the pool.
        assert_eq!(validator_participation_reward(1000, 1, 1000), 1);
        // Quality bonus: same proportional share, but ONLY when the epoch met φS.
        assert_eq!(validator_quality_bonus(300, 10, 100, true), 30);
        assert_eq!(validator_quality_bonus(300, 10, 100, false), 0); // below φS → 0, pool rolls over
        assert_eq!(validator_quality_bonus(300, 10, 0, true), 0); // expected == 0
        // Combined per-validator payout = participation + (bonus iff floor met). A 1000 pool
        // split 70/30, validator holds 10% of expected stake → 70 + 30 above φS, 70 below.
        let (part_pool, bonus_pool) = split_validator_pool(1000, 7000);
        let combined = |met| validator_participation_reward(part_pool, 10, 100) + validator_quality_bonus(bonus_pool, 10, 100, met);
        assert_eq!(combined(true), 70 + 30);
        assert_eq!(combined(false), 70);
    }

    // ---- ADR-0018 §F fee / subsidy split -----------------------------

    #[test]
    fn fee_splits_are_dust_free_and_match_adr_ratios() {
        let p = fixture_fee_split();
        // Subsidy 67/8/25/0 on a round 1_000_000; Worker total = base + inclusion = 75%.
        let s = split_block_subsidy(1_000_000, &p);
        assert_eq!((s.worker_base_sompi, s.worker_inclusion_sompi, s.validator_sompi, s.service_sompi), (670_000, 80_000, 250_000, 0));
        assert_eq!(s.worker_base_sompi + s.worker_inclusion_sompi, 750_000); // Worker 75%
        // Dust-free: the four parts always sum to the input, even on a non-round value;
        // the primary (worker_base) absorbs the rounding remainder, so it is ≥ its nominal.
        let s = split_block_subsidy(1_000_003, &p);
        assert_eq!(s.worker_base_sompi + s.worker_inclusion_sompi + s.validator_sompi + s.service_sompi, 1_000_003);
        assert!(s.worker_base_sompi >= 670_001);

        // Normal-tx 90/10/0 (Worker primary).
        let n = split_normal_tx_fees(1_000_000, &p);
        assert_eq!((n.worker_sompi, n.validator_sompi, n.service_sompi), (900_000, 100_000, 0));
        let n = split_normal_tx_fees(777, &p);
        assert_eq!(n.worker_sompi + n.validator_sompi + n.service_sompi, 777); // dust-free

        // DNS-finality 75/25/0 (Validator primary; unwired).
        let f = split_finality_fees(1_000_000, &p);
        assert_eq!((f.validator_sompi, f.worker_sompi, f.service_sompi), (750_000, 250_000, 0));
        let f = split_finality_fees(333, &p);
        assert_eq!(f.worker_sompi + f.validator_sompi + f.service_sompi, 333); // dust-free

        // Zero input → all-zero (inert: no subsidy/fees → no split outputs).
        let z = split_block_subsidy(0, &p);
        assert_eq!((z.worker_base_sompi, z.worker_inclusion_sompi, z.validator_sompi, z.service_sompi), (0, 0, 0, 0));
    }

    #[test]
    fn fee_split_params_partition_to_100_percent() {
        let p = fixture_fee_split();
        assert_eq!(
            p.subsidy_worker_base_bps + p.subsidy_worker_inclusion_bps + p.subsidy_validator_bps + p.subsidy_service_bps,
            10_000
        );
        assert_eq!(p.normal_fee_worker_bps + p.normal_fee_validator_bps + p.normal_fee_service_bps, 10_000);
        assert_eq!(p.finality_fee_validator_bps + p.finality_fee_worker_bps + p.finality_fee_service_bps, 10_000);
    }

    #[test]
    fn fee_split_types_borsh_roundtrip() {
        let fsp = fixture_fee_split();
        assert_eq!(borsh::from_slice::<FeeSplitParams>(&borsh::to_vec(&fsp).unwrap()).unwrap(), fsp);
        let s = SubsidySplit {
            worker_base_sompi: 620_000,
            worker_inclusion_sompi: 80_000,
            validator_sompi: 250_000,
            service_sompi: 50_000,
        };
        assert_eq!(borsh::from_slice::<SubsidySplit>(&borsh::to_vec(&s).unwrap()).unwrap(), s);
        let f = FeeSplit { worker_sompi: 850_000, validator_sompi: 100_000, service_sompi: 50_000 };
        assert_eq!(borsh::from_slice::<FeeSplit>(&borsh::to_vec(&f).unwrap()).unwrap(), f);
    }

    #[test]
    fn split_block_reward_combines_subsidy_and_fee_splits() {
        let p = fixture_fee_split();
        // 1_000_000 subsidy (67+8/25/0) + 1_000_000 all-normal fees (90/10/0):
        // worker 670k+80k+900k = 1_650_000; validator 250k+100k = 350_000; service 0.
        let r = split_block_reward(1_000_000, 1_000_000, 0, &p);
        assert_eq!((r.worker_sompi, r.validator_sompi, r.service_sompi), (1_650_000, 350_000, 0));
        // Value-conserving: the three sum to subsidy + fees exactly.
        assert_eq!(r.worker_sompi + r.validator_sompi + r.service_sompi, 2_000_000);
        // No fees → just the subsidy split (75/25/0 of 1_000_000).
        let r = split_block_reward(1_000_000, 0, 0, &p);
        assert_eq!((r.worker_sompi, r.validator_sompi, r.service_sompi), (750_000, 250_000, 0));
    }

    /// ADR-0018 §F bridge wiring: the finality-class subset of a block's fees is split at the
    /// validator-primary finality ratios (25/75/0) while the normal-class remainder keeps the
    /// 90/10/0 normal ratios; `finality = 0` is byte-identical to the pre-wiring math; the
    /// split conserves value and clamps a (construction-impossible) finality > total.
    #[test]
    fn split_block_reward_finality_class_pays_validator_primary() {
        let p = fixture_fee_split();
        // 1_000_000 subsidy + 1_000_000 total fees of which 400_000 finality-class:
        //   subsidy:   worker 670k+80k / validator 250k
        //   normal 600k (90/10): worker 540k / validator 60k
        //   finality 400k (25/75): worker 100k / validator 300k
        // ⇒ worker 1_390_000; validator 610_000; service 0.
        let r = split_block_reward(1_000_000, 1_000_000, 400_000, &p);
        assert_eq!((r.worker_sompi, r.validator_sompi, r.service_sompi), (1_390_000, 610_000, 0));
        assert_eq!(r.worker_sompi + r.validator_sompi + r.service_sompi, 2_000_000, "value-conserving");

        // ALL fees finality-class: worker 670k+80k+250k = 1_000_000; validator 250k+750k = 1_000_000.
        let r = split_block_reward(1_000_000, 1_000_000, 1_000_000, &p);
        assert_eq!((r.worker_sompi, r.validator_sompi, r.service_sompi), (1_000_000, 1_000_000, 0));

        // Fence-off equivalence: finality 0 == the historical two-class math (the pre-wiring shape).
        assert_eq!(split_block_reward(123_456, 789_012, 0, &p), {
            let s = split_block_subsidy(123_456, &p);
            let f = split_normal_tx_fees(789_012, &p);
            FeeSplit {
                worker_sompi: s.worker_base_sompi + s.worker_inclusion_sompi + f.worker_sompi,
                validator_sompi: s.validator_sompi + f.validator_sompi,
                service_sompi: s.service_sompi + f.service_sompi,
            }
        });

        // Defensive clamp: finality > total behaves as finality == total (cannot over-split).
        assert_eq!(split_block_reward(0, 1_000, 5_000, &p), split_block_reward(0, 1_000, 1_000, &p));
    }

    /// Defensive conservation: a (mis)configured bps group summing past 10_000 cannot
    /// over-mint — every split still sums EXACTLY to its input (the sequential caps floor
    /// the over-shares; the primary floors at 0). All current presets sum to exactly
    /// 10_000, where the caps are no-ops (the exact-ratio tests above cover that).
    #[test]
    fn splits_conserve_value_under_malformed_bps() {
        let mut p = fixture_fee_split();
        p.finality_fee_worker_bps = 9_000;
        p.finality_fee_service_bps = 9_000; // worker+service "180%" — must cap, never over-mint
        let f = split_finality_fees(1_000_000, &p);
        assert_eq!(f.worker_sompi + f.validator_sompi + f.service_sompi, 1_000_000, "finality split conserves");
        assert_eq!((f.worker_sompi, f.service_sompi, f.validator_sompi), (900_000, 100_000, 0));

        p.subsidy_worker_inclusion_bps = 8_000;
        p.subsidy_validator_bps = 8_000;
        p.subsidy_service_bps = 8_000; // "240%"
        let s = split_block_subsidy(1_000_000, &p);
        assert_eq!(
            s.worker_base_sompi + s.worker_inclusion_sompi + s.validator_sompi + s.service_sompi,
            1_000_000,
            "subsidy split conserves"
        );

        p.normal_fee_validator_bps = 7_000;
        p.normal_fee_service_bps = 7_000; // "140%"
        let n = split_normal_tx_fees(1_000_000, &p);
        assert_eq!(n.worker_sompi + n.validator_sompi + n.service_sompi, 1_000_000, "normal split conserves");

        // And the combined per-block split conserves under the same malformed config.
        let r = split_block_reward(1_000_000, 1_000_000, 400_000, &p);
        assert_eq!(r.worker_sompi + r.validator_sompi + r.service_sompi, 2_000_000, "combined split conserves");
    }

    #[test]
    fn validator_participation_outputs_distribute_proportionally() {
        let a = [0xa1u8; 64];
        let b = [0xb2u8; 64];
        // Pool 1000, expected 100. A holds 60 → 600, B holds 30 → 300; the 10 uncovered stake's
        // 100 is unspent → not minted (don't-mint rollover).
        let outs = validator_participation_outputs(1000, &[(a, 60), (b, 30)], 100);
        assert_eq!(outs.len(), 2);
        assert_eq!((outs[0].value, outs[1].value), (600, 300));
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&a));
        assert!(outs.iter().map(|o| o.value).sum::<u64>() <= 1000); // never over-mints the pool
        // Zero-value shares (zero stake / empty set) and a degenerate expected_stake emit nothing.
        assert!(validator_participation_outputs(1000, &[([0xcc; 64], 0)], 100).is_empty());
        assert!(validator_participation_outputs(1000, &[], 100).is_empty());
        assert!(validator_participation_outputs(1000, &[(a, 60)], 0).is_empty());
    }

    #[test]
    fn validator_participation_reward_outputs_dedup_and_cap() {
        let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
        let (a, b) = ([0xa1u8; 64], [0xb2u8; 64]);
        let empty = RewardedEpochSet::new();

        // Pool 1000, expected 100: op1/epoch5 stake 60 → 600; op2/epoch5 stake 30 → 300 (10 uncovered).
        let atts = vec![(op1, 5u64, a, 60u64), (op2, 5, b, 30)];
        let (outs, keys) = validator_participation_reward_outputs(1000, 100, &atts, &empty);
        assert_eq!(outs.iter().map(|o| o.value).collect::<Vec<_>>(), vec![600, 300]);
        assert_eq!(keys, vec![(op1, 5), (op2, 5)]);

        // Within-block dedup: a repeated (op1, epoch5) earns nothing the second time.
        let (outs, keys) = validator_participation_reward_outputs(1000, 100, &[(op1, 5, a, 60), (op1, 5, a, 60)], &empty);
        assert_eq!((outs.len(), keys), (1, vec![(op1, 5)]));

        // Cross-block dedup: a (bond, epoch) already rewarded on the prefix is skipped.
        let mut seen = RewardedEpochSet::new();
        seen.insert(op1, 5);
        let (outs, keys) = validator_participation_reward_outputs(1000, 100, &atts, &seen);
        assert_eq!((outs.iter().map(|o| o.value).collect::<Vec<_>>(), keys), (vec![300], vec![(op2, 5)]));

        // Whole-output pool cap: op1 in epochs 5 AND 6 (distinct keys), each 600 → 1200 > pool 1000.
        // The first fits; the second is dropped (break) and left unrewarded → Σ ≤ pool.
        let (outs, keys) = validator_participation_reward_outputs(1000, 100, &[(op1, 5, a, 60), (op1, 6, a, 60)], &empty);
        assert_eq!(outs.iter().map(|o| o.value).sum::<u64>(), 600);
        assert_eq!(keys, vec![(op1, 5)]); // (op1, 6) NOT marked rewarded — a later block may pay it

        // Degenerate denominator → no outputs.
        assert!(validator_participation_reward_outputs(1000, 0, &[(op1, 5, a, 60)], &empty).0.is_empty());
    }

    #[test]
    fn total_active_stake_at_sums_only_active() {
        let op1 = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let op2 = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
        let mut active = fixture_bond_record(op1);
        active.amount = 100;
        active.activation_daa_score = 50;
        let mut pending = fixture_bond_record(op2);
        pending.amount = 200;
        pending.activation_daa_score = 1000; // not active until daa 1000
        let view = ActiveBondView::from_records([(op1, active), (op2, pending)]);
        assert_eq!(view.total_active_stake_at(500), 100); // only op1 active; op2 still pending
        assert_eq!(view.total_active_stake_at(2000), 300); // both active
    }

    #[test]
    fn slashing_distribution_borsh_roundtrip() {
        let s = SlashingDistribution {
            reporter_reward_sompi: 100_000,
            security_reserve_sompi: 0,
            victim_epoch_pool_sompi: 0,
            burned_sompi: 900_000,
        };
        let bytes = borsh::to_vec(&s).unwrap();
        let back: SlashingDistribution = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, s);
    }

    // ---- compute_attestation_reward_payouts -----------------------

    #[test]
    fn attestation_payout_under_cap_pays_full_amount() {
        // 8 attestations × 100_000 sompi = 800_000 sompi, well
        // under the 5_000_000_000 cap.
        let r = compute_attestation_reward_payouts(100_000, 8, 5_000_000_000);
        assert_eq!(r.total_payout_sompi, 800_000);
        assert_eq!(r.refunded_sompi, 0);
    }

    #[test]
    fn attestation_payout_at_exact_cap_pays_cap_no_refund() {
        // Boundary: per × count == cap.
        let r = compute_attestation_reward_payouts(1_000_000, 5, 5_000_000);
        assert_eq!(r.total_payout_sompi, 5_000_000);
        assert_eq!(r.refunded_sompi, 0);
    }

    #[test]
    fn attestation_payout_over_cap_emits_refund() {
        // 10 × 1_000_000 = 10_000_000, but cap is 5_000_000.
        // Refund = 10_000_000 - 5_000_000 = 5_000_000.
        let r = compute_attestation_reward_payouts(1_000_000, 10, 5_000_000);
        assert_eq!(r.total_payout_sompi, 5_000_000);
        assert_eq!(r.refunded_sompi, 5_000_000);
    }

    #[test]
    fn attestation_payout_zero_count_is_zero() {
        let r = compute_attestation_reward_payouts(100_000, 0, 5_000_000_000);
        assert_eq!(r.total_payout_sompi, 0);
        assert_eq!(r.refunded_sompi, 0);
    }

    #[test]
    fn attestation_payout_zero_reward_is_zero() {
        // Defensive — a network with per_attestation_reward = 0
        // pays nothing regardless of count.
        let r = compute_attestation_reward_payouts(0, 100, 5_000_000_000);
        assert_eq!(r.total_payout_sompi, 0);
        assert_eq!(r.refunded_sompi, 0);
    }

    #[test]
    fn attestation_payout_saturates_on_huge_inputs() {
        // Defensive saturation — a bogus `(u64::MAX, u64::MAX as usize)`
        // input must produce a defined value rather than panic.
        let r = compute_attestation_reward_payouts(u64::MAX, usize::MAX, 1_000_000);
        // Saturated multiplication then capped at 1_000_000.
        assert_eq!(r.total_payout_sompi, 1_000_000);
        // Refund overflows u64 in absolute terms; we documented
        // refund as u64 so it saturates — what matters is the
        // helper does not panic. The refund is the uncapped product
        // (`u64::MAX * usize::MAX`, saturating) minus the 1_000_000 cap,
        // truncated to u64 — assert it matches that exact computation.
        let expected_refund = (u64::MAX as u128).saturating_mul(usize::MAX as u128).saturating_sub(1_000_000) as u64;
        assert_eq!(r.refunded_sompi, expected_refund);
    }

    // ---- p2pkh_mldsa87_spk + validator_reward_outputs -------------

    #[test]
    fn p2pkh_mldsa87_spk_byte_layout() {
        // ADR-0019 §8: widened to OP_BLAKE2B_512 (0xc4) + OP_DATA64 (0x40)
        // over a 64-byte BLAKE2b-512 payload — total 69 bytes + spk version 0.
        let payload = [0x11u8; 64];
        let spk = p2pkh_mldsa87_spk(&payload);
        assert_eq!(spk.version(), 0);
        let script = spk.script();
        assert_eq!(script.len(), 69);
        assert_eq!(script[0], 0x76, "OpDup");
        assert_eq!(script[1], 0xc4, "OpBlake2b512");
        assert_eq!(script[2], 0x40, "OpData64");
        assert_eq!(&script[3..67], &payload, "64-byte payload");
        assert_eq!(script[67], 0x88, "OpEqualVerify");
        assert_eq!(script[68], 0xa6, "OpCheckSigMlDsa87");
    }

    #[test]
    fn p2pkh_mldsa87_spk_distinct_payloads_distinct_scripts() {
        // The only varying region is script[3..67]; distinct payloads
        // must yield distinct scripts (no accidental collision).
        let a = p2pkh_mldsa87_spk(&[0x01u8; 64]);
        let b = p2pkh_mldsa87_spk(&[0x02u8; 64]);
        assert_ne!(a, b);
        // Same payload → identical script (deterministic).
        assert_eq!(p2pkh_mldsa87_spk(&[0x01u8; 64]), a);
    }

    #[test]
    fn validator_reward_outputs_one_per_attestation() {
        let reward = 1_000_000u64;
        let cap = 100_000_000u64; // far above reward × count
        let payloads = [[0x01u8; 64], [0x02u8; 64], [0x03u8; 64]];
        let outs = validator_reward_outputs(reward, cap, &payloads);
        assert_eq!(outs.len(), payloads.len());
        for (out, p) in outs.iter().zip(payloads.iter()) {
            assert_eq!(out.value, reward);
            assert_eq!(out.script_public_key, p2pkh_mldsa87_spk(p));
        }
    }

    #[test]
    fn validator_reward_outputs_preserves_canonical_order() {
        // Outputs must follow the caller's supplied order verbatim.
        let payloads = [[0x0au8; 64], [0x0bu8; 64], [0x0cu8; 64]];
        let outs = validator_reward_outputs(10, 1_000, &payloads);
        let got: Vec<_> = outs.iter().map(|o| o.script_public_key.clone()).collect();
        let want: Vec<_> = payloads.iter().map(p2pkh_mldsa87_spk).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn validator_reward_outputs_whole_output_cap_truncates_tail() {
        // cap = 25, reward = 10 → only 2 whole outputs (20), never a
        // partial third. Tail (3rd payload) is dropped.
        let reward = 10u64;
        let cap = 25u64;
        let payloads = [[0x01u8; 64], [0x02u8; 64], [0x03u8; 64]];
        let outs = validator_reward_outputs(reward, cap, &payloads);
        assert_eq!(outs.len(), 2);
        assert_eq!(outs.iter().map(|o| o.value).sum::<u64>(), 20);
        // The two emitted outputs are the canonical-order head.
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&payloads[0]));
        assert_eq!(outs[1].script_public_key, p2pkh_mldsa87_spk(&payloads[1]));
    }

    #[test]
    fn validator_reward_outputs_empty_when_reward_zero() {
        // reward = 0 → no validator-side outflow regardless of count.
        let payloads = [[0x01u8; 64], [0x02u8; 64]];
        assert!(validator_reward_outputs(0, 1_000_000, &payloads).is_empty());
    }

    #[test]
    fn validator_reward_outputs_empty_when_no_payloads() {
        // No included attestations → empty validator side → coinbase
        // unchanged. This is the every-current-network case.
        assert!(validator_reward_outputs(1_000_000, 100_000_000, &[]).is_empty());
    }

    #[test]
    fn validator_reward_outputs_duplicate_payloads_not_combined() {
        // ADR-0013 §"Coinbase fan-out": two attestations sharing an
        // owner payload emit two outputs, never one combined output.
        let reward = 5u64;
        let payloads = [[0x07u8; 64], [0x07u8; 64]];
        let outs = validator_reward_outputs(reward, 1_000, &payloads);
        assert_eq!(outs.len(), 2);
        assert_eq!(outs[0], outs[1]);
        assert_eq!(outs[0].value, reward);
    }

    // ---- validator_reward_outputs_from_attestations (within-block dedup) ----

    fn op_n(b: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
    }

    #[test]
    fn reward_from_attestations_one_per_distinct_bond_epoch() {
        let reward = 100u64;
        let atts = [(op_n(1), 5u64, [0x01u8; 64]), (op_n(2), 5u64, [0x02u8; 64]), (op_n(3), 7u64, [0x03u8; 64])];
        let (outs, keys) = validator_reward_outputs_from_attestations(reward, 10_000, &atts, &RewardedEpochSet::new());
        assert_eq!(outs.len(), 3);
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&[0x01u8; 64]));
        assert_eq!(outs[2].script_public_key, p2pkh_mldsa87_spk(&[0x03u8; 64]));
        assert_eq!(keys, vec![(op_n(1), 5), (op_n(2), 5), (op_n(3), 7)]);
    }

    #[test]
    fn reward_from_attestations_dedups_same_bond_epoch_first_wins() {
        // Same (bond, epoch) twice → one reward; the FIRST occurrence's
        // payload is used (canonical order preserved).
        let atts = [(op_n(1), 5u64, [0xaau8; 64]), (op_n(1), 5u64, [0xbbu8; 64])];
        let (outs, keys) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &RewardedEpochSet::new());
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&[0xaau8; 64]));
        assert_eq!(keys, vec![(op_n(1), 5)]);
    }

    #[test]
    fn reward_from_attestations_same_bond_distinct_epochs_both_paid() {
        // ADR-0013: a validator with attestations across two epochs in one
        // block earns for each — dedup is per (bond, epoch), not per bond.
        let atts = [(op_n(1), 5u64, [0xaau8; 64]), (op_n(1), 6u64, [0xaau8; 64])];
        let (outs, _) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &RewardedEpochSet::new());
        assert_eq!(outs.len(), 2);
    }

    #[test]
    fn reward_from_attestations_cap_applies_after_dedup() {
        // 3 distinct (after dedup) but cap allows only 2 whole rewards.
        let atts = [
            (op_n(1), 1u64, [0x01u8; 64]),
            (op_n(1), 1u64, [0x01u8; 64]),
            (op_n(2), 1u64, [0x02u8; 64]),
            (op_n(3), 1u64, [0x03u8; 64]),
        ];
        // dedup → {(1,1),(2,1),(3,1)} = 3 payloads; cap 25 / reward 10 = 2.
        let (outs, keys) = validator_reward_outputs_from_attestations(10, 25, &atts, &RewardedEpochSet::new());
        assert_eq!(outs.len(), 2);
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn reward_from_attestations_skips_already_rewarded_prefix() {
        // Cross-block uniqueness (§B.3(c)): a (bond, epoch) already rewarded on
        // the prefix earns nothing now; the rest are still paid.
        let mut prefix = RewardedEpochSet::new();
        prefix.insert(op_n(1), 5);
        let atts = [(op_n(1), 5u64, [0x01u8; 64]), (op_n(2), 5u64, [0x02u8; 64])];
        let (outs, keys) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &prefix);
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa87_spk(&[0x02u8; 64]));
        assert_eq!(keys, vec![(op_n(2), 5)]);
    }

    #[test]
    fn reward_from_attestations_empty_is_empty() {
        let (outs, keys) = validator_reward_outputs_from_attestations(100, 10_000, &[], &RewardedEpochSet::new());
        assert!(outs.is_empty());
        assert!(keys.is_empty());
    }

    // ---- compute_slashing_distribution ----------------------------

    #[test]
    fn slashing_distribution_sums_to_slashed_amount() {
        // The invariant ADR-0013 §"Slashing distribution"
        // requires: no value created or destroyed by rounding.
        for slashed in [1u64, 100, 12345, 1_000_000_000, u64::MAX / 2] {
            for bps in [0u16, 1, 1000, 5000, 9999, 10000] {
                let d = compute_slashing_distribution(slashed, bps, 0, 0);
                assert_eq!(
                    d.reporter_reward_sompi + d.security_reserve_sompi + d.victim_epoch_pool_sompi + d.burned_sompi,
                    slashed,
                    "slashed={slashed} bps={bps}"
                );
            }
        }
    }

    #[test]
    fn slashing_distribution_zero_bps_burns_everything() {
        let d = compute_slashing_distribution(1_000_000_000, 0, 0, 0);
        assert_eq!(d.reporter_reward_sompi, 0);
        assert_eq!(d.burned_sompi, 1_000_000_000);
    }

    #[test]
    fn slashing_distribution_full_bps_burns_nothing() {
        let d = compute_slashing_distribution(1_000_000_000, 10000, 0, 0);
        assert_eq!(d.reporter_reward_sompi, 1_000_000_000);
        assert_eq!(d.burned_sompi, 0);
    }

    #[test]
    fn slashing_distribution_mainnet_10pct_recommendation() {
        // ADR-0013 §"Slashing distribution" mainnet recommendation:
        // 1000 bps = 10% to reporter, 90% burned.
        let d = compute_slashing_distribution(100_000_000_000, 1000, 0, 0);
        assert_eq!(d.reporter_reward_sompi, 10_000_000_000);
        assert_eq!(d.burned_sompi, 90_000_000_000);
    }

    #[test]
    fn slashing_distribution_no_overflow_at_u64_max() {
        // u64::MAX × 10000 would overflow u64; the helper promotes
        // to u128 internally so it cannot. Pin this with the
        // largest plausible slashed amount (full u64 supply).
        let d = compute_slashing_distribution(u64::MAX, 10000, 0, 0);
        assert_eq!(d.reporter_reward_sompi, u64::MAX);
        assert_eq!(d.burned_sompi, 0);
    }

    // ---- ADR-0018 "本格版" (PoS-v2) Phase 3: 4-way slashing + victim compensation ----

    /// 4-way split (reporter→reserve→victim→burn) sums to S; reserve/victim = 0 ⇒ byte-identical 2-way.
    #[test]
    fn slashing_distribution_four_way_splits_and_conserves() {
        // 10% reporter, 20% reserve, 30% victim, 40% burn of 1_000_000.
        let d = compute_slashing_distribution(1_000_000, 1000, 2000, 3000);
        assert_eq!(d.reporter_reward_sompi, 100_000);
        assert_eq!(d.security_reserve_sompi, 200_000);
        assert_eq!(d.victim_epoch_pool_sompi, 300_000);
        assert_eq!(d.burned_sompi, 400_000);
        assert_eq!(d.reporter_reward_sompi + d.security_reserve_sompi + d.victim_epoch_pool_sompi + d.burned_sompi, 1_000_000);
        // Fenced (reserve/victim bps = 0): exactly the pre-v2 reporter + burn.
        let two = compute_slashing_distribution(1_000_000, 1000, 0, 0);
        assert_eq!(
            (two.reporter_reward_sompi, two.security_reserve_sompi, two.victim_epoch_pool_sompi, two.burned_sompi),
            (100_000, 0, 0, 900_000)
        );
    }

    /// Misconfigured Σbps > 10_000: priority clamp (reporter→reserve→victim) keeps burn ≥ 0 and Σ = S.
    #[test]
    fn slashing_distribution_overallocated_bps_clamps_in_priority() {
        // 6000+6000+6000 = 18000 bps. reporter 600k, reserve min(600k, 400k)=400k, victim 0, burn 0.
        let d = compute_slashing_distribution(1_000_000, 6000, 6000, 6000);
        assert_eq!(
            (d.reporter_reward_sompi, d.security_reserve_sompi, d.victim_epoch_pool_sompi, d.burned_sompi),
            (600_000, 400_000, 0, 0)
        );
        assert_eq!(d.reporter_reward_sompi + d.security_reserve_sompi + d.victim_epoch_pool_sompi + d.burned_sompi, 1_000_000);
    }

    /// Victim compensation is stake-proportional over the honest set; Σ ≤ pool; degenerate ⇒ empty.
    #[test]
    fn victim_compensation_outputs_stake_proportional() {
        let honest = vec![(Hash64::from_bytes([0xA1; 64]), 100u64), (Hash64::from_bytes([0xB2; 64]), 300u64)];
        let out = victim_compensation_outputs(1000, &honest, 400);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].value, 250); // 1000 · 100/400
        assert_eq!(out[0].script_public_key, p2pkh_mldsa87_spk(&[0xA1; 64]));
        assert_eq!(out[1].value, 750); // 1000 · 300/400
        assert!(out.iter().map(|o| o.value as u128).sum::<u128>() <= 1000);
        // Zero pool / empty honest set / zero denominator ⇒ no outputs.
        assert!(victim_compensation_outputs(0, &honest, 400).is_empty());
        assert!(victim_compensation_outputs(1000, &[], 0).is_empty());
    }

    // ---- apply_unreveal_reporter_min_cap --------------------------

    #[test]
    fn unreveal_min_cap_clamps_when_bps_reward_exceeds_floor() {
        // bps-derived reporter = 1_000_000 (10% of 10_000_000),
        // floor = 500_000. After cap: reporter = 500_000, burned
        // grows by 500_000.
        let base = compute_slashing_distribution(10_000_000, 1000, 0, 0);
        assert_eq!(base.reporter_reward_sompi, 1_000_000);
        assert_eq!(base.burned_sompi, 9_000_000);

        let capped = apply_unreveal_reporter_min_cap(base, 500_000);
        assert_eq!(capped.reporter_reward_sompi, 500_000);
        assert_eq!(capped.burned_sompi, 9_500_000);
        // Invariant survives the cap.
        assert_eq!(capped.reporter_reward_sompi + capped.burned_sompi, 10_000_000);
    }

    #[test]
    fn unreveal_min_cap_noop_when_bps_reward_under_floor() {
        // bps-derived reporter = 1_000 (10% of 10_000),
        // floor = 500_000. cap is a no-op.
        let base = compute_slashing_distribution(10_000, 1000, 0, 0);
        let capped = apply_unreveal_reporter_min_cap(base, 500_000);
        assert_eq!(capped, base);
    }

    #[test]
    fn unreveal_min_cap_at_exact_floor_is_noop() {
        // bps-derived reporter == floor. No clamp applies.
        let base = compute_slashing_distribution(5_000_000, 1000, 0, 0); // → reporter = 500_000
        assert_eq!(base.reporter_reward_sompi, 500_000);
        let capped = apply_unreveal_reporter_min_cap(base, 500_000);
        assert_eq!(capped, base);
    }

    // ---- slashing_distribution_output (Addendum C) ----------------

    #[test]
    fn slashing_distribution_output_equivocation_pays_reporter_and_burns_rest() {
        // S = 1_000_000, bps = 1000 (10%) → reporter 100_000, burn 900_000.
        let payload = [0x5au8; 64];
        let (out, dist) = slashing_distribution_output(1_000_000, 1000, 0, 0, &payload, None);
        let out = out.expect("non-zero reporter reward");
        assert_eq!(out.value, 100_000);
        assert_eq!(out.script_public_key, p2pkh_mldsa87_spk(&payload));
        assert_eq!(dist.burned_sompi, 900_000);
        // Conservation: reporter + burn == S (reserve + victim are 0 in the 2-way case).
        assert_eq!(out.value + dist.burned_sompi, 1_000_000);
    }

    #[test]
    fn slashing_distribution_output_zero_bps_emits_no_output() {
        // bps = 0 → everything burns, no reporter output.
        let (out, dist) = slashing_distribution_output(1_000_000, 0, 0, 0, &[0x5au8; 64], None);
        assert!(out.is_none());
        assert_eq!(dist.burned_sompi, 1_000_000);
    }

    #[test]
    fn slashing_distribution_output_unreveal_applies_floor() {
        // S = 5_000_000, bps = 1000 → bps-reward 500_000, but the unreveal
        // floor caps the reporter at 100_000; the extra burns.
        let payload = [0x5au8; 64];
        let (out, dist) = slashing_distribution_output(5_000_000, 1000, 0, 0, &payload, Some(100_000));
        let out = out.expect("non-zero reporter reward");
        assert_eq!(out.value, 100_000);
        assert_eq!(dist.burned_sompi, 4_900_000);
        assert_eq!(out.value + dist.burned_sompi, 5_000_000);
    }

    // ---- slashing_side_effects_from_evidence (Addendum C / D.4) ----

    fn slash_outpoint(b: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
    }

    #[test]
    fn slashing_side_effects_distinct_bonds_pay_each_reporter() {
        // Two distinct bonds, bps = 1000 (10%): each yields one side-effect
        // with reporter = 10% and burn = 90% of its own S.
        let evidence = [
            (slash_outpoint(1), 1_000_000, [0x11u8; 64], Hash64::from_bytes([0xa1; 64]), 0),
            (slash_outpoint(2), 2_000_000, [0x22u8; 64], Hash64::from_bytes([0xa2; 64]), 0),
        ];
        let effects = slashing_side_effects_from_evidence(&evidence, 1000, 0, 0, None);
        assert_eq!(effects.len(), 2);

        assert_eq!(effects[0].bond_outpoint, slash_outpoint(1));
        assert_eq!(effects[0].slashed_amount_sompi, 1_000_000);
        assert_eq!(effects[0].reporter_output.as_ref().unwrap().value, 100_000);
        assert_eq!(effects[0].reporter_output.as_ref().unwrap().script_public_key, p2pkh_mldsa87_spk(&[0x11u8; 64]));
        assert_eq!(effects[0].burned_sompi, 900_000);
        // The reporter UTXO is minted at (slashing_tx_id, 0), so each effect
        // carries the id of the tx that fixes its mint outpoint.
        assert_eq!(effects[0].slashing_tx_id, Hash64::from_bytes([0xa1; 64]));

        assert_eq!(effects[1].bond_outpoint, slash_outpoint(2));
        assert_eq!(effects[1].reporter_output.as_ref().unwrap().value, 200_000);
        assert_eq!(effects[1].burned_sompi, 1_800_000);
        assert_eq!(effects[1].slashing_tx_id, Hash64::from_bytes([0xa2; 64]));
        // Conservation per bond: reporter + burn == S.
        assert_eq!(effects[1].reporter_output.as_ref().unwrap().value + effects[1].burned_sompi, 2_000_000);
    }

    #[test]
    fn slashing_side_effects_dedup_same_bond_within_block() {
        // Two evidences against the SAME bond in one block collapse to a
        // single side-effect (its UTXO can be removed only once).
        let evidence = [
            (slash_outpoint(7), 1_000_000, [0x11u8; 64], Hash64::from_bytes([0xb1; 64]), 0),
            (slash_outpoint(7), 1_000_000, [0x99u8; 64], Hash64::from_bytes([0xb2; 64]), 0),
        ];
        let effects = slashing_side_effects_from_evidence(&evidence, 1000, 0, 0, None);
        assert_eq!(effects.len(), 1);
        // First occurrence wins (the 0x11 reporter payload) — and its tx id fixes
        // the mint outpoint (slashing_tx_id, 0), so a second tx for the same bond
        // can neither double-mint nor relocate the reporter UTXO.
        assert_eq!(effects[0].reporter_output.as_ref().unwrap().script_public_key, p2pkh_mldsa87_spk(&[0x11u8; 64]));
        assert_eq!(effects[0].slashing_tx_id, Hash64::from_bytes([0xb1; 64]));
    }

    #[test]
    fn slashing_side_effects_zero_bps_burns_everything() {
        // bps = 0 → no reporter output, full S burns, but the bond is still
        // slashed (a side-effect is produced so its UTXO is removed).
        let evidence = [(slash_outpoint(3), 1_000_000, [0x33u8; 64], Hash64::from_bytes([0xc3; 64]), 0)];
        let effects = slashing_side_effects_from_evidence(&evidence, 0, 0, 0, None);
        assert_eq!(effects.len(), 1);
        assert!(effects[0].reporter_output.is_none());
        assert_eq!(effects[0].burned_sompi, 1_000_000);
        assert_eq!(effects[0].slashed_amount_sompi, 1_000_000);
    }

    #[test]
    fn slashing_side_effects_unreveal_floor_caps_reporter() {
        // unreveal floor caps the reporter below the bps reward; the extra burns.
        let evidence = [(slash_outpoint(4), 5_000_000, [0x44u8; 64], Hash64::from_bytes([0xc4; 64]), 0)];
        let effects = slashing_side_effects_from_evidence(&evidence, 1000, 0, 0, Some(100_000));
        assert_eq!(effects[0].reporter_output.as_ref().unwrap().value, 100_000);
        assert_eq!(effects[0].burned_sompi, 4_900_000);
    }

    #[test]
    fn slashing_side_effects_empty_is_noop() {
        assert!(slashing_side_effects_from_evidence(&[], 1000, 0, 0, None).is_empty());
    }

    // ---- resolve_slashing_side_effects (Addendum C / D.4): resolve a block's
    // accepted evidence against its selected-parent bond view. Genuineness is a
    // separate rule, so these only exercise the bond-status resolution.

    // A slashing-evidence tx referencing `op` as its bond, paying `reporter`.
    fn evidence_tx_for(op: TransactionOutpoint, reporter: [u8; 64]) -> Transaction {
        let ev = SlashingEvidencePayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint: op,
            attestation_a: fixture_attestation(),
            attestation_b: fixture_attestation(),
            reporter_reward_spk_payload: reporter,
        };
        dns_overlay_tx(SUBNETWORK_ID_SLASHING_EVIDENCE, borsh::to_vec(&ev).unwrap())
    }

    // fixture_bond: amount = 100_000_000_000, activation = 5_000.
    const RESOLVE_DAA: u64 = 10_000;

    #[test]
    fn resolve_slashing_side_effects_active_bond_yields_removal() {
        // Bond Active at the block DAA (activation 5_000 < 10_000) ⇒ one
        // side-effect, S = amount, reporter = 10% to the evidence's payload.
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0);
        let view = ActiveBondView::from_records([(op, fixture_bond_record(op))]);
        let effects = resolve_slashing_side_effects(&[evidence_tx_for(op, [0xab; 64])], &view, RESOLVE_DAA, 1000, 0, 0);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].bond_outpoint, op);
        assert_eq!(effects[0].slashed_amount_sompi, 100_000_000_000);
        assert_eq!(effects[0].reporter_output.as_ref().unwrap().value, 10_000_000_000);
        assert_eq!(effects[0].reporter_output.as_ref().unwrap().script_public_key, p2pkh_mldsa87_spk(&[0xab; 64]));
        assert_eq!(effects[0].burned_sompi, 90_000_000_000);
    }

    #[test]
    fn resolve_slashing_side_effects_unbonding_bond_is_slashable() {
        // An Unbonding bond (unbond requested, far from release) still holds a
        // removable stake ⇒ slashable, so a validator cannot escape a slash by
        // unbonding first (ADR-0016 §D.3/§D.4).
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x22; 64]), 0);
        let mut b = fixture_bond_record(op);
        b.unbond_request_daa_score = Some(RESOLVE_DAA - 1); // release = +unbonding_period ≫ DAA.
        assert_eq!(effective_bond_status(&b, RESOLVE_DAA), BondStatus::Unbonding);
        let view = ActiveBondView::from_records([(op, b)]);
        let effects = resolve_slashing_side_effects(&[evidence_tx_for(op, [0xcd; 64])], &view, RESOLVE_DAA, 1000, 0, 0);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].slashed_amount_sompi, 100_000_000_000);
    }

    #[test]
    fn resolve_slashing_side_effects_skips_pending_bond() {
        // A Pending bond (DAA below activation) is not yet slashable-for-
        // distribution ⇒ no side-effect.
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x33; 64]), 0);
        let view = ActiveBondView::from_records([(op, fixture_bond_record(op))]);
        assert!(resolve_slashing_side_effects(&[evidence_tx_for(op, [0x01; 64])], &view, 1_000, 1000, 0, 0).is_empty());
    }

    #[test]
    fn resolve_slashing_side_effects_skips_already_slashed_bond() {
        // An already-Slashed bond yields no side-effect: its stake was already
        // removed, so it is never removed twice (idempotent).
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x44; 64]), 0);
        let mut b = fixture_bond_record(op);
        b.slashed_at_daa_score = Some(6_000);
        b.status = BondStatus::Slashed;
        let view = ActiveBondView::from_records([(op, b)]);
        assert!(resolve_slashing_side_effects(&[evidence_tx_for(op, [0x02; 64])], &view, RESOLVE_DAA, 1000, 0, 0).is_empty());
    }

    #[test]
    fn resolve_slashing_side_effects_skips_unknown_bond() {
        // Evidence whose bond is absent from the view contributes nothing. (The
        // genuineness rule would already reject such a block.)
        let op = TransactionOutpoint::new(Hash64::from_bytes([0x55; 64]), 0);
        assert!(
            resolve_slashing_side_effects(&[evidence_tx_for(op, [0x03; 64])], &ActiveBondView::new(), RESOLVE_DAA, 1000, 0, 0)
                .is_empty()
        );
    }

    #[test]
    fn resolve_slashing_side_effects_empty_without_evidence() {
        let native = dns_overlay_tx(SubnetworkId::from_byte(0), vec![]);
        assert!(resolve_slashing_side_effects(&[native], &ActiveBondView::new(), RESOLVE_DAA, 1000, 0, 0).is_empty());
    }

    // ---- Coordinated-failover protocol (ADR-0014) -----------------

    fn fixture_host_id(byte: u8) -> HostId {
        Hash::from_bytes([byte; 32])
    }

    fn fixture_takeover_token() -> TakeoverToken {
        TakeoverToken {
            version: DNS_PAYLOAD_VERSION_V1,
            yielding_host_id: fixture_host_id(0xa1),
            taking_over_host_id: fixture_host_id(0xa2),
            validator_id: Hash64::from_bytes([0x42u8; 64]),
            valid_from_epoch: 12345,
            grace_epochs: 1,
            issued_at_unix_secs: 1_700_000_000,
            signature: vec![0xccu8; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    #[test]
    fn host_id_is_deterministic() {
        let nonce = [0x11u8; 32];
        let a = compute_host_id(b"primary.kaspa-pq.example.com", &nonce);
        let b = compute_host_id(b"primary.kaspa-pq.example.com", &nonce);
        assert_eq!(a, b);
    }

    #[test]
    fn host_id_changes_with_hostname() {
        let nonce = [0x11u8; 32];
        let a = compute_host_id(b"primary.kaspa-pq.example.com", &nonce);
        let b = compute_host_id(b"standby.kaspa-pq.example.com", &nonce);
        assert_ne!(a, b);
    }

    #[test]
    fn host_id_changes_with_nonce() {
        // Rebuilding the secondary host with a fresh nonce must
        // change its host_id — anti-spoofing rationale in ADR-0014
        // §"`host_id` derivation".
        let a = compute_host_id(b"primary.kaspa-pq.example.com", &[0x11u8; 32]);
        let b = compute_host_id(b"primary.kaspa-pq.example.com", &[0x12u8; 32]);
        assert_ne!(a, b);
    }

    #[test]
    fn host_id_uses_host_id_key_domain() {
        // Hashing the same bytes with the generic (no-key)
        // BLAKE2b-256 yields a different value — pins the domain
        // separation. Defends against a future refactor accidentally
        // dropping the key.
        let nonce = [0x11u8; 32];
        let with_key = compute_host_id(b"hostname", &nonce);

        let mut without_key = Blake2bParams::new().hash_length(32).to_state();
        without_key.update(b"hostname");
        without_key.update(&nonce);
        let undomained = without_key.finalize();

        assert_ne!(with_key.as_bytes(), undomained.as_bytes());
    }

    #[test]
    fn takeover_token_borsh_roundtrip() {
        let t = fixture_takeover_token();
        let bytes = borsh::to_vec(&t).unwrap();
        let back: TakeoverToken = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, t);
        // Sanity: the dominant size component is the 4627-byte
        // ML-DSA-87 signature.
        assert!(bytes.len() >= STAKE_ATTESTATION_SIG_LEN);
    }

    #[test]
    fn takeover_token_message_is_deterministic() {
        let m1 = takeover_token_message(fixture_host_id(0xa1), fixture_host_id(0xa2), Hash64::from_bytes([0x42u8; 64]), 100, 1);
        let m2 = takeover_token_message(fixture_host_id(0xa1), fixture_host_id(0xa2), Hash64::from_bytes([0x42u8; 64]), 100, 1);
        assert_eq!(m1, m2);
    }

    #[test]
    fn takeover_token_message_changes_with_each_field() {
        let base = takeover_token_message(fixture_host_id(0xa1), fixture_host_id(0xa2), Hash64::from_bytes([0x42u8; 64]), 100, 1);
        // yielding_host_id differs
        assert_ne!(
            base,
            takeover_token_message(fixture_host_id(0xa3), fixture_host_id(0xa2), Hash64::from_bytes([0x42u8; 64]), 100, 1)
        );
        // taking_over_host_id differs
        assert_ne!(
            base,
            takeover_token_message(fixture_host_id(0xa1), fixture_host_id(0xa3), Hash64::from_bytes([0x42u8; 64]), 100, 1)
        );
        // validator_id differs
        assert_ne!(
            base,
            takeover_token_message(fixture_host_id(0xa1), fixture_host_id(0xa2), Hash64::from_bytes([0x43u8; 64]), 100, 1)
        );
        // valid_from_epoch differs
        assert_ne!(
            base,
            takeover_token_message(fixture_host_id(0xa1), fixture_host_id(0xa2), Hash64::from_bytes([0x42u8; 64]), 101, 1)
        );
        // grace_epochs differs
        assert_ne!(
            base,
            takeover_token_message(fixture_host_id(0xa1), fixture_host_id(0xa2), Hash64::from_bytes([0x42u8; 64]), 100, 2)
        );
    }

    #[test]
    fn takeover_token_message_uses_distinct_domain_key() {
        // Hashing the same bytes with the attestation domain key
        // yields a different value — the takeover-token signing
        // surface must be cryptographically distinct from the
        // attestation surface (ADR-0014 §"Public-claim discipline":
        // takeover signatures can never be replayed as
        // attestations and vice versa).
        let inputs = |key: &[u8]| {
            let mut h = Blake2bParams::new().hash_length(32).key(key).to_state();
            h.update(&[0xa1u8; 32]);
            h.update(&[0xa2u8; 32]);
            h.update(&[0x42u8; 64]);
            h.update(&100u64.to_le_bytes());
            h.update(&[1u8]);
            h.finalize()
        };
        let with_takeover = inputs(TAKEOVER_TOKEN_MESSAGE_DOMAIN);
        let with_attestation = inputs(ATTESTATION_MESSAGE_DOMAIN);
        assert_ne!(with_takeover.as_bytes(), with_attestation.as_bytes());

        let actual = takeover_token_message(fixture_host_id(0xa1), fixture_host_id(0xa2), Hash64::from_bytes([0x42u8; 64]), 100, 1);
        assert_eq!(actual.as_bytes(), with_takeover.as_bytes());
    }

    // ---- Remote-signer protocol (ADR-0015) ------------------------

    #[test]
    fn signing_purpose_discriminants_are_api_stable() {
        // Wire-format discriminant; reordering is a protocol
        // hard fork. Pin to immediately trip drift.
        assert_eq!(SigningPurpose::Transaction as u8, 0);
        assert_eq!(SigningPurpose::Attestation as u8, 1);
        assert_eq!(SigningPurpose::TakeoverToken as u8, 2);
        assert_eq!(SigningPurpose::Unbond as u8, 3);
    }

    #[test]
    fn signing_purpose_default_is_transaction() {
        // Conservative default — `Transaction` is the original
        // ML-DSA-87 use site (ADR-0002), pre-DNS-overlay.
        assert_eq!(SigningPurpose::default(), SigningPurpose::Transaction);
    }

    #[test]
    fn signing_purpose_borsh_roundtrip() {
        for p in [SigningPurpose::Transaction, SigningPurpose::Attestation, SigningPurpose::TakeoverToken, SigningPurpose::Unbond] {
            let bytes = borsh::to_vec(&p).unwrap();
            let back: SigningPurpose = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, p);
            assert_eq!(bytes.len(), 1);
        }
    }

    #[test]
    fn signer_policy_discriminants_are_api_stable() {
        assert_eq!(SignerPolicy::Permissive as u8, 0);
        assert_eq!(SignerPolicy::AuditOnly as u8, 1);
        assert_eq!(SignerPolicy::Strict as u8, 2);
    }

    #[test]
    fn signer_policy_default_is_permissive() {
        // Matches the ADR-0010 local-key-file behaviour, so a
        // signer with no policy configured behaves like the
        // pre-ADR-0015 baseline.
        assert_eq!(SignerPolicy::default(), SignerPolicy::Permissive);
    }

    #[test]
    fn signer_policy_borsh_roundtrip() {
        for p in [SignerPolicy::Permissive, SignerPolicy::AuditOnly, SignerPolicy::Strict] {
            let bytes = borsh::to_vec(&p).unwrap();
            let back: SignerPolicy = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, p);
        }
    }

    #[test]
    fn signer_error_borsh_roundtrip_all_variants() {
        for e in [
            SignerError::ProtocolVersionMismatch,
            SignerError::KeyNotFound,
            SignerError::UnknownPurpose,
            SignerError::PolicyViolation("equivocation: target_hash differs".into()),
            SignerError::HsmError(0xCAFE_BABE, "CKR_DEVICE_ERROR".into()),
            SignerError::RateLimit,
            SignerError::InternalError("disk full".into()),
        ] {
            let bytes = borsh::to_vec(&e).unwrap();
            let back: SignerError = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, e);
        }
    }

    #[test]
    fn signer_metadata_borsh_roundtrip_all_variants() {
        let none = SignerMetadata::None;
        let att = SignerMetadata::Attestation { epoch: 42, target_hash: Hash64::from_bytes([0x11u8; 64]), target_daa_score: 100 };
        let tk = SignerMetadata::TakeoverToken {
            yielding_host_id: fixture_host_id(0xa1),
            taking_over_host_id: fixture_host_id(0xa2),
            valid_from_epoch: 12345,
            grace_epochs: 1,
        };
        for m in [none, att, tk] {
            let bytes = borsh::to_vec(&m).unwrap();
            let back: SignerMetadata = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, m);
        }
    }

    #[test]
    fn signer_hello_borsh_roundtrip() {
        let h = SignerHello {
            protocol_version: SIGNER_PROTOCOL_VERSION,
            capabilities: CAP_SIGN_ATTESTATION | CAP_POLICY_STRICT | CAP_AUDIT_LOG,
            client_identity: fixture_host_id(0xa1),
        };
        let bytes = borsh::to_vec(&h).unwrap();
        let back: SignerHello = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn signer_hello_ack_borsh_roundtrip() {
        let h = SignerHelloAck {
            protocol_version: SIGNER_PROTOCOL_VERSION,
            capabilities: CAP_SIGN_TRANSACTION | CAP_SIGN_ATTESTATION | CAP_SIGN_TAKEOVER_TOKEN | CAP_HSM_BACKED,
            server_identity: fixture_host_id(0xb1),
        };
        let bytes = borsh::to_vec(&h).unwrap();
        let back: SignerHelloAck = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, h);
    }

    fn fixture_signer_request() -> SignerRequest {
        SignerRequest {
            request_id: 7,
            validator_id: Hash64::from_bytes([0x42u8; 64]),
            purpose: SigningPurpose::Attestation,
            context: ATTESTATION_MLDSA87_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Attestation(Hash::from_bytes([0xcdu8; 32])),
            metadata: SignerMetadata::Attestation { epoch: 42, target_hash: Hash64::from_bytes([0x11u8; 64]), target_daa_score: 100 },
        }
    }

    #[test]
    fn signer_request_borsh_roundtrip() {
        let r = fixture_signer_request();
        let bytes = borsh::to_vec(&r).unwrap();
        let back: SignerRequest = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn signer_request_purpose_matches_typed_digest() {
        // audit H-03: the typed digest's purpose must agree with the request purpose, and a
        // transaction request carries a 64-byte sighash (unrepresentable under the old 32-byte field).
        assert!(fixture_signer_request().purpose_matches_digest()); // Attestation + Attestation
        let tx = SignerRequest {
            purpose: SigningPurpose::Transaction,
            message_digest: SignerMessageDigest::Transaction(Hash64::from_bytes([0x09u8; 64])),
            ..fixture_signer_request()
        };
        assert!(tx.purpose_matches_digest());
        // Transaction tag + Attestation digest (from the fixture) => mismatch, must be refused.
        let mismatched = SignerRequest { purpose: SigningPurpose::Transaction, ..fixture_signer_request() };
        assert!(!mismatched.purpose_matches_digest());
        assert_eq!(SignerMessageDigest::Unbond(Hash::from_bytes([0x1u8; 32])).purpose(), SigningPurpose::Unbond);
    }

    #[test]
    fn signer_response_borsh_roundtrip_ok() {
        let r = SignerResponse { request_id: 7, result: Ok(vec![0xccu8; STAKE_ATTESTATION_SIG_LEN]) };
        let bytes = borsh::to_vec(&r).unwrap();
        let back: SignerResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn signer_response_borsh_roundtrip_err() {
        let r = SignerResponse {
            request_id: 7,
            result: Err(SignerError::PolicyViolation("equivocation: target differs from epoch 42 record".into())),
        };
        let bytes = borsh::to_vec(&r).unwrap();
        let back: SignerResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
    }

    fn fixture_audit_record(outcome: SignerOutcome, sig_fingerprint: Hash64) -> SignerAuditRecord {
        SignerAuditRecord {
            timestamp_unix_secs: 1_700_000_000,
            client_identity: fixture_host_id(0xa1),
            request_id: 7,
            validator_id: Hash64::from_bytes([0x42u8; 64]),
            purpose: SigningPurpose::Attestation,
            metadata: SignerMetadata::Attestation { epoch: 42, target_hash: Hash64::from_bytes([0x11u8; 64]), target_daa_score: 100 },
            message_digest: SignerMessageDigest::Attestation(Hash::from_bytes([0xcdu8; 32])),
            signature_fingerprint: sig_fingerprint,
            outcome,
        }
    }

    #[test]
    fn signer_audit_record_borsh_roundtrip_signed() {
        let r = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xeeu8; 64]));
        let bytes = borsh::to_vec(&r).unwrap();
        let back: SignerAuditRecord = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn signer_audit_record_borsh_roundtrip_refused() {
        let r = fixture_audit_record(SignerOutcome::Refused(SignerError::RateLimit), kaspa_hashes::ZERO_HASH64);
        let bytes = borsh::to_vec(&r).unwrap();
        let back: SignerAuditRecord = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn audit_chain_entry_is_deterministic() {
        let prev = Hash64::from_bytes([0x33u8; 64]);
        let rec = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xeeu8; 64]));
        let a = compute_signer_audit_chain_entry(prev, &rec);
        let b = compute_signer_audit_chain_entry(prev, &rec);
        assert_eq!(a, b);
    }

    #[test]
    fn audit_chain_entry_changes_with_prev_hash() {
        let rec = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xeeu8; 64]));
        let a = compute_signer_audit_chain_entry(Hash64::from_bytes([0x33u8; 64]), &rec);
        let b = compute_signer_audit_chain_entry(Hash64::from_bytes([0x34u8; 64]), &rec);
        assert_ne!(a, b);
    }

    #[test]
    fn audit_chain_entry_changes_with_record_content() {
        let prev = Hash64::from_bytes([0x33u8; 64]);
        let a = compute_signer_audit_chain_entry(prev, &fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xeeu8; 64])));
        let b = compute_signer_audit_chain_entry(prev, &fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xefu8; 64])));
        assert_ne!(a, b);
    }

    #[test]
    fn audit_chain_walks_three_records_consistently() {
        // ADR-0015 §"Audit log" promises that walking the chain
        // from a known-good genesis hash deterministically
        // produces the same terminal hash for the same record
        // sequence. Pin a 3-record walk to verify the chaining
        // discipline.
        let genesis = kaspa_hashes::ZERO_HASH64;
        let r1 = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe1u8; 64]));
        let r2 = fixture_audit_record(SignerOutcome::Refused(SignerError::RateLimit), kaspa_hashes::ZERO_HASH64);
        let r3 = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe3u8; 64]));

        let h1 = compute_signer_audit_chain_entry(genesis, &r1);
        let h2 = compute_signer_audit_chain_entry(h1, &r2);
        let h3 = compute_signer_audit_chain_entry(h2, &r3);

        // Re-walking the same sequence must produce the same
        // terminal hash (chain replay).
        let h1b = compute_signer_audit_chain_entry(genesis, &r1);
        let h2b = compute_signer_audit_chain_entry(h1b, &r2);
        let h3b = compute_signer_audit_chain_entry(h2b, &r3);
        assert_eq!(h3, h3b);
    }

    #[test]
    fn audit_chain_detects_record_insertion() {
        // Inserting a record between r1 and r2 must shift every
        // subsequent chain hash — this is the cryptographic
        // tamper-detection property.
        let genesis = kaspa_hashes::ZERO_HASH64;
        let r1 = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe1u8; 64]));
        let r_evil = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe9u8; 64]));
        let r2 = fixture_audit_record(SignerOutcome::Signed, Hash64::from_bytes([0xe2u8; 64]));

        let h1 = compute_signer_audit_chain_entry(genesis, &r1);
        let h_evil = compute_signer_audit_chain_entry(h1, &r_evil);
        let h2_after_insert = compute_signer_audit_chain_entry(h_evil, &r2);

        let h2_clean = compute_signer_audit_chain_entry(h1, &r2);
        assert_ne!(h2_after_insert, h2_clean, "post-insertion chain hash must differ from clean chain hash");
    }

    // ---- PR-10.5: the former `*_stub` panics are now implemented ----
    // `compute_stake_score` / `check_dns_reorg_rule` replace the PR-10.3
    // `unimplemented!()` stubs; behaviour is covered by the StakeScore +
    // reorg-gate tests above.
}
