use std::fmt::{Debug, Display, Formatter};
use std::str::{self, FromStr};

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::hex::{FromHex, ToHex};
use kaspa_utils::{serde_impl_deser_fixed_bytes_ref, serde_impl_ser_fixed_bytes_ref};
use thiserror::Error;

/// The size of the array used to store subnetwork IDs.
pub const SUBNETWORK_ID_SIZE: usize = 20;

/// The domain representation of a Subnetwork ID
#[derive(Clone, Default, Eq, PartialEq, Ord, PartialOrd, Hash, BorshSerialize, BorshDeserialize)]
pub struct SubnetworkId([u8; SUBNETWORK_ID_SIZE]);

impl Debug for SubnetworkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubnetworkId").field("", &self.to_hex()).finish()
    }
}

serde_impl_ser_fixed_bytes_ref!(SubnetworkId, SUBNETWORK_ID_SIZE);
serde_impl_deser_fixed_bytes_ref!(SubnetworkId, SUBNETWORK_ID_SIZE);

impl AsRef<[u8; SUBNETWORK_ID_SIZE]> for SubnetworkId {
    fn as_ref(&self) -> &[u8; SUBNETWORK_ID_SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for SubnetworkId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; SUBNETWORK_ID_SIZE]> for SubnetworkId {
    fn from(value: [u8; SUBNETWORK_ID_SIZE]) -> Self {
        Self::from_bytes(value)
    }
}

impl SubnetworkId {
    pub const fn from_byte(b: u8) -> SubnetworkId {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        bytes[0] = b;
        SubnetworkId(bytes)
    }

    pub const fn from_bytes(bytes: [u8; SUBNETWORK_ID_SIZE]) -> SubnetworkId {
        SubnetworkId(bytes)
    }

    /// Returns true if the subnetwork is a built-in subnetwork, which
    /// means all nodes, including partial nodes, must validate it, and its transactions
    /// always use 0 gas.
    #[inline]
    pub fn is_builtin(&self) -> bool {
        *self == SUBNETWORK_ID_COINBASE || *self == SUBNETWORK_ID_REGISTRY
    }

    /// Returns true if the subnetwork is the native subnetwork
    #[inline]
    pub fn is_native(&self) -> bool {
        *self == SUBNETWORK_ID_NATIVE
    }

    /// Returns true if the subnetwork is the native or a built-in subnetwork
    #[inline]
    pub fn is_builtin_or_native(&self) -> bool {
        self.is_native() || self.is_builtin()
    }

    /// kaspa-pq Phase 10 (ADR-0009): true for the DNS finality overlay
    /// subnetworks (stake-bond / attestation-shard / slashing-evidence).
    /// These are validated by full nodes but are **not** `is_builtin()`
    /// (neither coinbase nor the zero-gas registry subnetwork).
    #[inline]
    pub fn is_dns_overlay(&self) -> bool {
        *self == SUBNETWORK_ID_STAKE_BOND
            || *self == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD
            || *self == SUBNETWORK_ID_SLASHING_EVIDENCE
            || *self == SUBNETWORK_ID_STAKE_UNBOND
            || *self == SUBNETWORK_ID_COMPUTE_CERTIFICATE
            || *self == SUBNETWORK_ID_COMPUTE_CHALLENGE
            || *self == SUBNETWORK_ID_COMPUTE_CAPABILITY
            || *self == SUBNETWORK_ID_COMPUTE_COMMITMENT
            || *self == SUBNETWORK_ID_COMPUTE_VERDICT
            || *self == SUBNETWORK_ID_STAKE_PRECOMMIT
            || *self == SUBNETWORK_ID_PRECOMMIT_EVIDENCE
    }

    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020): true for the EVM bridge
    /// subnetworks (UTXO→EVM deposit, plus the reserved withdraw-claim / admin
    /// ids). Like the DNS overlay these are full-node-validated but are **not**
    /// `is_builtin()` (neither coinbase nor the zero-gas registry subnetwork).
    #[inline]
    pub fn is_evm_overlay(&self) -> bool {
        *self == SUBNETWORK_ID_EVM_DEPOSIT || *self == SUBNETWORK_ID_EVM_WITHDRAW_CLAIM || *self == SUBNETWORK_ID_EVM_ADMIN
    }
}

