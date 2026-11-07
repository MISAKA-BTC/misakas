//!
//! kaspa-pq ML-DSA-87 single-key account implementation (ADR-0019 §13).
//!
//! This is the post-quantum analogue of [`keypair`](super::keypair): a
//! single-key account whose receive and change addresses are
//! [`Version::PubKeyHashMlDsa87`] P2PKH addresses (64-byte BLAKE2b-512 of the
//! 2592-byte ML-DSA-87 verification key). It is the *only* account variant that
//! produces spendable addresses on a PQ-only network — the legacy secp256k1
//! variants emit `Version::PubKey`/`PubKeyECDSA` addresses, which are
//! unrepresentable as standard outputs once `PqEnforcementMode::Consensus` is in
//! force (Phase 2 §7).
//!
//! The account stores the ML-DSA verification key (for address display and the
//! deterministic account id) plus the `account_index` used to derive it. The
//! ML-DSA *signing* key is never stored; it is re-derived from the wallet's
//! BIP39 master seed at signing time via
//! `kaspa_wallet_keys::kaspa_pq::derive_keypair` (see the PQ signing path in
//! `tx/generator/signer.rs`).

use crate::account::Inner;
use crate::imports::*;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_hashes::blake2b_512_address_payload;
use kaspa_wallet_keys::kaspa_pq::{KaspaPqMlDsa87KeyPair, derive_keypair};

pub const MLDSA_ACCOUNT_KIND: &str = "kaspa-mldsa-standard";

/// kaspa-pq ML-DSA-87 P2PKH [`Address`] for `public_key` (the 2592-byte
/// verification key) under `prefix`: `Version::PubKeyHashMlDsa87` over the
/// 64-byte keyed BLAKE2b-512 of the key (md2 §4.2, `kaspa-pq-v2/address/mldsa87`;
/// ADR-0019 §8/§13). This is the single source of truth for receive/change
/// address derivation and is kept free-standing so it can be unit-tested without
/// constructing a [`Wallet`].
pub fn mldsa_p2pkh_address(prefix: Prefix, public_key: &[u8]) -> Address {
    let payload = blake2b_512_address_payload(public_key);
    Address::new(prefix, Version::PubKeyHashMlDsa87, payload.as_byte_slice())
}

pub struct Ctor {}

#[async_trait]
impl Factory for Ctor {
    fn name(&self) -> String {
        "ML-DSA Keypair".to_string()
    }

    fn description(&self) -> String {
        "kaspa-pq ML-DSA-87 single-key (PQ-only) account".to_string()
    }

    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>> {
        Ok(Arc::new(MlDsa::try_load(wallet, storage, meta).await?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Payload {
    /// 2592-byte ML-DSA-87 verification key.
    pub public_key: Vec<u8>,
    /// Account index used to derive this key from the wallet master seed.
    pub account_index: u64,
}

impl Payload {
    pub fn new(public_key: Vec<u8>, account_index: u64) -> Self {
        Self { public_key, account_index }
    }

    pub fn try_load(storage: &AccountStorage) -> Result<Self> {
        Ok(Self::try_from_slice(storage.serialized.as_slice())?)
    }
}

impl Storable for Payload {
    const STORAGE_MAGIC: u32 = 0x4144_4c4d; // "MLDA"
    const STORAGE_VERSION: u32 = 0;
}

impl AccountStorable for Payload {}

impl BorshSerialize for Payload {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.account_index, writer)?;
        Ok(())
    }
}

impl BorshDeserialize for Payload {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize_reader(reader)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        let public_key: Vec<u8> = BorshDeserialize::deserialize_reader(reader)?;
        let account_index: u64 = BorshDeserialize::deserialize_reader(reader)?;

        Ok(Self { public_key, account_index })
    }
}

pub struct MlDsa {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    public_key: Vec<u8>,
    account_index: u64,
}

impl MlDsa {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        name: Option<String>,
        public_key: Vec<u8>,
        account_index: u64,
        prv_key_data_id: PrvKeyDataId,
    ) -> Result<Self> {
        let storable = Payload::new(public_key, account_index);
        let settings = AccountSettings { name, ..Default::default() };

        let (id, storage_key) = make_account_hashes(from_mldsa(&prv_key_data_id, &storable));
        let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));

        let Payload { public_key, account_index, .. } = storable;
        Ok(Self { inner, prv_key_data_id, public_key, account_index })
    }

    pub async fn try_load(wallet: &Arc<Wallet>, storage: &AccountStorage, _meta: Option<Arc<AccountMetadata>>) -> Result<Self> {
        let storable = Payload::try_load(storage)?;
        let inner = Arc::new(Inner::from_storage(wallet, storage));

        let Payload { public_key, account_index, .. } = storable;
        Ok(Self { inner, prv_key_data_id: storage.prv_key_data_ids.clone().try_into()?, public_key, account_index })
    }

    /// The ML-DSA-87 verification key (2592 bytes) this account locks to.
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// The account index used to derive this account's key from the master seed.
    pub fn account_index(&self) -> u64 {
        self.account_index
    }
}

