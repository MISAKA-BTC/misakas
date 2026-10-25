//! kaspa-pq Phase 5: ML-DSA-87 wallet key derivation.
//!
//! BIP32-style hierarchical key derivation assumes a discrete-log-friendly
//! curve (secp256k1) and is therefore unavailable for an ML-DSA-87 wallet.
//! kaspa-pq replaces it with a domain-separated XOF keyed by the BIP39
//! master seed:
//!
//! ```text
//!   keygen_seed =
//!       BLAKE2b-256(
//!           key   = b"kaspa-pq-wallet-v1/mldsa87/keygen",
//!           input = network_id || account_le || change_le || index_le || master_seed,
//!       )
//!   (verification_key, signing_key) = ML-DSA-87.KeyGen(keygen_seed)
//!   address = (prefix, Version::PubKeyHashMlDsa87,
//!             keyed_BLAKE2b-512("kaspa-pq-v2/address/mldsa87", verification_key))  // md2 §4.2 / ADR-0019 §8
//! ```
//!
//! See docs/kaspa-pq-spec.md §8 for the normative spec. Phase 5 keeps the
//! derivation deterministic and side-effect free; persistent storage of
//! the master seed and the wallet-CLI plumbing
//! (`create`/`show-address`/`build-tx`/`sign-tx`/`submit-tx`) are
//! follow-ups on top of this module.

use blake2b_simd::Params;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_txscript::{MLDSA87_PK_LEN, MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT};
use libcrux_ml_dsa::ml_dsa_87;
use zeroize::Zeroize;

/// Domain separator for the kaspa-pq wallet keygen XOF. Used as the BLAKE2b
/// key (max 64 bytes; this string is 33 bytes).
pub const KASPA_PQ_WALLET_KEYGEN_DOMAIN: &[u8] = b"kaspa-pq-wallet-v1/mldsa87/keygen";

/// Domain separator (BLAKE2b key) for per-input ML-DSA-87 transaction-signing DETERMINISTIC
/// randomness ([`KaspaPqMlDsa87KeyPair::deterministic_input_signing_randomness`], audit M-05/M-06).
/// (Wire value unchanged to preserve existing signatures; only the Rust identifier was clarified.)
pub const KASPA_PQ_SIGNING_HEDGE_DOMAIN: &[u8] = b"kaspa-pq-wallet-v1/mldsa87/sign-hedge";

/// kaspa-pq (ADR-0019 §13): true when legacy secp256k1 addresses must not be
/// produced for `network` — i.e. its consensus params enforce PQ-only
/// (`PqEnforcementMode::Consensus`). The wallet-key `to_address` /
/// `to_address_ecdsa` helpers (Keypair / PublicKey / XOnlyPublicKey /
/// PrivateKey) return [`crate::error::Error::LegacyAddressDisabled`] in that
/// case: a PQ-only wallet must not hand out a spendable legacy address.
///
/// The consensus `Params` type is referenced fully-qualified because this
/// module already binds `Params` to `blake2b_simd::Params`.
pub fn legacy_address_disabled(network: kaspa_consensus_core::network::NetworkType) -> bool {
    use kaspa_consensus_core::config::params::PqEnforcementMode;
    matches!(kaspa_consensus_core::config::params::Params::from(network).pq_enforcement, PqEnforcementMode::Consensus)
}

/// kaspa-pq ML-DSA-87 wallet keypair, deterministically derived from a
/// 32-byte `keygen_seed` (see [`derive_keygen_seed`]).
pub struct KaspaPqMlDsa87KeyPair {
    inner: ml_dsa_87::MLDSA87KeyPair,
}

impl KaspaPqMlDsa87KeyPair {
    /// Build a fresh keypair from a 32-byte deterministic seed. The seed
    /// should come from [`derive_keygen_seed`] in production paths so the
    /// address can be recomputed from the BIP39 mnemonic + account/index
    /// alone.
    pub fn from_seed(mut seed: [u8; 32]) -> Self {
        let inner = ml_dsa_87::generate_key_pair(seed);
        // audit QM-2: scrub the keygen seed (the master secret that re-derives this keypair) from
        // the stack after use, so it does not linger in a core dump / swap. The libcrux key material
        // itself is opaque (no upstream Zeroize), so the seed is the highest-value secret we control.
        seed.zeroize();
        Self { inner }
    }

