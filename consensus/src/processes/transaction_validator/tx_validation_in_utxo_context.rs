use crate::constants::{MAX_SOMPI, SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK};
use kaspa_consensus_core::{
    hashing::sighash::{SigHashReusedValuesSync, SigHashReusedValuesUnsync},
    tx::{TransactionInput, VerifiableTransaction},
};
use kaspa_txscript::{ScriptPolicy, SigCacheKey, TxScriptEngine, caches::Cache, script_class::ScriptClass};
use kaspa_txscript_errors::TxScriptError;
use rayon::ThreadPool;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::marker::Sync;

use super::{
    TransactionValidator,
    errors::{TxResult, TxRuleError},
};

/// The threshold above which we apply parallelism to input script processing
const CHECK_SCRIPTS_PARALLELISM_THRESHOLD: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TxValidationFlags {
    /// Perform full validation including script verification
    Full,

    /// Perform fee and sequence/maturity validations but skip script checks. This is usually
    /// an optimization to be applied when it is known that scripts were already checked
    SkipScriptChecks,

    /// When validating mempool transactions, we just set this value ourselves
    SkipMassCheck,
}

impl TransactionValidator {
    pub fn validate_populated_transaction_and_get_fee(
        &self,
        tx: &(impl VerifiableTransaction + Sync),
        pov_daa_score: u64,
        flags: TxValidationFlags,
        mass_and_feerate_threshold: Option<(u64, f64)>,
    ) -> TxResult<u64> {
        self.check_transaction_coinbase_maturity(tx, pov_daa_score)?;
        let total_in = self.check_transaction_input_amounts(tx)?;
        let fee = self.check_output_values_and_compute_fee(tx, total_in)?;
        if flags != TxValidationFlags::SkipMassCheck {
            self.check_mass_commitment(tx)?;
        }
        Self::check_sequence_lock(tx, pov_daa_score)?;

        // The following call is not a consensus check (it could not be one in the first place since it uses a floating number)
        // but rather a mempool Replace by Fee validation rule. It is placed here purposely for avoiding unneeded script checks.
        Self::check_feerate_threshold(fee, mass_and_feerate_threshold)?;

        match flags {
            TxValidationFlags::Full | TxValidationFlags::SkipMassCheck => {
                // kaspa-pq: resolve the PQ-only script policy from the point-of-view
                // DAA score (ADR-0019). On PQ-active networks this makes legacy
                // secp256k1 signature opcodes and P2SH hard consensus errors.
                //
                // PQ-only spend-side backstop (§6): before running scripts, require
                // every *spent* UTXO to be the standard ML-DSA-87 P2PKH class. The
                // script engine alone would let a non-PQ but truthy spk (e.g.
                // `OP_TRUE`) — or an unknown script version, which the engine treats
                // as anyone-can-spend — move value without any ML-DSA signature.
                // This renders such a UTXO unspendable, complementing the
                // creation-side output-class rule so non-PQ value can never move.
                if self.resolved_script_policy(pov_daa_score).pq_only {
                    self.check_input_script_classes(tx, pov_daa_score)?;
                }
                self.check_scripts_with_policy(tx, pov_daa_score)?;
            }
            TxValidationFlags::SkipScriptChecks => {}
        }
        Ok(fee)
    }

    fn check_feerate_threshold(fee: u64, mass_and_feerate_threshold: Option<(u64, f64)>) -> TxResult<()> {
        // An actual check can only occur if some mass and threshold are provided,
        // otherwise, the check does not verify anything and exits successfully.
        if let Some((contextual_mass, feerate_threshold)) = mass_and_feerate_threshold {
            assert!(contextual_mass > 0);
            if fee as f64 / contextual_mass as f64 <= feerate_threshold {
                return Err(TxRuleError::FeerateTooLow);
            }
        }
        Ok(())
    }

    fn check_transaction_coinbase_maturity(&self, tx: &impl VerifiableTransaction, pov_daa_score: u64) -> TxResult<()> {
        if let Some((index, (input, entry))) = tx
            .populated_inputs()
            .enumerate()
            .find(|(_, (_, entry))| entry.is_coinbase && entry.block_daa_score + self.coinbase_maturity > pov_daa_score)
        {
            return Err(TxRuleError::ImmatureCoinbaseSpend(
                index,
                input.previous_outpoint,
                entry.block_daa_score,
                pov_daa_score,
                self.coinbase_maturity,
            ));
        }

        Ok(())
    }