#[derive(Error, Debug, Clone)]
pub enum SubnetworkConversionError {
    #[error(transparent)]
    SliceError(#[from] std::array::TryFromSliceError),

    #[error(transparent)]
    HexError(#[from] faster_hex::Error),
}

impl TryFrom<&[u8]> for SubnetworkId {
    type Error = SubnetworkConversionError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes = <[u8; SUBNETWORK_ID_SIZE]>::try_from(value)?;
        Ok(Self(bytes))
    }
}

impl Display for SubnetworkId {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; SUBNETWORK_ID_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        f.write_str(str::from_utf8(&hex).expect("hex is always valid UTF-8"))
    }
}

impl ToHex for SubnetworkId {
    fn to_hex(&self) -> String {
        let mut hex = [0u8; SUBNETWORK_ID_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        str::from_utf8(&hex).expect("hex is always valid UTF-8").to_string()
    }
}

impl FromStr for SubnetworkId {
    type Err = SubnetworkConversionError;

    #[inline]
    fn from_str(hex_str: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut bytes)?;
        Ok(Self(bytes))
    }
}

impl FromHex for SubnetworkId {
    type Error = SubnetworkConversionError;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut bytes)?;
        Ok(Self(bytes))
    }
}

/// The default subnetwork ID which is used for transactions without related payload data
pub const SUBNETWORK_ID_NATIVE: SubnetworkId = SubnetworkId::from_byte(0);

/// The subnetwork ID which is used for the coinbase transaction
pub const SUBNETWORK_ID_COINBASE: SubnetworkId = SubnetworkId::from_byte(1);

/// The subnetwork ID which is used for adding new sub networks to the registry
pub const SUBNETWORK_ID_REGISTRY: SubnetworkId = SubnetworkId::from_byte(2);

// kaspa-pq Phase 10 (ADR-0009) DNS finality overlay subnetwork ids. Byte
// values 0x10/0x11/0x12 avoid the upstream built-ins (0/1/2) and the
// test-only 3. Routed + payload-validated by full nodes (see
// `dns_finality::dns_tx_kind` + `validate_*_payload`).
pub const SUBNETWORK_ID_STAKE_BOND: SubnetworkId = SubnetworkId::from_byte(0x10);
pub const SUBNETWORK_ID_STAKE_ATTESTATION_SHARD: SubnetworkId = SubnetworkId::from_byte(0x11);
pub const SUBNETWORK_ID_SLASHING_EVIDENCE: SubnetworkId = SubnetworkId::from_byte(0x12);
/// kaspa-pq H-05 (ADR-0010 "Unbonding"): an owner-authorized request to begin unbonding a bond.
pub const SUBNETWORK_ID_STAKE_UNBOND: SubnetworkId = SubnetworkId::from_byte(0x13);

// MISAKA Verified LLM Token-Weighted BFT (`vlt`) subnetwork ids. They extend the DNS overlay band
// (0x10-0x13 → 0x10-0x15) because they feed the same finality overlay: these are what mint the
// voting weight the attestation shards then spend.
/// A verified LLM job's compute certificate — [`crate::vlt::ComputeCertificatePayload`]. Surviving
/// the challenge window credits `X_i(e)`, which becomes voting weight from epoch `e + 1`.
pub const SUBNETWORK_ID_COMPUTE_CERTIFICATE: SubnetworkId = SubnetworkId::from_byte(0x14);
/// A fraud proof against an accepted compute certificate —
/// [`crate::vlt::ComputeChallengePayload`].
pub const SUBNETWORK_ID_COMPUTE_CHALLENGE: SubnetworkId = SubnetworkId::from_byte(0x15);
/// A validator's declaration that it runs a given `(model, runtime, determinism class)` profile
/// — [`crate::vlt::ComputeCapabilityPayload`]. This is what gives verifier sortition a
/// class-matched candidate pool to draw from.
pub const SUBNETWORK_ID_COMPUTE_CAPABILITY: SubnetworkId = SubnetworkId::from_byte(0x16);
/// Phase 1 of the two-phase sortition: an executor's commitment to a job, published before the
/// beacon that picks its auditors exists — [`crate::vlt::ComputeCommitmentPayload`].
pub const SUBNETWORK_ID_COMPUTE_COMMITMENT: SubnetworkId = SubnetworkId::from_byte(0x17);
/// A sortitioned verifier's standalone verdict on a compute certificate —
/// [`crate::vlt::ComputeVerdictPayload`]. Standalone rather than embedded in the certificate so
/// no off-chain executor↔verifier round trip is needed and every verdict is publicly auditable.
pub const SUBNETWORK_ID_COMPUTE_VERDICT: SubnetworkId = SubnetworkId::from_byte(0x18);