    /// 2592-byte ML-DSA-87 public key bytes. This is exactly
    /// `MLDSA87_PK_LEN` long.
    pub fn public_key_bytes(&self) -> &[u8; MLDSA87_PK_LEN] {
        // The libcrux constants match ours by construction (Phase 1 spec).
        self.inner.verification_key.as_ref()
    }

    /// 64-byte address payload: keyed `BLAKE2b-512(public_key)` under
    /// `kaspa-pq-v2/address/mldsa87` (md2 §4.2 / ADR-0019 §8). Delegates to the
    /// shared [`kaspa_hashes::blake2b_512_address_payload`] so this stays in
    /// lock-step with the `OP_BLAKE2B_512` consensus opcode and the premine.
    pub fn public_key_hash(&self) -> [u8; 64] {
        kaspa_hashes::blake2b_512_address_payload(self.public_key_bytes()).as_bytes()
    }

    /// kaspa-pq P2PKH `Address` for the given network prefix.
    pub fn address(&self, prefix: Prefix) -> Address {
        Address::new(prefix, Version::PubKeyHashMlDsa87, &self.public_key_hash())
    }

    /// Sign an arbitrary message with the kaspa-pq transaction context
    /// ([`MLDSA87_TX_CONTEXT`]). Returns the 4627-byte signature bytes.
    ///
    /// The caller is responsible for choosing `message` correctly — for a
    /// transaction input that means the sighash digest from
    /// `kaspa_consensus_core::hashing::sighash::calc_schnorr_signature_hash`.
    /// `signing_randomness` is 32 bytes of fresh randomness per signature;
    /// reusing it across signatures is **not** required for ML-DSA security
    /// (the scheme is hedged-randomized), but reusing the *same* signing
    /// key with predictable randomness is bad hygiene.
    pub fn sign(&self, message: &[u8], signing_randomness: [u8; 32]) -> [u8; MLDSA87_SIG_LEN] {
        self.sign_with_context(message, MLDSA87_TX_CONTEXT, signing_randomness)
    }

    /// kaspa-pq Phase 11 (ADR-0010): sign `message` under an explicit ML-DSA-87
    /// `context` (domain separator). [`sign`](Self::sign) uses the transaction
    /// context (`MLDSA87_TX_CONTEXT`); the in-process validator service signs
    /// stake attestations with `dns_finality::ATTESTATION_MLDSA87_CONTEXT` so the
    /// produced signature verifies via
    /// `kaspa_txscript::verify_mldsa87_with_context` and can never be replayed
    /// as a transaction signature (distinct context ⇒ distinct domain).
    pub fn sign_with_context(&self, message: &[u8], context: &[u8], signing_randomness: [u8; 32]) -> [u8; MLDSA87_SIG_LEN] {
        // audit L: ML-DSA `sign` only fails for an over-long (>255-byte) context; callers pass a
        // short fixed domain-separator, so this precondition makes the unreachable failure an
        // explicit, attributed panic rather than an opaque libcrux error.
        assert!(context.len() <= 255, "ML-DSA signing context must be <= 255 bytes, got {}", context.len());
        let sig = ml_dsa_87::sign(&self.inner.signing_key, message, context, signing_randomness)
            .expect("ML-DSA-87 sign is infallible for a <= 255-byte context");
        // `MLDSA87Signature::as_ref()` returns `&[u8; SIGNATURE_SIZE]`.
        *sig.as_ref()
    }