    fn check_transaction_input_amounts(&self, tx: &impl VerifiableTransaction) -> TxResult<u64> {
        let mut total: u64 = 0;
        for (_, entry) in tx.populated_inputs() {
            if let Some(new_total) = total.checked_add(entry.amount) {
                total = new_total
            } else {
                return Err(TxRuleError::InputAmountOverflow);
            }

            if total > MAX_SOMPI {
                return Err(TxRuleError::InputAmountTooHigh);
            }
        }

        Ok(total)
    }

    /// Verifies output-value conservation and returns the transaction fee.
    ///
    /// The strict rule, in force for every transaction on every network, is
    /// `Σ outputs ≤ Σ inputs` with `fee = Σ inputs − Σ outputs`. No transaction
    /// may mint. kaspa-pq's only consensus-sanctioned mint — the slashing
    /// reporter reward (ADR-0013 Addendum C.2) — is **not** a transaction output:
    /// a slashing transaction declares no outputs (enforced in the isolation
    /// validator) and the reward is minted by consensus as an atomic side-effect
    /// at `(slashing_tx_id, 0)`. So this per-transaction check needs no overlay
    /// special-case.
    fn check_output_values_and_compute_fee(&self, tx: &impl VerifiableTransaction, total_in: u64) -> TxResult<u64> {
        // There's no need to check for overflow here because it was already checked by check_transaction_output_value_ranges
        let total_out: u64 = tx.outputs().iter().map(|out| out.value).sum();

        if total_in < total_out {
            return Err(TxRuleError::SpendTooHigh(total_out, total_in));
        }

        Ok(total_in - total_out)
    }

    fn check_mass_commitment(&self, tx: &impl VerifiableTransaction) -> TxResult<()> {
        let calculated_contextual_mass =
            self.mass_calculator.calc_contextual_masses(tx).ok_or(TxRuleError::MassIncomputable)?.storage_mass;
        let committed_contextual_mass = tx.tx().mass();
        if committed_contextual_mass != calculated_contextual_mass {
            return Err(TxRuleError::WrongMass(calculated_contextual_mass, committed_contextual_mass));
        }
        Ok(())
    }

    fn check_sequence_lock(tx: &impl VerifiableTransaction, pov_daa_score: u64) -> TxResult<()> {
        let pov_daa_score: i64 = pov_daa_score as i64;
        if tx.populated_inputs().filter(|(input, _)| input.sequence & SEQUENCE_LOCK_TIME_DISABLED != SEQUENCE_LOCK_TIME_DISABLED).any(
            |(input, entry)| {
                // Given a sequence number, we apply the relative time lock
                // mask in order to obtain the time lock delta required before
                // this input can be spent.
                let relative_lock = (input.sequence & SEQUENCE_LOCK_TIME_MASK) as i64;

                // The relative lock-time for this input is expressed
                // in blocks so we calculate the relative offset from
                // the input's DAA score as its converted absolute
                // lock-time. We subtract one from the relative lock in
                // order to maintain the original lockTime semantics.
                //
                // Note: in the kaspad codebase there's a use in i64 in order to use the -1 value
                // as None. Here it's not needed, but we still use it to avoid breaking consensus.
                let lock_daa_score = entry.block_daa_score as i64 + relative_lock - 1;

                lock_daa_score >= pov_daa_score
            },
        ) {
            return Err(TxRuleError::SequenceLockConditionsAreNotMet);
        }
        Ok(())
    }

    /// Back-compat / test entry point: runs scripts under [`ScriptPolicy::LEGACY`]
    /// (no PQ restriction). Production validation uses
    /// [`Self::check_scripts_with_policy`] so PQ-only enforcement is applied.
    pub fn check_scripts(&self, tx: &(impl VerifiableTransaction + Sync)) -> TxResult<()> {
        check_scripts(&self.sig_cache, tx, ScriptPolicy::LEGACY)
    }

