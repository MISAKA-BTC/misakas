use crate::mempool::{
    Mempool,
    errors::{NonStandardError, NonStandardResult},
};
use kaspa_consensus_core::{
    constants::{MAX_SCRIPT_PUBLIC_KEY_VERSION, MAX_SOMPI},
    mass,
    tx::{MutableTransaction, PopulatedTransaction, TransactionOutput},
};
use kaspa_consensus_core::{hashing::sighash::SigHashReusedValuesUnsync, mass::NonContextualMasses};
use kaspa_txscript::{get_sig_op_count_upper_bound, is_unspendable, script_class::ScriptClass};

/// MAX_STANDARD_P2SH_SIG_OPS is the maximum number of signature operations
/// that are considered standard in a pay-to-script-hash script.
const MAX_STANDARD_P2SH_SIG_OPS: u8 = 15;

/// kaspa-pq PQ-only: estimated serialized size (bytes) of the ML-DSA-87 P2PKH
/// input that spends an output, used by the dust calculation — versus the legacy
/// 148-byte p2pk input. Reflects the true cost to spend. Breakdown:
///   outpoint (Hash64 txid 64 + index 4) ............ 68
///   signature-script length field (u64) ............  8
///   sig push: <sig 4627 + sighash 1> + PUSHDATA2 3 . 4631
///   pubkey push: <pubkey 2592> + PUSHDATA2 3 ....... 2595
///   sequence (u64) .................................  8
///   total .......................................... 7310
const MLDSA87_P2PKH_SPEND_INPUT_SIZE: u64 = 7310;

/// MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE is the maximum size allowed for a
/// transaction input signature script to be considered standard.
///
/// kaspa-pq (ML-DSA-87): a standard ML-DSA-87 P2PKH signature script holds a
/// 4628-byte signature + 2592-byte public key (plus push opcodes) for a max
/// standard unlock of ~7.3 KB, so the legacy 1,650-byte limit is far too
/// small. Launch scope is ML-DSA-87 P2PKH only (ADR-0019 §11.1); multisig /
/// P2SH is out of scope. Kept in lockstep with the 16_384 design cap in
/// `kaspa_txscript::MAX_SCRIPTS_SIZE` (md2 §3.2).
const MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE: u64 = 16_384;

/// MAXIMUM_STANDARD_TRANSACTION_MASS is the maximum mass allowed for transactions that
/// are considered standard and will therefore be relayed and considered for mining.
///
/// kaspa-pq: raised from the upstream 100_000. This limit is checked against BOTH the
/// compute mass and the transient (KIP-0009 storage) mass. ML-DSA-87 P2PKH spends are
/// heavy on both axes: ~12_000 compute mass per input (5268-byte sig script + sig-op),
/// and multi-input transactions also accrue large transient mass (e.g. a 16-input send
/// measured 183_160 compute / 341_296 transient). To let any block-mineable ML-DSA tx
/// also be relay-standard, set this just under the consensus block-mass budget
/// (`max_block_mass` = 500_000), leaving headroom for the coinbase.
/// NOTE (mainnet): for a high-traffic network this should be lowered well below the
/// block budget to preserve an anti-monopolization margin; it is devnet-generous here.
const MAXIMUM_STANDARD_TRANSACTION_MASS: u64 = 480_000;

