//! **RFC-0009 stage C — public receipt redemption** (`Params::palw_receipt_spend_v4`; dormant on every preset but testnet-12 as shipped,
//! which arms it with the int-13 list at DAA 9,000 — `PALW_T12_INT13_FENCES_V1`).
//!
//! Today a free-prompt `Final` claim's winning quantum is spent into a receipt BLOCK, and the block's producer must be the
//! claim's executor (`ProducerNotExecutor`): the miner's PC has to stay on after the claim to collect the receipt reward. This
//! module separates the two parties without touching V3:
//!
//! ```text
//!   executor (miner)                       builder (any bonded block producer)
//!   ─────────────────                      ──────────────────────────────────
//!   signs ONCE, position-free:             builds the receipt block, signs its header position:
//!     PalwRedemptionAuthV4                   PalwReceiptSpendUnsignedV4 { claim, quantum, beacon, executor_bond,
//!       claim, executor bond,                   builder_bond, builder key, challenge, authorization, executor key,
//!       quantum range [lo, hi),                 authorization signature }  +  builder signature
//!       beacon RULE (not a value),
//!       builder fee bps, expiry
//! ```
//!
//! **Why the authorization signs the beacon RULE and a quantum range, not a beacon value** (RFC §5.1's open choice, decided here).
//! The draw beacon exists only after `Final + receipt_maturity` — after the miner's PC may be off. An authorization over a beacon
//! value would need the miner online at redemption, which is exactly what this lane removes. The beacon is a function the chain
//! computes from the claim's `final_daa` ([`crate::palw_freeprompt_v3::fp_draw_slot_v3`]); signing "rule 1" loses nothing and
//! forecloses nothing: the builder cannot pick the beacon (the processor derives it from the candidate's own selected parent).
//!
//! **What is unchanged.** Items 1–5 and 8 of ADR-0044 Decision 6 (the claim is a certified free-prompt claim, the quantum is
//! unspent on this chain, the beacon is the claim's, the win is used in its window, the ticket beats the receipt target, the class
//! stands) are the V3 admission's own code ([`crate::palw_fp_admission_v3`]'s shared helpers). The fold is shared too: it reads only
//! `(claim_id, quantum_index)`, so a V4 spend folds through a V3-shaped view ([`PalwReceiptSpendEnvelopeV4::to_fold_envelope`]) and
//! weight, census, single use and reorg revert are byte-for-byte the V3 ones.
//!
//! **What changes.** Item 6/7 (`ProducerNotExecutor`, the producer's key) become: the authorization is the executor's, under the
//! claim's executor bond; the builder is any `Active` bond holding its key. And the coinbase pays the receipt block's
//! subsidy-derived worker reward as `miner leg → the executor bond's registered payout`, `builder fee → the block's own miner
//! script` (see [`palw_receipt_v4_split_v1`]); the total equals what V3 pays, so no issuance, panel leg, reserve or maturity moves.
//!
//! **Size.** Two ML-DSA-87 signatures and two keys are ≈ 14.9 KB, above the 8,192-byte header carriage cap. The cap is raised ONLY
//! for a `PFS4` payload ([`PALW_COMMITMENT_MAX_BYTES_V4`]); a `PFS4` header below the fence is refused by name at the header stage, so
//! below the fence the set of valid blocks is exactly what it was.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::mldsa87_primitives::{MLDSA87_PUBKEY_LEN, MLDSA87_SIGNATURE_LEN};
use crate::palw_fp_admission_v3::{PalwFpAdmissionV3Error, ReceiptSpendFacts, receipt_item_8, receipt_items_1_to_5};
use crate::palw_freeprompt_v3::{
    PALW_FP_V3_L1_TAG_BYTES, PALW_FP_V3_VERSION, PalwBeaconFactV3, PalwReceiptSpendEnvelopeV3, PalwReceiptSpendUnsignedV3,
};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2};
use crate::tx::{ScriptPublicKey, TransactionOutpoint};
use blake2b_simd::Params as Blake2bParams;

/// The object version of both [`PalwRedemptionAuthV4`] and [`PalwReceiptSpendUnsignedV4`].
pub const PALW_RECEIPT_V4_VERSION: u16 = 1;
/// The header-carriage wire magic of a V4 spend (V3's is `PFS3`): a carriage of one version can never decode as the other.
pub const PALW_RECEIPT_V4_CARRIAGE_MAGIC: [u8; 4] = *b"PFS4";
/// The header carriage cap for a `PFS4` payload. Every other PALW carriage keeps [`crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES`].
pub const PALW_COMMITMENT_MAX_BYTES_V4: usize = 16_384;
/// The most the chain lets an authorization pay a builder, in basis points of the subsidy-derived worker reward. A companion value of
/// the fence (hashed with its height), so changing it is a new network id.
pub const PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS: u16 = 1_000;
/// `beacon_rule == 1`: the ADR-0044 slot rule on the candidate chain (`fp_draw_slot_v3(final_daa, receipt_maturity)`).
pub const PALW_RECEIPT_V4_BEACON_RULE_SLOT: u8 = 1;