    /// kaspa-pq: run scripts with the PQ-only policy resolved from `pov_daa_score`
    /// (ADR-0019). Legacy secp256k1 signature opcodes and P2SH become hard errors
    /// on PQ-active networks.
    pub fn check_scripts_with_policy(&self, tx: &(impl VerifiableTransaction + Sync), pov_daa_score: u64) -> TxResult<()> {
        check_scripts(&self.sig_cache, tx, self.resolved_script_policy(pov_daa_score))
    }

    /// kaspa-pq PQ-only (ADR-0019 §6): every spent input UTXO must itself be the
    /// standard ML-DSA-87 P2PKH script class. Run only on PQ-active networks (the
    /// caller gates on `resolved_script_policy(...).pq_only`). The spend-side
    /// complement to the creation-side `check_transaction_pq_output_classes`:
    /// together they guarantee no UTXO can be spent without an ML-DSA signature,
    /// even one that reached the set via a coinbase / overlay path or an unknown
    /// script version (which the engine would otherwise treat as anyone-can-spend).
    /// kaspa-pq EVM Lane v0.4 §9.2: an `EVM_DEPOSIT_LOCK` input is additionally
    /// admitted, but ONLY once its refund window opens (`pov_daa ≥ timeout` —
    /// the AC-2 exclusivity rule: while `pov_daa < timeout` the lock is
    /// claimable by a `DepositClaim` system op and NOT spendable; the two
    /// windows never overlap). The spend itself then satisfies the embedded
    /// ML-DSA P2PKH refund script (PQ-safe — the lock-data prefix is a script
    /// no-op push-and-drop).
    fn check_input_script_classes(&self, tx: &impl VerifiableTransaction, pov_daa_score: u64) -> TxResult<()> {
        for (i, (_, entry)) in tx.populated_inputs().enumerate() {
            let class = ScriptClass::from_script(&entry.script_public_key);
            if class == ScriptClass::EvmDepositLock {
                let fields = kaspa_txscript::script_class::parse_evm_deposit_lock(&entry.script_public_key)
                    .expect("class detection implies the lock parses");
                if pov_daa_score < fields.timeout_daa_score {
                    return Err(TxRuleError::EvmDepositLockNotRefundableYet(i, pov_daa_score, fields.timeout_daa_score));
                }
                continue;
            }
            if !class.is_pq_standard() {
                return Err(TxRuleError::NonPqStandardInputClass(i));
            }
        }
        Ok(())
    }
}

pub fn check_scripts(sig_cache: &Cache<SigCacheKey, bool>, tx: &(impl VerifiableTransaction + Sync), policy: ScriptPolicy) -> TxResult<()> {
    if tx.inputs().len() > CHECK_SCRIPTS_PARALLELISM_THRESHOLD {
        check_scripts_par_iter(sig_cache, tx, policy)
    } else {
        check_scripts_sequential(sig_cache, tx, policy)
    }
}

pub fn check_scripts_sequential(sig_cache: &Cache<SigCacheKey, bool>, tx: &impl VerifiableTransaction, policy: ScriptPolicy) -> TxResult<()> {
    let reused_values = SigHashReusedValuesUnsync::new();
    for (i, (input, entry)) in tx.populated_inputs().enumerate() {
        TxScriptEngine::from_transaction_input(tx, input, i, entry, &reused_values, sig_cache)
            .with_script_policy(policy)
            .execute()
            .map_err(|err| map_script_err(err, input))?;
    }
    Ok(())
}

pub fn check_scripts_par_iter(sig_cache: &Cache<SigCacheKey, bool>, tx: &(impl VerifiableTransaction + Sync), policy: ScriptPolicy) -> TxResult<()> {
    let reused_values = SigHashReusedValuesSync::new();
    (0..tx.inputs().len()).into_par_iter().try_for_each(|idx| {
        let (input, utxo) = tx.populated_input(idx);
        TxScriptEngine::from_transaction_input(tx, input, idx, utxo, &reused_values, sig_cache)
            .with_script_policy(policy)
            .execute()
            .map_err(|err| map_script_err(err, input))
    })
}

pub fn check_scripts_par_iter_pool(
    sig_cache: &Cache<SigCacheKey, bool>,
    tx: &(impl VerifiableTransaction + Sync),
    pool: &ThreadPool,
    policy: ScriptPolicy,
) -> TxResult<()> {
    pool.install(|| check_scripts_par_iter(sig_cache, tx, policy))
}

