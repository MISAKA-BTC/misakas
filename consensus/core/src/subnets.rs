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
    }

    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020): true for the EVM bridge
    /// subnetworks (UTXO→EVM deposit, plus the reserved withdraw-claim / admin
    /// ids). Like the DNS overlay these are full-node-validated but are **not**
    /// `is_builtin()` (neither coinbase nor the zero-gas registry subnetwork).
    #[inline]
    pub fn is_evm_overlay(&self) -> bool {
        *self == SUBNETWORK_ID_EVM_DEPOSIT || *self == SUBNETWORK_ID_EVM_WITHDRAW_CLAIM || *self == SUBNETWORK_ID_EVM_ADMIN
    }

    /// ADR-0039 PALW Replica-GEMM audited-compute lane: true for the PALW overlay
    /// subnetworks (provider bond, batch manifest, leaf chunk, batch certificate,
    /// beacon, authorization, reserved slashing, DA challenge lifecycle, and the
    /// search-availability lifecycle). Like the DNS/EVM overlays
    /// these are full-node-routed + payload-validated but are **not** `is_builtin()`.
    /// The band `0x30-0x3f` (one full nibble) sits above the EVM band (0x20-0x22) and
    /// the DNS band (0x10-0x13) with no collision. Recognition is a pure wire property;
    /// every byte in the band stays inert until the PALW activation fence (pre-activation
    /// blocks reject any recognized PALW tx, and no dispatch runs below the fence).
    #[inline]
    pub fn is_palw_overlay(&self) -> bool {
        matches!(self.0[0], 0x30..=0x3f) && self.0[1..].iter().all(|&b| b == 0)
    }

    /// Returns the PALW overlay transaction byte (0x30-0x3f) if this is a PALW
    /// overlay subnetwork, else `None`. Used by stateless routing to dispatch a
    /// PALW payload to the right validator without a match on the full 20-byte id.
    #[inline]
    pub fn palw_tx_kind(&self) -> Option<u8> {
        if self.is_palw_overlay() { Some(self.0[0]) } else { None }
    }

    /// True for the model-agnostic **Compute Set registry** band (`0x40-0x44`):
    /// proposal / activation certificate / policy update / allocation plan / emergency halt
    /// (PALW_Model_Agnostic_Compute_Set_Architecture §17.1 — one generic band forever, never a
    /// subnetwork per model). Deliberately a SEPARATE recognizer from [`Self::is_palw_overlay`]:
    /// the 0x30-0x3f nibble is fully allocated and its sixteen bytes keep their existing
    /// dispatch semantics untouched. Like every overlay band, recognition is a pure wire
    /// property — registry bytes stay inert until their own activation fence.
    #[inline]
    pub fn is_palw_compute_registry(&self) -> bool {
        matches!(self.0[0], 0x40..=0x44) && self.0[1..].iter().all(|&b| b == 0)
    }

    /// Returns the Compute Set registry transaction byte (0x40-0x44) if this is a registry
    /// subnetwork, else `None` — the registry-band analogue of [`Self::palw_tx_kind`].
    #[inline]
    pub fn palw_compute_registry_tx_kind(&self) -> Option<u8> {
        if self.is_palw_compute_registry() { Some(self.0[0]) } else { None }
    }

    /// True for the PCPB band (`0x45`, ADR-0045 D3-b): today the single `PalwACommitV1` ordering
    /// anchor. A SEPARATE recognizer for the same reason the Compute Set registry got one — the
    /// 0x30-0x3f nibble is fully allocated and its dispatch semantics stay untouched.
    #[inline]
    pub fn is_palw_pcpb(&self) -> bool {
        self.0[0] == 0x45 && self.0[1..].iter().all(|&b| b == 0)
    }

    /// Returns the PCPB transaction byte (`0x45`) if this is a PCPB-band subnetwork, else `None`.
    #[inline]
    pub fn palw_pcpb_tx_kind(&self) -> Option<u8> {
        if self.is_palw_pcpb() { Some(self.0[0]) } else { None }
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

// ADR-0039/DA-01 PALW Replica-GEMM audited-compute lane subnetwork ids. The re-genesis band
// `0x30-0x3f` (one full nibble) sits above the EVM band (0x20-0x22), the DNS overlay band
// (0x10-0x13), and the upstream built-ins (0/1/2). Routed + payload-validated by
// full nodes; all are inert until the PALW activation fence. `0x39` stays reserved for
// cross-fork slashing; DA-01 uses 0x3a-0x3c; the search-availability lifecycle uses 0x3d-0x3f.
/// Provider bond registration (`PalwProviderBondPayloadV1`, design §24.3).
pub const SUBNETWORK_ID_PALW_PROVIDER_BOND: SubnetworkId = SubnetworkId::from_byte(0x30);
/// Batch manifest publication (`PalwBatchManifestV1`, design §9.3).
pub const SUBNETWORK_ID_PALW_BATCH_MANIFEST: SubnetworkId = SubnetworkId::from_byte(0x31);
/// Public leaf chunk (`PalwLeafChunkV1`, ≤64 leaves; design §9.2/§9.3).
pub const SUBNETWORK_ID_PALW_LEAF_CHUNK: SubnetworkId = SubnetworkId::from_byte(0x32);
/// Batch certificate (`PalwBatchCertificateV2`, design §10.1).
pub const SUBNETWORK_ID_PALW_BATCH_CERT: SubnetworkId = SubnetworkId::from_byte(0x33);
/// Batch revocation (`PalwRevocationV1`, design §9.5) — what the overlay tx byte `0x34` actually
/// decodes to.
///
/// Revocation subnetwork. ADR-0040 SLASH-01 (§5.16): the earlier
/// `SUBNETWORK_ID_PALW_SLASHING` name here was a dangling MISLABEL — `parse_palw_overlay(0x34)` resolves
/// to `PalwTxKind::Revocation` and always has, so a transaction submitted "as slashing" on `0x34` was
/// decoded and validated as a revocation: a live consensus-fault landmine. Renamed to match what the
/// byte does. Cross-fork double-use slashing (§12.4) is design-only and, when built, rides a NEW byte
/// (0x39, extending the band under re-genesis), because it is blocked on the authority→bond LINK the
/// signed authorization does not carry (§5.16.9).
pub const SUBNETWORK_ID_PALW_REVOCATION: SubnetworkId = SubnetworkId::from_byte(0x34);
/// PALW beacon commit (`PalwBeaconCommitV1`, design §11.2).
pub const SUBNETWORK_ID_PALW_BEACON_COMMIT: SubnetworkId = SubnetworkId::from_byte(0x35);
/// PALW beacon reveal (`PalwBeaconRevealV1`, design §11.2).
pub const SUBNETWORK_ID_PALW_BEACON_REVEAL: SubnetworkId = SubnetworkId::from_byte(0x36);
/// Provider unbond (mirrors the DNS stake-unbond flow; design §9.6).
pub const SUBNETWORK_ID_PALW_PROVIDER_UNBOND: SubnetworkId = SubnetworkId::from_byte(0x37);
/// kaspa-pq **ADR-0040 P1-6 (AUTH-01/02/03)** — per-block ticket authorization
/// (`PalwBlockAuthorizationV1`, design §12.4).
///
/// Carried in the algo-4 block's OWN body, not in the mergeset flow: it authorizes *this* block, so it
/// must be verifiable at this block's body validation rather than after acceptance.
///
/// Extending the band from `0x30..=0x37` to `0x30..=0x38` is a wire change, and PALW activates only via
/// re-genesis, so it is in scope. The alternative — binding the miner's script into `eligibility_hash`
/// instead — was rejected: it would let a miner GRIND over payout scripts to find a winning draw,
/// destroying the reason the nonce is pinned to `low64(nullifier)` in the first place. Only a signature
/// is simultaneously fixed for the legitimate holder and unforgeable by an observer.
pub const SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION: SubnetworkId = SubnetworkId::from_byte(0x38);
/// Reserved by ADR-0040 for future cross-fork double-use slashing evidence. DA-01 MUST NOT reuse
/// this byte; the evidence verifier remains design-only and `PalwTxKind` intentionally does not
/// decode it yet. The reservation itself is part of the re-genesis wire table.
pub const SUBNETWORK_ID_PALW_CROSS_FORK_SLASHING_RESERVED: SubnetworkId = SubnetworkId::from_byte(0x39);
/// DA-01 bonded availability challenge (`PalwDaChallengeV1`).
pub const SUBNETWORK_ID_PALW_DA_CHALLENGE: SubnetworkId = SubnetworkId::from_byte(0x3a);
/// DA-01 provider-owner signed chunk response (`PalwDaResponseV1`).
pub const SUBNETWORK_ID_PALW_DA_RESPONSE: SubnetworkId = SubnetworkId::from_byte(0x3b);
/// DA-01 objective post-deadline timeout evidence (`PalwDaTimeoutEvidenceV1`).
pub const SUBNETWORK_ID_PALW_DA_TIMEOUT_EVIDENCE: SubnetworkId = SubnetworkId::from_byte(0x3c);

// Node-anchored web-search availability overlay (ADR node-anchored-web-search-da). These occupy the
// three bytes the module comment reserved (`0x3d-0x3f`), completing the PALW nibble. LIVE IN THE
// RECOGNITION BAND (`0x30..=0x3f`) as of the bonded-scheduler-registry activation step: the on-chain
// provider bond is the scheduler authorization every node resolves identically, so accepted-tx
// dispatch of these bytes is consensus-objective. Like the rest of the band they are inert until the
// PALW activation fence — pre-activation blocks reject them and no dispatch/state write runs below
// the fence (PALW activates only via re-genesis, so extending the band is part of that wire table).
/// Search-availability challenge (`PalwSearchChallengeTxV1`), bond-owner signed; may carry the
/// scheduler-signed registration proof that lazily registers the obligation it challenges.
pub const SUBNETWORK_ID_PALW_SEARCH_CHALLENGE: SubnetworkId = SubnetworkId::from_byte(0x3d);
/// Search-availability chunk response (`PalwSearchResponseTxV1`), proof-self-authorizing.
pub const SUBNETWORK_ID_PALW_SEARCH_RESPONSE: SubnetworkId = SubnetworkId::from_byte(0x3e);
/// Search-availability post-deadline timeout evidence (`PalwSearchTimeoutTxV1`), bond-owner signed.
pub const SUBNETWORK_ID_PALW_SEARCH_TIMEOUT: SubnetworkId = SubnetworkId::from_byte(0x3f);

// ============================================================================================
// Model-agnostic Compute Set registry band (0x40-0x44) — ADR-MA / §17.1.
//
// One FIXED generic band for every present and future model: adding an LLM registers data
// through these five payload kinds; it never allocates a new subnetwork (that is the point of
// the architecture). The band sits directly above the full PALW overlay nibble (0x30-0x3f) and
// is recognized by `is_palw_compute_registry`, NOT by `is_palw_overlay` — the sixteen overlay
// bytes keep their existing dispatch table byte-identically. Inert until the Compute Set
// registry activation fence; pre-activation blocks reject any recognized registry tx.
// ============================================================================================

/// `PalwComputeSetProposalV1` — immutable Descriptor V2 + proposer credential + bond reference
/// (§17.2). Registers the set in `Proposed`.
pub const SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL: SubnetworkId = SubnetworkId::from_byte(0x40);
/// `PalwComputeSetActivationCertificateV1` — validator-quorum certificate over conformance /
/// capacity / reproducibility evidence (§17.3). Prerequisite for the Shadow stage.
pub const SUBNETWORK_ID_PALW_COMPUTE_SET_ACTIVATION_CERT: SubnetworkId = SubnetworkId::from_byte(0x41);
/// `PalwComputeSetPolicyUpdateV1` — one mutable-policy revision (§8/§9).
pub const SUBNETWORK_ID_PALW_COMPUTE_SET_POLICY_UPDATE: SubnetworkId = SubnetworkId::from_byte(0x42);
/// `PalwModelAllocationPlanV1` — the atomic whole-lane share plan (§10).
pub const SUBNETWORK_ID_PALW_MODEL_ALLOCATION_PLAN: SubnetworkId = SubnetworkId::from_byte(0x43);
/// `PalwComputeSetEmergencyHaltV1` — immediate stop of new tickets/blocks for one set (§18.6).
pub const SUBNETWORK_ID_PALW_COMPUTE_SET_EMERGENCY_HALT: SubnetworkId = SubnetworkId::from_byte(0x44);

// ============================================================================================
// PCPB band (0x45) — ADR-0045 D3-b / ADR-0040 §5.14.7, docs/palw-pcpb-leaf-v2-wiring-design.md.
//
// The PALW overlay nibble (0x30-0x3f) is fully allocated (0x39 stays reserved for cross-fork
// slashing evidence), so PCPB follows the Compute Set registry precedent: a new band above it
// with its own recognizer. Like every overlay band, recognition is a pure wire property; the
// kind shares the PALW overlay activation fence (`check_palw_overlay_activation`).
// ============================================================================================

/// `PalwACommitV1` — the self-serial PCPB ordering anchor: registers `a_commit` on-chain so the
/// post-commit beacon `R_{a_commit_epoch + Δ}` provably post-dates it (the anti-grind ordering of
/// design §4.1). Unsigned and content-keyed: WHO registered is meaningless, WHEN is everything.
pub const SUBNETWORK_ID_PALW_ACOMMIT: SubnetworkId = SubnetworkId::from_byte(0x45);

#[cfg(test)]
mod palw_subnet_tests {
    use super::*;

    const PALW_BAND: [SubnetworkId; 16] = [
        SUBNETWORK_ID_PALW_PROVIDER_BOND,
        SUBNETWORK_ID_PALW_BATCH_MANIFEST,
        SUBNETWORK_ID_PALW_LEAF_CHUNK,
        SUBNETWORK_ID_PALW_BATCH_CERT,
        SUBNETWORK_ID_PALW_REVOCATION,
        SUBNETWORK_ID_PALW_BEACON_COMMIT,
        SUBNETWORK_ID_PALW_BEACON_REVEAL,
        SUBNETWORK_ID_PALW_PROVIDER_UNBOND,
        // ADR-0040 P1-6: per-block ticket authorization (AUTH-01/02/03).
        SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION,
        // 0x39 is a strict reservation, never a DA carrier.
        SUBNETWORK_ID_PALW_CROSS_FORK_SLASHING_RESERVED,
        SUBNETWORK_ID_PALW_DA_CHALLENGE,
        SUBNETWORK_ID_PALW_DA_RESPONSE,
        SUBNETWORK_ID_PALW_DA_TIMEOUT_EVIDENCE,
        // Node-anchored web-search availability lifecycle (0x3d-0x3f).
        SUBNETWORK_ID_PALW_SEARCH_CHALLENGE,
        SUBNETWORK_ID_PALW_SEARCH_RESPONSE,
        SUBNETWORK_ID_PALW_SEARCH_TIMEOUT,
    ];

    #[test]
    fn palw_band_is_0x30_to_0x3f_and_classified() {
        for (i, id) in PALW_BAND.iter().enumerate() {
            assert!(id.is_palw_overlay(), "{id:?} must be a PALW overlay");
            assert_eq!(id.palw_tx_kind(), Some(0x30 + i as u8));
            // PALW overlay is NOT a builtin/native, DNS, or EVM overlay.
            assert!(!id.is_builtin_or_native());
            assert!(!id.is_dns_overlay());
            assert!(!id.is_evm_overlay());
        }
    }

    #[test]
    fn palw_band_disjoint_from_other_bands_and_edges() {
        // adjacent / other bands are NOT PALW.
        for id in [
            SUBNETWORK_ID_NATIVE,
            SUBNETWORK_ID_COINBASE,
            SUBNETWORK_ID_STAKE_BOND,      // 0x10
            SUBNETWORK_ID_EVM_ADMIN,       // 0x22
            SubnetworkId::from_byte(0x2f), // just below band
            SubnetworkId::from_byte(0x40), // just above the full PALW nibble
        ] {
            assert!(!id.is_palw_overlay());
            assert_eq!(id.palw_tx_kind(), None);
        }
        // a 0x30 first byte with non-zero trailing bytes is NOT in-band (canonical single-byte only).
        let mut noncanonical = [0u8; SUBNETWORK_ID_SIZE];
        noncanonical[0] = 0x31;
        noncanonical[1] = 0x01;
        assert!(!SubnetworkId::from_bytes(noncanonical).is_palw_overlay());
    }

    const COMPUTE_REGISTRY_BAND: [SubnetworkId; 5] = [
        SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL,
        SUBNETWORK_ID_PALW_COMPUTE_SET_ACTIVATION_CERT,
        SUBNETWORK_ID_PALW_COMPUTE_SET_POLICY_UPDATE,
        SUBNETWORK_ID_PALW_MODEL_ALLOCATION_PLAN,
        SUBNETWORK_ID_PALW_COMPUTE_SET_EMERGENCY_HALT,
    ];

    #[test]
    fn compute_registry_band_is_0x40_to_0x44_and_disjoint() {
        for (i, id) in COMPUTE_REGISTRY_BAND.iter().enumerate() {
            assert!(id.is_palw_compute_registry(), "{id:?} must be a Compute Set registry subnetwork");
            assert_eq!(id.palw_compute_registry_tx_kind(), Some(0x40 + i as u8));
            // The registry band never aliases the PALW overlay nibble or any other overlay.
            assert!(!id.is_palw_overlay());
            assert_eq!(id.palw_tx_kind(), None);
            assert!(!id.is_builtin_or_native());
            assert!(!id.is_dns_overlay());
            assert!(!id.is_evm_overlay());
        }
        // Edges and non-canonical forms stay out of band.
        for id in [SubnetworkId::from_byte(0x3f), SubnetworkId::from_byte(0x45)] {
            assert_eq!(id.palw_compute_registry_tx_kind(), None);
        }
        let mut noncanonical = [0u8; SUBNETWORK_ID_SIZE];
        noncanonical[0] = 0x40;
        noncanonical[1] = 0x01;
        assert!(!SubnetworkId::from_bytes(noncanonical).is_palw_compute_registry());
        // And the PALW overlay band never answers to the registry recognizer.
        for id in PALW_BAND {
            assert!(!id.is_palw_compute_registry());
        }
    }

    #[test]
    fn pcpb_band_is_0x45_and_disjoint() {
        let id = SUBNETWORK_ID_PALW_ACOMMIT;
        assert!(id.is_palw_pcpb());
        assert_eq!(id.palw_pcpb_tx_kind(), Some(0x45));
        // Never aliases the overlay nibble, the registry band, or any other overlay.
        assert!(!id.is_palw_overlay());
        assert_eq!(id.palw_tx_kind(), None);
        assert!(!id.is_palw_compute_registry());
        assert!(!id.is_builtin_or_native());
        assert!(!id.is_dns_overlay());
        assert!(!id.is_evm_overlay());
        // Edges and non-canonical forms stay out of band.
        for other in [SubnetworkId::from_byte(0x44), SubnetworkId::from_byte(0x46)] {
            assert_eq!(other.palw_pcpb_tx_kind(), None);
        }
        let mut noncanonical = [0u8; SUBNETWORK_ID_SIZE];
        noncanonical[0] = 0x45;
        noncanonical[1] = 0x01;
        assert!(!SubnetworkId::from_bytes(noncanonical).is_palw_pcpb());
        // Neither established band answers to the PCPB recognizer.
        for other in PALW_BAND.iter().chain(COMPUTE_REGISTRY_BAND.iter()) {
            assert!(!other.is_palw_pcpb());
        }
    }
}