pub const PALW_RECEIPT_V4_DOMAIN_AUTH_ID: &[u8] = b"misaka-palw/fp-v4/redeem-auth/v1";
pub const PALW_RECEIPT_V4_DOMAIN_SPEND_ID: &[u8] = b"misaka-palw/fp-v4/spend-id/v1";
pub const PALW_RECEIPT_V4_DOMAIN_SPEND_CHALLENGE: &[u8] = b"misaka-palw/fp-v4/spend-challenge/v1";
pub const PALW_RECEIPT_V4_DOMAIN_SPEND_L1_TAG: &[u8] = b"misaka-palw/fp-v4/spend-l1-tag/v1";
/// ML-DSA-87 contexts: distinct from each other and from V3's, so no signature of one object verifies as another.
pub const PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/fp-v4/redeem-auth-mldsa87/v1";
pub const PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/fp-v4/spend-mldsa87/v1";

/// **The values the fingerprint hashes beside the fence's height** — `[builder fee cap in bps]`.
pub const fn palw_receipt_spend_v4_value_v1() -> [u64; 1] {
    [PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS as u64]
}

/// **The fence's entry for a flag-day list** (testnet-12's int-13 list, `PALW_T12_INT13_FENCES_V1`): the fence is a bare height — no
/// bundle mirror, the processor reads `Params::palw_receipt_spend_v4_active_at` — so the entry sets that one field. Its prerequisites
/// (`palw_audit_2026_09_11`, `palw_audit_2026_09_23`) are asked by [`Params::validate_palw_receipt_spend_v4`].
pub const PALW_T12_RECEIPT_SPEND_V4_ENTRY: crate::config::params::PalwPostLaunchFenceV1 = crate::config::params::PalwPostLaunchFenceV1 {
    name: "palw_receipt_spend_v4",
    set: |params, at| params.palw_receipt_spend_v4 = at,
};

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwReceiptV4Error {
    #[error("the V4 receipt carriage is undecodable: {0}")]
    Undecodable(&'static str),
    #[error("unsupported V4 receipt object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("a V4 receipt names another network's domain")]
    NetworkDomainMismatch,
    #[error("a V4 receipt key is {got} bytes, not {expected}")]
    PublicKeyLength { got: usize, expected: usize },
    #[error("a V4 receipt signature is {got} bytes, not {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("the spend challenge does not bind this header position")]
    ChallengeMismatch,
    #[error("the authorization is for another claim or executor bond than the spend names")]
    AuthorizationMismatch,
    #[error("beacon rule {0} is not a rule this chain knows (1 = the ADR-0044 slot rule)")]
    BeaconRuleUnknown(u8),
    #[error("the authorized quantum range [{lo}, {hi}) is empty")]
    QuantumRangeEmpty { lo: u32, hi: u32 },
    #[error("quantum {index} is outside the authorized range [{lo}, {hi})")]
    QuantumOutsideAuthorizedRange { index: u32, lo: u32, hi: u32 },
    #[error("the authorization offers the builder {bps} bps, above the chain's {cap} bps cap")]
    FeeAboveCap { bps: u16, cap: u16 },
    #[error("the authorization expired at DAA {expiry}; this block is at {block_daa}")]
    AuthorizationExpired { block_daa: u64, expiry: u64 },
    #[error("a V4 receipt signature does not verify")]
    SignatureInvalid,
    #[error("the claim's executor bond {0:?} does not exist at the candidate chain point")]
    ExecutorBondMissing(PalwBondKeyV2),
    #[error("the spend's executor bond is not the claim's executor bond")]
    ExecutorBondMismatch,
    #[error("the executor bond {0:?} is retiring: spend before you retire")]
    ExecutorBondRetiring(PalwBondKeyV2),
    #[error("the carried executor key is not the executor bond's registered key")]
    ExecutorKeyMismatch,
    #[error("the builder bond {0:?} does not exist at the candidate chain point")]
    BuilderBondMissing(PalwBondKeyV2),
    #[error("the builder bond {0:?} is retiring and may build no new blocks")]
    BuilderBondRetiring(PalwBondKeyV2),
    #[error("the carried builder key is not the builder bond's registered key")]
    BuilderKeyMismatch,
    #[error("a V4 receipt spend is not valid below `palw_receipt_spend_v4`")]
    BelowFence,
}

/// The executor's redemption authorization: signed once, bound to no header position and to no beacon value.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRedemptionAuthV4 {
    pub version: u16,
    pub network_domain: Hash64,
    pub claim_id: Hash64,
    pub executor_bond: TransactionOutpoint,
    /// Half-open: any quantum `lo ≤ q < hi` may be redeemed by an eligible builder.
    pub quantum_lo: u32,
    pub quantum_hi: u32,
    /// [`PALW_RECEIPT_V4_BEACON_RULE_SLOT`].
    pub beacon_rule: u8,
    /// The share of the subsidy-derived worker reward the miner pays the builder; at most [`PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS`].
    pub builder_fee_bps: u16,
    /// The block's DAA score must be at most this. `u64::MAX` = no expiry.
    pub expiry_daa: u64,
}

