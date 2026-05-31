//!
//! Transaction signing trait and generic signer implementations..
//!

use crate::imports::*;
use kaspa_bip32::PrivateKey;
use kaspa_consensus_core::{sign::sign_with_multiple_v2, tx::SignableTransaction};
use kaspa_wallet_keys::kaspa_pq::sign_transaction_inputs_mldsa87;

pub trait SignerT: Send + Sync + 'static {
    fn try_sign(&self, transaction: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction>;
}

struct Inner {
    keydata: PrvKeyData,
    account: Arc<dyn Account>,
    payment_secret: Option<Secret>,
    keys: Mutex<AHashMap<Address, [u8; 32]>>,
}

pub struct Signer {
    inner: Arc<Inner>,
}

impl Signer {
    pub fn new(account: Arc<dyn Account>, keydata: PrvKeyData, payment_secret: Option<Secret>) -> Self {
        Self { inner: Arc::new(Inner { keydata, account, payment_secret, keys: Mutex::new(AHashMap::new()) }) }
    }

    fn ingest(&self, addresses: &[Address]) -> Result<()> {
        let mut keys = self.inner.keys.lock().unwrap();
        // skip address that are already present in the key map
        let addresses = addresses.iter().filter(|a| !keys.contains_key(a)).collect::<Vec<_>>();
        if !addresses.is_empty() {
            // let account = self.inner.account.clone().as_derivation_capable().expect("expecting derivation capable account");
            // let (receive, change) = account.derivation().addresses_indexes(&addresses)?;
            // let private_keys = account.create_private_keys(&self.inner.keydata, &self.inner.payment_secret, &receive, &change)?;
            let private_keys = self.inner.account.clone().create_address_private_keys(
                &self.inner.keydata,
                &self.inner.payment_secret,
                addresses.as_slice(),
            )?;
            for (address, private_key) in private_keys {
                keys.insert(address.clone(), private_key.to_bytes());
            }
        }

        Ok(())
    }
}

impl SignerT for Signer {
    fn try_sign(&self, mutable_tx: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction> {
        // kaspa-pq (ADR-0019 §13): a post-quantum (ML-DSA-87) account signs every
        // owned input with the native ML-DSA signer — it has no secp256k1 secret
        // keys, so the secp256k1 key map below never applies to it.
        if let Some(keypair) = self.inner.account.try_pq_keypair(&self.inner.keydata, &self.inner.payment_secret)? {
            let mut mutable_tx = mutable_tx;
            sign_transaction_inputs_mldsa87(&keypair, &mut mutable_tx, |i| {
                // ML-DSA is hedged-randomized; deterministic per-input randomness is
                // secure (see kaspa_wallet_keys::kaspa_pq). Vary per input for tidiness.
                let mut r = [0u8; 32];
                r[..8].copy_from_slice(&(i as u64).to_le_bytes());
                r
            });
            return Ok(mutable_tx);
        }

        self.ingest(addresses)?;

        let keys = self.inner.keys.lock().unwrap();
        let mut keys_for_signing = addresses.iter().map(|address| *keys.get(address).unwrap()).collect::<Vec<_>>();
        // TODO - refactor for multisig
        let signable_tx = sign_with_multiple_v2(mutable_tx, &keys_for_signing).fully_signed()?;
        keys_for_signing.zeroize();
        Ok(signable_tx)
    }
}

// ---

struct KeydataSignerInner {
    keys: HashMap<Address, [u8; 32]>,
}

pub struct KeydataSigner {
    inner: Arc<KeydataSignerInner>,
}

impl KeydataSigner {
    pub fn new(keydata: Vec<(Address, secp256k1::SecretKey)>) -> Self {
        let keys = keydata.into_iter().map(|(address, key)| (address, key.to_bytes())).collect();
        Self { inner: Arc::new(KeydataSignerInner { keys }) }
    }
}

impl SignerT for KeydataSigner {
    fn try_sign(&self, mutable_tx: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction> {
        let mut keys_for_signing = addresses.iter().map(|address| *self.inner.keys.get(address).unwrap()).collect::<Vec<_>>();
        // TODO - refactor for multisig
        let signable_tx = sign_with_multiple_v2(mutable_tx, &keys_for_signing).fully_signed()?;
        keys_for_signing.zeroize();
        Ok(signable_tx)
    }
}
