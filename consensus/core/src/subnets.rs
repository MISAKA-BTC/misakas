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