/// What the builder signs: its own header position plus the executor's authorization and signature, carried whole.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReceiptSpendUnsignedV4 {
    pub version: u16,
    pub network_domain: Hash64,
    /// [`spend_challenge_v4`] over the header position, the claim, the quantum and both bonds (the V3 lane's anti-free-identity rule).
    pub challenge: Hash64,
    pub claim_id: Hash64,
    pub quantum_index: u32,
    pub beacon_block: Hash64,
    pub executor_bond: TransactionOutpoint,
    pub builder_bond: TransactionOutpoint,
    pub builder_pubkey: Vec<u8>,
    pub authorization: PalwRedemptionAuthV4,
    pub executor_pubkey: Vec<u8>,
    pub authorization_signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReceiptSpendEnvelopeV4 {
    pub spend: PalwReceiptSpendUnsignedV4,
    pub builder_signature: Vec<u8>,
}

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    Blake2bParams::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn canonical_id(domain: &[u8], object_bytes: &[u8]) -> Hash64 {
    let mut state = keyed(domain);
    state.update(&(object_bytes.len() as u64).to_le_bytes());
    state.update(object_bytes);
    finish(state)
}

/// The message the executor signs for an authorization.
pub fn redeem_auth_id_v4(auth: &PalwRedemptionAuthV4) -> Hash64 {
    canonical_id(PALW_RECEIPT_V4_DOMAIN_AUTH_ID, &borsh::to_vec(auth).expect("borsh-serializable"))
}

/// `H(domain ‖ network ‖ pre_pow_hash ‖ timestamp ‖ nonce ‖ claim ‖ quantum ‖ executor bond ‖ builder bond)`.
#[allow(clippy::too_many_arguments)]
pub fn spend_challenge_v4(
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    claim_id: Hash64,
    quantum_index: u32,
    executor_bond: &TransactionOutpoint,
    builder_bond: &TransactionOutpoint,
) -> Hash64 {
    let mut state = keyed(PALW_RECEIPT_V4_DOMAIN_SPEND_CHALLENGE);
    state.update(network_domain.as_byte_slice());
    state.update(pre_pow_hash.as_byte_slice());
    state.update(&timestamp.to_le_bytes());
    state.update(&nonce.to_le_bytes());
    state.update(claim_id.as_byte_slice());
    state.update(&quantum_index.to_le_bytes());
    for bond in [executor_bond, builder_bond] {
        state.update(bond.transaction_id.as_byte_slice());
        state.update(&bond.index.to_le_bytes());
    }
    finish(state)
}

/// `H(canonical(spend))` — total over the authorization and its signature, so the header cannot swap them on a fixed digest. The
/// builder signs it, and the PoW tag expands it.
pub fn fp_spend_id_v4(spend: &PalwReceiptSpendUnsignedV4) -> Hash64 {
    canonical_id(PALW_RECEIPT_V4_DOMAIN_SPEND_ID, &borsh::to_vec(spend).expect("borsh-serializable"))
}

/// `Expand(spend_id_v4)` — the V3 tag's shape under this family's own domain.
pub fn fp_spend_l1_tag_v4(spend_id: Hash64) -> [u8; PALW_FP_V3_L1_TAG_BYTES] {
    let mut out = [0u8; PALW_FP_V3_L1_TAG_BYTES];
    for (chunk_index, chunk) in out.chunks_mut(64).enumerate() {
        let mut state = keyed(PALW_RECEIPT_V4_DOMAIN_SPEND_L1_TAG);
        state.update(spend_id.as_byte_slice());
        state.update(&(chunk_index as u32).to_le_bytes());
        chunk.copy_from_slice(&state.finalize().as_bytes()[..chunk.len()]);
    }
    out
}

/// Does this header payload claim to be a V4 spend? (The magic decides; nothing else is read.)
pub fn palw_receipt_v4_carriage_is_v4(palw_commitment: &[u8]) -> bool {
    palw_commitment.starts_with(&PALW_RECEIPT_V4_CARRIAGE_MAGIC)
}