    /// Per-input ML-DSA-87 **deterministic** signing randomness (audit M-05/M-06). Despite ML-DSA's
    /// `rnd` parameter being called "hedging" in FIPS-204, this derives the value DETERMINISTICALLY —
    /// a domain-keyed BLAKE2b over the public-key hash and the input's sighash — with NO OS RNG and
    /// NO secret entropy (the keypair deliberately never exposes its secret, and wallet-core also
    /// compiles to WASM where OS RNG is constrained). Mirrors the WASM signer's `root ⊕ sighash` and
    /// replaces the old 8-byte-index randomness. This is safe BECAUSE ML-DSA stays secure even fully
    /// deterministic (re-using `rnd` across distinct messages is fine, unlike ECDSA/Schnorr): it is a
    /// hygiene improvement (a distinct value per input), NOT fresh hedging entropy (audit M-06).
    pub fn deterministic_input_signing_randomness(&self, sig_hash: &[u8; 64]) -> [u8; 32] {
        let mut state = Params::new().hash_length(32).key(KASPA_PQ_SIGNING_HEDGE_DOMAIN).to_state();
        state.update(&self.public_key_hash());
        state.update(sig_hash);
        let mut out = [0u8; 32];
        out.copy_from_slice(state.finalize().as_bytes());
        out
    }
}

