use crate::constants::{MAX_SOMPI, TX_VERSION};
use kaspa_consensus_core::config::params::PqEnforcementMode;
use kaspa_consensus_core::dns_finality::{
    DnsTxKind, dns_tx_kind, validate_slashing_evidence_tx, validate_stake_attestation_shard_payload, validate_stake_bond_tx,
    validate_stake_unbond_payload,
};
use kaspa_consensus_core::tx::Transaction;
use kaspa_txscript::script_class::{ScriptClass, parse_evm_deposit_lock};
use std::collections::HashSet;

use super::{
    TransactionValidator,
    errors::{TxResult, TxRuleError},
};

impl TransactionValidator {
    /// Performs a variety of transaction validation checks which are independent of any
    /// context -- header or utxo. **Note** that any check performed here should be moved to
    /// header contextual validation if it becomes HF activation dependent. This is bcs we rely
    /// on checks here to be truly independent and avoid calling it multiple times wherever possible
    /// (e.g., BBT relies on mempool in isolation checks even though virtual daa score might have changed)   
    pub fn validate_tx_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_inputs_in_isolation(tx)?;
        self.check_transaction_outputs_in_isolation(tx)?;
        self.check_transaction_pq_output_classes(tx)?;
        self.check_coinbase_in_isolation(tx)?;