impl PalwReceiptSpendEnvelopeV4 {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_RECEIPT_V4_CARRIAGE_MAGIC.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh serialization of a plain struct cannot fail"));
        out
    }

    /// Magic, borsh, then an exact-length check (a payload is not a container) and the V4 cap.
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwReceiptV4Error> {
        let Some(body) = bytes.strip_prefix(&PALW_RECEIPT_V4_CARRIAGE_MAGIC) else {
            return Err(PalwReceiptV4Error::Undecodable("payload does not start with the PFS4 magic"));
        };
        if bytes.len() > PALW_COMMITMENT_MAX_BYTES_V4 {
            return Err(PalwReceiptV4Error::Undecodable("payload exceeds the V4 carriage cap"));
        }
        let mut slice = body;
        let decoded =
            <Self as borsh::BorshDeserialize>::deserialize(&mut slice).map_err(|_| PalwReceiptV4Error::Undecodable("borsh body"))?;
        if !slice.is_empty() {
            return Err(PalwReceiptV4Error::Undecodable("trailing bytes"));
        }
        Ok(decoded)
    }

    /// Stateless admission: versions, network, key and signature lengths, authorization/spend agreement, the beacon rule, the range and
    /// the fee cap, and the challenge RECOMPUTED from the header position. No chain lookup, no signature verification.
    pub fn validate_stateless_v4(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        timestamp: u64,
        nonce: u64,
    ) -> Result<(), PalwReceiptV4Error> {
        let s = &self.spend;
        let a = &s.authorization;
        for got in [s.version, a.version] {
            if got != PALW_RECEIPT_V4_VERSION {
                return Err(PalwReceiptV4Error::UnsupportedVersion { got, expected: PALW_RECEIPT_V4_VERSION });
            }
        }
        if s.network_domain != network_domain || a.network_domain != network_domain {
            return Err(PalwReceiptV4Error::NetworkDomainMismatch);
        }
        for key in [&s.builder_pubkey, &s.executor_pubkey] {
            if key.len() != MLDSA87_PUBKEY_LEN {
                return Err(PalwReceiptV4Error::PublicKeyLength { got: key.len(), expected: MLDSA87_PUBKEY_LEN });
            }
        }
        for sig in [&self.builder_signature, &s.authorization_signature] {
            if sig.len() != MLDSA87_SIGNATURE_LEN {
                return Err(PalwReceiptV4Error::SignatureLength { got: sig.len(), expected: MLDSA87_SIGNATURE_LEN });
            }
        }
        if a.claim_id != s.claim_id || a.executor_bond != s.executor_bond {
            return Err(PalwReceiptV4Error::AuthorizationMismatch);
        }
        if a.beacon_rule != PALW_RECEIPT_V4_BEACON_RULE_SLOT {
            return Err(PalwReceiptV4Error::BeaconRuleUnknown(a.beacon_rule));
        }
        if a.quantum_lo >= a.quantum_hi {
            return Err(PalwReceiptV4Error::QuantumRangeEmpty { lo: a.quantum_lo, hi: a.quantum_hi });
        }
        if s.quantum_index < a.quantum_lo || s.quantum_index >= a.quantum_hi {
            return Err(PalwReceiptV4Error::QuantumOutsideAuthorizedRange {
                index: s.quantum_index,
                lo: a.quantum_lo,
                hi: a.quantum_hi,
            });
        }
        if a.builder_fee_bps > PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS {
            return Err(PalwReceiptV4Error::FeeAboveCap { bps: a.builder_fee_bps, cap: PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS });
        }
        if s.challenge
            != spend_challenge_v4(
                network_domain,
                pre_pow_hash,
                timestamp,
                nonce,
                s.claim_id,
                s.quantum_index,
                &s.executor_bond,
                &s.builder_bond,
            )
        {
            return Err(PalwReceiptV4Error::ChallengeMismatch);
        }
        Ok(())
    }

    /// Both signatures: the executor's over [`redeem_auth_id_v4`] under the carried executor key, the builder's over [`fp_spend_id_v4`]
    /// under the carried builder key. Checked on the relay path as well as in the chain walk (the carriage is inside the block identity
    /// and outside the PoW pre-image, so an unverified signature would be free bytes — one solve, unbounded distinct blocks).
    pub fn validate_signatures_v4<V>(&self, verify_mldsa87: V) -> Result<(), PalwReceiptV4Error>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        let s = &self.spend;
        let auth_msg = redeem_auth_id_v4(&s.authorization);
        if !verify_mldsa87(&s.executor_pubkey, auth_msg.as_byte_slice(), &s.authorization_signature, PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT) {
            return Err(PalwReceiptV4Error::SignatureInvalid);
        }
        let spend_msg = fp_spend_id_v4(s);
        if !verify_mldsa87(&s.builder_pubkey, spend_msg.as_byte_slice(), &self.builder_signature, PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT) {
            return Err(PalwReceiptV4Error::SignatureInvalid);
        }
        Ok(())
    }

    /// The V3-shaped view the fold, the per-mergeset quantum dedup and the weight accounting read: `(claim, quantum, executor bond)`.
    /// **Never serialized, never verified** — the signature field is empty on purpose.
    pub fn to_fold_envelope(&self) -> PalwReceiptSpendEnvelopeV3 {
        let s = &self.spend;
        PalwReceiptSpendEnvelopeV3 {
            spend: PalwReceiptSpendUnsignedV3 {
                version: PALW_FP_V3_VERSION,
                network_domain: s.network_domain,
                challenge: s.challenge,
                claim_id: s.claim_id,
                quantum_index: s.quantum_index,
                beacon_block: s.beacon_block,
                producer_bond: s.executor_bond,
                producer_pubkey: s.executor_pubkey.clone(),
            },
            signature: Vec::new(),
        }
    }
}

/// The wire magic of a stand-alone redemption authorization file/message (what a miner hands to builders): `RDA4`.
pub const PALW_REDEMPTION_AUTH_BUNDLE_MAGIC: [u8; 4] = *b"RDA4";

/// **A redemption authorization as the miner publishes it** — the authorization, the executor key and the executor's signature, in one
/// self-checking object. The miner writes it once (`misaka-palw-fp-rail --redeem-auth-out`), at claim time, and may then switch its PC off:
/// any builder holding the bundle can spend the claim's winning quanta into its own block ([`PalwReceiptSpendEnvelopeV4`] carries the same
/// three fields). Nothing in it is secret and nothing in it lets the holder do more than the authorization says.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRedemptionAuthBundleV4 {
    pub authorization: PalwRedemptionAuthV4,
    pub executor_pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

impl PalwRedemptionAuthBundleV4 {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_REDEMPTION_AUTH_BUNDLE_MAGIC.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh serialization of a plain struct cannot fail"));
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PalwReceiptV4Error> {
        let Some(body) = bytes.strip_prefix(&PALW_REDEMPTION_AUTH_BUNDLE_MAGIC) else {
            return Err(PalwReceiptV4Error::Undecodable("payload does not start with the RDA4 magic"));
        };
        let mut slice = body;
        let decoded =
            <Self as borsh::BorshDeserialize>::deserialize(&mut slice).map_err(|_| PalwReceiptV4Error::Undecodable("borsh body"))?;
        if !slice.is_empty() {
            return Err(PalwReceiptV4Error::Undecodable("trailing bytes"));
        }
        Ok(decoded)
    }

