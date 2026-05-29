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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Display, Formatter};

use blake2b_simd::Params as Blake2bParams;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};
use kaspa_utils::mem_size::MemSizeEstimator;

use crate::subnets::{SUBNETWORK_ID_SLASHING_EVIDENCE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, SUBNETWORK_ID_STAKE_BOND, SubnetworkId};
use crate::{
    BlueWorkType,
    tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionOutpoint, TransactionOutput},
};

/// 1952 bytes — matches `kaspa_txscript::MLDSA65_PK_LEN`. Repeated
/// here so this module does not have to depend on `kaspa-txscript`;
/// asserted-equal by [`tests::dns_constants_have_expected_values`].
pub const STAKE_VALIDATOR_PUBKEY_LEN: usize = 1952;

/// 3309 bytes — matches `kaspa_txscript::MLDSA65_SIG_LEN`. Same
/// re-export rationale as [`STAKE_VALIDATOR_PUBKEY_LEN`].
pub const STAKE_ATTESTATION_SIG_LEN: usize = 3309;

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

/// kaspa-pq Phase 10 wire-format version of every payload struct.
/// Bumped only by a hard-fork ADR; consumers reject foreign versions.
pub const DNS_PAYLOAD_VERSION_V1: u16 = 1;

/// kaspa-pq Phase 10 ML-DSA-65 attestation signing context. Distinct
/// from the transaction context (`b"kaspa-pq-v1/tx/mldsa65"`,
/// ADR-0002) so an attestation signature can never be replayed as a
/// transaction signature (and vice versa).
pub const ATTESTATION_MLDSA65_CONTEXT: &[u8] = b"kaspa-pq-v1/att/mldsa65";

/// kaspa-pq Phase 10 BLAKE2b-256 domain key used when constructing
/// the attestation message that ML-DSA-65 signs over. Consumed by
/// [`stake_attestation_message`]. See ADR-0009 §"Attestation target".
pub const ATTESTATION_MESSAGE_DOMAIN: &[u8] = b"kaspa-pq-v1/stake-attestation";

/// kaspa-pq Phase 11 BLAKE2b-512 domain key used by
/// [`validator_set_commitment`]. Consensus-fixed and bumped only by
/// a hard-fork ADR (the `-v1` suffix is the contract). See
/// ADR-0010 §"Validator-set commitment derivation".
pub const VALIDATOR_SET_COMMITMENT_KEY: &[u8] = b"kaspa-pq-validator-set-v1";

/// kaspa-pq Phase 13 sortition domain keys. All four are
/// consensus-fixed and bumped only by a hard-fork ADR (the `-v1`
/// suffix is the contract). See
/// [ADR-0012](../../docs/adr/0012-mainnet-validator-sortition-commit-reveal.md).
///
/// - `SORTITION_COMMIT_KEY` — keys the BLAKE2b-512 hash a validator
///   commits to during epoch `E−2`. Distinct from every other
///   sortition key so a commit hash is never confusable with a seed
///   or priority value.
/// - `SORTITION_SEED_KEY` — keys the BLAKE2b-512 over the reveal set
///   that produces `epoch_seed_E` when the ≥ 2/3 reveal threshold
///   is met.
/// - `SORTITION_FALLBACK_KEY` — keys the BLAKE2b-512 over
///   `epoch_seed_{E-1} || epoch.to_le_bytes()` used when the reveal
///   threshold is **not** met. Distinct from `SORTITION_SEED_KEY` so
///   a node cannot mistake a fallback seed for a regular one.
/// - `SORTITION_PRIORITY_KEY` — keys the BLAKE2b-512 over
///   `epoch_seed_E || validator_id` that yields per-validator
///   priority in the deterministic stake-weighted top-K selection.
/// - `SORTITION_DETERMINISTIC_KEY` — keys the BLAKE2b-512 over
///   `epoch.to_le_bytes()` used as `epoch_seed_E` in the simnet /
///   devnet / testnet-initial deterministic mode.
pub const SORTITION_COMMIT_KEY: &[u8] = b"kaspa-pq-sortition-commit-v1";
pub const SORTITION_SEED_KEY: &[u8] = b"kaspa-pq-sortition-seed-v1";
pub const SORTITION_FALLBACK_KEY: &[u8] = b"kaspa-pq-sortition-fallback-v1";
pub const SORTITION_PRIORITY_KEY: &[u8] = b"kaspa-pq-sortition-priority-v1";
pub const SORTITION_DETERMINISTIC_KEY: &[u8] = b"kaspa-pq-sortition-deterministic-v1";

/// kaspa-pq Phase 13 coordinated-failover domain keys (ADR-0014).
///
/// - `HOST_ID_KEY` — keys the BLAKE2b-256 over `hostname ||
///   host_boot_nonce` that produces a stable, rebuild-resistant
///   `HostId` for each validator host.
/// - `TAKEOVER_TOKEN_MESSAGE_DOMAIN` — keys the BLAKE2b-256 over
///   the takeover-token signing material (see
///   [`takeover_token_message`]).
/// - `TAKEOVER_TOKEN_CONTEXT` — ML-DSA-65 `ctx` parameter for the
///   `sign_ctx` call that produces the
///   [`TakeoverToken::signature`]. Distinct from both the
///   transaction context (`b"kaspa-pq-v1/tx/mldsa65"`) and the
///   attestation context (`b"kaspa-pq-v1/att/mldsa65"`,
///   ADR-0009 §"Attestation target") so a takeover-token
///   signature can never be replayed as a transaction or
///   attestation signature, and vice versa.
///
/// These three are consensus-irrelevant (the entire coordinated-
/// failover protocol is node-local; no on-chain surface), but
/// the `-v1` suffix is the contract — renaming auditable.
pub const HOST_ID_KEY: &[u8] = b"kaspa-pq-validator-host-id-v1";
pub const TAKEOVER_TOKEN_MESSAGE_DOMAIN: &[u8] = b"kaspa-pq-takeover-token-v1";
pub const TAKEOVER_TOKEN_CONTEXT: &[u8] = b"kaspa-pq-v1/takeover/mldsa65";

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
    /// the validator is removed from all future committees.
    Slashed = 3,
}

/// Per-network sortition mode (ADR-0012 §"Two sortition modes").
///
/// `Deterministic` keys the epoch seed by epoch number alone —
/// fine for simnet, devnet, and testnet-initial because predictability
/// is a feature there (reproducible runs). `CommitReveal` keys the
/// seed from on-chain validator-contributed randomness — mainnet
/// uses it from genesis because the deterministic mode is broken
/// against bond-grinding and anchor-targeted attacks.
///
/// Default is `Deterministic` so a node booted with no DNS
/// parameters configured behaves as the simnet does (loud about
/// the missing production seed). The mode is bumped from
/// `Deterministic` to `CommitReveal` by a hard-fork DAA-score gate
/// (`DnsParams::commit_reveal_activation_daa_score`); the reverse
/// is not supported.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SortitionMode {
    #[default]
    Deterministic = 0,
    CommitReveal = 1,
}

// ---------------------------------------------------------------------
// Wire payloads (transaction-level).
// ---------------------------------------------------------------------

/// kaspa-pq Phase 10 stake-bond payload.
///
/// Carried inside a transaction with subnetwork id
/// `SUBNETWORK_ID_STAKE_BOND` (consensus rule to be added in PR-10.4).
/// The bond locks an amount of coins to a validator ML-DSA-65 key for
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

    /// Raw 1952-byte ML-DSA-65 public key for the validator. Stored
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

    /// The owner's **declared** ML-DSA-65 P2PKH spend payload —
    /// `BLAKE2b-256(owner_public_key)`, i.e. the 32-byte
    /// `Address { version: PubKeyHashMlDsa65 }` payload (ADR-0002).
    /// This is the **only** field validator rewards (ADR-0013
    /// coinbase fan-out) are paid to; `owner_pubkey_hash` above is
    /// the 64-byte BLAKE2b-512 *identity* hash (ADR-0008) and is
    /// **not** a payable target — the two are different widths and
    /// the 64→32 reduction is not derivable. See ADR-0013
    /// Addendum B. A malformed value only misdirects the owner's
    /// own rewards (self-griefing), so consensus imposes no check
    /// beyond the fixed 32-byte width guaranteed by the type.
    /// Appended last to keep the borsh layout change localized
    /// (pre-activation wire change — no live bond exists).
    pub owner_reward_spk_payload: [u8; 32],
}

/// One validator attestation over a selected-chain anchor.
///
/// Many `StakeAttestation`s are batched into
/// `StakeAttestationShardPayload` for on-chain commitment. A raw
/// attestation is ~3300+100 bytes (the ML-DSA-65 signature
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

    /// 3309-byte ML-DSA-65 signature over the BLAKE2b-256
    /// attestation message produced by [`stake_attestation_message`]
    /// with `ATTESTATION_MLDSA65_CONTEXT` as the libcrux `ctx`
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
/// The transaction has no inputs/outputs — the payload is the whole point. NOTE:
/// how such zero-input overlay transactions are admitted to the mempool and blocks
/// (fee-funded input vs. a consensus exemption) is the validator submission path
/// (ADR-0010 §"Validator service runtime" step 9) and is not yet wired; today the
/// stock `NoTxInputs` rule would reject this at mempool ingestion.
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
}

/// Phase 13 sortition commit transaction payload (ADR-0012
/// §"Commit window"). Submitted during epoch `target_epoch − 2`
/// inside a transaction routed by `SUBNETWORK_ID_SORTITION_COMMIT`
/// (consensus rule lands in PR-10.9).
///
/// At most one commit per `(validator_id, target_epoch)` is
/// accepted on-chain; duplicates are rejected at tx-validation
/// time (not slashable — duplicate is a rebroadcast, not
/// equivocation).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SortitionCommitPayload {
    pub version: u16,
    pub validator_id: Hash64,
    pub target_epoch: u64,
    /// `commit = BLAKE2b-512(key=SORTITION_COMMIT_KEY,
    ///                       input = r || target_epoch.to_le_bytes()
    ///                            || validator_id.as_bytes())` where
    /// `r` is a fresh 32-byte secret kept off-chain until the
    /// reveal window. Derived via [`compute_commit`].
    pub commit: Hash64,
}