fn map_script_err(script_err: TxScriptError, input: &TransactionInput) -> TxRuleError {
    if input.signature_script.is_empty() { TxRuleError::SignatureEmpty(script_err) } else { TxRuleError::SignatureInvalid(script_err) }
}

// kaspa-pq Phase 9: these tests were re-enabled and rewritten to sign over the
// 64-byte (Hash64) sighash per ADR-0008. The original fixtures pinned external
// Schnorr signatures computed against the old 32-byte `TransactionId`, which
// cannot be re-signed (their private keys are unavailable). Each test now
// generates a fresh keypair, derives the matching `script_public_key`, signs the
// freshly-computed sighash, and asserts the same success/failure outcome the
// original fixture asserted.
// kaspa-pq PQ-only (ADR-0019 §14): this module's signature-validation fixtures are
// built on legacy secp256k1, so it compiles only under `legacy-secp256k1`. The
// value-conservation tests (no secp) live in `conservation_tests` below.
#[cfg(all(test, feature = "legacy-secp256k1"))]
mod tests {
    use super::super::errors::TxRuleError;
    use super::CHECK_SCRIPTS_PARALLELISM_THRESHOLD;
    use core::str::FromStr;
    use itertools::Itertools;
    use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
    use kaspa_consensus_core::hashing::sighash::calc_schnorr_signature_hash;
    use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
    use kaspa_consensus_core::sign::sign;
    use kaspa_consensus_core::subnets::SubnetworkId;
    use kaspa_consensus_core::tx::{MutableTransaction, PopulatedTransaction, ScriptVec, TransactionId, UtxoEntry};
    use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
    use kaspa_txscript::opcodes::codes::OpData65;
    use kaspa_txscript::script_builder::ScriptBuilder;
    use kaspa_txscript::{multisig_redeem_script, pay_to_script_hash_script};
    use kaspa_txscript_errors::TxScriptError;
    use secp256k1::{Keypair, Secp256k1};
    use smallvec::SmallVec;
    use std::iter::once;

    use crate::{params::MAINNET_PARAMS, processes::transaction_validator::TransactionValidator};

    /// Helper function to duplicate the last input
    fn duplicate_input(tx: &Transaction, entries: &[UtxoEntry]) -> (Transaction, Vec<UtxoEntry>) {
        let mut tx2 = tx.clone();
        let mut entries2 = entries.to_owned();
        tx2.inputs.push(tx2.inputs.last().unwrap().clone());
        entries2.push(entries2.last().unwrap().clone());
        (tx2, entries2)
    }