    /// The rules a spend would apply to this authorization before any chain lookup: version, network, key and signature lengths, the beacon
    /// rule, a non-empty range, the fee cap — and, with `verify`, the executor's signature over [`redeem_auth_id_v4`].
    pub fn validate_v4<V>(&self, network_domain: Hash64, verify_mldsa87: V) -> Result<(), PalwReceiptV4Error>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        let a = &self.authorization;
        if a.version != PALW_RECEIPT_V4_VERSION {
            return Err(PalwReceiptV4Error::UnsupportedVersion { got: a.version, expected: PALW_RECEIPT_V4_VERSION });
        }
        if a.network_domain != network_domain {
            return Err(PalwReceiptV4Error::NetworkDomainMismatch);
        }
        if self.executor_pubkey.len() != MLDSA87_PUBKEY_LEN {
            return Err(PalwReceiptV4Error::PublicKeyLength { got: self.executor_pubkey.len(), expected: MLDSA87_PUBKEY_LEN });
        }
        if self.signature.len() != MLDSA87_SIGNATURE_LEN {
            return Err(PalwReceiptV4Error::SignatureLength { got: self.signature.len(), expected: MLDSA87_SIGNATURE_LEN });
        }
        if a.beacon_rule != PALW_RECEIPT_V4_BEACON_RULE_SLOT {
            return Err(PalwReceiptV4Error::BeaconRuleUnknown(a.beacon_rule));
        }
        if a.quantum_lo >= a.quantum_hi {
            return Err(PalwReceiptV4Error::QuantumRangeEmpty { lo: a.quantum_lo, hi: a.quantum_hi });
        }
        if a.builder_fee_bps > PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS {
            return Err(PalwReceiptV4Error::FeeAboveCap { bps: a.builder_fee_bps, cap: PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS });
        }
        if !verify_mldsa87(&self.executor_pubkey, redeem_auth_id_v4(a).as_byte_slice(), &self.signature, PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT) {
            return Err(PalwReceiptV4Error::SignatureInvalid);
        }
        Ok(())
    }
}

/// **The stateful admission of a V4 spend** (producer and verifier call this one function). Items 1–5 (shared with V3), then the
/// V4 items 6' (the executor) and 7' (the builder), then item 8 (the class).
pub fn check_palw_receipt_spend_admission_v5(
    state: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    receipt_maturity_daa: u64,
    receipt_use_window_daa: u64,
    beacon: &PalwBeaconFactV3,
    envelope: &PalwReceiptSpendEnvelopeV4,
    pricing: Option<&crate::palw_state_v2::PalwFpPricingV1>,
) -> Result<Hash64, PalwFpAdmissionV3Error> {
    let spend = &envelope.spend;
    let facts = ReceiptSpendFacts {
        network_domain: spend.network_domain,
        claim_id: spend.claim_id,
        quantum_index: spend.quantum_index,
        beacon_block: spend.beacon_block,
    };
    let claim = receipt_items_1_to_5(state, ctx, receipt_maturity_daa, receipt_use_window_daa, beacon, &facts, pricing)?;

    // 6'. The authorization is the claim's executor's: same bond, still standing, its registered key.
    let executor_key = PalwBondKeyV2(spend.executor_bond);
    if executor_key != claim.bond {
        return Err(PalwReceiptV4Error::ExecutorBondMismatch.into());
    }
    let executor = state.bond(&executor_key).ok_or(PalwReceiptV4Error::ExecutorBondMissing(executor_key))?;
    if let PalwBondStatusV2::Retiring { .. } = executor.status {
        return Err(PalwReceiptV4Error::ExecutorBondRetiring(executor_key).into());
    }
    if executor.pubkey != spend.executor_pubkey {
        return Err(PalwReceiptV4Error::ExecutorKeyMismatch.into());
    }
    let expiry = spend.authorization.expiry_daa;
    if ctx.daa_score > expiry {
        return Err(PalwReceiptV4Error::AuthorizationExpired { block_daa: ctx.daa_score, expiry }.into());
    }
    // The range may not name quanta the claim does not have (the fold refuses an out-of-range index anyway; named here first).
    if let crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, .. } = &claim.source {
        if spend.quantum_index >= *quanta {
            return Err(PalwFpAdmissionV3Error::QuantumOutOfRange { claim: spend.claim_id, index: spend.quantum_index, quanta: *quanta });
        }
    }

    // 7'. The builder: any `Active` bond holding the key that signed the block's position.
    let builder_key = PalwBondKeyV2(spend.builder_bond);
    let builder = state.bond(&builder_key).ok_or(PalwReceiptV4Error::BuilderBondMissing(builder_key))?;
    if let PalwBondStatusV2::Retiring { .. } = builder.status {
        return Err(PalwReceiptV4Error::BuilderBondRetiring(builder_key).into());
    }
    if builder.pubkey != spend.builder_pubkey {
        return Err(PalwReceiptV4Error::BuilderKeyMismatch.into());
    }

    receipt_item_8(state, claim)?;
    Ok(fp_spend_id_v4(spend))
}