/// Round 2 of DNS finality — [`crate::dns_finality::StakePrecommitPayload`]. A validator that has
/// seen the prevote quorum for an epoch's anchor **locks** on it and says so on chain. An anchor
/// is DNS-confirmed only once the precommit round reaches quorum too, so finality is a two-round
/// commit rather than a single tally, and a validator that moves its lock has published the
/// evidence of doing so. In the 0x10-0x18 overlay band because it is the same finality overlay:
/// the attestation shard is the prevote, this is the precommit.
pub const SUBNETWORK_ID_STAKE_PRECOMMIT: SubnetworkId = SubnetworkId::from_byte(0x19);

/// Two precommits from one validator that cannot both be honest —
/// [`crate::dns_finality::PrecommitEvidencePayload`]. The round-2 sibling of
/// `SUBNETWORK_ID_SLASHING_EVIDENCE`: a lock is only worth carrying if breaking it costs the
/// bond, and the two signed payloads are the whole proof, so no reachability and no access to the
/// losing branch's blocks is needed to check it.
pub const SUBNETWORK_ID_PRECOMMIT_EVIDENCE: SubnetworkId = SubnetworkId::from_byte(0x1a);

// kaspa-pq Selected-Parent EVM Lane (ADR-0020) EVM bridge subnetwork ids. Byte
// values 0x20/0x21/0x22 sit above the DNS overlay band (0x10-0x13) and the
// upstream built-ins (0/1/2). Routed + payload-validated by full nodes.
/// UTXO → EVM native-coin deposit (ADR-0020 §6). Payload: version, evm_address,
/// amount_atomic, asset_id, memo.
pub const SUBNETWORK_ID_EVM_DEPOSIT: SubnetworkId = SubnetworkId::from_byte(0x20);
/// Reserved for a future claim-style withdrawal; unused in the initial design
/// (EVM → UTXO withdrawals are an in-consensus side-effect, ADR-0020 §7).
pub const SUBNETWORK_ID_EVM_WITHDRAW_CLAIM: SubnetworkId = SubnetworkId::from_byte(0x21);
/// Reserved for future EVM fork-activation / system-contract migration admin
/// txs; unused on a governance-free network.
pub const SUBNETWORK_ID_EVM_ADMIN: SubnetworkId = SubnetworkId::from_byte(0x22);

// MISAKA Compute Token Program token-op band 0x30-0x33 (design
// `docs/misaka-compute-token-program-design-v0.1.md` §4.3). Above the EVM
// bridge band (0x20-0x22) as that band sits above the finality overlay
// (0x10-0x1a). Phase A defines transfer/burn; 0x32 (`CreateMint`) and 0x33
// (`MintTo`) are reserved for Phase B and intentionally not defined yet — the
// TOK asset itself has no mint authority (§4.1), its only issuance is emission.
/// Move TOK between ledger accounts — [`crate::token::TokenTransferPayload`].
pub const SUBNETWORK_ID_TOKEN_TRANSFER: SubnetworkId = SubnetworkId::from_byte(0x30);
/// Destroy TOK from the signer's own ledger account —
/// [`crate::token::TokenBurnPayload`].
pub const SUBNETWORK_ID_TOKEN_BURN: SubnetworkId = SubnetworkId::from_byte(0x31);