/// Phase 13 sortition reveal transaction payload (ADR-0012
/// §"Reveal window"). Submitted during epoch `target_epoch − 1`
/// inside a transaction routed by `SUBNETWORK_ID_SORTITION_REVEAL`.
///
/// Consensus rule (PR-10.9):
/// 1. A `SortitionCommitPayload` for the same `(validator_id,
///    target_epoch)` must exist on-chain;
/// 2. `compute_commit(&reveal, target_epoch, validator_id)` must
///    equal that commit's `commit` field.
/// Mis-matching reveals are rejected at tx validation; valid
/// reveals contribute to `epoch_seed_{target_epoch}` derivation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SortitionRevealPayload {
    pub version: u16,
    pub validator_id: Hash64,
    pub target_epoch: u64,
    /// The 32-byte secret committed to in the prior epoch. Borsh
    /// encodes `[u8; 32]` as the raw bytes (no length prefix),
    /// matching the wire format described in ADR-0012.
    pub reveal: [u8; 32],
}

/// Phase 13 slashing evidence for a validator that committed but
/// did not reveal within the reveal window (ADR-0012 §"Slashing
/// rule: commit-without-reveal"). Independent of the existing
/// equivocation [`SlashingEvidencePayload`]; both can fire in the
/// same epoch.
///
/// Any node can be the reporter; consensus pays
/// `DnsParams::unreveal_reporter_reward_sompi` (small, gas-cost
/// scale) to the reporter and burns the remainder of the slash
/// (`DnsParams::commit_without_reveal_slash_sompi`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct UnrevealSlashingEvidencePayload {
    pub version: u16,
    pub target_epoch: u64,
    pub validator_id: Hash64,
    /// Outpoint of the unrevealed-commit transaction. Consensus
    /// re-derives whether a matching reveal exists by walking the
    /// reveal window blocks; the outpoint here is enough to bind
    /// the evidence to a specific commit.
    pub commit_outpoint: TransactionOutpoint,
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
    pub unbonding_period_blocks: u64,

    /// Copied verbatim from [`StakeBondPayload::owner_reward_spk_payload`]:
    /// the owner's declared 32-byte ML-DSA-65 P2PKH spend payload that
    /// ADR-0013 validator rewards are paid to. See ADR-0013 Addendum B.
    pub owner_reward_spk_payload: [u8; 32],

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
/// Built by `consensus/src/processes/validator_sortition.rs`
/// (PR-10.9). The `validators` vector **must** be sorted ascending
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

    pub epoch_length_blocks: u64,

    /// `cW` — minimum work-depth for history confirmation.
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

    // ---- Sortition (ADR-0012) ----
    /// Per-network sortition mode. `Deterministic` for simnet /
    /// devnet / testnet-initial; `CommitReveal` for mainnet from
    /// genesis. See ADR-0012 §"Two sortition modes".
    pub sortition_mode: SortitionMode,
    /// Block window inside epoch `E−2` during which
    /// `SortitionCommitPayload` txs are accepted on-chain.
    pub commit_window_blocks: u64,
    /// Block window inside epoch `E−1` during which
    /// `SortitionRevealPayload` txs are accepted on-chain.
    pub reveal_window_blocks: u64,
    /// Numerator of the reveal-threshold fraction (default 2/3).
    /// When `reveals * threshold_denom < commits * threshold_num`,
    /// `epoch_seed_E` falls back per ADR-0012 §"Fallback rule".
    pub min_reveal_threshold_num: u32,
    /// Denominator of the reveal-threshold fraction (default 3 for
    /// the 2/3 Byzantine threshold).
    pub min_reveal_threshold_denom: u32,
    /// Per-epoch committee size — the number of validators sorted
    /// out for attestation eligibility. Bounded so the per-epoch
    /// attestation total fits in
    /// `max_attestations_per_block × blocks_per_epoch`.
    pub committee_size: u32,
    /// Number of epochs between commit and sortition use; ADR-0012
    /// fixes this at 2 (commit at `E−2`, reveal at `E−1`, use at
    /// `E`). The field is carried so future ADRs can raise it for
    /// extra finality margin without an unrelated field shape
    /// migration.
    pub commit_reveal_lookahead_epochs: u8,
    /// Bond slash applied to a validator that committed but did
    /// not reveal within `reveal_window_blocks`. Calibration
    /// target: ≥ `committee_size × per_attestation_reward ×
    /// epochs_until_unbond` so deliberate withholding is
    /// economically irrational.
    pub commit_without_reveal_slash_sompi: u64,
    /// Paid to whoever submits the `UnrevealSlashingEvidencePayload`.
    /// Small fixed amount (gas-cost scale) — large enough to cover
    /// reporter cost, small enough not to incentivise spurious
    /// reports.
    pub unreveal_reporter_reward_sompi: u64,
    /// If `Some`, the testnet `Deterministic → CommitReveal`
    /// switchover DAA score. `None` on mainnet (always
    /// `CommitReveal` from genesis). On simnet / devnet this is
    /// also `None` (always `Deterministic`).
    pub commit_reveal_activation_daa_score: Option<u64>,

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

    /// Validator reward-distribution parameters (ADR-0013). Consumed
    /// by the PR-10.5′-b coinbase fan-out and the PR-10.12′ slashing
    /// split. Carried here (rather than as a separate `Params` field)
    /// so it inherits `DnsParams`'s `Option` gating — the whole
    /// validator-reward track is inert wherever `dns_params` is `None`
    /// or `daa_score < dns_activation_daa_score`.
    pub reward_params: RewardParams,
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
/// coinbase fan-out, and by `consensus/src/processes/slashing.rs`
/// (PR-10.12′) for the equivocation / unreveal distribution
/// split. The `unreveal_reporter_reward_sompi` floor stays in
/// [`DnsParams`] where ADR-0012 placed it; the slashing helper
/// here takes that floor as a separate argument for the unreveal
/// case (`min`-cap rule per ADR-0013 §"Slashing distribution").
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RewardParams {
    /// Flat per-included-attestation reward. ADR-0013 makes this
    /// flat (not stake-proportional) on purpose: sortition
    /// already stake-weights via committee membership, so a flat
    /// reward gives every staked sompi a uniform expected APY
    /// regardless of validator size.
    pub per_attestation_reward_sompi: u64,

    /// Basis points (10000 = 100%) of any slashed bond paid to
    /// the slashing reporter. Mainnet recommendation: 1000 bps
    /// = 10%. Applies to both equivocation
    /// ([`SlashingEvidencePayload`]) and unreveal
    /// ([`UnrevealSlashingEvidencePayload`]) slashes; the unreveal
    /// case additionally `min`-caps the reward at the
    /// [`DnsParams::unreveal_reporter_reward_sompi`] floor from
    /// ADR-0012.
    pub slashing_reporter_reward_bps: u16,

    /// Hard cap on the per-block validator-side coinbase outflow.
    /// Defensive — `per_attestation_reward_sompi ×
    /// max_attestations_per_block` should never exceed this; if
    /// it does, the consensus rule prefers the cap and refunds
    /// the difference rather than overflowing into the coinbase
    /// accumulator. See ADR-0013 §"Inflation cap".
    pub max_validator_inflation_per_block_sompi: u64,
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

/// Outcome of [`compute_slashing_distribution`] — how to split a
/// slashed bond between the reporter and the burn sink.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SlashingDistribution {
    /// Sompi paid to whoever submitted the slashing evidence.
    pub reporter_reward_sompi: u64,
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
}

/// Per-epoch validator committee view surfaced by the consensus pipeline to the
/// in-process validator service (and, later, the `getValidatorStatus` RPC).
///
/// Computed deterministically from the current sink DAA score, the stake-bond
/// store, and `DnsParams` (epoch length, committee size, sortition mode). The
/// `members` list is the canonical (`validator_id`-sorted) committee for `epoch`;
/// a validator is eligible to attest iff its `validator_id` appears in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorCommittee {
    /// Epoch this committee governs (`= pov_daa_score / epoch_length_blocks`).
    pub epoch: u64,
    /// Sink DAA score the active set was evaluated at (point of view).
    pub pov_daa_score: u64,
    /// `committee_size` parameter used for selection this epoch.
    pub committee_size: usize,
    /// Number of active validators considered for selection at `pov_daa_score`.
    pub active_validator_count: usize,
    /// Selected committee, sorted ascending by `validator_id`.
    pub members: Vec<Hash64>,
}

/// Everything the in-process validator service needs to issue one stake
/// attestation for the current epoch, assembled by the consensus pipeline so the
/// network-, committee-, and target-binding match the verifier (`virtual_processor`)
/// byte-for-byte. The service's only remaining job is to sign [`Self::message`]
/// under [`ATTESTATION_MLDSA65_CONTEXT`] with its ML-DSA-65 key.
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
/// `validator_pubkey_hash`) from its ML-DSA-65 public key, per ADR-0008
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

/// Local-only fingerprint of an ML-DSA-65 signature: unkeyed `BLAKE2b-512` of the
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

/// Compute the BLAKE2b-256 attestation message that ML-DSA-65 signs
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
/// `Hash32`) so it composes directly with the libcrux ML-DSA-65 `sign_ctx`
/// API. The signing context (`ATTESTATION_MLDSA65_CONTEXT`) is applied at
/// the ML-DSA-65 layer, not inside this hasher — keeping the two domain
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

// ---------------------------------------------------------------------
// Sortition derivations (ADR-0012).
// ---------------------------------------------------------------------

/// Compute the BLAKE2b-512 commitment for a sortition reveal
/// (ADR-0012 §"Commit window"):
///
/// ```text
/// commit = BLAKE2b-512(
///     key   = SORTITION_COMMIT_KEY,
///     input = reveal (32 B)
///          || target_epoch.to_le_bytes()
///          || validator_id.as_bytes() (64 B),
/// )
/// ```
///
/// The keyed BLAKE2b-512 plus the explicit `target_epoch` and
/// `validator_id` make the commit binding (each validator can
/// commit at most one value per target epoch; consensus rule in
/// PR-10.9 enforces) and domain-separated from every other 64-byte
/// hash on the wire.
pub fn compute_commit(reveal: &[u8; 32], target_epoch: u64, validator_id: Hash64) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(SORTITION_COMMIT_KEY).to_state();
    hasher.update(reveal);
    hasher.update(&target_epoch.to_le_bytes());
    hasher.update(validator_id.as_byte_slice());

    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Compute `epoch_seed_E` in [`SortitionMode::Deterministic`]