        check_transaction_output_value_ranges(tx)?;
        check_duplicate_transaction_inputs(tx)?;
        check_gas(tx)?;
        check_transaction_subnetwork(tx)?;
        check_transaction_version(tx)
    }

    /// kaspa-pq PQ-only (ADR-0019 §7 / docs/kaspa-pq-design-mldsa87.md): on a
    /// PQ-active network **every** transaction output — native spend, coinbase
    /// (miner payout *and* validator-reward), and DNS-overlay — must use the sole
    /// standard ML-DSA-87 P2PKH script class, so no non-PQ output (legacy
    /// secp256k1, P2SH, or a signature-free script such as `OP_TRUE`) can ever
    /// enter the UTXO set. This complements §6 (which rejects *spending* non-PQ
    /// UTXOs at the script engine and the input-class check) by blocking their
    /// *creation*.
    ///
    /// There are intentionally **no exemptions**. The earlier coinbase / DNS
    /// carve-outs were a consensus hole: a block producer could put a non-PQ
    /// script in the coinbase miner output, or in a stake-bond output-1+ /
    /// attestation output, and mint a UTXO spendable without an ML-DSA signature.
    /// Every legitimate output is already ML-DSA P2PKH — validator-reward and
    /// stake-bond outputs are built by `p2pkh_mldsa87_spk`, and miners must pay a
    /// real ML-DSA P2PKH address (the no-wallet placeholder is ML-DSA P2PKH too).
    /// `SlashingEvidence` carries no outputs, so it is unaffected.
    ///
    /// This is a context-free rule, so it lives in isolation. kaspa-pq networks
    /// activate PQ enforcement at genesis (`pq_activation_daa_score = 0`), so
    /// gating on `pq_enforcement == Consensus` alone is correct here (isolation
    /// has no DAA score available). The genesis block is committed directly
    /// (`process_genesis`), never through this validator, and its premine output
    /// is ML-DSA P2PKH regardless. M-06 (launch policy): this design assumes PQ is
    /// genesis-active. A future net wanting a NON-genesis PQ activation could not
    /// reuse this isolation rule as-is — it would have to thread the activation DAA
    /// score into a context-bearing check instead.
    fn check_transaction_pq_output_classes(&self, tx: &Transaction) -> TxResult<()> {
        if !matches!(self.pq_enforcement, PqEnforcementMode::Consensus) {
            return Ok(());
        }
        for (i, output) in tx.outputs.iter().enumerate() {
            let class = ScriptClass::from_script(&output.script_public_key);
            // kaspa-pq EVM Lane v0.4 §9.2: the EVM_DEPOSIT_LOCK output class is
            // consensus-allowed (PQ-safe — its only script spend path is the
            // embedded ML-DSA P2PKH refund, gated by the timeout context rule;
            // the claim path consumes it via the accepting block's diff with no
            // script run). It is NOT a standard send class: wallets/mempool
            // standardness still treat it as deliberate-construction-only.
            if class == ScriptClass::EvmDepositLock {
                // Audit F3: reject an EVM_DEPOSIT_LOCK whose embedded claim_tip exceeds its own
                // value. The claim path rejects claim_tip > amount (consensus/.../evm/mod.rs), so
                // such a lock can NEVER be claimed — it would only strand value until the refund
                // window (permanent if timeout == u64::MAX). RPC + validator builders already reject
                // it; this closes the raw-tx hole so consensus never mints an unclaimable deposit.
                // (Context-free, so it belongs in isolation; class detection implies it parses.)
                let lock = parse_evm_deposit_lock(&output.script_public_key)
                    .expect("EvmDepositLock class detection implies the lock script parses");
                if lock.claim_tip_sompi > output.value {
                    return Err(TxRuleError::EvmDepositLockTipExceedsValue(i, lock.claim_tip_sompi, output.value));
                }
                continue;
            }
            if !class.is_pq_standard() {
                return Err(TxRuleError::NonPqStandardOutputClass(i));
            }
        }
        Ok(())
    }

    fn check_transaction_inputs_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_inputs_count(tx)?;
        self.check_transaction_signature_scripts(tx)
    }

    fn check_transaction_outputs_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_outputs_count(tx)?;
        self.check_transaction_script_public_keys(tx)
    }

    fn check_coinbase_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        if !tx.is_coinbase() {
            return Ok(());
        }
        if !tx.inputs.is_empty() {
            return Err(TxRuleError::CoinbaseHasInputs(tx.inputs.len()));
        }

        if tx.mass() > 0 {
            return Err(TxRuleError::CoinbaseNonZeroMassCommitment);
        }

        let outputs_limit = self.ghostdag_k as u64 + 2;
        if tx.outputs.len() as u64 > outputs_limit {
            return Err(TxRuleError::CoinbaseTooManyOutputs(tx.outputs.len(), outputs_limit));
        }

        for (i, output) in tx.outputs.iter().enumerate() {
            if output.script_public_key.script().len() > self.coinbase_payload_script_public_key_max_len as usize {
                return Err(TxRuleError::CoinbaseScriptPublicKeyTooLong(i));
            }
        }
        Ok(())
    }

    fn check_transaction_outputs_count(&self, tx: &Transaction) -> TxResult<()> {
        if tx.is_coinbase() {
            // We already check coinbase outputs count vs. Ghostdag K + 2
            return Ok(());
        }
        if tx.outputs.len() > self.max_tx_outputs {
            return Err(TxRuleError::TooManyOutputs(tx.outputs.len(), self.max_tx_inputs));
        }

        Ok(())
    }

    fn check_transaction_inputs_count(&self, tx: &Transaction) -> TxResult<()> {
        if !tx.is_coinbase() && tx.inputs.is_empty() {
            return Err(TxRuleError::NoTxInputs);
        }

        if tx.inputs.len() > self.max_tx_inputs {
            return Err(TxRuleError::TooManyInputs(tx.inputs.len(), self.max_tx_inputs));
        }

        Ok(())
    }

    // The main purpose of this check is to avoid overflows when calculating transaction mass later.
    fn check_transaction_signature_scripts(&self, tx: &Transaction) -> TxResult<()> {
        if let Some(i) = tx.inputs.iter().position(|input| input.signature_script.len() > self.max_signature_script_len) {
            return Err(TxRuleError::TooBigSignatureScript(i, self.max_signature_script_len));
        }

        Ok(())
    }

    // The main purpose of this check is to avoid overflows when calculating transaction mass later.
    fn check_transaction_script_public_keys(&self, tx: &Transaction) -> TxResult<()> {
        if let Some(i) = tx.outputs.iter().position(|out| out.script_public_key.script().len() > self.max_script_public_key_len) {
            return Err(TxRuleError::TooBigScriptPublicKey(i, self.max_script_public_key_len));
        }

        Ok(())
    }
}

fn check_duplicate_transaction_inputs(tx: &Transaction) -> TxResult<()> {
    let mut existing = HashSet::new();
    for input in &tx.inputs {
        if !existing.insert(input.previous_outpoint) {
            return Err(TxRuleError::TxDuplicateInputs);
        }
    }
    Ok(())
}