impl Mempool {
    pub(crate) fn check_transaction_standard_in_isolation(&self, transaction: &MutableTransaction) -> NonStandardResult<()> {
        let transaction_id = transaction.id();

        // The transaction must be a currently supported version.
        //
        // This check is currently mirrored in consensus.
        // However, in a later version of Kaspa the consensus-valid transaction version range might diverge from the
        // standard transaction version range, and thus the validation should happen in both levels.
        if transaction.tx.version > self.config.maximum_standard_transaction_version
            || transaction.tx.version < self.config.minimum_standard_transaction_version
        {
            return Err(NonStandardError::RejectVersion(
                transaction_id,
                transaction.tx.version,
                self.config.minimum_standard_transaction_version,
                self.config.maximum_standard_transaction_version,
            ));
        }

        // Since extremely large transactions with a lot of inputs can cost
        // almost as much to process as the sender fees, limit the maximum
        // size of a transaction. This also helps mitigate CPU exhaustion
        // attacks.
        let NonContextualMasses { compute_mass, transient_mass } = transaction.calculated_non_contextual_masses.unwrap();
        if compute_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            return Err(NonStandardError::RejectComputeMass(transaction_id, compute_mass, MAXIMUM_STANDARD_TRANSACTION_MASS));
        }
        if transient_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            return Err(NonStandardError::RejectTransientMass(transaction_id, transient_mass, MAXIMUM_STANDARD_TRANSACTION_MASS));
        }

        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            // Each transaction input signature script must not exceed the
            // maximum size allowed for a standard transaction.
            //
            // See the comment on MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE for
            // more details.
            let signature_script_len = input.signature_script.len() as u64;
            if signature_script_len > MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE {
                return Err(NonStandardError::RejectSignatureScriptSize(
                    transaction_id,
                    i,
                    signature_script_len,
                    MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE,
                ));
            }
        }

        // None of the output public key scripts can be a non-standard script or be "dust".
        for (i, output) in transaction.tx.outputs.iter().enumerate() {
            if output.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
                return Err(NonStandardError::RejectScriptPublicKeyVersion(transaction_id, i));
            }

            // kaspa-pq PQ-only relay: require the ML-DSA-87 P2PKH class (matching the hard
            // consensus rule `check_transaction_pq_output_classes`), so the mempool never relays a
            // transaction that consensus will reject. The legacy-permissive "reject only
            // NonStandard" rule is kept for the (legacy-fixture) unit tests via `pq_only = false`.
            //
            // kaspa-pq EVM Lane v0.4 (§7.2/§9.2): an EVM_DEPOSIT_LOCK output is the
            // consensus-legal bridge-deposit creation (`check_transaction_pq_output_classes`
            // has the identical carve-out). The mempool must relay it or deposits become
            // miner-direct-only — the same reasoning as the lock's refund INPUT carve-out
            // below. The lock parses strictly (108-byte fixed form with an embedded standard
            // ML-DSA-87 refund P2PKH), so this admits no free-form script.
            let output_class = ScriptClass::from_script(&output.script_public_key);
            let output_rejected = if self.config.pq_only {
                !output_class.is_pq_standard() && output_class != ScriptClass::EvmDepositLock
            } else {
                output_class == ScriptClass::NonStandard
            };
            if output_rejected {
                return Err(NonStandardError::RejectOutputScriptClass(transaction_id, i));
            }

            if self.is_transaction_output_dust(output) {
                return Err(NonStandardError::RejectDust(transaction_id, i, output.value));
            }
        }

        Ok(())
    }

    /// is_transaction_output_dust returns whether or not the passed transaction output
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    ///
    /// Dust is defined in terms of the minimum transaction relay fee. In particular,
    /// if the cost to the network to spend coins is more than 1/3 of the minimum
    /// transaction relay fee, it is considered dust.
    ///
    /// It is exposed by [MiningManager] for use by transaction generators and wallets.
    pub(crate) fn is_transaction_output_dust(&self, transaction_output: &TransactionOutput) -> bool {
        // Unspendable outputs are considered dust.
        if is_unspendable::<PopulatedTransaction, SigHashReusedValuesUnsync>(transaction_output.script_public_key.script()) {
            return true;
        }

        // The total serialized size consists of the output plus the input that
        // would redeem it. In kaspa-pq the only spend class is ML-DSA-87 P2PKH, so
        // the redeeming input is ~7.3 KB (a 68-byte Hash64 outpoint + a ~7226-byte
        // `<sig 4628><pubkey 2592>` signature script + sequence) — not the legacy
        // 148-byte p2pk input. Using the real ML-DSA spend size makes "dust" reflect
        // the true cost to spend the output.
        let total_serialized_size =
            mass::transaction_output_estimated_serialized_size(transaction_output) + MLDSA87_P2PKH_SPEND_INPUT_SIZE;

        // The output is considered dust if the cost to the network to spend the
        // coins is more than 1/3 of the minimum free transaction relay fee.
        // mp.config.MinimumRelayTransactionFee is in sompi/KB, so multiply
        // by 1000 to convert to bytes.
        //
        // Using the typical values for a pay-to-pubkey transaction from
        // the breakdown above and the default minimum free transaction relay
        // fee of 1000, this equates to values less than 546 sompi being
        // considered dust.
        //
        // The following is equivalent to (value/total_serialized_size) * (1/3) * 1000
        // without needing to do floating point math.
        //
        // Since the multiplication may overflow a u64, 2 separate calculation paths
        // are considered to avoid overflowing.
        match transaction_output.value.checked_mul(1000) {
            Some(value_1000) => value_1000 / (3 * total_serialized_size) < self.config.minimum_relay_transaction_fee,
            None => {
                (transaction_output.value as u128 * 1000 / (3 * total_serialized_size as u128))
                    < self.config.minimum_relay_transaction_fee as u128
            }
        }
    }

    /// check_transaction_standard_in_context performs a series of checks on a transaction's
    /// inputs to ensure they are "standard". A standard transaction input within the
    /// context of this function is one whose referenced public key script is of a
    /// standard form and, for pay-to-script-hash, does not have more than
    /// maxStandardP2SHSigOps signature operations.
    /// In addition, makes sure that the transaction's fee is above the minimum for acceptance
    /// into the mempool and relay.
    pub(crate) fn check_transaction_standard_in_context(&self, transaction: &MutableTransaction) -> NonStandardResult<()> {
        let transaction_id = transaction.id();
        let contextual_mass = transaction.tx.mass();
        if contextual_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            return Err(NonStandardError::RejectStorageMass(transaction_id, contextual_mass, MAXIMUM_STANDARD_TRANSACTION_MASS));
        }
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            // It is safe to elide existence and index checks here since
            // they have already been checked prior to calling this
            // function.
            let entry = transaction.entries[i].as_ref().unwrap();
            // kaspa-pq PQ-only relay: the spent input UTXO must be ML-DSA-87 P2PKH (matching the
            // consensus input-class rule in the UTXO-context validator), so the mempool never
            // relays a transaction consensus will reject. The upstream-permissive class table
            // below is kept for the (legacy-fixture) unit tests via `pq_only = false`.
            let input_class = ScriptClass::from_script(&entry.script_public_key);
            // kaspa-pq EVM Lane v0.4 (AC-2): an EVM_DEPOSIT_LOCK input is the
            // embedded ML-DSA-87 refund spend — consensus-valid at/after the lock
            // timeout (the UTXO-context validator has this exact carve-out). The
            // mempool must relay it or refunds become miner-direct-only; a
            // premature refund is rejected by consensus validation at mempool
            // entry, not by standardness.
            if self.config.pq_only && !input_class.is_pq_standard() && input_class != ScriptClass::EvmDepositLock {
                return Err(NonStandardError::RejectInputScriptClass(transaction_id, i));
            }
            match input_class {
                ScriptClass::NonStandard => {
                    return Err(NonStandardError::RejectInputScriptClass(transaction_id, i));
                }
                ScriptClass::PubKey => {}
                ScriptClass::PubKeyECDSA => {}
                // kaspa-pq: ML-DSA-87 P2PKH is the kaspa-pq standard spend template.
                // Each input contributes exactly one ML-DSA verify (= one sig-op); no
                // per-input sig-op-count check is needed because the script template
                // is fixed and the engine length-checks the public key and signature
                // before the libcrux verify. The mass-budget side of the policy is
                // calibrated via `mass_per_sig_op` (docs/adr/0005-mass-policy.md).
                // The EVM_DEPOSIT_LOCK refund spend IS that same template (the
                // lock's push-and-drop prefix + embedded ML-DSA-87 refund P2PKH):
                // exactly one sig-op.
                ScriptClass::PubKeyHashMlDsa87 | ScriptClass::EvmDepositLock => {}
                ScriptClass::ScriptHash => {
                    // todo relax due to on fly calculation
                    let num_sig_ops = get_sig_op_count_upper_bound::<PopulatedTransaction, SigHashReusedValuesUnsync>(
                        &input.signature_script,
                        &entry.script_public_key,
                    );
                    if num_sig_ops > MAX_STANDARD_P2SH_SIG_OPS as u64 {
                        return Err(NonStandardError::RejectSignatureCount(transaction_id, i, num_sig_ops, MAX_STANDARD_P2SH_SIG_OPS));
                    }
                }
            }

            // TODO: For now, until wallets adapt, we only require minimum fee as function of compute mass (but the fee/mass ratio will
            // use the max over all masses and will affect tx selection to block template)
            let minimum_fee =
                self.minimum_required_transaction_relay_fee(transaction.calculated_non_contextual_masses.unwrap().compute_mass);
            if transaction.calculated_fee.unwrap() < minimum_fee {
                return Err(NonStandardError::RejectInsufficientFee(transaction_id, transaction.calculated_fee.unwrap(), minimum_fee));
            }
        }

        Ok(())
    }

    /// minimum_required_transaction_relay_fee returns the minimum transaction fee required
    /// for a transaction with the passed mass to be accepted into the mempool and relayed.
    fn minimum_required_transaction_relay_fee(&self, mass: u64) -> u64 {
        // Calculate the minimum fee for a transaction to be allowed into the
        // mempool and relayed by scaling the base fee. MinimumRelayTransactionFee is in
        // sompi/kg so multiply by mass (which is in grams) and divide by 1000 to get
        // minimum sompis.
        let mut minimum_fee = (mass * self.config.minimum_relay_transaction_fee) / 1000;

        if minimum_fee == 0 {
            minimum_fee = self.config.minimum_relay_transaction_fee;
        }

        // Set the minimum fee to the maximum possible value if the calculated
        // fee is not in the valid range for monetary amounts.
        minimum_fee = minimum_fee.min(MAX_SOMPI);

        minimum_fee
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MiningCounters,
        mempool::config::{Config, DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE},
    };
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_consensus_core::{
        config::params::Params,
        constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TX_VERSION},
        mass::NonContextualMasses,
        network::NetworkType,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
    };
    use kaspa_txscript::{
        opcodes::codes::{OpReturn, OpTrue},
        script_builder::ScriptBuilder,
    };
    use smallvec::smallvec;
    use std::sync::Arc;

    #[test]
    fn test_calc_min_required_tx_relay_fee() {
        struct Test {
            name: &'static str,
            size: u64,
            minimum_relay_transaction_fee: u64,
            want: u64,
        }

        let tests = [
            Test {
                // Ensure combination of size and fee that are less than 1000
                // produce a non-zero fee.
                name: "250 bytes with relay fee of 3",
                size: 250,
                minimum_relay_transaction_fee: 3,
                want: 3,
            },
            Test {
                name: "100 bytes with default minimum relay fee",
                size: 100,
                minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
                want: 100,
            },
            Test {
                name: "max standard tx size with default minimum relay fee",
                size: MAXIMUM_STANDARD_TRANSACTION_MASS,
                minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
                // kaspa-pq raised MAXIMUM_STANDARD_TRANSACTION_MASS to 480_000 (was the
                // upstream 100_000); the expected fee tracks that at the default fee rate.
                want: 480000,
            },
            Test { name: "1500 bytes with 5000 relay fee", size: 1500, minimum_relay_transaction_fee: 5000, want: 7500 },
            Test { name: "1500 bytes with 3000 relay fee", size: 1500, minimum_relay_transaction_fee: 3000, want: 4500 },
            Test { name: "782 bytes with 5000 relay fee", size: 782, minimum_relay_transaction_fee: 5000, want: 3910 },
            Test { name: "782 bytes with 3000 relay fee", size: 782, minimum_relay_transaction_fee: 3000, want: 2346 },
            Test { name: "782 bytes with 2550 relay fee", size: 782, minimum_relay_transaction_fee: 2550, want: 1994 },
        ];

        for test in tests.iter() {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let mut config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
                config.minimum_relay_transaction_fee = test.minimum_relay_transaction_fee;
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                let got = mempool.minimum_required_transaction_relay_fee(test.size);
                if got != test.want {
                    println!("test_calc_min_required_tx_relay_fee test '{}' failed: got {}, want {}", test.name, got, test.want);
                }
                assert_eq!(test.want, got);
            }
        }
    }

    #[test]
    fn test_is_transaction_output_dust() {
        let script_public_key = ScriptPublicKey::new(
            0,
            smallvec![
                0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76,
                0xd9, 0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e
            ],
        );
        let invalid_script_public_key = ScriptPublicKey::new(0, smallvec![0x01]);

        struct Test {
            name: &'static str,
            tx_out: TransactionOutput,
            minimum_relay_transaction_fee: u64,
            is_dust: bool,
        }

        let tests = vec![
            // Any value is allowed with a zero relay fee.
            Test {
                name: "zero value with zero relay fee",
                tx_out: TransactionOutput::new(0, script_public_key.clone()),
                minimum_relay_transaction_fee: 0,
                is_dust: false,
            },
            // Zero value is dust with any relay fee"
            Test {
                name: "zero value with very small tx fee",
                tx_out: TransactionOutput::new(0, script_public_key.clone()),
                minimum_relay_transaction_fee: 1,
                is_dust: true,
            },
            Test {
                name: "36 byte public key script with value 605",
                tx_out: TransactionOutput::new(605, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: true,
            },
            // kaspa-pq: with the ~7.3 KB ML-DSA-87 spend-input size, the fee=1000
            // dust boundary is ~22_077 sompi (was ~606 with the legacy 148-byte
            // input). 605 above stays dust; a clearly-larger value is not dust.
            Test {
                name: "value above the ML-DSA dust threshold",
                tx_out: TransactionOutput::new(50_000, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: false,
            },
            // Maximum allowed value is never dust.
            Test {
                name: "max sompi amount is never dust",
                tx_out: TransactionOutput::new(MAX_SOMPI, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: false,
            },
            // Maximum uint64 value causes NO overflow.
            // Rust rewrite: caution, this differs from the golang version
            Test {
                // Still exercises the u128 overflow path (value * 1000 overflows u64);
                // a max-value output is never dust at a normal relay fee.
                name: "maximum uint64 value",
                tx_out: TransactionOutput::new(u64::MAX, script_public_key),
                minimum_relay_transaction_fee: 1000,
                is_dust: false,
            },
            // Unspendable script_public_key due to an invalid public key script.
            Test {
                name: "unspendable script_public_key",
                tx_out: TransactionOutput::new(5000, invalid_script_public_key),
                minimum_relay_transaction_fee: 0,
                is_dust: true,
            },
        ];
        for test in tests {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let mut config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
                config.minimum_relay_transaction_fee = test.minimum_relay_transaction_fee;
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                println!("test_is_transaction_output_dust test '{}' ", test.name);
                let res = mempool.is_transaction_output_dust(&test.tx_out);
                if res != test.is_dust {
                    println!("test_is_transaction_output_dust test '{}' failed: got {}, want {}", test.name, res, test.is_dust);
                }
                assert_eq!(test.is_dust, res);
            }
        }
    }

    #[test]
    fn test_check_transaction_standard_in_isolation() {
        // Create some dummy, but otherwise standard, data for transactions.
        let dummy_prev_out = TransactionOutpoint::new(kaspa_hashes::Hash64::from_u64_word(1), 1); // PR-9.5e: TransactionId is Hash64
        let dummy_sig_script = vec![0u8; 65];
        let dummy_tx_input = TransactionInput::new(dummy_prev_out, dummy_sig_script, MAX_TX_IN_SEQUENCE_NUM, 1);
        let addr_hash = vec![1u8; 32];

        let addr = Address::new(Prefix::Testnet, Version::PubKey, &addr_hash);
        let dummy_script_public_key = kaspa_txscript::pay_to_address_script(&addr);
        let dummy_tx_out = TransactionOutput::new(SOMPI_PER_KASPA, dummy_script_public_key);

        struct Test {
            name: &'static str,
            mtx: MutableTransaction,
            is_standard: bool,
        }

        fn new_mtx(tx: Transaction, mass: u64) -> MutableTransaction {
            let mut mtx = MutableTransaction::from_tx(tx);
            mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(mass, mass));
            mtx
        }

        let tests = vec![
            Test {
                name: "Typical pay-to-pubkey transaction",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input.clone()],
                        vec![dummy_tx_out.clone()],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: true,
            },
            Test {
                name: "Transaction version too high",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION + 1,
                        vec![dummy_tx_input.clone()],
                        vec![dummy_tx_out.clone()],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Transaction size is too large",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input.clone()],
                        vec![TransactionOutput::new(
                            0u64,
                            ScriptPublicKey::new(
                                MAX_SCRIPT_PUBLIC_KEY_VERSION,
                                ScriptVec::from_vec(vec![0u8; MAXIMUM_STANDARD_TRANSACTION_MASS as usize + 1]),
                            ),
                        )],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Signature script size is too large",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION + 1,
                        vec![TransactionInput::new(
                            dummy_prev_out,
                            vec![0u8; MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE as usize + 1],
                            MAX_TX_IN_SEQUENCE_NUM,
                            1,
                        )],
                        vec![dummy_tx_out.clone()],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Valid but non standard public key script",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input.clone()],
                        vec![TransactionOutput::new(
                            SOMPI_PER_KASPA,
                            ScriptPublicKey::new(
                                MAX_SCRIPT_PUBLIC_KEY_VERSION,
                                ScriptBuilder::new().add_op(OpTrue).unwrap().script().into(),
                            ),
                        )],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Dust output",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input.clone()],
                        vec![TransactionOutput::new(0, dummy_tx_out.script_public_key)],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Null-data transaction",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input],
                        vec![TransactionOutput::new(
                            SOMPI_PER_KASPA,
                            ScriptPublicKey::new(
                                MAX_SCRIPT_PUBLIC_KEY_VERSION,
                                ScriptBuilder::new().add_op(OpReturn).unwrap().script().into(),
                            ),
                        )],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
        ];

        for test in tests {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let mut config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
                // This test exercises the upstream (legacy-permissive) output-class table with
                // non-ML-DSA fixtures, so opt out of the kaspa-pq PQ-only relay tightening.
                config.pq_only = false;
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                // Ensure standard-ness is as expected.
                println!("test_check_transaction_standard_in_isolation test '{}' ", test.name);
                let res = mempool.check_transaction_standard_in_isolation(&test.mtx);
                if res.is_ok() && test.is_standard {
                    // Test passes since function returned standard for a
                    // transaction which is intended to be standard.
                    continue;
                }
                if res.is_ok() && !test.is_standard {
                    println!("test_check_transaction_standard_in_isolation ({}): standard when it should not be", test.name);
                }
                if res.is_err() && test.is_standard {
                    println!(
                        "test_check_transaction_standard_in_isolation ({}): nonstandard when it should not be: {:?}",
                        test.name, res
                    );
                }
                assert_eq!(res.is_ok(), test.is_standard, "ensuring transaction standard-ness is as expected");
            }
        }
    }
}
