//! kaspa-pq Phase 10 (PR-10.3): DNS Probabilistic Finality Overlay
//! type stubs.
//!
//! See [ADR-0009](../../docs/adr/0009-dns-probabilistic-finality.md)
//! for the full design. This module carries the **type surface only**
//! that Phase 10 follow-up PRs will reference:
//!
//! - `StakeBondPayload`              — bonds coins to a validator
//!   ML-DSA-65 key (transaction payload).
//! - `StakeAttestation`              — one validator-signed attestation
//!   over a selected-chain anchor.
//! - `StakeAttestationShardPayload`  — bounded batch of attestations,
//!   committed as a transaction payload (8–16 per block).
//! - `SlashingEvidencePayload`       — incompatible-attestation
//!   evidence carrier (transaction payload).
//! - `DnsParams`                     — per-network consensus parameters.
//! - `DnsConfirmation`               — RPC view type returned by the
//!   confirmation API.
//!
//! Consensus rule and aggregation logic is **not** in this module.
//! `check_dns_reorg_rule`, `compute_stake_score`, sortition, and the
//! activation-phase gate land in subsequent PRs (10.4 — 10.9) once
//! Phases 1–9 stabilise. Calls into the un-implemented helpers
//! `unimplemented!()` so the missing surface is loud rather than
//! silently-zero.
//!
//! Hash widths follow ADR-0008: selected-chain anchor identifiers are
//! [`Hash64`]; the validator short-id is a 32-byte BLAKE2b-256 digest
//! (the upstream `Hash` / `Hash32` alias).
//!
//! All payload types derive `BorshSerialize` / `BorshDeserialize` so
//! they round-trip through the existing wRPC Borsh path; `serde` JSON
//! is added via manual impls in the consumer-facing RPC types only.

use std::fmt::{self, Display, Formatter};

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};

use crate::{BlueWorkType, tx::TransactionOutpoint};

/// 1952 bytes — matches `kaspa_txscript::MLDSA65_PK_LEN`. Repeated
/// here so this module does not have to depend on `kaspa-txscript`;
/// asserted-equal by [`tests::dns_constants_match_txscript`].
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
/// the attestation message that ML-DSA-65 signs over. See
/// ADR-0009 §"Attestation target".
pub const ATTESTATION_MESSAGE_DOMAIN: &[u8] = b"kaspa-pq-v1/stake-attestation";

/// Fixed-point scaled stake score. Wrapper for documentation /
/// arithmetic clarity; the underlying `u128` is the same number of
/// "stake-score units" used throughout the overlay.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
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
/// The validator never appears explicitly in tx payloads; it is a
/// view of network state used by the RPC layer and by node-internal
/// activation gating. Persisted as a single byte.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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

    /// `BLAKE2b-256(owner_public_key)` — same 32-byte payload shape
    /// used by `Version::PubKeyHashMlDsa65` addresses today. After
    /// the PR-9.5 widening this will become a Hash64.
    pub owner_pubkey_hash: Hash,
    /// `BLAKE2b-256(validator_public_key)`.
    pub validator_pubkey_hash: Hash,

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

    /// Short 32-byte validator identifier. Conventionally equal to
    /// the `validator_pubkey_hash` from the corresponding `StakeBond`.
    pub validator_id: Hash,

    /// Refers to the transaction outpoint that created the bond. The
    /// outpoint's txid widens to `Hash64` in the PR-9.5 cascade.
    pub bond_outpoint: TransactionOutpoint,

    /// `daa_score / epoch_length_blocks`.
    pub epoch: u64,

    /// Selected-chain anchor this attestation approves. ADR-0008
    /// `Hash64`.
    pub target_hash: Hash64,

    /// `daa_score` of the anchor; redundant with `target_hash` but
    /// included so an attestation can be partially-verified without a
    /// header lookup.
    pub target_daa_score: u64,

    /// Hash64 of the committee snapshot the attestation is bound to.
    /// Lets a verifier reject attestations issued under a stale
    /// validator set.
    pub validator_set_commitment: Hash64,

    /// 3309-byte ML-DSA-65 signature over the BLAKE2b-256
    /// attestation message (see ADR-0009 §"Attestation target") with
    /// `ATTESTATION_MLDSA65_CONTEXT` as the libcrux `ctx` parameter.
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

