use crate::MLDSA87_PK_LEN;
use crate::opcodes::codes::{OpCheckMultiSig, OpCheckMultiSigECDSA, OpCheckMultiSigMlDsa87};
use crate::script_builder::{ScriptBuilder, ScriptBuilderError};
use std::borrow::Borrow;
use thiserror::Error;

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum Error {
    // ErrTooManyRequiredSigs is returned from multisig_script when the
    // specified number of required signatures is larger than the number of
    // provided public keys.
    #[error("too many required signatures")]
    ErrTooManyRequiredSigs,
    #[error(transparent)]
    ScriptBuilderError(#[from] ScriptBuilderError),
    #[error("provided public keys should not be empty")]
    EmptyKeys,
}
pub fn multisig_redeem_script(pub_keys: impl Iterator<Item = impl Borrow<[u8; 32]>>, required: usize) -> Result<Vec<u8>, Error> {
    if pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    let mut builder = ScriptBuilder::new();
    builder.add_i64(required as i64)?;

    let mut count = 0i64;
    for pub_key in pub_keys {
        count += 1;
        builder.add_data(pub_key.borrow().as_slice())?;
    }

    if (count as usize) < required {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    if count == 0 {
        return Err(Error::EmptyKeys);
    }

    builder.add_i64(count)?;
    builder.add_op(OpCheckMultiSig)?;

    Ok(builder.drain())
}

pub fn multisig_redeem_script_ecdsa(pub_keys: impl Iterator<Item = impl Borrow<[u8; 33]>>, required: usize) -> Result<Vec<u8>, Error> {
    if pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    let mut builder = ScriptBuilder::new();
    builder.add_i64(required as i64)?;

    let mut count = 0i64;
    for pub_key in pub_keys {
        count += 1;
        builder.add_data(pub_key.borrow().as_slice())?;
    }

    if (count as usize) < required {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    if count == 0 {
        return Err(Error::EmptyKeys);
    }

    builder.add_i64(count)?;
    builder.add_op(OpCheckMultiSigECDSA)?;

    Ok(builder.drain())
}

/// Build an M-of-N **ML-DSA-87** multisig redeem script (kaspa-pq, post-quantum):
/// `<required> <pk_1> <pk_2> ... <pk_N> <N> OP_CHECKMULTISIGMLDSA87`.
///
/// Each public key must be the 2592-byte ML-DSA-87 (FIPS 204) encoding.
///
/// **CONSENSUS-DISABLED in PQ-only (audit M-04).** This redeem script is meant to be wrapped in
/// P2SH via [`crate::pay_to_script_hash_script`], but the PQ-only output gate
/// (`ScriptClass::PubKeyHashMlDsa87` only — see `script_class.rs` / `tx_validation_in_isolation`)
/// **rejects P2SH outputs as non-standard**, so coins sent to a P2SH-wrapped multisig of this
/// script are unspendable on the launch network. This builder therefore exists only for tests and
/// future use (a PQ-native multisig *output* class would be a separate design); it is
/// `#[doc(hidden)]` and **must NOT be surfaced by the wallet/CLI as a spendable address** — doing so
/// would lock funds. Note the large size: each key adds ~2594 bytes, so an M-of-N spend script is
/// constrained by `MAX_SCRIPTS_SIZE`.
#[doc(hidden)]
pub fn multisig_redeem_script_mldsa87(
    pub_keys: impl Iterator<Item = impl Borrow<[u8; MLDSA87_PK_LEN]>>,
    required: usize,
) -> Result<Vec<u8>, Error> {
    if pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    let mut builder = ScriptBuilder::new();
    builder.add_i64(required as i64)?;

    let mut count = 0i64;
    for pub_key in pub_keys {
        count += 1;
        builder.add_data(pub_key.borrow().as_slice())?;
    }

    if (count as usize) < required {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    if count == 0 {
        return Err(Error::EmptyKeys);
    }

    builder.add_i64(count)?;
    builder.add_op(OpCheckMultiSigMlDsa87)?;

    Ok(builder.drain())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TxScriptEngine, caches::Cache, pay_to_script_hash_script};
    use core::str::FromStr;
    use kaspa_consensus_core::{
        hashing::{
            sighash::{SigHashReusedValuesUnsync, calc_schnorr_signature_hash},
            sighash_type::SIG_HASH_ALL,
        },
        subnets::SubnetworkId,
        tx::*,
    };
    // kaspa-pq PQ-only: the legacy secp256k1 multisig sign/verify tests below
    // compile only under `legacy-secp256k1` (ADR-0019 §14).
    #[cfg(feature = "legacy-secp256k1")]
    use kaspa_utils::hex::FromHex;
    #[cfg(feature = "legacy-secp256k1")]
    use rand::thread_rng;
    #[cfg(feature = "legacy-secp256k1")]
    use secp256k1::Keypair;
    use std::{iter, iter::empty};

    #[cfg(feature = "legacy-secp256k1")]
    struct Input {
        kp: Keypair,
        required: bool,
        sign: bool,
    }

    #[cfg(feature = "legacy-secp256k1")]
    fn kp() -> [Keypair; 3] {
        let kp1 = Keypair::from_seckey_slice(
            secp256k1::SECP256K1,
            Vec::from_hex("1d99c236b1f37b3b845336e6c568ba37e9ced4769d83b7a096eec446b940d160").unwrap().as_slice(),
        )
        .unwrap();
        let kp2 = Keypair::from_seckey_slice(
            secp256k1::SECP256K1,
            Vec::from_hex("349ca0c824948fed8c2c568ce205e9d9be4468ef099cad76e3e5ec918954aca4").unwrap().as_slice(),
        )
        .unwrap();
        let kp3 = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());
        [kp1, kp2, kp3]
    }

    #[test]
    fn test_too_many_required_sigs() {
        let result = multisig_redeem_script(iter::once([0u8; 32]), 2);
        assert_eq!(result, Err(Error::ErrTooManyRequiredSigs));
        let result = multisig_redeem_script_ecdsa(iter::once(&[0u8; 33]), 2);
        assert_eq!(result, Err(Error::ErrTooManyRequiredSigs));
    }

    #[test]
    fn test_empty_keys() {
        let result = multisig_redeem_script(empty::<[u8; 32]>(), 0);
        assert_eq!(result, Err(Error::EmptyKeys));
    }

    #[cfg(feature = "legacy-secp256k1")]
    fn check_multisig_scenario(inputs: Vec<Input>, required: usize, is_ok: bool, is_ecdsa: bool) {
        use crate::opcodes::codes::OpData65;
        use kaspa_consensus_core::hashing::sighash::calc_ecdsa_signature_hash;
        // Taken from: d839d29b549469d0f9a23e51febe68d4084967a6a477868b511a5a8d88c5ae06
        // PR-9.5c: TransactionId is now Hash64 (128 hex chars); the original 64-char
        // fixture is zero-extended. The value is arbitrary — this is a sign/verify
        // roundtrip, so any valid txid works.
        let prev_tx_id = TransactionId::from_str(
            "63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff840000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        let filtered = inputs.iter().filter(|input| input.required);
        let script = if !is_ecdsa {
            let pks = filtered.map(|input| input.kp.x_only_public_key().0.serialize());
            multisig_redeem_script(pks, required).unwrap()
        } else {
            let pks = filtered.map(|input| input.kp.public_key().serialize());
            multisig_redeem_script_ecdsa(pks, required).unwrap()
        };

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let entries = vec![UtxoEntry {
            amount: 12793000000000,
            script_public_key: pay_to_script_hash_script(&script),
            block_daa_score: 36151168,
            is_coinbase: false,
        }];
        let mut tx = MutableTransaction::with_entries(tx, entries);

        let reused_values = SigHashReusedValuesUnsync::new();
        let sig_hash = if !is_ecdsa {
            calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_values)
        } else {
            calc_ecdsa_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_values)
        };
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
        let signatures: Vec<_> = inputs
            .iter()
            .filter(|input| input.sign)
            .flat_map(|input| {
                if !is_ecdsa {
                    let sig = *input.kp.sign_schnorr(msg).as_ref();
                    iter::once(OpData65).chain(sig).chain([SIG_HASH_ALL.to_u8()])
                } else {
                    let sig = input.kp.secret_key().sign_ecdsa(msg).serialize_compact();
                    iter::once(OpData65).chain(sig).chain([SIG_HASH_ALL.to_u8()])
                }
            })
            .collect();

        {
            tx.tx.inputs[0].signature_script =
                signatures.into_iter().chain(ScriptBuilder::new().add_data(&script).unwrap().drain()).collect();
        }

        let tx = tx.as_verifiable();
        let (input, entry) = tx.populated_inputs().next().unwrap();

        let cache = Cache::new(10_000);
        let mut engine = TxScriptEngine::from_transaction_input(&tx, input, 0, entry, &reused_values, &cache);
        assert_eq!(engine.execute().is_ok(), is_ok);
    }
    #[cfg(feature = "legacy-secp256k1")]
    #[test]
    fn test_multisig_1_2() {
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: false }, Input { kp: kp2, required: true, sign: true }],
            1,
            true,
            false,
        );
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: true }, Input { kp: kp2, required: true, sign: false }],
            1,
            true,
            false,
        );

        // ecdsa
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: false }, Input { kp: kp2, required: true, sign: true }],
            1,
            true,
            true,
        );
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: true }, Input { kp: kp2, required: true, sign: false }],
            1,
            true,
            true,
        );
    }

    #[cfg(feature = "legacy-secp256k1")]
    #[test]
    fn test_multisig_2_2() {
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: true }, Input { kp: kp2, required: true, sign: true }],
            2,
            true,
            false,
        );

        // ecdsa
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: true }, Input { kp: kp2, required: true, sign: true }],
            2,
            true,
            true,
        );
    }

    #[cfg(feature = "legacy-secp256k1")]
    #[test]
    fn test_multisig_wrong_signer() {
        let [kp1, kp2, kp3] = kp();
        check_multisig_scenario(
            vec![
                Input { kp: kp1, required: true, sign: false },
                Input { kp: kp2, required: true, sign: false },
                Input { kp: kp3, required: false, sign: true },
            ],
            1,
            false,
            false,
        );

        // ecdsa
        let [kp1, kp2, kp3] = kp();
        check_multisig_scenario(
            vec![
                Input { kp: kp1, required: true, sign: false },
                Input { kp: kp2, required: true, sign: false },
                Input { kp: kp3, required: false, sign: true },
            ],
            1,
            false,
            true,
        );
    }

    #[cfg(feature = "legacy-secp256k1")]
    #[test]
    fn test_multisig_not_enough() {
        let [kp1, kp2, kp3] = kp();
        check_multisig_scenario(
            vec![
                Input { kp: kp1, required: true, sign: true },
                Input { kp: kp2, required: true, sign: true },
                Input { kp: kp3, required: true, sign: false },
            ],
            3,
            false,
            false,
        );

        let [kp1, kp2, kp3] = kp();
        check_multisig_scenario(
            vec![
                Input { kp: kp1, required: true, sign: true },
                Input { kp: kp2, required: true, sign: true },
                Input { kp: kp3, required: true, sign: false },
            ],
            3,
            false,
            true,
        );
    }

    /// kaspa-pq devnet multisig key generator (run manually). Derives three
    /// ML-DSA-87 keypairs from documented devnet seeds, builds the 2-of-3
    /// redeem script + P2SH, and prints the multisig address, P2SH
    /// `script_public_key` (hex), redeem-script hash, and saves seeds+pubkeys to
    /// `misaka-devnet-multisig-keys.json` so the premine can be wired and later
    /// spent. Run:
    ///   cargo test -p kaspa-txscript --lib standard::multisig::tests::gen_misaka_devnet_multisig -- --ignored --nocapture
    #[test]
    #[ignore = "generator: prints/saves devnet multisig keys"]
    fn gen_misaka_devnet_multisig() {
        use crate::MLDSA87_PK_LEN;
        use blake2b_simd::Params;
        use kaspa_addresses::{Address, Prefix, Version};
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        // Deterministic, documented devnet seeds (reproducible; DEVNET ONLY).
        let seeds: Vec<[u8; 32]> = (0..3)
            .map(|i| {
                let mut s = [0u8; 32];
                s.copy_from_slice(
                    Params::new()
                        .hash_length(32)
                        .to_state()
                        .update(format!("misaka-devnet-multisig-key-{i}").as_bytes())
                        .finalize()
                        .as_bytes(),
                );
                s
            })
            .collect();

        let keypairs: Vec<_> = seeds.iter().map(|s| mldsa::generate_key_pair(*s)).collect();
        let pubkeys: Vec<[u8; MLDSA87_PK_LEN]> = keypairs
            .iter()
            .map(|kp| {
                let mut a = [0u8; MLDSA87_PK_LEN];
                a.copy_from_slice(kp.verification_key.as_ref());
                a
            })
            .collect();

        let redeem = multisig_redeem_script_mldsa87(pubkeys.iter(), 2).unwrap();
        let redeem_hash = Params::new().hash_length(32).to_state().update(&redeem).finalize();
        let spk = pay_to_script_hash_script(&redeem);
        let addr = Address::new(Prefix::Devnet, Version::ScriptHash, redeem_hash.as_bytes()).to_string();

        println!("=== misaka devnet 2-of-3 ML-DSA-87 multisig ===");
        println!("multisig address (devnet) : {addr}");
        println!("redeem script len         : {} bytes", redeem.len());
        println!("redeem script hash (b2b)  : {}", hex(redeem_hash.as_bytes()));
        println!("P2SH spk version          : {}", spk.version());
        println!("P2SH spk script (hex)     : {}", hex(spk.script()));

        let json = format!(
            "{{\n  \"scheme\": \"ml-dsa-65 2-of-3 P2SH multisig\",\n  \"devnet_address\": \"{addr}\",\n  \"redeem_script_blake2b\": \"{}\",\n  \"p2sh_script_public_key_hex\": \"{}\",\n  \"keys\": [\n{}\n  ]\n}}\n",
            hex(redeem_hash.as_bytes()),
            hex(spk.script()),
            (0..3)
                .map(|i| format!(
                    "    {{ \"index\": {i}, \"seed_hex\": \"{}\", \"pubkey_hex\": \"{}\" }}",
                    hex(&seeds[i]),
                    hex(&pubkeys[i])
                ))
                .collect::<Vec<_>>()
                .join(",\n")
        );
        std::fs::write("misaka-devnet-multisig-keys.json", &json).expect("write keys file");
        println!("\nsaved seeds + pubkeys to misaka-devnet-multisig-keys.json (DEVNET ONLY — keep safe)");
    }

    /// End-to-end ML-DSA 2-of-3 P2SH multisig spend through the script engine:
    /// build the redeem script, wrap in P2SH, sign the sighash with M of the N
    /// keys, and execute. Exercises `OP_CHECKMULTISIGMLDSA87` (0xa7).
    ///
    /// kaspa-pq PQ-only (ADR-0019 §6.5 / docs/kaspa-pq-design-mldsa87.md §11.1):
    /// P2SH/multisig is **out of launch scope**. With ML-DSA-87 a 2-of-3 unlock
    /// is ~17 KB (2 × (3 + 4628) sig pushes + (3 + 7788) redeem push), exceeding
    /// the P2PKH-only `MAX_SCRIPTS_SIZE` (10_000), and the `redeem.len() == 5868`
    /// assertion below is ML-DSA-87-specific. Ignored until multisig returns via a
    /// dedicated ADR (redeem static-analysis class + recalculated caps).
    #[test]
    #[ignore = "P2SH/multisig out of PQ-only launch scope (ADR-0019 §6.5)"]
    fn test_multisig_mldsa87_2_of_3() {
        use crate::{MLDSA87_PK_LEN, MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT};
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        // Three deterministic ML-DSA-87 keypairs.
        let keypairs: Vec<_> = [[0x11u8; 32], [0x22u8; 32], [0x33u8; 32]].iter().map(|s| mldsa::generate_key_pair(*s)).collect();
        let pubkeys: Vec<[u8; MLDSA87_PK_LEN]> = keypairs
            .iter()
            .map(|kp| {
                let mut a = [0u8; MLDSA87_PK_LEN];
                a.copy_from_slice(kp.verification_key.as_ref());
                a
            })
            .collect();

        // 2-of-3 redeem script (5868 bytes) wrapped in P2SH.
        let redeem = multisig_redeem_script_mldsa87(pubkeys.iter(), 2).unwrap();
        assert_eq!(redeem.len(), 5868, "2-of-3 ML-DSA-87 redeem script size");
        let spk = pay_to_script_hash_script(&redeem);

        let prev_tx_id = TransactionId::from_str(
            "63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff840000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();

        // Run a spend signed by `signers` (must be ascending to match in-order
        // CHECKMULTISIG verification); assert the engine result equals `is_ok`.
        let run = |signers: &[usize], is_ok: bool| {
            let tx = Transaction::new(
                0,
                vec![TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: 3,
                }],
                vec![],
                0,
                SubnetworkId::from_bytes([0; 20]),
                0,
                vec![],
            );
            let entries =
                vec![UtxoEntry { amount: 5_000_000_000, script_public_key: spk.clone(), block_daa_score: 0, is_coinbase: false }];
            let mut tx = MutableTransaction::with_entries(tx, entries);

            let reused = SigHashReusedValuesUnsync::new();
            let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &reused);

            let mut builder = ScriptBuilder::new();
            for &i in signers {
                let sig = mldsa::sign(&keypairs[i].signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x99u8; 32])
                    .expect("ML-DSA-87 sign");
                let mut item = Vec::with_capacity(MLDSA87_SIG_LEN + 1);
                item.extend_from_slice(sig.as_ref());
                item.push(SIG_HASH_ALL.to_u8());
                builder.add_data(&item).unwrap();
            }
            builder.add_data(&redeem).unwrap();
            tx.tx.inputs[0].signature_script = builder.drain();

            let tx = tx.as_verifiable();
            let (input, entry) = tx.populated_inputs().next().unwrap();
            let cache = Cache::new(10_000);
            let mut engine = TxScriptEngine::from_transaction_input(&tx, input, 0, entry, &reused, &cache);
            assert_eq!(engine.execute().is_ok(), is_ok, "signers {signers:?} expected is_ok={is_ok}");
        };

        // Any 2 of the 3 keys (ascending order) satisfy the 2-of-3 threshold.
        run(&[0, 1], true);
        run(&[1, 2], true);
        run(&[0, 2], true);
        // Too few signatures (CHECKMULTISIG pops M=2 sigs; only 1 provided).
        run(&[0], false);
        // Wrong order fails in-order matching.
        run(&[2, 0], false);
    }
}