fn check_gas(tx: &Transaction) -> TxResult<()> {
    // This should be revised if subnetworks are activated (along with other validations that weren't copied from kaspad)
    if tx.gas > 0 {
        return Err(TxRuleError::TxHasGas);
    }
    Ok(())
}

fn check_transaction_version(tx: &Transaction) -> TxResult<()> {
    if tx.version != TX_VERSION {
        return Err(TxRuleError::UnknownTxVersion(tx.version));
    }
    Ok(())
}

fn check_transaction_output_value_ranges(tx: &Transaction) -> TxResult<()> {
    let mut total: u64 = 0;
    for (i, output) in tx.outputs.iter().enumerate() {
        if output.value == 0 {
            return Err(TxRuleError::TxOutZero(i));
        }

        if output.value > MAX_SOMPI {
            return Err(TxRuleError::TxOutTooHigh(i));
        }

        if let Some(new_total) = total.checked_add(output.value) {
            total = new_total
        } else {
            return Err(TxRuleError::OutputsValueOverflow);
        }

        if total > MAX_SOMPI {
            return Err(TxRuleError::TotalTxOutTooHigh);
        }
    }

    Ok(())
}

fn check_transaction_subnetwork(tx: &Transaction) -> TxResult<()> {
    if tx.is_coinbase() || tx.subnetwork_id.is_native() {
        Ok(())
    } else if let Some(kind) = dns_tx_kind(&tx.subnetwork_id) {
        // kaspa-pq Phase 10 (ADR-0009): DNS finality overlay subnetworks are
        // routed + stateless-validated by full nodes (unlike the upstream
        // `SubnetworksDisabled` blanket reject). Stateful checks — on-chain
        // bond existence, rollout-stage gating, ML-DSA-87 signature
        // verification, the `U ≥ R + E` dominance bound — land in later PRs.
        match kind {
            // ADR-0016 D.1: the StakeBond stateless check also verifies its
            // output-0 locks the declared stake (value == amount, owner P2PKH).
            DnsTxKind::StakeBond => validate_stake_bond_tx(&tx.payload, &tx.outputs),
            DnsTxKind::StakeAttestationShard => validate_stake_attestation_shard_payload(&tx.payload),
            // ADR-0013 Addendum C.2: a slashing tx is a pure evidence carrier —
            // it must declare no outputs so consensus can mint the reporter
            // reward at (slashing_tx_id, 0) without colliding with a tx output.
            DnsTxKind::SlashingEvidence => validate_slashing_evidence_tx(&tx.payload, &tx.outputs),
            // kaspa-pq H-05: stateless shape of a stake-unbond request (owner-key
            // binding + signature are verified in the stateful block-validity rule).
            DnsTxKind::StakeUnbond => validate_stake_unbond_payload(&tx.payload),
        }
        .map_err(TxRuleError::InvalidDnsOverlayPayload)?;
        Ok(())
    } else {
        Err(TxRuleError::SubnetworksDisabled(tx.subnetwork_id.clone()))
    }
}

// kaspa-pq Phase 9: re-enabled with 128-char (64-byte Hash64) txids per ADR-0008.
// Isolation validation does not verify signatures, so the only change required
// from the original fixtures is widening the spent-outpoint id to Hash64.
#[cfg(test)]
mod tests {
    use kaspa_consensus_core::{
        subnets::{SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_NATIVE, SubnetworkId},
        tx::{ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, scriptvec},
    };
    use kaspa_core::assert_match;

    use crate::{
        constants::TX_VERSION,
        params::MAINNET_PARAMS,
        processes::transaction_validator::{TransactionValidator, errors::TxRuleError},
    };

    #[test]
    fn validate_tx_in_isolation_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
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