// MISAKA PALW chain carriage band 0x40-0x45 (ADR-0029 Stage 1). Above the
// token-op band (0x30-0x33) as that band sits above the EVM bridge (0x20-0x22)
// and the finality overlay (0x10-0x1a) — one fresh 0x10-aligned band per
// family, ids sequential from the band base in carriage-kind order. The
// payload on each id is the SAME Borsh body Stage 0 wraps in the
// `"MPALW2" ‖ kind` magic envelope, minus that 7-byte prefix: at Stage 1 the
// kind lives in the subnetwork id, and
// [`crate::palw_carriage::palw_carriage_tx_kind`] maps it back. Routed +
// stateless-validated by full nodes like every band before it.
//
// DEPLOYMENT: these ids activate ONLY at a coordinated release. To every node
// without this code an unknown subnetwork id is `SubnetworksDisabled` at
// admission — so a block carrying one of these transactions splits an
// unupgraded fleet, exactly the reason admitting 0x10-0x1a, 0x20-0x22 and
// 0x30-0x31 was itself release-coordinated. Merely shipping these constants
// and their validators IS the release artifact, not a live activation:
// nothing rides these ids on any chain until the whole fleet runs a build
// that admits them.
/// A miner's on-chain job commitment (carriage kind 0x01) —
/// [`crate::palw_carriage::PalwCommitmentCarriageV1`].
pub const SUBNETWORK_ID_PALW_COMMITMENT: SubnetworkId = SubnetworkId::from_byte(0x40);
/// An assigned re-executor's bonded attestation (kind 0x02) —
/// [`crate::palw_carriage::PalwAttestationCarriageV1`].
pub const SUBNETWORK_ID_PALW_ATTESTATION: SubnetworkId = SubnetworkId::from_byte(0x41);
/// An opening challenge (kind 0x03) —
/// [`crate::palw_carriage::PalwOpeningCallCarriageV1`].
pub const SUBNETWORK_ID_PALW_OPENING_CALL: SubnetworkId = SubnetworkId::from_byte(0x42);
/// An opening answer, bound to its call's carrier (kind 0x04) —
/// [`crate::palw_carriage::PalwOpeningAnswerCarriageV1`].
pub const SUBNETWORK_ID_PALW_OPENING_ANSWER: SubnetworkId = SubnetworkId::from_byte(0x43);
/// A refutation that fits one transaction (kind 0x05) —
/// [`crate::palw_carriage::PalwRefutationCarriageV1`]. Pure evidence carrier:
/// must declare no outputs (the slashing-evidence rule).
pub const SUBNETWORK_ID_PALW_REFUTATION: SubnetworkId = SubnetworkId::from_byte(0x44);
/// One chunk of an over-mass refutation (kind 0x06, ADR-0029 §6) —
/// [`crate::palw_carriage::PalwEvidenceChunkCarriageV1`]. Pure evidence
/// carrier like the refutation it reassembles into.
pub const SUBNETWORK_ID_PALW_EVIDENCE_CHUNK: SubnetworkId = SubnetworkId::from_byte(0x45);
/// An executor-equivocation certificate (kind 0x07) —
/// [`crate::palw_carriage::PalwEquivocationCarriageV1`]. Unlike every band member
/// above it, this one can COST somebody their bond: it is the single PALW offence
/// that is objectively provable at acceptance, by two signatures from one bonded
/// key that cannot both be true. Nothing is re-executed and nothing is looked up
/// beyond the accused bond's own public key.
pub const SUBNETWORK_ID_PALW_EQUIVOCATION: SubnetworkId = SubnetworkId::from_byte(0x46);
/// An arithmetic conviction (kind 0x08) —
/// [`crate::palw_carriage::PalwStepConvictionCarriageV1`]. The second band member
/// that can cost a bond, and the one ADR-0028 §6 makes Stage 2's prerequisite:
/// a signed trace root plus a proof that one step under it is not what the class
/// computes. Adjudicated by recomputing that single step from opened tiles — a
/// bounded CPU primitive, never a model run.
pub const SUBNETWORK_ID_PALW_STEP_CONVICTION: SubnetworkId = SubnetworkId::from_byte(0x47);

