//! AccountMetadata is an associative structure that contains
//! additional information about an account. This structure
//! is not encrypted and is stored in plain text. This is meant
//! to provide an ability to perform various operations (such as
//! new address generation) without the need to re-encrypt the
//! wallet data when storing.

use crate::imports::*;
use crate::storage::IdT;

/// kaspa-pq PQ-only (ADR-0019 §14): the receive/change address-derivation index
/// pair carried by [`AccountMetadata`]. It is curve-independent (two counters),
/// so it lives here in `storage::metadata` rather than in the secp256k1-gated
/// `derivation` module — keeping it available to the PQ-only wallet build.
#[derive(Default, Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AddressDerivationMeta([u32; 2]);

impl AddressDerivationMeta {
    pub fn new(receive: u32, change: u32) -> Self {
        Self([receive, change])
    }

    pub fn receive(&self) -> u32 {
        self.0[0]
    }

    pub fn change(&self) -> u32 {
        self.0[1]
    }
}

impl std::fmt::Display for AddressDerivationMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {}]", self.receive(), self.change())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountMetadata {
    pub id: AccountId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexes: Option<AddressDerivationMeta>,
}

impl AccountMetadata {
    const STORAGE_MAGIC: u32 = 0x4154454d;
    const STORAGE_VERSION: u32 = 0;

    pub fn new(id: AccountId, indexes: AddressDerivationMeta) -> Self {
        Self { id, indexes: Some(indexes) }
    }

    pub fn address_derivation_indexes(&self) -> Option<AddressDerivationMeta> {
        self.indexes.clone()
    }
}

impl IdT for AccountMetadata {
    type Id = AccountId;
    fn id(&self) -> &AccountId {
        &self.id
    }
}

impl BorshSerialize for AccountMetadata {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.id, writer)?;
        BorshSerialize::serialize(&self.indexes, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for AccountMetadata {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize_reader(reader)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        let id = BorshDeserialize::deserialize_reader(reader)?;
        let indexes = BorshDeserialize::deserialize_reader(reader)?;

        Ok(Self { id, indexes })
    }
}