/// The composed entry point: stateless shape → both signatures → the stateful list.
#[allow(clippy::too_many_arguments)]
pub fn check_palw_receipt_spend_admission_full_v5<V>(
    state: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    receipt_maturity_daa: u64,
    receipt_use_window_daa: u64,
    beacon: &PalwBeaconFactV3,
    envelope: &PalwReceiptSpendEnvelopeV4,
    verify_mldsa87: V,
    pricing: Option<&crate::palw_state_v2::PalwFpPricingV1>,
) -> Result<Hash64, PalwFpAdmissionV3Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    envelope.validate_stateless_v4(network_domain, pre_pow_hash, timestamp, nonce)?;
    envelope.validate_signatures_v4(verify_mldsa87)?;
    check_palw_receipt_spend_admission_v5(state, ctx, receipt_maturity_daa, receipt_use_window_daa, beacon, envelope, pricing)
}

/// **The reward split of a V4 receipt block.** `subsidy_part` is the worker-base share derived from the block's subsidy; the
/// builder's fee is `⌊subsidy_part × fee_bps / 10 000⌋` (rounding stays with the miner, so nothing is minted), the miner leg the rest.
/// `fee_bps` above 10 000 is clamped (the chain's cap is 1 000, so this only bounds a caller's mistake). Returns
/// `(miner_leg, builder_fee)`; the two always sum to `subsidy_part`.
pub fn palw_receipt_v4_split_v1(subsidy_part: u64, fee_bps: u16) -> (u64, u64) {
    let bps = (fee_bps as u128).min(10_000);
    let fee = ((subsidy_part as u128) * bps / 10_000) as u64;
    (subsidy_part - fee, fee)
}

/// What the coinbase needs to pay one V4 receipt block: the miner leg's script and the builder's fee rate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwReceiptV4Payout {
    pub miner_script: ScriptPublicKey,
    pub fee_bps: u16,
}

/// The script an executor bond's receipt leg is paid to: derived from its registered payout payload, exactly as every other PALW
/// payout is (one script it can become).
pub fn palw_receipt_v4_miner_script(payout_payload: &Hash64) -> ScriptPublicKey {
    crate::mldsa87_primitives::p2pkh_mldsa87_spk(&payout_payload.as_bytes())
}