#[async_trait]
impl Account for MlDsa {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        MLDSA_ACCOUNT_KIND.into()
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Ok(&self.prv_key_data_id)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn sig_op_count(&self) -> u8 {
        1
    }

    fn minimum_signatures(&self) -> u16 {
        1
    }

    fn receive_address(&self) -> Result<Address> {
        Ok(mldsa_p2pkh_address(self.inner().wallet.network_id()?.into(), &self.public_key))
    }

    fn change_address(&self) -> Result<Address> {
        // Single-key account: change == receive, always Version::PubKeyHashMlDsa87.
        Ok(mldsa_p2pkh_address(self.inner().wallet.network_id()?.into(), &self.public_key))
    }

    fn to_storage(&self) -> Result<AccountStorage> {
        let settings = self.context().settings.clone();
        let storable = Payload::new(self.public_key.clone(), self.account_index);
        let account_storage = AccountStorage::try_new(
            MLDSA_ACCOUNT_KIND.into(),
            self.id(),
            self.storage_key(),
            self.prv_key_data_id.into(),
            settings,
            storable,
        )?;

        Ok(account_storage)
    }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        Ok(None)
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let addresses = self.receive_address().ok().map(|address| vec![address]);

        let descriptor = AccountDescriptor::new(
            MLDSA_ACCOUNT_KIND.into(),
            *self.id(),
            self.name(),
            self.balance(),
            self.prv_key_data_id.into(),
            self.receive_address().ok(),
            self.change_address().ok(),
            addresses,
        );