    /// Builds a `TransactionValidator` configured the way every test in this
    /// module expects (10 inputs / 15 outputs caps).
    fn test_validator() -> TransactionValidator {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
            Default::default(),
        )
    }

    /// An arbitrary 64-byte (128 hex) previous-tx id. The value itself is
    /// irrelevant to these sign/verify roundtrips — any valid id works.
    const PREV_TX_ID: &str = "63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff840000000000000000000000000000000000000000000000000000000000000000";

    /// P2PK script for a x-only public key: `OP_DATA_32 <xonly> OP_CHECKSIG`.
    fn p2pk_script(kp: &Keypair) -> ScriptVec {
        ScriptVec::from_iter(once(0x20u8).chain(kp.x_only_public_key().0.serialize()).chain(once(0xac)))
    }

    /// How a given multisig signature slot should be filled.
    #[derive(Clone, Copy, PartialEq)]
    enum SigKind {
        /// Push a correct Schnorr signature over the freshly-computed sighash.
        Valid,
        /// Push a structurally-valid Schnorr signature whose last byte is
        /// flipped, so verification yields `false` (triggers NULLFAIL when the
        /// overall multisig fails).
        Corrupt,
        /// Push an empty data item in place of a signature.
        Empty,
    }

    /// Builds a 2-of-2 P2SH multisig spend modelled on
    /// `crypto/txscript/src/standard/multisig.rs::check_multisig_scenario`:
    /// it builds the redeem script over `kps`, computes the Hash64 sighash,
    /// and fills each signature slot according to `sig_kinds`. Returns a
    /// transaction + entries ready for `check_scripts`.
    fn build_multisig_tx(kps: &[Keypair], required: usize, sig_kinds: &[SigKind]) -> (Transaction, Vec<UtxoEntry>) {
        let script = multisig_redeem_script(kps.iter().map(|kp| kp.x_only_public_key().0.serialize()), required).unwrap();

        let prev_tx_id = TransactionId::from_str(PREV_TX_ID).unwrap();
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![TransactionOutput { value: 10000000000000, script_public_key: pay_to_script_hash_script(&script) }],
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
        let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();

        let signatures: Vec<u8> = kps
            .iter()
            .zip(sig_kinds)
            .flat_map(|(kp, kind)| match kind {
                SigKind::Empty => vec![0x00], // OP_0 / empty data push
                SigKind::Valid | SigKind::Corrupt => {
                    let mut sig = *kp.sign_schnorr(msg).as_ref();
                    if *kind == SigKind::Corrupt {
                        sig[63] ^= 0x01;
                    }
                    once(OpData65).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect()
                }
            })
            .collect();

        tx.tx.inputs[0].signature_script =
            signatures.into_iter().chain(ScriptBuilder::new().add_data(&script).unwrap().drain()).collect();

        let entries = tx.entries.iter().map(|e| e.clone().unwrap()).collect();
        (tx.tx, entries)
    }

    #[test]
    fn check_signature_test() {
        let tv = test_validator();

        // Fresh keypair + matching P2PK script-public-key.
        let kp = Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
        let script_pub_key = ScriptPublicKey::new(0, p2pk_script(&kp));

        let prev_tx_id = TransactionId::from_str(PREV_TX_ID).unwrap();
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![TransactionOutput { value: 10360487799, script_public_key: script_pub_key.clone() }],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let entries =
            vec![UtxoEntry { amount: 20879456551, script_public_key: script_pub_key, block_daa_score: 32022768, is_coinbase: false }];

        // Sign the single-input tx over the Hash64 sighash.
        let signed = sign(MutableTransaction::with_entries(tx, entries), kp);
        let signed_tx = signed.tx.clone();
        let signed_entries: Vec<UtxoEntry> = signed.entries.iter().map(|e| e.clone().unwrap()).collect();
        let populated_tx = PopulatedTransaction::new(&signed_tx, signed_entries);

        tv.check_scripts(&populated_tx).expect("Signature check failed");

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&signed_tx, &populated_tx.entries);
        // Duplicated sigs should fail due to wrong sighash (the signature was
        // computed for the single-input tx, but the two-input tx has a
        // different sighash for each input).
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2)),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
    }

    #[test]
    fn check_incorrect_signature_test() {
        let tv = test_validator();

        // Two distinct keypairs: we sign with `kp_signer` but the UTXO being
        // spent is locked to `kp_owner`'s P2PK script, so verification must fail.
        let kp_signer = Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
        let kp_owner = Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
        let signer_spk = ScriptPublicKey::new(0, p2pk_script(&kp_signer));
        let owner_spk = ScriptPublicKey::new(0, p2pk_script(&kp_owner));

        let prev_tx_id = TransactionId::from_str(PREV_TX_ID).unwrap();
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![TransactionOutput { value: 10360487799, script_public_key: owner_spk.clone() }],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        // Sign with the signer key against a (wrong) entry locked to the signer's
        // script, then re-point the entry to the owner's script so the signature
        // no longer matches the locking script.
        let entries =
            vec![UtxoEntry { amount: 20879456551, script_public_key: signer_spk, block_daa_score: 32022768, is_coinbase: false }];
        let signed = sign(MutableTransaction::with_entries(tx, entries), kp_signer);
        let signed_tx = signed.tx.clone();
        let mismatched_entries =
            vec![UtxoEntry { amount: 20879456551, script_public_key: owner_spk, block_daa_score: 32022768, is_coinbase: false }];
        let populated_tx = PopulatedTransaction::new(&signed_tx, mismatched_entries);

        assert!(tv.check_scripts(&populated_tx).is_err(), "Expecting signature check to fail");

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&signed_tx, &populated_tx.entries);
        tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2)).expect_err("Expecting signature check to fail");

        // Verify we are correctly testing the parallelism case (applied here as sanity for all tests)
        assert!(
            tx2.inputs.len() > CHECK_SCRIPTS_PARALLELISM_THRESHOLD,
            "The script tests must cover the case of a tx with inputs.len() > {}",
            CHECK_SCRIPTS_PARALLELISM_THRESHOLD
        );
    }

    #[test]
    fn check_multi_signature_test() {
        let tv = test_validator();

        // 2-of-2 multisig, both signatures correct.
        let kps =
            [Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng()), Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng())];
        let (tx, entries) = build_multisig_tx(&kps, 2, &[SigKind::Valid, SigKind::Valid]);

        let populated_tx = PopulatedTransaction::new(&tx, entries);
        tv.check_scripts(&populated_tx).expect("Signature check failed");

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        // Duplicated sigs should fail due to wrong sighash (non-empty sigs that
        // fail verification trigger NULLFAIL in OP_CHECKMULTISIG).
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2)),
            Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail))
        );
    }

    #[test]
    fn check_last_sig_incorrect_multi_signature_test() {
        let tv = test_validator();

        // 2-of-2 multisig where the last (second) signature is corrupted.
        let kps =
            [Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng()), Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng())];
        let (tx, entries) = build_multisig_tx(&kps, 2, &[SigKind::Valid, SigKind::Corrupt]);

        let populated_tx = PopulatedTransaction::new(&tx, entries);
        assert_eq!(tv.check_scripts(&populated_tx), Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail)));

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2)),
            Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail))
        );
    }

    #[test]
    fn check_first_sig_incorrect_multi_signature_test() {
        let tv = test_validator();

        // 2-of-2 multisig where the first signature is corrupted.
        let kps =
            [Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng()), Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng())];
        let (tx, entries) = build_multisig_tx(&kps, 2, &[SigKind::Corrupt, SigKind::Valid]);

        let populated_tx = PopulatedTransaction::new(&tx, entries);
        assert_eq!(tv.check_scripts(&populated_tx), Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail)));

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2)),
            Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail))
        );
    }

    #[test]
    fn check_empty_incorrect_multi_signature_test() {
        let tv = test_validator();

        // 2-of-2 multisig where both signature slots are empty data pushes.
        // The multisig fails with no non-empty signature present, which yields
        // EvalFalse (not NULLFAIL).
        let kps =
            [Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng()), Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng())];
        let (tx, entries) = build_multisig_tx(&kps, 2, &[SigKind::Empty, SigKind::Empty]);

        let populated_tx = PopulatedTransaction::new(&tx, entries);
        assert_eq!(tv.check_scripts(&populated_tx), Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse)));

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2)),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
    }

    #[test]
    fn check_non_push_only_script_sig_test() {
        // We test a situation where the script itself is valid, but the script signature is not push only
        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
            Default::default(),
        );

        let prev_tx_id = TransactionId::from_str(
            "11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
        )
        .unwrap();

        let mut bytes = [0u8; 2];
        faster_hex::hex_decode("5175".as_bytes(), &mut bytes).unwrap(); // OP_TRUE OP_DROP
        let signature_script = bytes.to_vec();

        let mut bytes = [0u8; 1];
        faster_hex::hex_decode("51".as_bytes(), &mut bytes) // OP_TRUE
            .unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) }],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 12793000000000,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                block_daa_score: 36151168,
                is_coinbase: false,
            }],
        );

        assert_eq!(tv.check_scripts(&populated_tx), Err(TxRuleError::SignatureInvalid(TxScriptError::SignatureScriptNotPushOnly)));

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2)),
            Err(TxRuleError::SignatureInvalid(TxScriptError::SignatureScriptNotPushOnly))
        );
    }

    #[test]
    fn test_sign() {
        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
            Default::default(),
        );

        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let (public_key, _) = public_key.x_only_public_key();
        let script_pub_key = once(0x20).chain(public_key.serialize()).chain(once(0xac)).collect_vec();
        let script_pub_key = ScriptVec::from_slice(&script_pub_key);

        let prev_tx_id = TransactionId::from_str(
            "880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3",
        )
        .unwrap();
        let unsigned_tx = Transaction::new(
            0,
            vec![
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                    signature_script: vec![],
                    sequence: 1,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 2 },
                    signature_script: vec![],
                    sequence: 2,
                    sig_op_count: 0,
                },
            ],
            vec![
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
            ],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let entries = vec![
            UtxoEntry {
                amount: 100,
                script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                block_daa_score: 0,
                is_coinbase: false,
            },
            UtxoEntry {
                amount: 200,
                script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                block_daa_score: 0,
                is_coinbase: false,
            },
            UtxoEntry {
                amount: 300,
                script_public_key: ScriptPublicKey::new(0, script_pub_key),
                block_daa_score: 0,
                is_coinbase: false,
            },
        ];
        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes()).unwrap();
        let signed_tx = sign(MutableTransaction::with_entries(unsigned_tx, entries), schnorr_key);
        let populated_tx = signed_tx.as_verifiable();
        assert_eq!(tv.check_scripts(&populated_tx), Ok(()));
    }
}