impl Params {
    /// `palw_receipt_spend_v4`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_receipt_spend_v4_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_receipt_spend_v4.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Is a V4 receipt spend valid at `daa_score`? `false` on every shipped preset.
    pub fn palw_receipt_spend_v4_active_at(&self, daa_score: u64) -> bool {
        self.palw_receipt_spend_v4_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * the fence off a `ConsensusV2` network (the receipt lane is a V2 lane);
    /// * without `palw_audit_2026_09_11` at or below it — B-5's per-mergeset (claim, quantum) dedup, which keeps two V4 siblings
    ///   of one quantum from being paid twice;
    /// * without `palw_audit_2026_09_23` at or below it — the forfeiture rule that closes the receipt lane for a convicted execution.
    pub fn validate_palw_receipt_spend_v4(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_receipt_spend_v4.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score()) else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_receipt_spend_v4 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let below = |fence: Option<ForkActivation>| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !below(self.palw_audit_2026_09_11) {
            return Err(PalwModeV2Error::Invalid(
                "palw_receipt_spend_v4 needs palw_audit_2026_09_11 at or below it: the per-mergeset quantum dedup keeps one quantum from being paid twice",
            ));
        }
        if !below(self.palw_audit_2026_09_23) {
            return Err(PalwModeV2Error::Invalid(
                "palw_receipt_spend_v4 needs palw_audit_2026_09_23 at or below it: a convicted execution's receipt rights must stay closed",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn op(n: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(h(n), 0)
    }
    fn sig() -> Vec<u8> {
        vec![0x5A; MLDSA87_SIGNATURE_LEN]
    }
    fn key(n: u8) -> Vec<u8> {
        vec![n; MLDSA87_PUBKEY_LEN]
    }

    const DOMAIN: u8 = 99;
    const PPH: u8 = 0xB0;
    const TS: u64 = 1_700;
    const NONCE: u64 = 9;

    pub(crate) fn envelope() -> PalwReceiptSpendEnvelopeV4 {
        let auth = PalwRedemptionAuthV4 {
            version: PALW_RECEIPT_V4_VERSION,
            network_domain: h(DOMAIN),
            claim_id: h(0xFC),
            executor_bond: op(1),
            quantum_lo: 0,
            quantum_hi: 3,
            beacon_rule: PALW_RECEIPT_V4_BEACON_RULE_SLOT,
            builder_fee_bps: 500,
            expiry_daa: u64::MAX,
        };
        PalwReceiptSpendEnvelopeV4 {
            spend: PalwReceiptSpendUnsignedV4 {
                version: PALW_RECEIPT_V4_VERSION,
                network_domain: h(DOMAIN),
                challenge: spend_challenge_v4(h(DOMAIN), h(PPH), TS, NONCE, h(0xFC), 1, &op(1), &op(2)),
                claim_id: h(0xFC),
                quantum_index: 1,
                beacon_block: h(0xBE),
                executor_bond: op(1),
                builder_bond: op(2),
                builder_pubkey: key(2),
                authorization: auth,
                executor_pubkey: key(1),
                authorization_signature: sig(),
            },
            builder_signature: sig(),
        }
    }

    fn stateless(e: &PalwReceiptSpendEnvelopeV4) -> Result<(), PalwReceiptV4Error> {
        e.validate_stateless_v4(h(DOMAIN), h(PPH), TS, NONCE)
    }

    #[test]
    fn an_honest_envelope_round_trips_and_passes_the_stateless_rules() {
        let e = envelope();
        let bytes = e.encode();
        assert!(bytes.starts_with(b"PFS4"));
        assert!(bytes.len() > crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES, "that is why the V4 cap exists");
        assert!(bytes.len() <= PALW_COMMITMENT_MAX_BYTES_V4);
        assert_eq!(PalwReceiptSpendEnvelopeV4::decode(&bytes).unwrap(), e);
        stateless(&e).unwrap();
    }

    #[test]
    fn carriages_of_the_two_versions_cannot_decode_as_each_other() {
        let v4 = envelope().encode();
        assert!(PalwReceiptSpendEnvelopeV3::decode(&v4).is_err(), "a PFS4 payload is not a V3 envelope");
        let v3 = envelope().to_fold_envelope().encode();
        assert!(v3.starts_with(b"PFS3"));
        assert!(PalwReceiptSpendEnvelopeV4::decode(&v3).is_err(), "a PFS3 payload is not a V4 envelope");
        assert!(palw_receipt_v4_carriage_is_v4(&v4) && !palw_receipt_v4_carriage_is_v4(&v3));
        // Trailing bytes and truncation are refused: a payload is not a container.
        let mut trailing = v4.clone();
        trailing.push(0);
        assert!(PalwReceiptSpendEnvelopeV4::decode(&trailing).is_err());
        assert!(PalwReceiptSpendEnvelopeV4::decode(&v4[..v4.len() - 1]).is_err());
        let mut oversized = v4;
        oversized.resize(PALW_COMMITMENT_MAX_BYTES_V4 + 1, 0);
        assert!(PalwReceiptSpendEnvelopeV4::decode(&oversized).is_err());
    }

    #[test]
    fn every_stateless_refusal_is_named() {
        let mut e = envelope();
        e.spend.version = 2;
        assert!(matches!(stateless(&e), Err(PalwReceiptV4Error::UnsupportedVersion { got: 2, .. })));
        let mut e = envelope();
        e.spend.authorization.version = 2;
        assert!(matches!(stateless(&e), Err(PalwReceiptV4Error::UnsupportedVersion { .. })));
        let mut e = envelope();
        e.spend.authorization.network_domain = h(1);
        assert_eq!(stateless(&e), Err(PalwReceiptV4Error::NetworkDomainMismatch));
        let mut e = envelope();
        e.spend.builder_pubkey.pop();
        assert!(matches!(stateless(&e), Err(PalwReceiptV4Error::PublicKeyLength { .. })));
        let mut e = envelope();
        e.builder_signature.pop();
        assert!(matches!(stateless(&e), Err(PalwReceiptV4Error::SignatureLength { .. })));
        let mut e = envelope();
        e.spend.authorization.claim_id = h(0xAB);
        assert_eq!(stateless(&e), Err(PalwReceiptV4Error::AuthorizationMismatch));
        let mut e = envelope();
        e.spend.authorization.executor_bond = op(7);
        assert_eq!(stateless(&e), Err(PalwReceiptV4Error::AuthorizationMismatch));
        let mut e = envelope();
        e.spend.authorization.beacon_rule = 2;
        assert_eq!(stateless(&e), Err(PalwReceiptV4Error::BeaconRuleUnknown(2)));
        let mut e = envelope();
        e.spend.authorization.quantum_hi = 0;
        assert!(matches!(stateless(&e), Err(PalwReceiptV4Error::QuantumRangeEmpty { .. })));
        let mut e = envelope();
        e.spend.quantum_index = 3;
        assert!(matches!(stateless(&e), Err(PalwReceiptV4Error::QuantumOutsideAuthorizedRange { index: 3, lo: 0, hi: 3 })));
        let mut e = envelope();
        e.spend.authorization.builder_fee_bps = PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS + 1;
        assert_eq!(stateless(&e), Err(PalwReceiptV4Error::FeeAboveCap { bps: 1_001, cap: 1_000 }));
        // The cap itself is allowed.
        let mut e = envelope();
        e.spend.authorization.builder_fee_bps = PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS;
        assert_eq!(stateless(&e), Ok(()));
        // A challenge for another header position (or another builder) does not bind this one.
        let mut e = envelope();
        e.spend.challenge = spend_challenge_v4(h(DOMAIN), h(PPH), TS, NONCE + 1, h(0xFC), 1, &op(1), &op(2));
        assert_eq!(stateless(&e), Err(PalwReceiptV4Error::ChallengeMismatch));
        let mut e = envelope();
        e.spend.challenge = spend_challenge_v4(h(DOMAIN), h(PPH), TS, NONCE, h(0xFC), 1, &op(1), &op(3));
        assert_eq!(stateless(&e), Err(PalwReceiptV4Error::ChallengeMismatch));
    }

    #[test]
    fn the_spend_id_is_total_so_swapping_the_authorization_or_its_signature_changes_the_block_identity() {
        let base = fp_spend_id_v4(&envelope().spend);
        let mut e = envelope();
        e.spend.authorization.builder_fee_bps = 1;
        assert_ne!(fp_spend_id_v4(&e.spend), base);
        let mut e = envelope();
        e.spend.authorization_signature[0] ^= 1;
        assert_ne!(fp_spend_id_v4(&e.spend), base);
        let mut e = envelope();
        e.spend.builder_bond = op(5);
        assert_ne!(fp_spend_id_v4(&e.spend), base);
        let tag = fp_spend_l1_tag_v4(base);
        assert_eq!(tag.len(), PALW_FP_V3_L1_TAG_BYTES);
        assert_ne!(tag.to_vec(), crate::palw_freeprompt_v3::fp_spend_l1_tag_v3(base).to_vec(), "own domain: a V4 tag is not a V3 tag");
    }

    #[test]
    fn signatures_are_checked_under_their_own_contexts_with_the_carried_keys() {
        let e = envelope();
        // A verifier that accepts only the exact (key, message, context) tuples the rules call for.
        let auth_msg = redeem_auth_id_v4(&e.spend.authorization);
        let spend_msg = fp_spend_id_v4(&e.spend);
        let exact = |key: &[u8], msg: &[u8], _sig: &[u8], ctx: &[u8]| {
            (key == e.spend.executor_pubkey.as_slice() && msg == auth_msg.as_byte_slice() && ctx == PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT)
                || (key == e.spend.builder_pubkey.as_slice()
                    && msg == spend_msg.as_byte_slice()
                    && ctx == PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT)
        };
        assert_eq!(e.validate_signatures_v4(exact), Ok(()));
        assert_eq!(e.validate_signatures_v4(|_, _, _, _| false), Err(PalwReceiptV4Error::SignatureInvalid));
        // Swapping the two keys is not accepted: each signature is checked under ITS key.
        let mut swapped = e.clone();
        std::mem::swap(&mut swapped.spend.executor_pubkey, &mut swapped.spend.builder_pubkey);
        assert_eq!(swapped.validate_signatures_v4(exact), Err(PalwReceiptV4Error::SignatureInvalid));
        assert_ne!(PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT, PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT);
    }

    #[test]
    fn the_fold_view_carries_claim_quantum_and_the_executor_bond_and_nothing_signed() {
        let e = envelope();
        let v3 = e.to_fold_envelope();
        assert_eq!(v3.spend.claim_id, h(0xFC));
        assert_eq!(v3.spend.quantum_index, 1);
        assert_eq!(v3.spend.producer_bond, op(1));
        assert!(v3.signature.is_empty());
    }

    #[test]
    fn the_split_always_sums_to_the_part_and_rounding_stays_with_the_miner() {
        for part in [0u64, 1, 9, 10, 9_999, 10_000, 1_234_567_890, u64::MAX] {
            for bps in [0u16, 1, 500, 1_000, 10_000, u16::MAX] {
                let (miner, fee) = palw_receipt_v4_split_v1(part, bps);
                assert_eq!(miner as u128 + fee as u128, part as u128, "part {part} bps {bps}");
                assert!(fee as u128 * 10_000 <= part as u128 * (bps.min(10_000) as u128), "the fee never rounds up");
            }
        }
        assert_eq!(palw_receipt_v4_split_v1(10_000, 500), (9_500, 500));
        assert_eq!(palw_receipt_v4_split_v1(19, 500), (19, 0), "below one fee unit the builder gets nothing: the miner keeps the dust");
        assert_eq!(palw_receipt_v4_split_v1(1_000, 0), (1_000, 0));
    }

    #[test]
    fn an_authorization_bundle_round_trips_and_checks_itself_before_any_chain_lookup() {
        let e = envelope();
        let bundle = PalwRedemptionAuthBundleV4 {
            authorization: e.spend.authorization.clone(),
            executor_pubkey: e.spend.executor_pubkey.clone(),
            signature: e.spend.authorization_signature.clone(),
        };
        assert_eq!(PalwRedemptionAuthBundleV4::decode(&bundle.encode()).unwrap(), bundle);
        assert!(PalwRedemptionAuthBundleV4::decode(&e.encode()).is_err(), "a PFS4 spend is not an RDA4 bundle");
        let msg = redeem_auth_id_v4(&bundle.authorization);
        let accepts = |_: &[u8], m: &[u8], _: &[u8], c: &[u8]| m == msg.as_byte_slice() && c == PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT;
        bundle.validate_v4(h(DOMAIN), accepts).unwrap();
        assert_eq!(bundle.validate_v4(h(1), accepts), Err(PalwReceiptV4Error::NetworkDomainMismatch));
        assert_eq!(bundle.validate_v4(h(DOMAIN), |_, _, _, _| false), Err(PalwReceiptV4Error::SignatureInvalid));
        let mut greedy = bundle.clone();
        greedy.authorization.builder_fee_bps = PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS + 1;
        assert!(matches!(greedy.validate_v4(h(DOMAIN), accepts), Err(PalwReceiptV4Error::FeeAboveCap { .. })));
        let mut empty = bundle.clone();
        empty.authorization.quantum_hi = empty.authorization.quantum_lo;
        assert!(matches!(empty.validate_v4(h(DOMAIN), accepts), Err(PalwReceiptV4Error::QuantumRangeEmpty { .. })));
        let mut short = bundle;
        short.executor_pubkey.pop();
        assert!(matches!(short.validate_v4(h(DOMAIN), accepts), Err(PalwReceiptV4Error::PublicKeyLength { .. })));
    }
}