        Ok(descriptor)
    }

    #[cfg(feature = "legacy-secp256k1")]
    fn create_address_private_keys<'l>(
        self: Arc<Self>,
        _key_data: &PrvKeyData,
        _payment_secret: &Option<Secret>,
        _addresses: &[&'l Address],
    ) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
        // A PQ account has no secp256k1 secret key. Transaction signing for this
        // account routes through the native ML-DSA path in the generator's
        // `Signer` (ADR-0019 §13), never through the secp256k1 key map, so this
        // returns no secp256k1 keys.
        Ok(vec![])
    }

    fn try_pq_keypair(&self, keydata: &PrvKeyData, payment_secret: &Option<Secret>) -> Result<Option<KaspaPqMlDsa87KeyPair>> {
        // Re-derive the ML-DSA-87 keypair from the wallet's BIP39 master seed,
        // matching the derivation used when the account's verification key was
        // first generated (network id + account index, change=0, index=0).
        let network_id = self.inner().wallet.network_id()?;
        let mnemonic = keydata
            .as_mnemonic(payment_secret.as_ref())?
            .ok_or_else(|| Error::custom("kaspa-pq ML-DSA account requires a BIP39 mnemonic private-key source"))?;
        let passphrase = payment_secret.as_ref().map(|s| std::str::from_utf8(s.as_ref())).transpose()?.unwrap_or_default();
        let seed = mnemonic.to_seed(passphrase);
        let keypair = derive_keypair(&network_id.to_string(), self.account_index as u32, 0, 0, seed.as_bytes());

        // The re-derived verification key must match the stored one; a mismatch
        // means the wrong wallet secret/passphrase/network — fail loudly rather
        // than sign with a key that does not own the account's UTXOs.
        if keypair.public_key_bytes().as_slice() != self.public_key.as_slice() {
            return Err(Error::custom(
                "kaspa-pq ML-DSA account: re-derived key does not match the stored verification key (wrong secret or network)",
            ));
        }
        Ok(Some(keypair))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_wallet_keys::kaspa_pq::derive_keypair;

    const TEST_MASTER_SEED: [u8; 64] = [0xab; 64];

    #[test]
    fn mldsa_address_is_pq_p2pkh() {
        // A real ML-DSA-87 verification key from the native wallet-keys derivation.
        let kp = derive_keypair("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let addr = mldsa_p2pkh_address(Prefix::Mainnet, kp.public_key_bytes());

        // Post-quantum P2PKH shape: 64-byte BLAKE2b-512 payload, ML-DSA version.
        assert_eq!(addr.version, Version::PubKeyHashMlDsa87);
        assert_eq!(addr.payload.len(), 64);

        // The account's address derivation must agree byte-for-byte with the
        // canonical `KaspaPqMlDsa87KeyPair::address` used by the WASM wallet and
        // the native signer (so the wallet and consensus see the same spk).
        assert_eq!(addr, kp.address(Prefix::Mainnet));

        let s: String = addr.into();
        assert!(s.starts_with("misaka:"), "got {s}");
    }

    #[test]
    fn payload_borsh_round_trip() {
        let kp = derive_keypair("testnet-10", 3, 0, 0, &TEST_MASTER_SEED);
        let storable_in = Payload::new(kp.public_key_bytes().to_vec(), 3);
        let bytes = borsh::to_vec(&storable_in).unwrap();
        let storable_out = Payload::try_from_slice(&bytes).unwrap();
        assert_eq!(storable_in.public_key, storable_out.public_key);
        assert_eq!(storable_in.account_index, storable_out.account_index);
        assert_eq!(storable_out.public_key.len(), 2592);
    }

    /// kaspa-pq (ADR-0019 §13) Phase 5d: an ML-DSA account re-derives its signing
    /// key from the wallet master seed (`try_pq_keypair`) and the generator's
    /// `Signer` uses it to produce an unlock script that verifies under the
    /// consensus script engine — the full native PQ sign path, end to end.
    #[tokio::test]
    async fn pq_account_signs_through_generator_signer() {
        use crate::encryption::EncryptionKind;
        use crate::tx::generator::{Signer, SignerT};
        use kaspa_bip32::{Language, Mnemonic};
        use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
        use kaspa_consensus_core::tx::{
            PopulatedTransaction, SignableTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint,
            TransactionOutput, UtxoEntry,
        };
        use kaspa_txscript::caches::Cache;
        use kaspa_txscript::{TxScriptEngine, pay_to_address_script};

        // In-memory wallet on a kaspa-pq network (testnet-10 => Consensus enforcement).
        let store = Wallet::resident_store().unwrap();
        let net = NetworkId::with_suffix(NetworkType::Testnet, 10);
        let wallet = Arc::new(Wallet::try_new(store, None, Some(net)).unwrap());
        let wallet_secret = Secret::new(vec![]);
        wallet
            .create_wallet(
                &wallet_secret,
                WalletCreateArgs {
                    title: None,
                    filename: None,
                    encryption_kind: EncryptionKind::XChaCha20Poly1305,
                    user_hint: None,
                    overwrite_wallet_storage: false,
                },
            )
            .await
            .unwrap();

        // BIP39 key data + the ML-DSA verification key derived from it the way the
        // wallet would at account-creation time (network id + account index 0).
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mnemonic = Mnemonic::new(phrase, Language::English).unwrap();
        let keydata = PrvKeyData::try_new_from_mnemonic(mnemonic.clone(), None, EncryptionKind::XChaCha20Poly1305).unwrap();
        let seed = mnemonic.to_seed("");
        let expected = derive_keypair(&net.to_string(), 0, 0, 0, seed.as_bytes());
        let vk = expected.public_key_bytes().to_vec();

        let account = Arc::new(MlDsa::try_new(&wallet, None, vk.clone(), 0, keydata.id).await.unwrap());

        // try_pq_keypair must re-derive exactly the stored verification key.
        let rederived = account.try_pq_keypair(&keydata, &None).unwrap().expect("PQ account yields a keypair");
        assert_eq!(rederived.public_key_bytes().as_slice(), vk.as_slice());

        // A 1-in / 1-out spend of a UTXO locked to the account's receive address.
        let address = account.receive_address().unwrap();
        assert_eq!(address.version, Version::PubKeyHashMlDsa87);
        let spk = pay_to_address_script(&address);
        let prev = TransactionOutpoint { transaction_id: TransactionId::from_bytes([0x11u8; 64]), index: 0 };
        let tx = Transaction::new(
            0,
            vec![TransactionInput { previous_outpoint: prev, signature_script: vec![], sequence: 0, sig_op_count: 1 }],
            vec![TransactionOutput { value: 500, script_public_key: spk.clone() }],
            0,
            Default::default(),
            0,
            vec![],
        );
        let entry = UtxoEntry { amount: 1000, script_public_key: spk.clone(), block_daa_score: 0, is_coinbase: false };
        let signable = SignableTransaction::with_entries(tx, vec![entry.clone()]);

        // Sign via the generator's Signer — routes through the native ML-DSA path.
        let signer = Signer::new(account.clone().as_dyn_arc(), keydata, None);
        let signed = signer.try_sign(signable, std::slice::from_ref(&address)).unwrap();
        assert!(!signed.tx.inputs[0].signature_script.is_empty(), "Signer produced an ML-DSA unlock script");

        // The produced signature must verify in the consensus script engine.
        let populated = PopulatedTransaction::new(&signed.tx, vec![entry]);
        let reused = SigHashReusedValuesUnsync::new();
        let cache = Cache::new(10_000);
        let mut vm =
            TxScriptEngine::from_transaction_input(&populated, &populated.tx.inputs[0], 0, &populated.entries[0], &reused, &cache);
        vm.execute().expect("ML-DSA signature from the generator Signer must verify in the script engine");
    }
}