        let valid_cb = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput {
                value: 0x12a05f200,
                script_public_key: ScriptPublicKey::new(
                    0,
                    scriptvec!(
                        0xa9, 0x14, 0xda, 0x17, 0x45, 0xe9, 0xb5, 0x49, 0xbd, 0x0b, 0xfa, 0x1a, 0x56, 0x99, 0x71, 0xc7, 0x7e, 0xba,
                        0x30, 0xcd, 0x5a, 0x4b, 0x87
                    ),
                ),
            }],
            0,
            SUBNETWORK_ID_COINBASE,
            0,
            vec![9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );

        tv.validate_tx_in_isolation(&valid_cb).unwrap();

        let valid_tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_slice(&[
                        0x03, 0x2e, 0x38, 0xe9, 0xc0, 0xa8, 0x4c, 0x60, 0x46, 0xd6, 0x87, 0xd1, 0x05, 0x56, 0xdc, 0xac, 0xc4, 0x1d,
                        0x27, 0x5e, 0xc5, 0x5f, 0xc0, 0x07, 0x79, 0xac, 0x88, 0xfd, 0xf3, 0x57, 0xa1, 0x87, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    ]),
                    index: 0,
                },
                signature_script: vec![
                    0x49, // OP_DATA_73
                    0x30, 0x46, 0x02, 0x21, 0x00, 0xc3, 0x52, 0xd3, 0xdd, 0x99, 0x3a, 0x98, 0x1b, 0xeb, 0xa4, 0xa6, 0x3a, 0xd1, 0x5c,
                    0x20, 0x92, 0x75, 0xca, 0x94, 0x70, 0xab, 0xfc, 0xd5, 0x7d, 0xa9, 0x3b, 0x58, 0xe4, 0xeb, 0x5d, 0xce, 0x82, 0x02,
                    0x21, 0x00, 0x84, 0x07, 0x92, 0xbc, 0x1f, 0x45, 0x60, 0x62, 0x81, 0x9f, 0x15, 0xd3, 0x3e, 0xe7, 0x05, 0x5c, 0xf7,
                    0xb5, 0xee, 0x1a, 0xf1, 0xeb, 0xcc, 0x60, 0x28, 0xd9, 0xcd, 0xb1, 0xc3, 0xaf, 0x77, 0x48,
                    0x01, // 73-byte signature
                    0x41, // OP_DATA_65
                    0x04, 0xf4, 0x6d, 0xb5, 0xe9, 0xd6, 0x1a, 0x9d, 0xc2, 0x7b, 0x8d, 0x64, 0xad, 0x23, 0xe7, 0x38, 0x3a, 0x4e, 0x6c,
                    0xa1, 0x64, 0x59, 0x3c, 0x25, 0x27, 0xc0, 0x38, 0xc0, 0x85, 0x7e, 0xb6, 0x7e, 0xe8, 0xe8, 0x25, 0xdc, 0xa6, 0x50,
                    0x46, 0xb8, 0x2c, 0x93, 0x31, 0x58, 0x6c, 0x82, 0xe0, 0xfd, 0x1f, 0x63, 0x3f, 0x25, 0xf8, 0x7c, 0x16, 0x1b, 0xc6,
                    0xf8, 0xa6, 0x30, 0x12, 0x1d, 0xf2, 0xb3, 0xd3, // 65-byte pubkey
                ],
                sequence: u64::MAX,
                sig_op_count: 0,
            }],
            vec![
                TransactionOutput {
                    value: 0x2123e300,
                    script_public_key: ScriptPublicKey::new(
                        0,
                        scriptvec!(
                            0x76, // OP_DUP
                            0xa9, // OP_HASH160
                            0x14, // OP_DATA_20
                            0xc3, 0x98, 0xef, 0xa9, 0xc3, 0x92, 0xba, 0x60, 0x13, 0xc5, 0xe0, 0x4e, 0xe7, 0x29, 0x75, 0x5e, 0xf7,
                            0xf5, 0x8b, 0x32, 0x88, // OP_EQUALVERIFY
                            0xac  // OP_CHECKSIG
                        ),
                    ),
                },
                TransactionOutput {
                    value: 0x108e20f00,
                    script_public_key: ScriptPublicKey::new(
                        0,
                        scriptvec!(
                            0x76, // OP_DUP
                            0xa9, // OP_HASH160
                            0x14, // OP_DATA_20
                            0x94, 0x8c, 0x76, 0x5a, 0x69, 0x14, 0xd4, 0x3f, 0x2a, 0x7a, 0xc1, 0x77, 0xda, 0x2c, 0x2f, 0x6b, 0x52,
                            0xde, 0x3d, 0x7c, 0x88, // OP_EQUALVERIFY
                            0xac  // OP_CHECKSIG
                        ),
                    ),
                },
            ],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );

        tv.validate_tx_in_isolation(&valid_tx).unwrap();

        let mut tx: Transaction = valid_tx.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(3);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::SubnetworksDisabled(_)));

        let mut tx = valid_tx.clone();
        tx.inputs = vec![];
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::NoTxInputs));

        let mut tx = valid_tx.clone();
        tx.inputs = (0..params.max_tx_inputs + 1).map(|_| valid_tx.inputs[0].clone()).collect();
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TooManyInputs(_, _)));

        let mut tx = valid_tx.clone();
        tx.inputs[0].signature_script = vec![0; params.max_signature_script_len + 1];
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TooBigSignatureScript(_, _)));

        let mut tx = valid_tx.clone();
        tx.outputs = (0..params.max_tx_outputs + 1).map(|_| valid_tx.outputs[0].clone()).collect();
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TooManyOutputs(_, _)));

        let mut tx = valid_tx.clone();
        tx.outputs[0].script_public_key = ScriptPublicKey::new(0, scriptvec![0u8; params.max_script_public_key_len + 1]);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TooBigScriptPublicKey(_, _)));

        let mut tx = valid_tx.clone();
        tx.inputs.push(tx.inputs[0].clone());
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TxDuplicateInputs));

        let mut tx = valid_tx.clone();
        tx.gas = 1;
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TxHasGas));

        let mut tx = valid_tx.clone();
        tx.payload = vec![0];
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        let mut tx = valid_tx;
        tx.version = TX_VERSION + 1;
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::UnknownTxVersion(_)));
    }

    /// kaspa-pq Phase 10 (ADR-0009): a transaction routed by a DNS finality
    /// overlay subnetwork is accepted when its payload passes stateless
    /// validation, and rejected with `InvalidDnsOverlayPayload` (not the
    /// upstream blanket `SubnetworksDisabled`) when it does not. Exhaustive
    /// per-field payload coverage lives in `kaspa_consensus_core::dns_finality`;
    /// this test only confirms the consensus-layer wiring.
    #[test]
    fn validate_dns_overlay_subnetwork_tx() {
        use kaspa_consensus_core::dns_finality::{
            DNS_PAYLOAD_VERSION_V1, DnsTxError, STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN, SlashingEvidencePayload,
            StakeAttestation, StakeBondPayload, p2pkh_mldsa87_spk, validator_id_from_pubkey,
        };
        use kaspa_consensus_core::subnets::{SUBNETWORK_ID_SLASHING_EVIDENCE, SUBNETWORK_ID_STAKE_BOND};
        use kaspa_hashes::Hash64;

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

        // A native funding-style tx (one input, one output) reused as the
        // carrier; only `subnetwork_id` + `payload` vary across cases.
        let base = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x11u8; 64]), index: 0 },
                signature_script: vec![0u8; 64],
                sequence: u64::MAX,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 0x2123e300, script_public_key: ScriptPublicKey::new(0, scriptvec!(0x76, 0xa9, 0x14)) }],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );

        // Well-formed stake-bond payload → accepted.
        let validator_pubkey = vec![0xccu8; STAKE_VALIDATOR_PUBKEY_LEN];
        let bond = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: Hash64::from_bytes([0xaau8; 64]),
            // audit H-04: canonical key-derived overlay identity.
            validator_pubkey_hash: validator_id_from_pubkey(&validator_pubkey),
            validator_pubkey,
            amount: 1_000,
            activation_daa_score: 0,
            unbonding_period_blocks: 1,
            owner_reward_spk_payload: [0xddu8; 64],
        };
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_STAKE_BOND;
        tx.payload = borsh::to_vec(&bond).unwrap();
        // ADR-0016 D.1: output-0 must lock the stake (value == amount, owner P2PKH).
        tx.outputs[0] = TransactionOutput::new(bond.amount, p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload));
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        // Bond whose output-0 does not lock `amount` (ADR-0016 D.1) → rejected.
        let mut tx_unlocked = tx.clone();
        tx_unlocked.outputs[0] = TransactionOutput::new(bond.amount - 1, p2pkh_mldsa87_spk(&bond.owner_reward_spk_payload));
        assert_match!(
            tv.validate_tx_in_isolation(&tx_unlocked),
            Err(TxRuleError::InvalidDnsOverlayPayload(DnsTxError::BondOutputValueMismatch { .. }))
        );

        // Malformed stake-bond payload (zero amount) → InvalidDnsOverlayPayload.
        let mut bad = bond.clone();
        bad.amount = 0;
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_STAKE_BOND;
        tx.payload = borsh::to_vec(&bad).unwrap();
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidDnsOverlayPayload(DnsTxError::ZeroBondAmount)));

        // Undecodable bytes on a DNS subnetwork → InvalidDnsOverlayPayload(Decode),
        // proving the id is routed to the validators rather than rejected outright.
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_SLASHING_EVIDENCE;
        tx.payload = vec![0xffu8, 0x00];
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidDnsOverlayPayload(DnsTxError::Decode)));

        // ADR-0013 Addendum C.2: a slashing-evidence tx is a pure evidence
        // carrier whose reporter reward is minted by consensus at
        // (slashing_tx_id, 0). A well-formed payload is accepted iff the tx
        // declares no outputs; any output would create a UTXO that collides
        // with that mint. Build valid equivocation evidence (two attestations
        // sharing one (bond_outpoint, validator_id, epoch) triple but
        // approving different anchors).
        let attestation = |target: u8| StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: Hash64::from_bytes([0xa5u8; 64]),
            bond_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x77u8; 64]), index: 42 },
            epoch: 7,
            target_hash: Hash64::from_bytes([target; 64]),
            target_daa_score: 1_234_567,
            validator_set_commitment: Hash64::default(), // audit #4: VSC is a fixed-zero invariant
            signature: vec![0x33u8; STAKE_ATTESTATION_SIG_LEN],
        };
        let evidence = SlashingEvidencePayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x77u8; 64]), index: 42 },
            attestation_a: attestation(0x11),
            attestation_b: attestation(0x33),
            reporter_reward_spk_payload: [0xeeu8; 64],
        };
        let evidence_payload = borsh::to_vec(&evidence).unwrap();

        // Valid evidence + no outputs → accepted.
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_SLASHING_EVIDENCE;
        tx.payload = evidence_payload.clone();
        tx.outputs = vec![];
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        // Valid evidence + a (non-zero) declared output → rejected with
        // SlashingEvidenceHasOutputs. (A zero-value output is independently
        // caught earlier by the `TxOutZero` range check, so the carrier here
        // keeps `base`'s non-zero output to exercise this rule specifically.)
        let mut tx_with_out = base;
        tx_with_out.subnetwork_id = SUBNETWORK_ID_SLASHING_EVIDENCE;
        tx_with_out.payload = evidence_payload;
        assert_match!(
            tv.validate_tx_in_isolation(&tx_with_out),
            Err(TxRuleError::InvalidDnsOverlayPayload(DnsTxError::SlashingEvidenceHasOutputs(1)))
        );
    }
}