/// (ADR-0012 §"Two sortition modes"):
///
/// ```text
/// epoch_seed_E = BLAKE2b-512(
///     key   = SORTITION_DETERMINISTIC_KEY,
///     input = target_epoch.to_le_bytes(),
/// )
/// ```
///
/// Used by simnet, devnet, and testnet-initial. Mainnet uses
/// [`derive_epoch_seed_commit_reveal`] instead.
pub fn derive_epoch_seed_deterministic(target_epoch: u64) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(SORTITION_DETERMINISTIC_KEY).to_state();
    hasher.update(&target_epoch.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Compute `epoch_seed_E` in [`SortitionMode::CommitReveal`]
/// (ADR-0012 §"Seed derivation"):
///
/// 1. Filter `valid_reveals` (caller guarantees these matched a
///    prior commit and survived `compute_commit` re-verification
///    at tx-validation time).
/// 2. Sort by `validator_id` ascending.
/// 3. If `valid_reveals.len() * threshold_denom ≥
///    num_commits * threshold_num`, derive the seed from the
///    reveal set; else fall back to
///    `BLAKE2b-512(SORTITION_FALLBACK_KEY, prev_epoch_seed ||
///    target_epoch.to_le_bytes())`.
///
/// Returns the seed regardless of which branch fires; callers can
/// detect the fallback by comparing against
/// [`derive_epoch_seed_fallback`] independently.
pub fn derive_epoch_seed_commit_reveal(
    target_epoch: u64,
    valid_reveals: &[SortitionRevealPayload],
    num_commits: u32,
    threshold_num: u32,
    threshold_denom: u32,
    prev_epoch_seed: Hash64,
) -> Hash64 {
    // Promote to u64 before multiplying so we cannot overflow a
    // u32 when `num_commits ≈ 2^31` (defensive — the real value
    // is bounded by committee_size, but the helper takes a raw
    // u32 so it must be safe at the type's extremes).
    let lhs = (valid_reveals.len() as u64).saturating_mul(threshold_denom as u64);
    let rhs = (num_commits as u64).saturating_mul(threshold_num as u64);
    if lhs < rhs {
        return derive_epoch_seed_fallback(target_epoch, prev_epoch_seed);
    }

    let mut sorted: Vec<&SortitionRevealPayload> = valid_reveals.iter().collect();
    sorted.sort_by(|a, b| a.validator_id.cmp(&b.validator_id));

    let mut hasher = Blake2bParams::new().hash_length(64).key(SORTITION_SEED_KEY).to_state();
    hasher.update(&target_epoch.to_le_bytes());
    hasher.update(&(sorted.len() as u32).to_le_bytes());
    for r in &sorted {
        hasher.update(r.validator_id.as_byte_slice());
        hasher.update(&r.reveal);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The fallback seed derivation used when reveals are below the
/// `≥ 2/3` threshold (ADR-0012 §"Fallback rule"):
///
/// ```text
/// epoch_seed_E = BLAKE2b-512(
///     key   = SORTITION_FALLBACK_KEY,
///     input = prev_epoch_seed.as_bytes() (64 B)
///          || target_epoch.to_le_bytes(),
/// )
/// ```
///
/// Distinct from [`derive_epoch_seed_commit_reveal`]'s primary
/// path by keying with `SORTITION_FALLBACK_KEY` rather than
/// `SORTITION_SEED_KEY`, so a node can never mistake a fallback
/// seed for a regular one. The fallback chain bottoms out at the
/// all-zero `Hash64` for `target_epoch == 0`, which is the
/// genesis case.
pub fn derive_epoch_seed_fallback(target_epoch: u64, prev_epoch_seed: Hash64) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(SORTITION_FALLBACK_KEY).to_state();
    hasher.update(prev_epoch_seed.as_byte_slice());
    hasher.update(&target_epoch.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Per-validator priority value used by [`select_committee`]
/// (ADR-0012 §"Sortition function"):
///
/// ```text
/// priority_v(E) =
///     BLAKE2b-512(
///         key   = SORTITION_PRIORITY_KEY,
///         input = epoch_seed_E.as_bytes() || validator_id.as_bytes(),
///     )
///     .first_u128()
///     /
///     stake_v.max(1)
/// ```
///
/// Stake-weighted: a validator with 2× stake has half the priority
/// of an equally-randomised peer, so they are twice as likely to
/// land in the bottom-K (selected) set. The `.max(1)` guard is
/// defensive — `select_committee` filters zero-stake records
/// upstream — but we keep it here so a misused call still produces
/// a defined value rather than a panic.
pub fn compute_validator_priority(epoch_seed: Hash64, validator_id: Hash64, stake_amount: u64) -> u128 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(SORTITION_PRIORITY_KEY).to_state();
    hasher.update(epoch_seed.as_byte_slice());
    hasher.update(validator_id.as_byte_slice());
    let digest = hasher.finalize();
    let bytes = digest.as_bytes();

    // Take the first u128 word (little-endian) of the 64-byte
    // digest. u128 is wide enough that stake-weighted division
    // does not lose precision for any plausible stake value
    // (< 2^64), and narrow enough that priority sorting fits
    // standard library primitives.
    let mut first16 = [0u8; 16];
    first16.copy_from_slice(&bytes[..16]);
    let h = u128::from_le_bytes(first16);
    h / (stake_amount.max(1) as u128)
}

/// Select the per-epoch committee per ADR-0012 §"Sortition
/// function": the `committee_size` validators with the lowest
/// [`compute_validator_priority`] values.
///
/// Inputs:
/// - `epoch_seed`: derived for the target epoch via either
///   [`derive_epoch_seed_deterministic`] or
///   [`derive_epoch_seed_commit_reveal`].
/// - `active`: the active validator set at `epoch_start(E)`.
/// - `committee_size`: per-network parameter; capped at
///   `active.len()` if `committee_size > active.len()`.
///
/// Returns the chosen validator IDs sorted by `validator_id`
/// ascending (canonical form — independent of priority order).
/// The canonical sort lets the result feed directly into
/// [`validator_set_commitment`] for the same epoch without an
/// extra sort step.
///
/// Zero-stake records are skipped — they should never appear in an
/// active set, but defensive filtering keeps the helper total over
/// arbitrary input.
pub fn select_committee(epoch_seed: Hash64, active: &[ValidatorRecord], committee_size: usize) -> Vec<Hash64> {
    let mut ranked: Vec<(u128, Hash64)> = active
        .iter()
        .filter(|v| v.stake_amount > 0)
        .map(|v| (compute_validator_priority(epoch_seed, v.validator_id, v.stake_amount), v.validator_id))
        .collect();
    // Stable sort by priority ascending — ties (cryptographically
    // implausible at u128 width, but defensively handled) fall
    // back to insertion order, which is the caller's iteration
    // order over `active`.
    ranked.sort_by_key(|p| p.0);
    ranked.truncate(committee_size.min(ranked.len()));

    let mut ids: Vec<Hash64> = ranked.into_iter().map(|(_, id)| id).collect();
    ids.sort();
    ids
}

/// Select the committee for `epoch` under the network's [`SortitionMode`],
/// deriving the epoch seed appropriately and delegating to [`select_committee`].
///
/// Returns `None` for [`SortitionMode::CommitReveal`]: that seed is built from
/// revealed `SortitionRevealPayload`s, which the validator-service path does not
/// yet read (a later Phase 11 slice). [`SortitionMode::Deterministic`] (simnet /
/// devnet / pre-switchover testnet) is fully supported via
/// [`derive_epoch_seed_deterministic`].
pub fn select_committee_for_epoch(
    epoch: u64,
    sortition_mode: SortitionMode,
    active: &[ValidatorRecord],
    committee_size: usize,
) -> Option<Vec<Hash64>> {
    let epoch_seed = match sortition_mode {
        SortitionMode::Deterministic => derive_epoch_seed_deterministic(epoch),
        SortitionMode::CommitReveal => return None,
    };
    Some(select_committee(epoch_seed, active, committee_size))
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
    /// Bond is active; current epoch validator-set sortition has
    /// not yet picked this validator.
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
    /// `BLAKE2b-512` of the 3309-byte ML-DSA-65 signature bytes.
    /// Pinned so the validator can recognise a re-broadcast of an
    /// in-flight attestation across restarts without re-storing
    /// the full ~3.3 KB signature. **Not** part of the
    /// equivocation predicate — ML-DSA-65 is hedged by default and
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
/// `signature_fingerprint`: two valid hedged ML-DSA-65 signatures
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
/// §"`TakeoverToken`"). Carries an ML-DSA-65 signature by the
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

    /// 3309-byte ML-DSA-65 signature by the validator key over
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
/// composes directly with the libcrux ML-DSA-65 `sign_ctx` /
/// `verify_ctx` APIs. The ML-DSA-65 signing context
/// (`TAKEOVER_TOKEN_CONTEXT`) is applied at the ML-DSA-65 layer,
/// not inside this hasher — keeping the two domain separators
/// independent and distinct from every other ML-DSA-65 use site
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
    /// the tx-script ML-DSA-65 sign path produces; context is
    /// `b"kaspa-pq-v1/tx/mldsa65"`.
    #[default]
    Transaction = 0,
    /// DNS overlay attestation — message digest is from
    /// [`stake_attestation_message`]; context is
    /// `ATTESTATION_MLDSA65_CONTEXT`.
    Attestation = 1,
    /// Coordinated-failover takeover token — message digest is
    /// from [`takeover_token_message`]; context is
    /// `TAKEOVER_TOKEN_CONTEXT`.
    TakeoverToken = 2,
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
    /// libcrux ML-DSA-65 `sign_ctx` ctx parameter. Caller
    /// provides; the signer does not infer the context from
    /// the purpose tag because future protocol extensions may
    /// need a non-standard context for the same purpose.
    pub context: Vec<u8>,
    /// 32-byte BLAKE2b-256 the ML-DSA-65 will sign over.
    pub message_digest: Hash,
    pub metadata: SignerMetadata,
}

/// Server → client response. The `result` payload is either the
/// 3309-byte ML-DSA-65 signature or a structured failure.
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
    pub message_digest: Hash,
    /// `BLAKE2b-512` of the 3309-byte signature bytes (zero
    /// `Hash64` if the request was refused). Pinned so the audit
    /// record stays small while still witnessing what was
    /// signed.
    pub signature_fingerprint: Hash64,
    pub outcome: SignerOutcome,
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

/// Build the canonical kaspa-pq ML-DSA-65 P2PKH `scriptPublicKey`
/// for a 32-byte spend payload (ADR-0002 / ADR-0013 Addendum B).
///
/// The 37-byte script is
/// `OpDup ‖ OpBlake2b ‖ OpData32 ‖ <payload32> ‖ OpEqualVerify ‖ OpCheckSigMlDsa65`
/// at `ScriptPublicKey` version 0. The opcode bytes are pinned as
/// literals here because `consensus-core` does not depend on full
/// `kaspa-txscript` (only `kaspa-txscript-errors`); the output is
/// **byte-identical** to
/// `kaspa_txscript::pay_to_address_script(&Address::new(_, Version::PubKeyHashMlDsa65, payload))`
/// and a parity test in the `consensus` crate
/// (`processes::coinbase`) pins that equality. The `ScriptPublicKey`
/// bytes are prefix-independent, so coinbase construction and
/// validation need agree only on the 32-byte payload.
pub fn p2pkh_mldsa65_spk(owner_reward_spk_payload: &[u8; 32]) -> ScriptPublicKey {
    // ADR-0002 §"Script template" opcode bytes (see
    // crypto/txscript/src/opcodes/mod.rs):
    const OP_DUP: u8 = 0x76;
    const OP_BLAKE2B: u8 = 0xaa;
    const OP_DATA32: u8 = 0x20;
    const OP_EQUAL_VERIFY: u8 = 0x88;
    const OP_CHECKSIG_MLDSA65: u8 = 0xa6;

    let mut script = Vec::with_capacity(37);
    script.push(OP_DUP);
    script.push(OP_BLAKE2B);
    script.push(OP_DATA32);
    script.extend_from_slice(owner_reward_spk_payload);
    script.push(OP_EQUAL_VERIFY);
    script.push(OP_CHECKSIG_MLDSA65);
    // P2PKH-ML-DSA spk version is `MAX_SCRIPT_PUBLIC_KEY_VERSION` == 0.
    ScriptPublicKey::new(0, ScriptVec::from_slice(&script))
}

/// Build the validator-side coinbase outputs for a block (ADR-0013
/// §"Coinbase fan-out", as amended by Addendum B): one
/// `TransactionOutput { value: per_attestation_reward_sompi,
/// script_public_key: p2pkh_mldsa65_spk(payload) }` per included
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
    reward_spk_payloads: &[[u8; 32]],
) -> Vec<TransactionOutput> {
    if per_attestation_reward_sompi == 0 {
        return Vec::new();
    }
    // Whole-output cap: never emit a partial per-attestation reward.
    let max_payable =
        (max_validator_inflation_per_block_sompi / per_attestation_reward_sompi).min(reward_spk_payloads.len() as u64) as usize;
    reward_spk_payloads[..max_payable]
        .iter()
        .map(|payload| TransactionOutput::new(per_attestation_reward_sompi, p2pkh_mldsa65_spk(payload)))
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
    attestations: &[(TransactionOutpoint, u64, [u8; 32])],
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
        outputs.push(TransactionOutput::new(per_attestation_reward_sompi, p2pkh_mldsa65_spk(payload)));
        rewarded_keys.push(key);
    }
    (outputs, rewarded_keys)
}

/// Compute the slashing distribution for a slashed bond
/// (ADR-0013 §"Slashing distribution"). Sums exactly to
/// `slashed_amount_sompi` — no value created or destroyed by
/// rounding.
///
/// Used for both the equivocation case
/// ([`SlashingEvidencePayload`]) and the unreveal case
/// ([`UnrevealSlashingEvidencePayload`]); for unreveal,
/// the caller `min`-caps the result through
/// [`apply_unreveal_reporter_min_cap`] (separate helper for
/// clarity).
pub fn compute_slashing_distribution(slashed_amount_sompi: u64, slashing_reporter_reward_bps: u16) -> SlashingDistribution {
    // Promote to u128 for the multiplication so a max-bond × bps
    // product cannot overflow. Maximum intermediate value is
    // `u64::MAX × 10000 ≈ 1.8e23`, well within u128.
    let numerator = (slashed_amount_sompi as u128).saturating_mul(slashing_reporter_reward_bps as u128);
    let reporter_reward_sompi = (numerator / 10000) as u64;
    // burned = slashed - reporter (subtract in u64; reporter
    // cannot exceed slashed because bps ≤ 10000 by type).
    let burned_sompi = slashed_amount_sompi - reporter_reward_sompi;
    SlashingDistribution { reporter_reward_sompi, burned_sompi }
}

/// Apply the ADR-0013 `min`-cap to the reporter reward for the
/// unreveal-slash case: the unreveal reporter receives
/// `min(bps_reward, unreveal_reporter_reward_sompi_floor)`. The
/// burn share grows by whatever the reporter no longer collects.
///
/// The floor is the
/// [`DnsParams::unreveal_reporter_reward_sompi`] value from
/// ADR-0012 (`commit_without_reveal_slash_sompi` is typically
/// much smaller than a full equivocation bond burn, so the bps
/// rule applied unmodified would over-pay the reporter relative
/// to the work done — the floor keeps the reporter pipeline
/// cheap).
pub fn apply_unreveal_reporter_min_cap(
    distribution: SlashingDistribution,
    unreveal_reporter_reward_sompi_floor: u64,
) -> SlashingDistribution {
    let capped_reporter = distribution.reporter_reward_sompi.min(unreveal_reporter_reward_sompi_floor);
    let extra_burn = distribution.reporter_reward_sompi - capped_reporter;
    SlashingDistribution { reporter_reward_sompi: capped_reporter, burned_sompi: distribution.burned_sompi + extra_burn }
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

/// Per-epoch StakeScore increment (ADR-0009 §"StakeScore mechanics"):
///
/// ```text
/// increment = floor(signed_stake × STAKE_SCORE_SCALE / total_active_stake)
/// ```
///
/// Returns `0` when `total_active_stake_sompi == 0` (no active stake →
/// the epoch contributes nothing). `signed` is clamped to `total`, so a
/// single epoch can never exceed `STAKE_SCORE_SCALE` ("1.0") and a
/// malformed input where the deduplicated signed stake exceeds the
/// denominator cannot inflate the score. The multiply is done in `u128`
/// and cannot overflow (`u64::MAX × 1e9 < u128::MAX`).
pub fn stake_score_increment(signed_stake_sompi: u64, total_active_stake_sompi: u64) -> u128 {
    if total_active_stake_sompi == 0 {
        return 0;
    }
    let signed = signed_stake_sompi.min(total_active_stake_sompi) as u128;
    signed * STAKE_SCORE_SCALE / (total_active_stake_sompi as u128)
}

/// Deterministic `StakeScore(H)` aggregation over the epochs whose
/// anchors lie on the selected chain ending at the target (ADR-0009
/// §"StakeScore mechanics"). Every node observing the same on-chain
/// shard set reaches the same number — `u128` fixed-point throughout,
/// no floats. Replaces the PR-10.3 `compute_stake_score_stub`.
pub fn compute_stake_score(per_epoch: &[EpochStakeTally]) -> StakeScore {
    let mut acc: u128 = 0;
    for e in per_epoch {
        acc = acc.saturating_add(stake_score_increment(e.signed_stake_sompi, e.total_active_stake_sompi));
    }
    StakeScore(acc)
}

/// History-confirmation predicate — the DNS paper's
/// `WorkDepth(B) ≥ cW ∧ StakeDepth(B) ≥ cS`. An anchor is DNS-confirmed
/// iff it clears **both** thresholds. Used by the consensus pipeline to
/// advance [`DnsState::last_dns_confirmed_anchor`].
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

// =====================================================================
// PR-10.4: DNS finality overlay transaction kinds + stateless payload
// validation (ADR-0009 §"On-chain artefacts").
//
// `dns_tx_kind` maps a routed subnetwork id to its payload kind; the
// three `validate_*_payload` functions perform *stateless* checks only:
// borsh-decodability, payload version, the fixed ML-DSA length
// invariants (1952-byte pubkey / 3309-byte signature), shard cardinality
// + single-anchor tuple consistency, and equivocation well-formedness.
// The consensus pipeline calls these from `check_transaction_subnetwork`
// (PR-10.4 wiring in `tx_validation_in_isolation.rs`).
//
// Deferred to later PRs (they need DAG / UTXO / rollout context): the
// on-chain bond existence + `pubkey_hash == BLAKE2b-512(pubkey)` binding,
// rollout-stage gating, ML-DSA-65 signature verification against the
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
    /// An attestation signature is not exactly `STAKE_ATTESTATION_SIG_LEN`.
    InvalidSignatureLen(usize),
    /// An attestation shard carries no attestations.
    EmptyShard,
    /// An attestation shard exceeds `MAX_ATTESTATIONS_PER_SHARD`.
    ShardTooLarge(usize),
    /// An attestation in a shard does not match the shard's
    /// `(epoch, target_hash, validator_set_commitment)` tuple.
    ShardTupleMismatch,
    /// The two attestations in slashing evidence do not share the same
    /// `(bond_outpoint, validator_id, epoch)` triple.
    EvidenceTripleMismatch,
    /// The two attestations approve the same anchor — not equivocation,
    /// so they are not slashable evidence.
    EvidenceNotIncompatible,
}

impl Display for DnsTxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DnsTxError::Decode => write!(f, "DNS overlay payload failed to decode"),
            DnsTxError::UnsupportedVersion(v) => write!(f, "unsupported DNS overlay payload version {v}"),
            DnsTxError::ZeroBondAmount => write!(f, "stake-bond amount must be non-zero"),
            DnsTxError::InvalidPubKeyLen(n) => write!(f, "validator public key length {n} is invalid"),
            DnsTxError::InvalidSignatureLen(n) => write!(f, "attestation signature length {n} is invalid"),
            DnsTxError::EmptyShard => write!(f, "attestation shard is empty"),
            DnsTxError::ShardTooLarge(n) => write!(f, "attestation shard has {n} attestations, above the maximum"),
            DnsTxError::ShardTupleMismatch => write!(f, "attestation does not match the shard's anchor tuple"),
            DnsTxError::EvidenceTripleMismatch => {
                write!(f, "slashing evidence attestations are not from the same (bond, validator, epoch) triple")
            }
            DnsTxError::EvidenceNotIncompatible => write!(f, "slashing evidence attestations approve the same anchor"),
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
    Ok(())
}

