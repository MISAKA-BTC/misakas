//!
//! Transaction signing trait and generic signer implementations..
//!

use crate::imports::*;
#[cfg(feature = "legacy-secp256k1")]
use kaspa_bip32::PrivateKey;
#[cfg(feature = "legacy-secp256k1")]
use kaspa_consensus_core::sign::sign_with_multiple_v2;
use kaspa_consensus_core::tx::SignableTransaction;
use kaspa_wallet_keys::kaspa_pq::sign_transaction_inputs_mldsa87;

pub trait SignerT: Send + Sync + 'static {
    fn try_sign(&self, transaction: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction>;
}

struct Inner {
    keydata: PrvKeyData,
    account: Arc<dyn Account>,
    payment_secret: Option<Secret>,
    // kaspa-pq PQ-only (ADR-0019 §14): the per-address secp256k1 key cache is only
    // used by the classical signing path (`ingest`).
    #[cfg(feature = "legacy-secp256k1")]
    keys: Mutex<AHashMap<Address, [u8; 32]>>,
}

pub struct Signer {
    inner: Arc<Inner>,
}

impl Signer {
    pub fn new(account: Arc<dyn Account>, keydata: PrvKeyData, payment_secret: Option<Secret>) -> Self {
        Self {
            inner: Arc::new(Inner {
                keydata,
                account,
                payment_secret,
                #[cfg(feature = "legacy-secp256k1")]
                keys: Mutex::new(AHashMap::new()),
            }),
        }
    }

    #[cfg(feature = "legacy-secp256k1")]
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
            sign_transaction_inputs_mldsa87(&keypair, &mut mutable_tx, |_i, sig_hash| {
                // audit M-05/M-06: per-input ML-DSA signing randomness = a DETERMINISTIC domain-keyed
                // BLAKE2b over the key's public hash and the input's sighash (a full 32-byte, per-key,
                // per-input value), mirroring the WASM signer's `root ⊕ sighash`. NO OS RNG / secret
                // entropy — ML-DSA is secure even fully deterministic, so this is hygiene, not hedging.
                keypair.deterministic_input_signing_randomness(sig_hash)
            });
            return Ok(mutable_tx);
        }

        #[cfg(feature = "legacy-secp256k1")]
        {
            self.ingest(addresses)?;

            let keys = self.inner.keys.lock().unwrap();
            let mut keys_for_signing = addresses.iter().map(|address| *keys.get(address).unwrap()).collect::<Vec<_>>();
            // TODO - refactor for multisig
            let signable_tx = sign_with_multiple_v2(mutable_tx, &keys_for_signing).fully_signed()?;
            keys_for_signing.zeroize();
            Ok(signable_tx)
        }
        // kaspa-pq PQ-only (ADR-0019 §14): there is no secp256k1 signing path. Every
        // spendable account is ML-DSA-87 and is handled by `try_pq_keypair` above; an
        // account that yields neither a PQ keypair nor secp keys cannot be signed.
        #[cfg(not(feature = "legacy-secp256k1"))]
        {
            let _ = (addresses, mutable_tx);
            Err(Error::custom("kaspa-pq PQ-only: account has no ML-DSA signing key (legacy secp256k1 signing is disabled)"))
        }
    }
}

// ---

#[cfg(feature = "legacy-secp256k1")]
struct KeydataSignerInner {
    keys: HashMap<Address, [u8; 32]>,
}

#[cfg(feature = "legacy-secp256k1")]
pub struct KeydataSigner {
    inner: Arc<KeydataSignerInner>,
}

#[cfg(feature = "legacy-secp256k1")]
impl KeydataSigner {
    pub fn new(keydata: Vec<(Address, secp256k1::SecretKey)>) -> Self {
        let keys = keydata.into_iter().map(|(address, key)| (address, key.to_bytes())).collect();
        Self { inner: Arc::new(KeydataSignerInner { keys }) }
    }
}

#[cfg(feature = "legacy-secp256k1")]
impl SignerT for KeydataSigner {
    fn try_sign(&self, mutable_tx: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction> {
        let mut keys_for_signing = addresses.iter().map(|address| *self.inner.keys.get(address).unwrap()).collect::<Vec<_>>();
        // TODO - refactor for multisig
        let signable_tx = sign_with_multiple_v2(mutable_tx, &keys_for_signing).fully_signed()?;
        keys_for_signing.zeroize();
        Ok(signable_tx)
    }
}