#[cfg(test)]
mod pq_output_class_enforcement_tests {
    //! kaspa-pq PQ-only (ADR-0019 §7 / docs/kaspa-pq-design-mldsa87.md): the
    //! consensus output-class rule. On a PQ-active network every transaction
    //! output — native, coinbase (miner + validator-reward), and DNS-overlay —
    //! must be ML-DSA P2PKH; there are no exemptions. Drives the private
    //! `check_transaction_pq_output_classes` directly so it is isolated from the
    //! other in-isolation checks.
    use super::TransactionValidator;
    use kaspa_consensus_core::config::params::{MAINNET_PARAMS, PqEnforcementMode};
    use kaspa_consensus_core::errors::tx::TxRuleError;
    use kaspa_consensus_core::subnets::{SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_STAKE_BOND};
    use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutput};
    use kaspa_txscript::opcodes::codes;
    use kaspa_txscript::caches::TxScriptCacheCounters;
    use smallvec::smallvec;
    use std::sync::Arc;

    fn validator(mode: PqEnforcementMode) -> TransactionValidator {
        let p = &MAINNET_PARAMS;
        let mut tv = TransactionValidator::new_for_tests(
            p.max_tx_inputs,
            p.max_tx_outputs,
            p.max_signature_script_len,
            p.max_script_public_key_len,
            p.coinbase_payload_script_public_key_max_len,
            p.coinbase_maturity,
            p.ghostdag_k(),
            Arc::new(TxScriptCacheCounters::default()),
        );
        // new_for_tests defaults to Disabled; set the mode under test.
        tv.pq_enforcement = mode;
        tv.pq_activation_daa_score = 0;
        tv
    }

    /// kaspa-pq ML-DSA-87 P2PKH (ADR-0019 §8):
    /// `OP_DUP OP_BLAKE2B_512 OP_DATA64 <64B> OP_EQUALVERIFY OP_CHECKSIGMLDSA87` (69 bytes).
    fn pq_p2pkh_spk() -> ScriptPublicKey {
        let mut s = vec![codes::OpDup, codes::OpBlake2b512, codes::OpData64];
        s.extend_from_slice(&[0u8; 64]);
        s.push(codes::OpEqualVerify);
        s.push(codes::OpCheckSigMlDsa87);
        ScriptPublicKey::new(0, s.into())
    }

    /// A non-ML-DSA-P2PKH script (`OP_TRUE` -> ScriptClass::NonStandard).
    fn legacy_spk() -> ScriptPublicKey {
        ScriptPublicKey::new(0, smallvec![codes::OpTrue])
    }

    fn tx_with_output(spk: ScriptPublicKey, subnetwork: kaspa_consensus_core::subnets::SubnetworkId) -> Transaction {
        Transaction::new(0, vec![], vec![TransactionOutput { value: 1000, script_public_key: spk }], 0, subnetwork, 0, vec![])
    }

    #[test]
    fn disabled_mode_allows_any_output_class() {
        let tv = validator(PqEnforcementMode::Disabled);
        assert!(tv.check_transaction_pq_output_classes(&tx_with_output(legacy_spk(), SUBNETWORK_ID_NATIVE)).is_ok());
    }

    #[test]
    fn consensus_mode_allows_mldsa_p2pkh_output() {
        let tv = validator(PqEnforcementMode::Consensus);
        assert!(tv.check_transaction_pq_output_classes(&tx_with_output(pq_p2pkh_spk(), SUBNETWORK_ID_NATIVE)).is_ok());
    }

    #[test]
    fn consensus_mode_rejects_legacy_output() {
        let tv = validator(PqEnforcementMode::Consensus);
        assert_eq!(
            tv.check_transaction_pq_output_classes(&tx_with_output(legacy_spk(), SUBNETWORK_ID_NATIVE)),
            Err(TxRuleError::NonPqStandardOutputClass(0))
        );
    }

    #[test]
    fn consensus_mode_rejects_non_pq_coinbase_output() {
        let tv = validator(PqEnforcementMode::Consensus);
        // The coinbase miner output is block-producer-controlled; a non-PQ script
        // there (e.g. OP_TRUE) would mint a signature-free UTXO. No exemption now.
        assert_eq!(
            tv.check_transaction_pq_output_classes(&tx_with_output(legacy_spk(), SUBNETWORK_ID_COINBASE)),
            Err(TxRuleError::NonPqStandardOutputClass(0))
        );
        // A canonical ML-DSA P2PKH coinbase output is accepted.
        assert!(tv.check_transaction_pq_output_classes(&tx_with_output(pq_p2pkh_spk(), SUBNETWORK_ID_COINBASE)).is_ok());
    }

    #[test]
    fn consensus_mode_rejects_non_pq_overlay_output() {
        let tv = validator(PqEnforcementMode::Consensus);
        // DNS-overlay outputs beyond the payload-pinned bond output-0 (stake-bond
        // change / attestation change) are class-checked too — no blanket exemption.
        assert_eq!(
            tv.check_transaction_pq_output_classes(&tx_with_output(legacy_spk(), SUBNETWORK_ID_STAKE_BOND)),
            Err(TxRuleError::NonPqStandardOutputClass(0))
        );
        assert!(tv.check_transaction_pq_output_classes(&tx_with_output(pq_p2pkh_spk(), SUBNETWORK_ID_STAKE_BOND)).is_ok());
    }
}