/// Derive the 32-byte ML-DSA-87 keygen seed from BIP39-style inputs.
///
/// Inputs are mixed via a keyed BLAKE2b-256 with
/// [`KASPA_PQ_WALLET_KEYGEN_DOMAIN`] as the key. The exact wire form is:
///
/// ```text
///   keyed_blake2b_256(
///       key   = KASPA_PQ_WALLET_KEYGEN_DOMAIN,
///       input = len(network_id)_le_u32 || network_id_bytes || account_le_u32 || change_le_u32
///               || index_le_u32 || len(master_seed)_le_u32 || master_seed,
///   )
/// ```
///
/// `network_id` is the kaspa-pq [`NetworkId::to_string`] form
/// (`"mainnet"`, `"testnet-10"`, etc.) so that the same BIP39 mnemonic on
/// mainnet and testnet produces distinct addresses. audit L (domain
/// separation): the two variable-length fields (network_id, master_seed) are
/// length-prefixed so the concatenation is unambiguous (no field can borrow a
/// byte from its neighbour). NOTE: this changed the derivation, so addresses
/// derived by an older build differ — re-derive wallets after this change.
pub fn derive_keygen_seed(network_id: &str, account: u32, change: u32, index: u32, master_seed: &[u8]) -> [u8; 32] {
    let mut state = Params::new().hash_length(32).key(KASPA_PQ_WALLET_KEYGEN_DOMAIN).to_state();
    // audit L: length-prefix the variable-length fields so the concatenation is unambiguous
    // (e.g. network_id "main" can never alias "mainnet"). Fixed-width u32 fields need no prefix.
    state.update(&(network_id.len() as u32).to_le_bytes());
    state.update(network_id.as_bytes());
    state.update(&account.to_le_bytes());
    state.update(&change.to_le_bytes());
    state.update(&index.to_le_bytes());
    state.update(&(master_seed.len() as u32).to_le_bytes());
    state.update(master_seed);
    let mut out = [0u8; 32];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// One-shot helper: derive a keygen seed and materialise the keypair.
pub fn derive_keypair(network_id: &str, account: u32, change: u32, index: u32, master_seed: &[u8]) -> KaspaPqMlDsa87KeyPair {
    KaspaPqMlDsa87KeyPair::from_seed(derive_keygen_seed(network_id, account, change, index, master_seed))
}

/// kaspa-pq PQ-only (ADR-0019 §13): native (non-WASM) ML-DSA-87 transaction
/// signer. Signs every input of `mutable_tx` whose previous-output P2PKH locks
/// to `keypair`'s address, producing the canonical unlock script
/// `<signature || sighash_type> <public_key>` for each. The signed message is
/// the 64-byte [`calc_mldsa87_signature_hash`] under `SIG_HASH_ALL` — the exact
/// digest the `OP_CHECKSIG_MLDSA87` consensus opcode recomputes — so this is
/// byte-for-byte equivalent to the WASM `signTransactionMlDsa87` helper, just
/// reachable from native Rust (wallet generator, CLI, tests).
///
/// `per_input_randomness(i)` supplies 32 bytes of signing randomness for input
/// `i` (ML-DSA is hedged-randomized; distinct values are tidy but not required
/// for security). Returns the number of inputs signed.
///
/// The transaction's inputs must be fully UTXO-populated (`entries` all `Some`).
/// Inputs already carrying a non-empty signature script, or whose previous
/// output is not this keypair's P2PKH, are left untouched.
pub fn sign_transaction_inputs_mldsa87(
    keypair: &KaspaPqMlDsa87KeyPair,
    mutable_tx: &mut kaspa_consensus_core::tx::SignableTransaction,
    per_input_randomness: impl Fn(usize, &[u8; 64]) -> [u8; 32],
) -> usize {
    use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
    use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
    use kaspa_txscript::pay_to_address_script;

    // The kaspa-pq P2PKH scriptPubKey this keypair owns; only inputs spending it
    // are signed here (single-key wallet semantics).
    let owned_spk = pay_to_address_script(&keypair.address(Prefix::Mainnet));
    // Address prefix doesn't affect the script bytes (the prefix is bech32-only),
    // so comparing the 69-byte script is prefix-independent.
    let owned_script = owned_spk.script().to_vec();

    let reused = Mldsa87SigHashReusedValuesUnsync::new();
    let input_len = mutable_tx.tx.inputs.len();
    let mut signed = 0usize;
    for i in 0..input_len {
        if !mutable_tx.tx.inputs[i].signature_script.is_empty() {
            continue;
        }
        let Some(entry) = mutable_tx.entries.get(i).and_then(|e| e.as_ref()) else {
            continue;
        };
        if entry.script_public_key.script() != owned_script.as_slice() {
            continue;
        }
        let sig_hash = {
            let verifiable = mutable_tx.as_verifiable();
            calc_mldsa87_signature_hash(&verifiable, i, SIG_HASH_ALL, &reused)
        };
        let sig_hash_bytes = sig_hash.as_bytes();
        let sig = keypair.sign(sig_hash_bytes.as_slice(), per_input_randomness(i, &sig_hash_bytes));
        // OP_CHECKSIG_MLDSA87 pops [sig, key] and strips the trailing sighash-type
        // byte from the signature, mirroring schnorr OP_CHECKSIG.
        let mut sig_item = Vec::with_capacity(MLDSA87_SIG_LEN + 1);
        sig_item.extend_from_slice(&sig);
        sig_item.push(SIG_HASH_ALL.to_u8());
        let script = kaspa_txscript::script_builder::ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(keypair.public_key_bytes())
            .expect("ML-DSA public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        mutable_tx.tx.inputs[i].signature_script = script;
        signed += 1;
    }
    signed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed BIP39-style 64-byte master seed (placeholder for
    /// tests — production paths derive this from
    /// `kaspa_bip32::Mnemonic::to_seed`).
    const TEST_MASTER_SEED: [u8; 64] = [0xab; 64];

    #[test]
    fn derivation_is_deterministic() {
        let a = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let b = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        assert_eq!(a, b);
    }

    #[test]
    fn legacy_address_disabled_on_all_kaspa_pq_nets() {
        use kaspa_consensus_core::network::NetworkType;
        // Every kaspa-pq preset sets PqEnforcementMode::Consensus, so legacy
        // secp256k1 addresses are disabled on every network of this chain.
        for net in [NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet] {
            assert!(legacy_address_disabled(net), "{net:?} must disable legacy secp256k1 addresses");
        }
    }

    #[test]
    fn network_id_separates_keys() {
        let mainnet = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let testnet = derive_keygen_seed("testnet-10", 0, 0, 0, &TEST_MASTER_SEED);
        assert_ne!(mainnet, testnet);
    }

    #[test]
    fn index_separates_keys() {
        let i0 = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let i1 = derive_keygen_seed("mainnet", 0, 0, 1, &TEST_MASTER_SEED);
        assert_ne!(i0, i1);
    }

    #[test]
    fn account_separates_keys() {
        let a0 = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let a1 = derive_keygen_seed("mainnet", 1, 0, 0, &TEST_MASTER_SEED);
        assert_ne!(a0, a1);
    }

    #[test]
    fn change_separates_keys() {
        let receive = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let change = derive_keygen_seed("mainnet", 0, 1, 0, &TEST_MASTER_SEED);
        assert_ne!(receive, change);
    }

    #[test]
    fn keypair_round_trip_and_address_shape() {
        let kp = derive_keypair("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        assert_eq!(kp.public_key_bytes().len(), MLDSA87_PK_LEN);

        let mainnet = kp.address(Prefix::Mainnet);
        let s: String = mainnet.into();
        assert!(s.starts_with("misaka:"), "got {s}");

        let testnet = kp.address(Prefix::Testnet);
        let s_tn: String = testnet.into();
        assert!(s_tn.starts_with("misakatest:"), "got {s_tn}");
    }

    #[test]
    fn sign_and_locally_verify() {
        // Sanity check that a signature produced by `KaspaPqMlDsa87KeyPair::sign`
        // verifies under `libcrux_ml_dsa::ml_dsa_87::verify` with the
        // kaspa-pq context. (The script engine's hash-keyed
        // `check_mldsa87_signature` is tested end-to-end in
        // `kaspa-txscript`'s `test_mldsa87_p2pkh_spend_roundtrip`.)
        let kp = derive_keypair("simnet", 0, 0, 7, &TEST_MASTER_SEED);
        let msg = b"kaspa-pq Phase 5 wallet derivation smoke test";
        let randomness = [0x33u8; 32];
        let sig_bytes = kp.sign(msg, randomness);
        assert_eq!(sig_bytes.len(), MLDSA87_SIG_LEN);

        let vk = libcrux_ml_dsa::ml_dsa_87::MLDSA87VerificationKey::new(*kp.public_key_bytes());
        let sig = libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(sig_bytes);
        libcrux_ml_dsa::ml_dsa_87::verify(&vk, msg, MLDSA87_TX_CONTEXT, &sig)
            .expect("kaspa-pq wallet signature must verify under the kaspa-pq tx context");
    }

    #[test]
    fn sign_with_context_roundtrip_and_separation() {
        // PR-11.4 (Phase 11): a signature produced under an explicit context
        // verifies only under that same context — the domain-separation property
        // the validator service relies on so an attestation signature can never
        // be replayed as a transaction signature.
        let kp = derive_keypair("simnet", 0, 0, 9, &TEST_MASTER_SEED);
        let msg = b"phase 11 attestation digest";
        let att_ctx = b"kaspa-pq-v1/att/mldsa87"; // == dns_finality::ATTESTATION_MLDSA87_CONTEXT
        let sig_bytes = kp.sign_with_context(msg, att_ctx, [0x44u8; 32]);
        assert_eq!(sig_bytes.len(), MLDSA87_SIG_LEN);

        let vk = libcrux_ml_dsa::ml_dsa_87::MLDSA87VerificationKey::new(*kp.public_key_bytes());
        let sig = libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(sig_bytes);
        // Verifies under the same (attestation) context...
        libcrux_ml_dsa::ml_dsa_87::verify(&vk, msg, att_ctx, &sig).expect("must verify under the signing context");
        // ...but NOT under the transaction context (domain separation).
        assert!(
            libcrux_ml_dsa::ml_dsa_87::verify(&vk, msg, MLDSA87_TX_CONTEXT, &sig).is_err(),
            "an attestation-context signature must not verify as a transaction signature"
        );
    }

    #[test]
    fn signature_does_not_verify_under_wrong_context() {
        let kp = derive_keypair("simnet", 0, 0, 1, &TEST_MASTER_SEED);
        let msg = b"context-binding test";
        let sig_bytes = kp.sign(msg, [0x11u8; 32]);
        let vk = libcrux_ml_dsa::ml_dsa_87::MLDSA87VerificationKey::new(*kp.public_key_bytes());
        let sig = libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(sig_bytes);
        // Wrong context => verify must reject.
        assert!(
            libcrux_ml_dsa::ml_dsa_87::verify(&vk, msg, b"not-the-kaspa-pq-context", &sig).is_err(),
            "ML-DSA must reject under a different ctx — domain separation is the whole point",
        );
    }

    #[test]
    fn native_signer_round_trips_through_engine() {
        // kaspa-pq PQ-only (ADR-0019 §13): a transaction signed by the native
        // ML-DSA-87 signer must verify under the kaspa_txscript engine — i.e. the
        // native signer is byte-for-byte compatible with the consensus verifier.
        use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
        use kaspa_consensus_core::tx::{
            PopulatedTransaction, SignableTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint,
            TransactionOutput, UtxoEntry,
        };
        use kaspa_txscript::caches::Cache;
        use kaspa_txscript::{TxScriptEngine, pay_to_address_script};

        let kp = derive_keypair("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let spk = pay_to_address_script(&kp.address(Prefix::Mainnet));
        assert_eq!(spk.script().len(), 69, "ML-DSA-87 P2PKH spk is 69 bytes");

        // A 1-input / 1-output spend of a UTXO locked to this keypair.
        let prev = TransactionOutpoint { transaction_id: TransactionId::from_bytes([0x33u8; 64]), index: 0 };
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
        let mut signable: SignableTransaction = SignableTransaction::with_entries(tx, vec![entry.clone()]);

        let n = sign_transaction_inputs_mldsa87(&kp, &mut signable, |i, _sig_hash| [0x40u8 ^ (i as u8); 32]);
        assert_eq!(n, 1, "exactly one input signed");
        assert!(!signable.tx.inputs[0].signature_script.is_empty(), "input 0 now has an unlock script");

        // Verify via the consensus script engine.
        let populated = PopulatedTransaction::new(&signable.tx, vec![entry]);
        let reused = SigHashReusedValuesUnsync::new();
        let cache = Cache::new(10_000);
        let mut vm =
            TxScriptEngine::from_transaction_input(&populated, &populated.tx.inputs[0], 0, &populated.entries[0], &reused, &cache);
        vm.execute().expect("native ML-DSA-87 signature must verify in the script engine");
    }

    #[test]
    fn native_signer_skips_foreign_and_prefilled_inputs() {
        use kaspa_consensus_core::tx::{
            SignableTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        };
        use kaspa_txscript::pay_to_address_script;

        let kp = derive_keypair("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let mine = pay_to_address_script(&kp.address(Prefix::Mainnet));
        // A different keypair's spk (foreign input — must be skipped).
        let other = derive_keypair("mainnet", 9, 0, 0, &TEST_MASTER_SEED);
        let foreign = pay_to_address_script(&other.address(Prefix::Mainnet));

        let mk_in = |pre_filled: bool| TransactionInput {
            previous_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_bytes([0x01u8; 64]), index: 0 },
            signature_script: if pre_filled { vec![0xaa] } else { vec![] },
            sequence: 0,
            sig_op_count: 1,
        };
        let tx = Transaction::new(
            0,
            vec![mk_in(false), mk_in(true), mk_in(false)],
            vec![TransactionOutput { value: 1, script_public_key: mine.clone() }],
            0,
            Default::default(),
            0,
            vec![],
        );
        let entries = vec![
            UtxoEntry { amount: 10, script_public_key: mine.clone(), block_daa_score: 0, is_coinbase: false }, // signable
            UtxoEntry { amount: 10, script_public_key: mine.clone(), block_daa_score: 0, is_coinbase: false }, // pre-filled -> skip
            UtxoEntry { amount: 10, script_public_key: foreign, block_daa_score: 0, is_coinbase: false },      // foreign -> skip
        ];
        let mut signable: SignableTransaction = SignableTransaction::with_entries(tx, entries);
        let n = sign_transaction_inputs_mldsa87(&kp, &mut signable, |_, _sig_hash| [0x77u8; 32]);
        assert_eq!(n, 1, "only the empty, owned input is signed");
        assert!(!signable.tx.inputs[0].signature_script.is_empty());
        assert_eq!(signable.tx.inputs[1].signature_script, vec![0xaa], "pre-filled input untouched");
        assert!(signable.tx.inputs[2].signature_script.is_empty(), "foreign input untouched");
    }
}