/// Stateless validation of a [`StakeBondPayload`] (subnetwork
/// `SUBNETWORK_ID_STAKE_BOND`): decodability, payload version, non-zero
/// bonded amount, and the fixed 1952-byte ML-DSA-65 validator
/// public-key length. The `validator_pubkey_hash ==
/// BLAKE2b-512(validator_pubkey)` and `owner_pubkey_hash`↔funding-input
/// bindings are deferred to the stateful PR.
pub fn validate_stake_bond_payload(payload: &[u8]) -> Result<(), DnsTxError> {
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
pub fn bond_mutations_from_accepted_txs(txs: &[Transaction], accepted_daa_score: u64) -> Vec<BondMutation> {
    let mut muts = Vec::new();
    for tx in txs {
        match dns_tx_kind(&tx.subnetwork_id) {
            Some(DnsTxKind::StakeBond) => {
                if let Ok(payload) = borsh::from_slice::<StakeBondPayload>(&tx.payload) {
                    let outpoint = TransactionOutpoint::new(tx.id(), 0);
                    muts.push(BondMutation::Insert(outpoint, stake_bond_record_from_payload(&payload, outpoint)));
                }
            }
            Some(DnsTxKind::SlashingEvidence) => {
                if let Ok(payload) = borsh::from_slice::<SlashingEvidencePayload>(&tx.payload) {
                    muts.push(BondMutation::Slash(payload.bond_outpoint, accepted_daa_score));
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
// bond's `validator_pubkey` under `ATTESTATION_MLDSA65_CONTEXT`, and
// gating by `is_bond_active_at` — then it passes the surviving
// contributions here. Keeping the aggregation pure makes the
// dedup + normalisation deterministic and unit-testable.
// =====================================================================

/// One signature-verified, bond-active attestation contribution fed into
/// [`aggregate_epoch_tallies`]. The caller (consensus aggregation pass)
/// has already (a) confirmed the referenced bond exists and is `Active` at
/// the attestation's `target_daa_score`, and (b) verified the ML-DSA-65
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
            // `signed` is clamped to `total` inside `stake_score_increment`,
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
/// or `None` before the overlay's first write). When `anchor` is not
/// itself confirmed, the previously-confirmed anchor is carried forward;
/// if there is no previous confirmation, `last_dns_confirmed_anchor`
/// defaults to the zero `Hash64` — meaning "nothing confirmed yet", which
/// the reorg gate treats as dormant (every candidate trivially includes it).
#[allow(clippy::too_many_arguments)]
pub fn advance_dns_confirmation(
    prev: Option<&DnsState>,
    anchor: Hash64,
    anchor_daa_score: u64,
    work_depth: BlueWorkType,
    stake_depth: StakeScore,
    rollout_stage: DnsRolloutStage,
    validator_set_commitment: Hash64,
    required_work_depth: BlueWorkType,
    required_stake_depth: StakeScore,
) -> DnsState {
    let confirmed = is_dns_confirmed(work_depth, stake_depth, required_work_depth, required_stake_depth);
    let (last_dns_confirmed_anchor, last_dns_confirmed_anchor_daa_score) = if confirmed {
        (anchor, anchor_daa_score)
    } else if let Some(p) = prev {
        (p.last_dns_confirmed_anchor, p.last_dns_confirmed_anchor_daa_score)
    } else {
        (Hash64::default(), 0)
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

/// Flattens every [`StakeAttestation`] from the `StakeAttestationShard`
/// payloads among `txs` (the decode-only first half of the A.5 aggregation
/// input). Pure; defensively skips undecodable shard payloads. Signature
/// verification + bond lookup happen in the consensus crate (which can call
/// `kaspa-txscript`), keeping the borsh decode here and out of the
/// virtual processor.
pub fn attestations_from_accepted_txs(txs: &[Transaction]) -> Vec<StakeAttestation> {
    let mut out = Vec::new();
    for tx in txs {
        if dns_tx_kind(&tx.subnetwork_id) == Some(DnsTxKind::StakeAttestationShard) {
            if let Ok(shard) = borsh::from_slice::<StakeAttestationShardPayload>(&tx.payload) {
                out.extend(shard.attestations);
            }
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
        note: format!("rollout_stage={:?}; pow_confirmed={pow_confirmed}; dns_confirmed={dns_confirmed}", state.rollout_stage),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- PR-10.5: StakeScore + DNS reorg gate ----

    #[test]
    fn stake_score_increment_floor_formula() {
        assert_eq!(stake_score_increment(1, 3), 333_333_333); // floor(1e9/3)
        assert_eq!(stake_score_increment(50, 50), STAKE_SCORE_SCALE); // 1.0
        assert_eq!(stake_score_increment(5, 10), STAKE_SCORE_SCALE / 2); // 0.5
        assert_eq!(stake_score_increment(0, 0), 0); // no active stake
        assert_eq!(stake_score_increment(7, 0), 0);
        assert_eq!(stake_score_increment(999, 50), STAKE_SCORE_SCALE); // signed clamped to total
        assert_eq!(stake_score_increment(u64::MAX, u64::MAX), STAKE_SCORE_SCALE); // no overflow
    }

    #[test]
    fn compute_stake_score_sums_increments_deterministically() {
        let epochs = vec![
            EpochStakeTally { epoch: 1, signed_stake_sompi: 10, total_active_stake_sompi: 10 }, // 1.0
            EpochStakeTally { epoch: 2, signed_stake_sompi: 5, total_active_stake_sompi: 10 },  // 0.5
            EpochStakeTally { epoch: 3, signed_stake_sompi: 0, total_active_stake_sompi: 10 },  // 0.0
        ];
        let s = compute_stake_score(&epochs);
        assert_eq!(s, StakeScore(STAKE_SCORE_SCALE + STAKE_SCORE_SCALE / 2));
        assert_eq!(compute_stake_score(&epochs), s); // deterministic
        assert_eq!(compute_stake_score(&[]), StakeScore(0));
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
    fn dns_constants_have_expected_values() {
        // Cross-check against the consensus-core kaspa-pq constant
        // values (the kaspa-txscript crate is downstream of
        // consensus-core, so we cannot pull MLDSA65_PK_LEN /
        // MLDSA65_SIG_LEN from there directly without creating a
        // dependency cycle; the values are duplicated here and the
        // assertion is the contract).
        assert_eq!(STAKE_VALIDATOR_PUBKEY_LEN, 1952);
        assert_eq!(STAKE_ATTESTATION_SIG_LEN, 3309);

        // ADR-0009 / ADR-0010 / ADR-0012 / ADR-0014 domain-
        // separator strings. All consensus-fixed (or consensus-
        // adjacent, for the node-local failover keys) and bumped
        // only by a hard-fork ADR — pin the bytes so any
        // accidental rename trips this test.
        assert_eq!(ATTESTATION_MLDSA65_CONTEXT, b"kaspa-pq-v1/att/mldsa65");
        assert_eq!(ATTESTATION_MESSAGE_DOMAIN, b"kaspa-pq-v1/stake-attestation");
        assert_eq!(VALIDATOR_SET_COMMITMENT_KEY, b"kaspa-pq-validator-set-v1");
        assert_eq!(SORTITION_COMMIT_KEY, b"kaspa-pq-sortition-commit-v1");
        assert_eq!(SORTITION_SEED_KEY, b"kaspa-pq-sortition-seed-v1");
        assert_eq!(SORTITION_FALLBACK_KEY, b"kaspa-pq-sortition-fallback-v1");
        assert_eq!(SORTITION_PRIORITY_KEY, b"kaspa-pq-sortition-priority-v1");
        assert_eq!(SORTITION_DETERMINISTIC_KEY, b"kaspa-pq-sortition-deterministic-v1");
        // ADR-0014 — node-local failover protocol keys.
        assert_eq!(HOST_ID_KEY, b"kaspa-pq-validator-host-id-v1");
        assert_eq!(TAKEOVER_TOKEN_MESSAGE_DOMAIN, b"kaspa-pq-takeover-token-v1");
        assert_eq!(TAKEOVER_TOKEN_CONTEXT, b"kaspa-pq-v1/takeover/mldsa65");
        // ADR-0015 — node-local remote-signer audit chain key
        // and protocol-version pin.
        assert_eq!(AUDIT_LOG_CHAIN_KEY, b"kaspa-pq-signer-audit-v1");
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
        assert_ne!(ATTESTATION_MLDSA65_CONTEXT, b"kaspa-pq-v1/tx/mldsa65");
        assert_ne!(TAKEOVER_TOKEN_CONTEXT, b"kaspa-pq-v1/tx/mldsa65");
        assert_ne!(TAKEOVER_TOKEN_CONTEXT, ATTESTATION_MLDSA65_CONTEXT);

        // Pairwise distinctness of all five sortition keys —
        // SORTITION_SEED_KEY ≠ SORTITION_FALLBACK_KEY is the most
        // important invariant (ADR-0012 §"Fallback rule": a node
        // must never mistake a fallback seed for a regular one),
        // but the test covers all 10 pairs defensively.
        let sortition_keys: [&[u8]; 5] =
            [SORTITION_COMMIT_KEY, SORTITION_SEED_KEY, SORTITION_FALLBACK_KEY, SORTITION_PRIORITY_KEY, SORTITION_DETERMINISTIC_KEY];
        for i in 0..sortition_keys.len() {
            for j in (i + 1)..sortition_keys.len() {
                assert_ne!(sortition_keys[i], sortition_keys[j], "sortition keys {i} and {j} must differ");
            }
        }
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

    fn fixture_attestation() -> StakeAttestation {
        StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: Hash64::from_bytes([0xa5u8; 64]),
            bond_outpoint: fixture_outpoint(),
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::from_bytes([0x22u8; 64]),
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
            owner_reward_spk_payload: [0xddu8; 32],
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
        // Spot-check the dominant size component: the ML-DSA-65
        // signature plus borsh framing. The Vec<u8> Borsh layout is
        // 4-byte length prefix + N data bytes, so a 3309-byte sig
        // contributes 4 + 3309 = 3313 bytes plus the other fixed
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
        };
        let bytes = borsh::to_vec(&evidence).unwrap();
        let back: SlashingEvidencePayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, evidence);
    }

    // ---- PR-10.4: DNS overlay tx kinds + stateless payload validation ----

    fn fixture_bond() -> StakeBondPayload {
        StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
            validator_pubkey_hash: Hash64::from_bytes([0xbbu8; 64]),
            validator_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
            amount: 100_000_000_000,
            activation_daa_score: 5_000,
            unbonding_period_blocks: 100_000,
            owner_reward_spk_payload: [0xddu8; 32],
        }
    }

    fn fixture_shard(n: usize) -> StakeAttestationShardPayload {
        StakeAttestationShardPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::from_bytes([0x22u8; 64]),
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

        let muts = bond_mutations_from_accepted_txs(&[bond_tx, slash_tx, shard_tx, native_tx], 12_345);
        assert_eq!(muts.len(), 2);
        assert_eq!(muts[0], BondMutation::Insert(expected_outpoint, stake_bond_record_from_payload(&bond_payload, expected_outpoint)));
        assert_eq!(muts[1], BondMutation::Slash(evidence.bond_outpoint, 12_345));
    }

    #[test]
    fn bond_mutations_skips_undecodable_overlay_payload() {
        // A malformed stake-bond payload is defensively skipped, not panicked on.
        let bad = dns_overlay_tx(SUBNETWORK_ID_STAKE_BOND, vec![0xff, 0x00, 0x12]);
        assert!(bond_mutations_from_accepted_txs(&[bad], 0).is_empty());
    }

    #[test]
    fn bond_mutations_empty_without_overlay_txs() {
        let native = dns_overlay_tx(SubnetworkId::from_byte(0), vec![]);
        let coinbase = dns_overlay_tx(SubnetworkId::from_byte(1), vec![]);
        assert!(bond_mutations_from_accepted_txs(&[native, coinbase], 100).is_empty());
    }

    // ---- Addendum B §B.1: ActiveBondView + RewardedEpochSet ----

    fn fixture_bond_record(op: TransactionOutpoint) -> StakeBondRecord {
        // activation_daa_score = 5_000, status = Pending.
        stake_bond_record_from_payload(&fixture_bond(), op)
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
        // End-to-end: 0.5 + 0.3 + 0.0 = 0.8.
        assert_eq!(compute_stake_score(&tallies), StakeScore(STAKE_SCORE_SCALE / 2 + STAKE_SCORE_SCALE * 3 / 10));
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
    fn advance_dns_confirmation_advances_only_when_both_thresholds_met() {
        let vsc = Hash64::from_bytes([0x22; 64]);
        let (cw, cs) = (BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE)); // require work>=1000, stake>=1.0
        let stage = DnsRolloutStage::Active;

        // Not confirmed (stake below cS) + no prev -> last confirmed = zero ("none yet").
        let a1 = Hash64::from_bytes([0x11; 64]);
        let s1 = advance_dns_confirmation(
            None,
            a1,
            500,
            BlueWorkType::from_u64(2000),
            StakeScore(STAKE_SCORE_SCALE / 2),
            stage,
            vsc,
            cw,
            cs,
        );
        assert_eq!(s1.selected_chain_anchor, a1);
        assert_eq!(s1.last_dns_confirmed_anchor, Hash64::default());

        // Both thresholds met -> last confirmed advances to this anchor.
        let a2 = Hash64::from_bytes([0x33; 64]);
        let s2 = advance_dns_confirmation(
            Some(&s1),
            a2,
            600,
            BlueWorkType::from_u64(2000),
            StakeScore(STAKE_SCORE_SCALE),
            stage,
            vsc,
            cw,
            cs,
        );
        assert_eq!(s2.last_dns_confirmed_anchor, a2);
        assert_eq!(s2.last_dns_confirmed_anchor_daa_score, 600);

        // Next anchor not confirmed -> carries s2's confirmed anchor forward.
        let a3 = Hash64::from_bytes([0x44; 64]);
        let s3 = advance_dns_confirmation(Some(&s2), a3, 700, BlueWorkType::from_u64(2000), StakeScore(0), stage, vsc, cw, cs);
        assert_eq!(s3.selected_chain_anchor, a3);
        assert_eq!(s3.last_dns_confirmed_anchor, a2);
        assert_eq!(s3.last_dns_confirmed_anchor_daa_score, 600);
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
        };
        // Both thresholds met -> pow + dns confirmed.
        let c = dns_confirmation_from_state(&state, BlueWorkType::from_u64(1000), StakeScore(STAKE_SCORE_SCALE / 2));
        assert_eq!(c.block_hash, state.selected_chain_anchor);
        assert!(c.pow_confirmed && c.dns_confirmed);
        assert_eq!(c.rollout_stage, DnsRolloutStage::Active);
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
            // ADR-0012 sortition parameters.
            sortition_mode: SortitionMode::CommitReveal,
            commit_window_blocks: 200, // = epoch_length / 3
            reveal_window_blocks: 200,
            min_reveal_threshold_num: 2,
            min_reveal_threshold_denom: 3,
            committee_size: 64,
            commit_reveal_lookahead_epochs: 2,
            commit_without_reveal_slash_sompi: 50_000_000_000,
            unreveal_reporter_reward_sompi: 100_000_000,
            commit_reveal_activation_daa_score: None, // mainnet: always CommitReveal
            reward_uniqueness_window_blocks: 3_600,   // ~6 epochs (epoch_length 600)
            reward_params: RewardParams {
                per_attestation_reward_sompi: 100_000_000,
                slashing_reporter_reward_bps: 1000,
                max_validator_inflation_per_block_sompi: 100_000_000 * MAX_ATTESTATIONS_PER_SHARD as u64,
            },
        };
        let bytes = borsh::to_vec(&params).unwrap();
        let back: DnsParams = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, params);

        // ADR-0009 §"Long-range bound" requires U >= R + E.
        assert!(params.unbonding_period_blocks >= params.max_reorg_horizon_blocks + params.evidence_window_blocks);

        // ADR-0012: commit + reveal windows must each fit comfortably
        // inside an epoch (the windows live in E−2 and E−1
        // respectively).
        assert!(params.commit_window_blocks < params.epoch_length_blocks);
        assert!(params.reveal_window_blocks < params.epoch_length_blocks);
        // Lookahead must be ≥ 2 (one full epoch each for commit and
        // reveal to land + finalise before seeding sortition).
        assert!(params.commit_reveal_lookahead_epochs >= 2);
        // Threshold sanity: 0 < num < denom.
        assert!(params.min_reveal_threshold_num > 0);
        assert!(params.min_reveal_threshold_num < params.min_reveal_threshold_denom);
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
            unbonding_period_blocks: 100_000,
            owner_reward_spk_payload: [0xddu8; 32],
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
        let pubkey = [0x42u8; 1952]; // MLDSA65_PK_LEN-sized sample
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
        let sig = [0x7eu8; 3309]; // MLDSA65_SIG_LEN-sized sample
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
        let with_tx_key = inputs(b"kaspa-pq-v1/tx/mldsa65");
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
        // ML-DSA-65 is hedged by default (FIPS 204 §3.4); two
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

    // ---- Sortition (ADR-0012) -------------------------------------

    fn fixture_reveal(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn fixture_validator(id_byte: u8, stake: u64) -> ValidatorRecord {
        ValidatorRecord { validator_id: Hash64::from_bytes([id_byte; 64]), stake_amount: stake, activation_daa_score: 0 }
    }

    #[test]
    fn sortition_mode_default_is_deterministic() {
        // A node booted with no DNS params configured behaves as
        // the simnet does — `Deterministic`. Mainnet flips this
        // via `DnsParams` parsing.
        assert_eq!(SortitionMode::default(), SortitionMode::Deterministic);
    }

    #[test]
    fn sortition_mode_borsh_roundtrip() {
        for m in [SortitionMode::Deterministic, SortitionMode::CommitReveal] {
            let bytes = borsh::to_vec(&m).unwrap();
            let back: SortitionMode = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, m);
            assert_eq!(bytes.len(), 1, "SortitionMode must encode as a single byte");
        }
        assert_eq!(SortitionMode::Deterministic as u8, 0);
        assert_eq!(SortitionMode::CommitReveal as u8, 1);
    }

    #[test]
    fn sortition_commit_payload_borsh_roundtrip() {
        let p = SortitionCommitPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: Hash64::from_bytes([0x42u8; 64]),
            target_epoch: 99,
            commit: Hash64::from_bytes([0xbbu8; 64]),
        };
        let bytes = borsh::to_vec(&p).unwrap();
        let back: SortitionCommitPayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn sortition_reveal_payload_borsh_roundtrip() {
        let p = SortitionRevealPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: Hash64::from_bytes([0x42u8; 64]),
            target_epoch: 99,
            reveal: fixture_reveal(0xcd),
        };
        let bytes = borsh::to_vec(&p).unwrap();
        let back: SortitionRevealPayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn unreveal_slashing_evidence_payload_borsh_roundtrip() {
        let p = UnrevealSlashingEvidencePayload {
            version: DNS_PAYLOAD_VERSION_V1,
            target_epoch: 7,
            validator_id: Hash64::from_bytes([0x42u8; 64]),
            commit_outpoint: fixture_outpoint(),
        };
        let bytes = borsh::to_vec(&p).unwrap();
        let back: UnrevealSlashingEvidencePayload = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }

    // ---- compute_commit -------------------------------------------

    #[test]
    fn compute_commit_is_deterministic() {
        let r = fixture_reveal(0x11);
        let vid = Hash64::from_bytes([0x42u8; 64]);
        let a = compute_commit(&r, 7, vid);
        let b = compute_commit(&r, 7, vid);
        assert_eq!(a, b);
    }

    #[test]
    fn compute_commit_changes_with_each_input() {
        let r = fixture_reveal(0x11);
        let vid = Hash64::from_bytes([0x42u8; 64]);
        let base = compute_commit(&r, 7, vid);

        // reveal differs
        assert_ne!(base, compute_commit(&fixture_reveal(0x12), 7, vid));
        // epoch differs
        assert_ne!(base, compute_commit(&r, 8, vid));
        // validator_id differs
        assert_ne!(base, compute_commit(&r, 7, Hash64::from_bytes([0x43u8; 64])));
    }

    #[test]
    fn compute_commit_round_trips_against_reveal_verification() {
        // The consensus rule in PR-10.9 re-derives compute_commit
        // from the reveal payload at tx-validation time and rejects
        // any reveal whose recomputed hash does not match the prior
        // commit. This test pins the inverse direction (compute →
        // verify) so a future helper refactor cannot silently
        // change the verification semantics.
        let r = fixture_reveal(0xaa);
        let vid = Hash64::from_bytes([0xbcu8; 64]);
        let epoch = 1234;
        let commit = compute_commit(&r, epoch, vid);
        assert_eq!(commit, compute_commit(&r, epoch, vid));
    }

    // ---- derive_epoch_seed_deterministic --------------------------

    #[test]
    fn deterministic_epoch_seed_is_per_epoch() {
        let s0 = derive_epoch_seed_deterministic(0);
        let s1 = derive_epoch_seed_deterministic(1);
        let s2 = derive_epoch_seed_deterministic(2);
        assert_ne!(s0, s1);
        assert_ne!(s1, s2);
        assert_ne!(s0, s2);
        // Must never collapse to ZERO_HASH64 (would be a hash break).
        assert_ne!(s0, kaspa_hashes::ZERO_HASH64);
    }

    // ---- derive_epoch_seed_commit_reveal --------------------------

    fn fixture_reveal_payload(vid_byte: u8, epoch: u64, r_byte: u8) -> SortitionRevealPayload {
        SortitionRevealPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: Hash64::from_bytes([vid_byte; 64]),
            target_epoch: epoch,
            reveal: fixture_reveal(r_byte),
        }
    }

    #[test]
    fn commit_reveal_seed_uses_reveal_set_when_threshold_met() {
        // 3 of 3 reveals — well above the 2/3 threshold.
        let reveals =
            vec![fixture_reveal_payload(0xaa, 5, 0x01), fixture_reveal_payload(0xbb, 5, 0x02), fixture_reveal_payload(0xcc, 5, 0x03)];
        let seed = derive_epoch_seed_commit_reveal(5, &reveals, 3, 2, 3, Hash64::from_bytes([0xeeu8; 64]));
        // Must not be the fallback seed for the same prev_epoch_seed.
        let fallback = derive_epoch_seed_fallback(5, Hash64::from_bytes([0xeeu8; 64]));
        assert_ne!(seed, fallback);
    }

    #[test]
    fn commit_reveal_seed_falls_back_when_threshold_not_met() {
        // 1 of 3 reveals — below the 2/3 threshold (1*3 < 3*2).
        let reveals = vec![fixture_reveal_payload(0xaa, 5, 0x01)];
        let prev = Hash64::from_bytes([0xeeu8; 64]);
        let seed = derive_epoch_seed_commit_reveal(5, &reveals, 3, 2, 3, prev);
        assert_eq!(seed, derive_epoch_seed_fallback(5, prev));
    }

    #[test]
    fn commit_reveal_seed_at_exact_two_thirds_threshold_uses_reveal_set() {
        // 2 of 3 reveals — at the 2/3 threshold (2*3 == 3*2). The
        // rule is "lhs >= rhs" so this case uses the reveal set,
        // not the fallback. Pins the boundary against off-by-one
        // drift.
        let reveals = vec![fixture_reveal_payload(0xaa, 5, 0x01), fixture_reveal_payload(0xbb, 5, 0x02)];
        let prev = Hash64::from_bytes([0xeeu8; 64]);
        let seed = derive_epoch_seed_commit_reveal(5, &reveals, 3, 2, 3, prev);
        assert_ne!(seed, derive_epoch_seed_fallback(5, prev));
    }

    #[test]
    fn commit_reveal_seed_is_order_independent() {
        // Same reveal set, different input orders -> same seed
        // (the helper sorts a clone by validator_id ascending).
        let a =
            vec![fixture_reveal_payload(0xaa, 5, 0x01), fixture_reveal_payload(0xbb, 5, 0x02), fixture_reveal_payload(0xcc, 5, 0x03)];
        let mut b = a.clone();
        b.reverse();
        let mut c = a.clone();
        c.swap(0, 2);

        let prev = Hash64::from_bytes([0xeeu8; 64]);
        let sa = derive_epoch_seed_commit_reveal(5, &a, 3, 2, 3, prev);
        let sb = derive_epoch_seed_commit_reveal(5, &b, 3, 2, 3, prev);
        let sc = derive_epoch_seed_commit_reveal(5, &c, 3, 2, 3, prev);
        assert_eq!(sa, sb);
        assert_eq!(sa, sc);
    }

    #[test]
    fn fallback_seed_distinct_from_primary_for_same_inputs() {
        // The primary `SORTITION_SEED_KEY` and fallback
        // `SORTITION_FALLBACK_KEY` must produce different outputs
        // for identical input — pinned here so a future ADR cannot
        // accidentally collapse the two domains.
        let prev = Hash64::from_bytes([0xeeu8; 64]);
        let target_epoch = 5u64;

        // Build the same input bytes both functions would consume
        // for a degenerate "empty reveal set" case to make the
        // domain-key the only differentiator. Reveals empty +
        // num_commits = 0 satisfies the threshold check trivially
        // (0 ≥ 0), so the primary path runs.
        let primary = derive_epoch_seed_commit_reveal(target_epoch, &[], 0, 2, 3, prev);
        let fallback = derive_epoch_seed_fallback(target_epoch, prev);
        // Different input layouts (primary hashes epoch || count ||
        // empty; fallback hashes prev || epoch), so we are
        // verifying that the FUNCTIONS produce different outputs —
        // a strong "no collision" check rather than a
        // same-input-different-key check.
        assert_ne!(primary, fallback);
    }

    // ---- compute_validator_priority -------------------------------

    #[test]
    fn priority_is_deterministic() {
        let seed = Hash64::from_bytes([0x11u8; 64]);
        let vid = Hash64::from_bytes([0x42u8; 64]);
        assert_eq!(compute_validator_priority(seed, vid, 100), compute_validator_priority(seed, vid, 100));
    }

    #[test]
    fn priority_inversely_proportional_to_stake() {
        // For a fixed (seed, validator_id), priority decreases with
        // stake. Use division composition (which IS exact under
        // integer arithmetic) rather than multiplication (which
        // loses the low bits of `h` for odd values).
        let seed = Hash64::from_bytes([0x11u8; 64]);
        let vid = Hash64::from_bytes([0x42u8; 64]);
        let p1 = compute_validator_priority(seed, vid, 1);
        let p2 = compute_validator_priority(seed, vid, 2);
        let p100 = compute_validator_priority(seed, vid, 100);
        // Monotone decreasing in stake.
        assert!(p2 < p1, "p2={p2} should be < p1={p1}");
        assert!(p100 < p2, "p100={p100} should be < p2={p2}");
        // Exact under integer division composition: priority(h, s2) ==
        // priority(h, 1) / s2 for any positive s2 (because
        // priority(h, 1) == h and (h) / s2 is itself the priority).
        assert_eq!(p2, p1 / 2);
        assert_eq!(p100, p1 / 100);
    }

    #[test]
    fn priority_changes_with_seed() {
        let vid = Hash64::from_bytes([0x42u8; 64]);
        let p1 = compute_validator_priority(Hash64::from_bytes([0x11u8; 64]), vid, 100);
        let p2 = compute_validator_priority(Hash64::from_bytes([0x12u8; 64]), vid, 100);
        // Cryptographically very unlikely to be equal at u128
        // width — if this trips by chance the universe owes
        // us a pizza.
        assert_ne!(p1, p2);
    }

    #[test]
    fn priority_changes_with_validator_id() {
        let seed = Hash64::from_bytes([0x11u8; 64]);
        let p1 = compute_validator_priority(seed, Hash64::from_bytes([0x42u8; 64]), 100);
        let p2 = compute_validator_priority(seed, Hash64::from_bytes([0x43u8; 64]), 100);
        assert_ne!(p1, p2);
    }

    #[test]
    fn priority_zero_stake_treated_as_one_stake() {
        // Defensive `.max(1)` guard — zero-stake records should
        // never appear in an active set, but the helper must not
        // panic on them.
        let seed = Hash64::from_bytes([0x11u8; 64]);
        let vid = Hash64::from_bytes([0x42u8; 64]);
        assert_eq!(compute_validator_priority(seed, vid, 0), compute_validator_priority(seed, vid, 1));
    }

    // ---- select_committee -----------------------------------------

    #[test]
    fn committee_respects_size_bound() {
        let seed = Hash64::from_bytes([0x11u8; 64]);
        let active: Vec<_> = (0u8..10).map(|i| fixture_validator(i, 100)).collect();
        let chosen = select_committee(seed, &active, 3);
        assert_eq!(chosen.len(), 3);
    }

    #[test]
    fn committee_capped_at_active_set_size_when_size_exceeds() {
        let seed = Hash64::from_bytes([0x11u8; 64]);
        let active: Vec<_> = (0u8..3).map(|i| fixture_validator(i, 100)).collect();
        // Ask for 10 from a 3-validator active set; should get 3.
        let chosen = select_committee(seed, &active, 10);
        assert_eq!(chosen.len(), 3);
    }

    #[test]
    fn committee_returned_sorted_by_validator_id() {
        let seed = Hash64::from_bytes([0x11u8; 64]);
        let active: Vec<_> = (0u8..10).map(|i| fixture_validator(i, 100)).collect();
        let chosen = select_committee(seed, &active, 5);
        // Canonical sort ascending — required so the result feeds
        // directly into validator_set_commitment without an extra
        // sort step.
        let mut sorted = chosen.clone();
        sorted.sort();
        assert_eq!(chosen, sorted);
    }

    #[test]
    fn select_committee_for_epoch_deterministic_vs_commit_reveal() {
        let active: Vec<_> = (0u8..10).map(|i| fixture_validator(i, 100)).collect();
        // Deterministic: Some, and equal to the explicit-seed path for this epoch.
        let det = select_committee_for_epoch(7, SortitionMode::Deterministic, &active, 4);
        assert_eq!(det, Some(select_committee(derive_epoch_seed_deterministic(7), &active, 4)));
        // CommitReveal: seed needs revealed commits (later slice) → None.
        assert!(select_committee_for_epoch(7, SortitionMode::CommitReveal, &active, 4).is_none());
    }

    #[test]
    fn committee_skips_zero_stake_records() {
        let seed = Hash64::from_bytes([0x11u8; 64]);
        let active = vec![fixture_validator(0x01, 0), fixture_validator(0x02, 100), fixture_validator(0x03, 0)];
        let chosen = select_committee(seed, &active, 10);
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0], Hash64::from_bytes([0x02u8; 64]));
    }

    #[test]
    fn committee_stake_weighted_in_expectation_over_many_seeds() {
        // Across many seeds, a validator with 10× stake should be
        // selected substantially more often than a peer with 1×
        // stake into a 1-slot committee.
        //
        // Method: 1024 distinct seeds, 5 validators (1 with 10× stake,
        // 4 with 1× stake), committee_size = 1. The heavy validator
        // should win the slot the vast majority of the time
        // (uniform expectation under perfect stake-weighting is
        // 10/14 ≈ 71%; we assert "> 50%" to keep the test stable
        // against statistical noise).
        let heavy = fixture_validator(0x01, 1000);
        let light_a = fixture_validator(0x02, 100);
        let light_b = fixture_validator(0x03, 100);
        let light_c = fixture_validator(0x04, 100);
        let light_d = fixture_validator(0x05, 100);
        let active = vec![heavy.clone(), light_a, light_b, light_c, light_d];

        let mut heavy_wins = 0u32;
        for s in 0u32..1024 {
            let seed_bytes: [u8; 64] = {
                let mut b = [0u8; 64];
                b[..4].copy_from_slice(&s.to_le_bytes());
                b
            };
            let chosen = select_committee(Hash64::from_bytes(seed_bytes), &active, 1);
            if chosen.len() == 1 && chosen[0] == heavy.validator_id {
                heavy_wins += 1;
            }
        }
        // 10×-stake validator must take > 50% of single-slot
        // committees across 1024 trials. Loose bound — the
        // true expectation is ~71% — picked so the test stays
        // stable across hasher-output randomness.
        assert!(heavy_wins > 512, "10×-stake validator only took {heavy_wins}/1024 single-slot committees");
    }

    // ---- Reward / slashing distribution (ADR-0013) ----------------

    #[test]
    fn reward_params_borsh_roundtrip() {
        let p = RewardParams {
            per_attestation_reward_sompi: 100_000,
            slashing_reporter_reward_bps: 1000, // 10%
            max_validator_inflation_per_block_sompi: 5_000_000_000,
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

    #[test]
    fn slashing_distribution_borsh_roundtrip() {
        let s = SlashingDistribution { reporter_reward_sompi: 100_000, burned_sompi: 900_000 };
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
        // helper does not panic.
        // (The actual refund value would be u128::MAX minus
        // 1_000_000 which exceeds u64; the cast clamps.)
        assert!(r.refunded_sompi <= u64::MAX);
    }

    // ---- p2pkh_mldsa65_spk + validator_reward_outputs -------------

    #[test]
    fn p2pkh_mldsa65_spk_byte_layout() {
        // Pins the exact 37-byte ADR-0002 script + spk version 0.
        let payload = [0x11u8; 32];
        let spk = p2pkh_mldsa65_spk(&payload);
        assert_eq!(spk.version(), 0);
        let script = spk.script();
        assert_eq!(script.len(), 37);
        assert_eq!(script[0], 0x76, "OpDup");
        assert_eq!(script[1], 0xaa, "OpBlake2b");
        assert_eq!(script[2], 0x20, "OpData32");
        assert_eq!(&script[3..35], &payload, "32-byte payload");
        assert_eq!(script[35], 0x88, "OpEqualVerify");
        assert_eq!(script[36], 0xa6, "OpCheckSigMlDsa65");
    }

    #[test]
    fn p2pkh_mldsa65_spk_distinct_payloads_distinct_scripts() {
        // The only varying region is script[3..35]; distinct payloads
        // must yield distinct scripts (no accidental collision).
        let a = p2pkh_mldsa65_spk(&[0x01u8; 32]);
        let b = p2pkh_mldsa65_spk(&[0x02u8; 32]);
        assert_ne!(a, b);
        // Same payload → identical script (deterministic).
        assert_eq!(p2pkh_mldsa65_spk(&[0x01u8; 32]), a);
    }

    #[test]
    fn validator_reward_outputs_one_per_attestation() {
        let reward = 1_000_000u64;
        let cap = 100_000_000u64; // far above reward × count
        let payloads = [[0x01u8; 32], [0x02u8; 32], [0x03u8; 32]];
        let outs = validator_reward_outputs(reward, cap, &payloads);
        assert_eq!(outs.len(), payloads.len());
        for (out, p) in outs.iter().zip(payloads.iter()) {
            assert_eq!(out.value, reward);
            assert_eq!(out.script_public_key, p2pkh_mldsa65_spk(p));
        }
    }

    #[test]
    fn validator_reward_outputs_preserves_canonical_order() {
        // Outputs must follow the caller's supplied order verbatim.
        let payloads = [[0x0au8; 32], [0x0bu8; 32], [0x0cu8; 32]];
        let outs = validator_reward_outputs(10, 1_000, &payloads);
        let got: Vec<_> = outs.iter().map(|o| o.script_public_key.clone()).collect();
        let want: Vec<_> = payloads.iter().map(|p| p2pkh_mldsa65_spk(p)).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn validator_reward_outputs_whole_output_cap_truncates_tail() {
        // cap = 25, reward = 10 → only 2 whole outputs (20), never a
        // partial third. Tail (3rd payload) is dropped.
        let reward = 10u64;
        let cap = 25u64;
        let payloads = [[0x01u8; 32], [0x02u8; 32], [0x03u8; 32]];
        let outs = validator_reward_outputs(reward, cap, &payloads);
        assert_eq!(outs.len(), 2);
        assert_eq!(outs.iter().map(|o| o.value).sum::<u64>(), 20);
        // The two emitted outputs are the canonical-order head.
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa65_spk(&payloads[0]));
        assert_eq!(outs[1].script_public_key, p2pkh_mldsa65_spk(&payloads[1]));
    }

    #[test]
    fn validator_reward_outputs_empty_when_reward_zero() {
        // reward = 0 → no validator-side outflow regardless of count.
        let payloads = [[0x01u8; 32], [0x02u8; 32]];
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
        let payloads = [[0x07u8; 32], [0x07u8; 32]];
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
        let atts = [(op_n(1), 5u64, [0x01u8; 32]), (op_n(2), 5u64, [0x02u8; 32]), (op_n(3), 7u64, [0x03u8; 32])];
        let (outs, keys) = validator_reward_outputs_from_attestations(reward, 10_000, &atts, &RewardedEpochSet::new());
        assert_eq!(outs.len(), 3);
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa65_spk(&[0x01u8; 32]));
        assert_eq!(outs[2].script_public_key, p2pkh_mldsa65_spk(&[0x03u8; 32]));
        assert_eq!(keys, vec![(op_n(1), 5), (op_n(2), 5), (op_n(3), 7)]);
    }

    #[test]
    fn reward_from_attestations_dedups_same_bond_epoch_first_wins() {
        // Same (bond, epoch) twice → one reward; the FIRST occurrence's
        // payload is used (canonical order preserved).
        let atts = [(op_n(1), 5u64, [0xaau8; 32]), (op_n(1), 5u64, [0xbbu8; 32])];
        let (outs, keys) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &RewardedEpochSet::new());
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa65_spk(&[0xaau8; 32]));
        assert_eq!(keys, vec![(op_n(1), 5)]);
    }

    #[test]
    fn reward_from_attestations_same_bond_distinct_epochs_both_paid() {
        // ADR-0013: a validator with attestations across two epochs in one
        // block earns for each — dedup is per (bond, epoch), not per bond.
        let atts = [(op_n(1), 5u64, [0xaau8; 32]), (op_n(1), 6u64, [0xaau8; 32])];
        let (outs, _) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &RewardedEpochSet::new());
        assert_eq!(outs.len(), 2);
    }

    #[test]
    fn reward_from_attestations_cap_applies_after_dedup() {
        // 3 distinct (after dedup) but cap allows only 2 whole rewards.
        let atts = [
            (op_n(1), 1u64, [0x01u8; 32]),
            (op_n(1), 1u64, [0x01u8; 32]),
            (op_n(2), 1u64, [0x02u8; 32]),
            (op_n(3), 1u64, [0x03u8; 32]),
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
        let atts = [(op_n(1), 5u64, [0x01u8; 32]), (op_n(2), 5u64, [0x02u8; 32])];
        let (outs, keys) = validator_reward_outputs_from_attestations(100, 10_000, &atts, &prefix);
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].script_public_key, p2pkh_mldsa65_spk(&[0x02u8; 32]));
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
                let d = compute_slashing_distribution(slashed, bps);
                assert_eq!(d.reporter_reward_sompi + d.burned_sompi, slashed, "slashed={slashed} bps={bps}");
            }
        }
    }

    #[test]
    fn slashing_distribution_zero_bps_burns_everything() {
        let d = compute_slashing_distribution(1_000_000_000, 0);
        assert_eq!(d.reporter_reward_sompi, 0);
        assert_eq!(d.burned_sompi, 1_000_000_000);
    }

    #[test]
    fn slashing_distribution_full_bps_burns_nothing() {
        let d = compute_slashing_distribution(1_000_000_000, 10000);
        assert_eq!(d.reporter_reward_sompi, 1_000_000_000);
        assert_eq!(d.burned_sompi, 0);
    }

    #[test]
    fn slashing_distribution_mainnet_10pct_recommendation() {
        // ADR-0013 §"Slashing distribution" mainnet recommendation:
        // 1000 bps = 10% to reporter, 90% burned.
        let d = compute_slashing_distribution(100_000_000_000, 1000);
        assert_eq!(d.reporter_reward_sompi, 10_000_000_000);
        assert_eq!(d.burned_sompi, 90_000_000_000);
    }

    #[test]
    fn slashing_distribution_no_overflow_at_u64_max() {
        // u64::MAX × 10000 would overflow u64; the helper promotes
        // to u128 internally so it cannot. Pin this with the
        // largest plausible slashed amount (full u64 supply).
        let d = compute_slashing_distribution(u64::MAX, 10000);
        assert_eq!(d.reporter_reward_sompi, u64::MAX);
        assert_eq!(d.burned_sompi, 0);
    }

    // ---- apply_unreveal_reporter_min_cap --------------------------

    #[test]
    fn unreveal_min_cap_clamps_when_bps_reward_exceeds_floor() {
        // bps-derived reporter = 1_000_000 (10% of 10_000_000),
        // floor = 500_000. After cap: reporter = 500_000, burned
        // grows by 500_000.
        let base = compute_slashing_distribution(10_000_000, 1000);
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
        let base = compute_slashing_distribution(10_000, 1000);
        let capped = apply_unreveal_reporter_min_cap(base, 500_000);
        assert_eq!(capped, base);
    }

    #[test]
    fn unreveal_min_cap_at_exact_floor_is_noop() {
        // bps-derived reporter == floor. No clamp applies.
        let base = compute_slashing_distribution(5_000_000, 1000); // → reporter = 500_000
        assert_eq!(base.reporter_reward_sompi, 500_000);
        let capped = apply_unreveal_reporter_min_cap(base, 500_000);
        assert_eq!(capped, base);
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
        // Sanity: the dominant size component is the 3309-byte
        // ML-DSA-65 signature.
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
    }

    #[test]
    fn signing_purpose_default_is_transaction() {
        // Conservative default — `Transaction` is the original
        // ML-DSA-65 use site (ADR-0002), pre-DNS-overlay.
        assert_eq!(SigningPurpose::default(), SigningPurpose::Transaction);
    }

    #[test]
    fn signing_purpose_borsh_roundtrip() {
        for p in [SigningPurpose::Transaction, SigningPurpose::Attestation, SigningPurpose::TakeoverToken] {
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
            context: ATTESTATION_MLDSA65_CONTEXT.to_vec(),
            message_digest: Hash::from_bytes([0xcdu8; 32]),
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
            message_digest: Hash::from_bytes([0xcdu8; 32]),
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