/// Per-network DNS consensus parameters. Stored alongside the
/// existing `consensus/core::config::params::Params` and consumed by
/// the PR-10.5 / PR-10.6 / PR-10.7 / PR-10.8 implementations.
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
}

/// RPC view returned by the `getDnsConfirmation` method (added in
/// PR-10.9). Surfaces both the PoW-only confirmation level and the
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

// ---------------------------------------------------------------------
// Consensus rule stubs — implementations land in subsequent PRs.
// ---------------------------------------------------------------------

/// PR-10.5 stub: deterministic `StakeScore` aggregation from on-chain
/// `StakeAttestationShardPayload` data.
#[doc(hidden)]
pub fn compute_stake_score_stub() -> StakeScore {
    unimplemented!(
        "kaspa-pq Phase 10 PR-10.5: deterministic StakeScore aggregation \
         from on-chain shards is not implemented in this PR (type stubs only); \
         see docs/adr/0009-dns-probabilistic-finality.md §StakeScore mechanics."
    )
}

/// PR-10.7 stub: the two-dimensional dominance rule (mainnet
/// behaviour). Returns `Ok(())` if the candidate either does not
/// touch the latest DNS-confirmed anchor, or beats the canonical
/// chain on both `WorkScore` and `StakeScore` by the configured
/// emergency margins.
#[doc(hidden)]
pub fn check_dns_reorg_rule_stub() -> Result<(), &'static str> {
    unimplemented!(
        "kaspa-pq Phase 10 PR-10.7: two-dimensional dominance rule is not \
         implemented in this PR (type stubs only); see \
         docs/adr/0009-dns-probabilistic-finality.md §Decision."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn stake_score_display() {
        assert_eq!(StakeScore(0).to_string(), "0.000000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE).to_string(), "1.000000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE + 500_000_000).to_string(), "1.500000000");
        assert_eq!(StakeScore(STAKE_SCORE_SCALE * 3 / 4).to_string(), "0.750000000");
    }

    fn fixture_outpoint() -> TransactionOutpoint {
        TransactionOutpoint::new(Hash::from_bytes([0x77u8; 32]), 42)
    }

    fn fixture_attestation() -> StakeAttestation {
        StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: Hash::from_bytes([0xa5u8; 32]),
            bond_outpoint: fixture_outpoint(),
            epoch: 7,
            target_hash: Hash64::from_bytes([0x11u8; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::from_bytes([0x22u8; 64]),
            signature: vec![0x33u8; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    #[test]
    fn stake_bond_payload_borsh_roundtrip() {
        let bond = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: Hash::from_bytes([0xaau8; 32]),
            validator_pubkey_hash: Hash::from_bytes([0xbbu8; 32]),
            validator_pubkey: vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN],
            amount: 100_000_000_000,
            activation_daa_score: 5_000,
            unbonding_period_blocks: 100_000,
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
        };
        let bytes = borsh::to_vec(&params).unwrap();
        let back: DnsParams = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, params);

        // ADR-0009 §"Long-range bound" requires U >= R + E.
        assert!(params.unbonding_period_blocks >= params.max_reorg_horizon_blocks + params.evidence_window_blocks);
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
    #[should_panic(expected = "kaspa-pq Phase 10 PR-10.5")]
    fn compute_stake_score_is_explicitly_unimplemented() {
        let _ = compute_stake_score_stub();
    }

    #[test]
    #[should_panic(expected = "kaspa-pq Phase 10 PR-10.7")]
    fn check_dns_reorg_rule_is_explicitly_unimplemented() {
        let _ = check_dns_reorg_rule_stub();
    }
}