#[cfg(test)]
mod tests {
    use super::*;

    /// Every routed subnetwork id in the fork, in one table. A collision between two of these
    /// would silently route one band's payloads through another band's validator, so
    /// distinctness is a tested invariant rather than a reading of the byte comments — and every
    /// new band (the PALW carriage ids most recently) must join this table when it lands.
    #[test]
    fn all_subnetwork_ids_are_distinct() {
        let all: &[(&str, SubnetworkId)] = &[
            ("NATIVE", SUBNETWORK_ID_NATIVE),
            ("COINBASE", SUBNETWORK_ID_COINBASE),
            ("REGISTRY", SUBNETWORK_ID_REGISTRY),
            ("STAKE_BOND", SUBNETWORK_ID_STAKE_BOND),
            ("STAKE_ATTESTATION_SHARD", SUBNETWORK_ID_STAKE_ATTESTATION_SHARD),
            ("SLASHING_EVIDENCE", SUBNETWORK_ID_SLASHING_EVIDENCE),
            ("STAKE_UNBOND", SUBNETWORK_ID_STAKE_UNBOND),
            ("COMPUTE_CERTIFICATE", SUBNETWORK_ID_COMPUTE_CERTIFICATE),
            ("COMPUTE_CHALLENGE", SUBNETWORK_ID_COMPUTE_CHALLENGE),
            ("COMPUTE_CAPABILITY", SUBNETWORK_ID_COMPUTE_CAPABILITY),
            ("COMPUTE_COMMITMENT", SUBNETWORK_ID_COMPUTE_COMMITMENT),
            ("COMPUTE_VERDICT", SUBNETWORK_ID_COMPUTE_VERDICT),
            ("STAKE_PRECOMMIT", SUBNETWORK_ID_STAKE_PRECOMMIT),
            ("PRECOMMIT_EVIDENCE", SUBNETWORK_ID_PRECOMMIT_EVIDENCE),
            ("EVM_DEPOSIT", SUBNETWORK_ID_EVM_DEPOSIT),
            ("EVM_WITHDRAW_CLAIM", SUBNETWORK_ID_EVM_WITHDRAW_CLAIM),
            ("EVM_ADMIN", SUBNETWORK_ID_EVM_ADMIN),
            ("TOKEN_TRANSFER", SUBNETWORK_ID_TOKEN_TRANSFER),
            ("TOKEN_BURN", SUBNETWORK_ID_TOKEN_BURN),
            ("PALW_COMMITMENT", SUBNETWORK_ID_PALW_COMMITMENT),
            ("PALW_ATTESTATION", SUBNETWORK_ID_PALW_ATTESTATION),
            ("PALW_OPENING_CALL", SUBNETWORK_ID_PALW_OPENING_CALL),
            ("PALW_OPENING_ANSWER", SUBNETWORK_ID_PALW_OPENING_ANSWER),
            ("PALW_REFUTATION", SUBNETWORK_ID_PALW_REFUTATION),
            ("PALW_EVIDENCE_CHUNK", SUBNETWORK_ID_PALW_EVIDENCE_CHUNK),
        ];
        for (i, (a_name, a)) in all.iter().enumerate() {
            for (b_name, b) in all.iter().skip(i + 1) {
                assert_ne!(a, b, "{a_name} and {b_name} share one subnetwork id");
            }
        }
        // The PALW band keeps the established byte-pattern scheme: a single tag byte in a fresh
        // 0x10-aligned band, sequential from the base in kind order (0x40..=0x45).
        for (i, id) in [
            SUBNETWORK_ID_PALW_COMMITMENT,
            SUBNETWORK_ID_PALW_ATTESTATION,
            SUBNETWORK_ID_PALW_OPENING_CALL,
            SUBNETWORK_ID_PALW_OPENING_ANSWER,
            SUBNETWORK_ID_PALW_REFUTATION,
            SUBNETWORK_ID_PALW_EVIDENCE_CHUNK,
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(*id, SubnetworkId::from_byte(0x40 + i as u8));
        }
    }
}