// kaspa-pq PQ-only (ADR-0019 §14): value-conservation tests use no secp256k1, so
// they stay in the default (pq-only) suite — split out of the `legacy-secp256k1`-
// gated `tests` module above.
#[cfg(test)]
mod conservation_tests {
    use super::super::errors::TxRuleError;
    use crate::{params::MAINNET_PARAMS, processes::transaction_validator::TransactionValidator};
    use kaspa_consensus_core::subnets::{SUBNETWORK_ID_SLASHING_EVIDENCE, SubnetworkId};
    use kaspa_consensus_core::tx::{PopulatedTransaction, ScriptPublicKey, ScriptVec, Transaction, TransactionOutput};

    /// Builds a `TransactionValidator` configured the way these tests expect.
    fn test_validator() -> TransactionValidator {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
            Default::default(),
        )
    }

    // -----------------------------------------------------------------
    // kaspa-pq: value conservation / fee accounting.
    //
    // `check_output_values_and_compute_fee` reads only the outputs; `total_in`
    // is passed explicitly, so the test transactions need no inputs/entries.
    // The strict `Σ outputs ≤ Σ inputs` rule has no overlay special-case: the
    // slashing reporter reward is a consensus side-effect mint, not a tx output
    // (ADR-0013 Addendum C.2), so a slashing tx declares no outputs.
    // -----------------------------------------------------------------

    /// Native (non-overlay) subnetwork id.
    fn native_subnet() -> SubnetworkId {
        SubnetworkId::from_bytes([0u8; 20])
    }

    /// Builds an input-less tx carrying `output_values` on `subnetwork_id`.
    fn outputs_only_tx(subnetwork_id: SubnetworkId, output_values: &[u64]) -> Transaction {
        let dummy_spk = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51])); // OP_TRUE
        let outputs =
            output_values.iter().map(|&value| TransactionOutput { value, script_public_key: dummy_spk.clone() }).collect();
        Transaction::new(0, vec![], outputs, 0, subnetwork_id, 0, vec![])
    }

    #[test]
    fn strict_conservation_holds_for_every_tx() {
        // A tx obeys Σ outputs ≤ inputs with fee = inputs − outputs, and any
        // over-spend (mint) is rejected — on every subnetwork, no exemption.
        let tv = test_validator();

        let ok_tx = outputs_only_tx(native_subnet(), &[300, 200]); // total_out = 500
        let ok_ptx = PopulatedTransaction::new(&ok_tx, vec![]);
        assert_eq!(tv.check_output_values_and_compute_fee(&ok_ptx, 800), Ok(300));

        let bad_tx = outputs_only_tx(native_subnet(), &[900]);
        let bad_ptx = PopulatedTransaction::new(&bad_tx, vec![]);
        assert_eq!(tv.check_output_values_and_compute_fee(&bad_ptx, 800), Err(TxRuleError::SpendTooHigh(900, 800)));

        // ADR-0013 Addendum C.2: a slashing tx declares no outputs, so its only
        // shape here is Σ outputs = 0 ⇒ the whole funding input is fee. The
        // reporter reward is minted by the consensus side-effect, not here.
        let slashing_tx = outputs_only_tx(SUBNETWORK_ID_SLASHING_EVIDENCE, &[]);
        let slashing_ptx = PopulatedTransaction::new(&slashing_tx, vec![]);
        assert_eq!(tv.check_output_values_and_compute_fee(&slashing_ptx, 500), Ok(500));
    }
}
