use crate::constants::{MAX_SOMPI, TX_VERSION};
use kaspa_consensus_core::config::params::PqEnforcementMode;
use kaspa_consensus_core::dns_finality::{
    DnsTxKind, dns_tx_kind, validate_compute_capability_payload, validate_compute_certificate_payload, validate_compute_challenge_tx,
    validate_compute_commitment_payload, validate_compute_verdict_payload, validate_precommit_evidence_tx,
    validate_slashing_evidence_tx, validate_stake_attestation_shard_payload, validate_stake_bond_tx, validate_stake_precommit_payload,
    validate_stake_unbond_payload,
};
use kaspa_consensus_core::palw_carriage::{palw_carriage_tx_kind, validate_palw_carriage_stage1_tx};
use kaspa_consensus_core::palw_fp_objects_v3::validate_palw_fp_commitment_tx_under_v3;
use kaspa_consensus_core::palw_lifecycle_objects_v2::validate_palw_lifecycle_tx;
use kaspa_consensus_core::subnets::{
    SUBNETWORK_ID_PALW_FP_COMMITMENT, SUBNETWORK_ID_PALW_LIFECYCLE, SUBNETWORK_ID_TOKEN_BURN, SUBNETWORK_ID_TOKEN_TRANSFER,
};
use kaspa_consensus_core::token::{validate_token_burn_payload, validate_token_transfer_payload};
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
        check_transaction_subnetwork(tx, self.palw_panel_da_admissible, self.palw_prompt_ids_form)?;
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
            // ADR-0087 Decision 3: a model market's sink is unspendable by design and holds the
            // MSK a buy pays into the curve; recognised by its exact script, and only on a network
            // that declares the market at all (a dormant network keeps this rule unchanged). Found
            // by the devnet drill: without this arm no carrier buy was ever consensus-valid on a
            // PQ-only network — ADR-0087's tests folded the object but never validated its carrier.
            if self.model_sink_outputs_allowed
                && kaspa_consensus_core::palw_model_market_v1::palw_model_sink_class_v1(&output.script_public_key).is_some()
            {
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

        // `ghostdag_k + 2` was the classic mergeset bound: one output per blue (at most `k + 1`)
        // plus one aggregate for the reds. It was never widened for ConsensusV2, which appends the
        // §D inclusion bounty, the §E validator payouts and the Decision 10 escrow releases — so on
        // testnet-11 the first claim to reach `Final` produced a 4-output coinbase against a limit
        // of 3, and the producer's own chain refused 112 consecutive blocks it had built itself.
        //
        // **And `k` was still the wrong bound afterwards, for the same reason one level down**
        // (mainnet audit, 2026-09-05). ADR-0058 pays every entitled in-window RED its own output
        // (`outputs.push(...)` per red in `expected_coinbase_transaction`), and the reds are not
        // bounded by `ghostdag_k` — they are bounded by the MERGESET, which is 180 on every V2
        // preset against a limit of `1 + 2 + 25 = 28`. A mergeset carrying more than ~26 entitled
        // reds therefore makes the coinbase this very node builds fail its own isolation check:
        // the 112-block halt above, reachable again, and reachable on purpose by anyone willing to
        // widen a mergeset. The flag is unfenced — `self.palw_state_params_v2.is_some()` — so it is
        // on for testnet-11 today and for any carded mainnet.
        //
        // Safe to widen because it is not the rule that decides coinbase correctness:
        // `validate_coinbase_transaction` compares against the coinbase this node computes, by
        // exact hash. This is a cheap size guard ahead of that work, and widening it can only stop
        // it rejecting coinbases that are in fact correct.
        // One output per mergeset block (every blue, and every entitled red under ADR-0058), one
        // aggregate for the reds that are not entitled, and the appended kinds.
        let outputs_limit = self.mergeset_size_limit + 1 + kaspa_consensus_core::palw_state_v2::PALW_V2_COINBASE_EXTRA_OUTPUTS;
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

fn check_transaction_subnetwork(
    tx: &Transaction,
    palw_panel_da_admissible: bool,
    palw_prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> TxResult<()> {
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
            // MISAKA Verified LLM Token-Weighted BFT: stateless shape of a compute
            // certificate. Executor/verifier signatures, sortition membership, the
            // model-cost-table lookup and the challenge window are stateful and run
            // in the credit walk — a structurally valid certificate still mints zero
            // VLT if any of those fail.
            DnsTxKind::ComputeCertificate => validate_compute_certificate_payload(&tx.payload),
            // Like slashing evidence, a compute challenge is a pure evidence carrier
            // and must declare no outputs (the reporter reward is minted at
            // (challenge_tx_id, 0)).
            DnsTxKind::ComputeChallenge => validate_compute_challenge_tx(&tx.payload, &tx.outputs),
            // A validator's declaration that it runs a given (model, runtime, determinism class)
            // profile. Signature, bond binding, model-table membership and the expiry cap are
            // stateful and run in the credit walk when it builds a job's verifier pool.
            DnsTxKind::ComputeCapability => validate_compute_capability_payload(&tx.payload),
            // Phase 1 of the two-phase sortition. Its whole job is to exist on chain BEFORE the
            // beacon epoch, so the stateless layer only checks shape; the binding to a
            // certificate and the beacon derivation are stateful.
            DnsTxKind::ComputeCommitment => validate_compute_commitment_payload(&tx.payload),
            // A sortitioned verifier's standalone verdict. The self-consistency check (the
            // declared verdict must be what comparing the two receipt hashes implies) is
            // context-free and lives here; committee membership and the signature are stateful.
            DnsTxKind::ComputeVerdict => validate_compute_verdict_payload(&tx.payload),
            // Round 2 of DNS finality. The declared lock's *internal* possibility is context-free
            // and checked here; whether it is the lock this chain actually shows is a question
            // about history, answered by the credit walk, which counts a precommit only if the
            // declaration matches.
            DnsTxKind::StakePrecommit => validate_stake_precommit_payload(&tx.payload),
            // Round 2's equivocation evidence. Like slashing evidence it is a pure evidence
            // carrier and must declare no outputs; the two signatures are verified as a
            // block-validity rule, because this payload burns a bond and anyone can author a
            // contradiction naming someone else's.
            DnsTxKind::PrecommitEvidence => validate_precommit_evidence_tx(&tx.payload, &tx.outputs),
        }
        .map_err(TxRuleError::InvalidDnsOverlayPayload)?;
        Ok(())
    } else if tx.subnetwork_id == SUBNETWORK_ID_TOKEN_TRANSFER {
        // MISAKA Compute Token Program (design v0.1 §4.3): the token-op band is
        // routed + stateless-validated like every overlay band — admitting the
        // ids is part of the coordinated release, exactly as 0x10-0x1a and
        // 0x20-0x22 were. What the DAA fence governs is the *effect*: the
        // ledger fold binds an op only past `tkn_activation_daa_score`, and a
        // stateless-valid op that fails statefully (nonce, balance, signature)
        // is void, not consensus-fatal.
        validate_token_transfer_payload(&tx.payload).map_err(TxRuleError::InvalidTokenPayload)?;
        Ok(())
    } else if tx.subnetwork_id == SUBNETWORK_ID_TOKEN_BURN {
        validate_token_burn_payload(&tx.payload).map_err(TxRuleError::InvalidTokenPayload)?;
        Ok(())
    } else if let Some(kind) = palw_carriage_tx_kind(&tx.subnetwork_id) {
        // MISAKA PALW chain carriage (ADR-0029 Stage 1): the 0x40-0x45 band is routed +
        // stateless-validated like every overlay band before it. The body is decoded DIRECTLY
        // as the id's kind — no Stage-0 magic; at Stage 1 the kind lives in the subnetwork id —
        // and judged by the SAME per-kind validators the Stage-0 extractor runs, plus the
        // evidence-carrier no-outputs rule for refutations and their chunks (the
        // slashing-evidence precedent). Bond existence, ML-DSA-87 signature validity and every
        // cross-object question are stateful and are NOT consensus rules yet (Stage 2 is
        // explicitly out of scope; the in-node store only indexes).
        //
        // DEPLOYMENT: admitting these ids is part of a coordinated release, exactly as
        // 0x10-0x1a, 0x20-0x22 and 0x30-0x31 were — to a node without this dispatch they are
        // `SubnetworksDisabled`, so a block carrying one splits an unupgraded fleet. Shipping
        // this arm IS the release artifact, not a live activation.
        validate_palw_carriage_stage1_tx(kind, &tx.payload, &tx.outputs).map_err(TxRuleError::InvalidPalwCarriagePayload)?;
        Ok(())
    } else if tx.subnetwork_id == SUBNETWORK_ID_PALW_FP_COMMITMENT {
        // ADR-0044 free-prompt commitment (0x4a). Its own id rather than a carriage kind, because
        // the codec and the signing context are the free-prompt family's, and routing one band's
        // payload through the other band's validator is what separate ids exist to prevent.
        //
        // **This arm and the one below it are why a V2 network can produce weight at all.** Both
        // ids were defined, both had extractors, both had tests — and neither had a route, so
        // `check_transaction_subnetwork` reached the blanket reject below and every carrier of
        // either kind was refused at admission. A commitment could not be published, a claim could
        // not be licensed, a court could not be opened; every claim that did exist voided at
        // `BindTimeout` and PALW weight stayed permanently zero.
        // **Both door facts come from the ruleset the validator was built with** (ADR-0077 D16,
        // ADR-0081 D3): the mode-2 shape is admitted only where the fence is carried, and the
        // carried prompt ids must hash to the job under the network's form — flat everywhere the
        // fence is dormant, the tiled Merkle root on a genesis that armed it. A door that spelled
        // `false, Flat` here would refuse every honest commitment on such a network.
        validate_palw_fp_commitment_tx_under_v3(&tx.payload, palw_panel_da_admissible, palw_prompt_ids_form)
            .map_err(TxRuleError::InvalidPalwFpPayload)?;
        Ok(())
    } else if tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE {
        // ADR-0042 Decisions 7/8 claim lifecycle (0x4b): the licensing, the producer default and
        // the four court moves. The kinds that may NOT ride — bond and class registration, the
        // chain-derived panel binding, the free-prompt commitment that has its own id — are
        // refused here by the same table the extraction walk applies, so admission and extraction
        // give one answer.
        validate_palw_lifecycle_tx(&tx.payload).map_err(TxRuleError::InvalidPalwLifecyclePayload)?;
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
            params.mergeset_size_limit(),
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

    /// MISAKA Compute Token Program (design v0.1 §4.3): a transaction on the
    /// token-op band is routed to the token validators — accepted when the
    /// payload passes stateless validation, rejected with `InvalidTokenPayload`
    /// (not the blanket `SubnetworksDisabled`) when it does not. Per-field
    /// coverage lives in `kaspa_consensus_core::token`; this confirms the wiring.
    #[test]
    fn validate_token_subnetwork_tx() {
        use kaspa_consensus_core::dns_finality::{STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN};
        use kaspa_consensus_core::subnets::{SUBNETWORK_ID_TOKEN_BURN, SUBNETWORK_ID_TOKEN_TRANSFER};
        use kaspa_consensus_core::token::{TOK_ASSET_ID, TOKEN_PAYLOAD_VERSION_V1, TokenTransferPayload, TokenTxError};
        use kaspa_hashes::Hash64;

        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.mergeset_size_limit(),
            Default::default(),
        );
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

        // Well-formed transfer payload → accepted (the effect stays fenced; this
        // is admission only).
        let transfer = TokenTransferPayload {
            version: TOKEN_PAYLOAD_VERSION_V1,
            asset_id: TOK_ASSET_ID,
            from_pubkey: vec![0x11u8; STAKE_VALIDATOR_PUBKEY_LEN],
            to: Hash64::from_bytes([0x22u8; 64]),
            amount: 1_000,
            nonce: 1,
            signature: vec![0x33u8; STAKE_ATTESTATION_SIG_LEN],
        };
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_TOKEN_TRANSFER;
        tx.payload = borsh::to_vec(&transfer).unwrap();
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        // Phase A knows only TOK — any other asset id is rejected at the door.
        let mut alien = transfer.clone();
        alien.asset_id = 7;
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_TOKEN_TRANSFER;
        tx.payload = borsh::to_vec(&alien).unwrap();
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidTokenPayload(TokenTxError::UnknownAsset(7))));

        // Undecodable bytes on the burn subnetwork → InvalidTokenPayload(Decode),
        // proving 0x31 is routed to the validator rather than rejected outright.
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_TOKEN_BURN;
        tx.payload = vec![0xffu8, 0x00];
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidTokenPayload(TokenTxError::Decode)));
    }

    /// MISAKA PALW chain carriage (ADR-0029 Stage 1): a transaction on the PALW band
    /// (0x40-0x45) is routed to the per-kind stateless validators — accepted when the bare
    /// Borsh BODY (no Stage-0 magic) passes, rejected with `InvalidPalwCarriagePayload` (not
    /// the blanket `SubnetworksDisabled`) when it does not. Per-field coverage lives in
    /// `kaspa_consensus_core::palw_carriage`; this confirms the consensus-layer wiring, that
    /// the band has hard edges (an unrouted id still rejects), and that Stage-0 native
    /// carriage stays untouched.
    #[test]
    fn validate_palw_carriage_subnetwork_tx() {
        use kaspa_consensus_core::dns_finality::STAKE_ATTESTATION_SIG_LEN;
        use kaspa_consensus_core::palw_carriage::{
            PALW_CARRIAGE_VERSION_V1, PalwAttestationCarriageV1, PalwCarriageError, PalwEvidenceChunkCarriageV1,
        };
        use kaspa_consensus_core::subnets::{SUBNETWORK_ID_PALW_ATTESTATION, SUBNETWORK_ID_PALW_EVIDENCE_CHUNK};
        use kaspa_hashes::Hash64;

        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.mergeset_size_limit(),
            Default::default(),
        );
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

        // Well-formed attestation carriage BODY (bare Borsh — the Stage-0 payload minus its
        // 7-byte envelope) → accepted, outputs and all (only evidence kinds refuse outputs).
        let attestation = PalwAttestationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            commitment_root: Hash64::from_bytes([0x71u8; 64]),
            attestation: kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
                version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
                executor_id: Hash64::from_bytes([0xA2u8; 64]),
                job_context_hash: Hash64::from_bytes([0xC7u8; 64]),
                full_logits_trace_root: Hash64::from_bytes([0x71u8; 64]),
                committed_root: Hash64::from_bytes([0x71u8; 64]),
                bond_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x77u8; 64]), index: 1 },
                signature: vec![0x33u8; STAKE_ATTESTATION_SIG_LEN],
            },
            attester_id: Hash64::from_bytes([0xA2u8; 64]),
            bond_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x77u8; 64]), index: 1 },
        };
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_PALW_ATTESTATION;
        tx.payload = borsh::to_vec(&attestation).unwrap();
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        // Same id + garbage body → InvalidPalwCarriagePayload(BodyDecode), proving the id is
        // routed to the validators rather than rejected outright.
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_PALW_ATTESTATION;
        tx.payload = vec![0xffu8, 0x00];
        assert_match!(
            tv.validate_tx_in_isolation(&tx),
            Err(TxRuleError::InvalidPalwCarriagePayload(PalwCarriageError::BodyDecode(_)))
        );

        // Decodable body that fails the SAME stateless rule Stage 0 enforces (attester drifted
        // from the inner signer) → rejected at the door.
        let mut drifted = attestation.clone();
        drifted.attester_id = Hash64::from_bytes([0xA3u8; 64]);
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_PALW_ATTESTATION;
        tx.payload = borsh::to_vec(&drifted).unwrap();
        assert_match!(
            tv.validate_tx_in_isolation(&tx),
            Err(TxRuleError::InvalidPalwCarriagePayload(PalwCarriageError::AttesterMismatch))
        );

        // Evidence carriers declare no outputs (ADR-0029 §2, the slashing-evidence rule): a
        // valid chunk with `base`'s output → rejected; with none → accepted.
        let chunk = PalwEvidenceChunkCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            evidence_group_id: Hash64::from_bytes([0xF1u8; 64]),
            chunk_index: 0,
            chunk_count: 2,
            bytes: vec![0xABu8; 8],
        };
        let mut tx_with_out = base.clone();
        tx_with_out.subnetwork_id = SUBNETWORK_ID_PALW_EVIDENCE_CHUNK;
        tx_with_out.payload = borsh::to_vec(&chunk).unwrap();
        assert_match!(
            tv.validate_tx_in_isolation(&tx_with_out),
            Err(TxRuleError::InvalidPalwCarriagePayload(PalwCarriageError::EvidenceCarrierHasOutputs(1)))
        );
        let mut tx = tx_with_out;
        tx.outputs = vec![];
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        // The band grew by one: 0x46 is the equivocation kind, so it ROUTES — it is judged on its
        // body like every other band member, not turned away at the subnetwork. (`base`'s payload
        // is another kind's body, so the rejection is a decode failure, which is the proof that
        // routing happened at all.)
        let mut tx = base.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(0x46);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidPalwCarriagePayload(_)));

        // 0x47 is the step-conviction kind, so it routes too — the band grew again when
        // arithmetic conviction landed.
        let mut tx = base.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(0x47);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidPalwCarriagePayload(_)));

        // 0x48 is the bisection-move kind, so it routes too.
        let mut tx = base.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(0x48);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidPalwCarriagePayload(_)));

        // 0x49 is the receipt kind, so it routes too.
        let mut tx = base.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(0x49);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidPalwCarriagePayload(_)));

        // 0x4A and 0x4B are routed too, but NOT as carriage kinds — they are the free-prompt
        // commitment and the claim lifecycle, each with its own codec, so an attestation body on
        // either lands in that band's own validator and is refused by that band's own error. The
        // error type IS the assertion: a `InvalidPalwCarriagePayload` here would mean one band's
        // payload had reached another band's validator, which is what separate ids prevent.
        let mut tx = base.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(0x4A);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidPalwFpPayload(_)));

        let mut tx = base.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(0x4B);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidPalwLifecyclePayload(_)));

        // The hard edge moved with them: 0x4C (one past the last routed id) is NOT routed and
        // still rejects with the blanket `SubnetworksDisabled` — an unknown id stays a
        // coordinated-release matter.
        let mut tx = base.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(0x4C);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::SubnetworksDisabled(_)));

        // Stage-0 carriage is untouched: the SAME object with its magic envelope on the NATIVE
        // subnetwork remains admission-legal (native payloads are opaque to consensus).
        let mut tx = base;
        tx.payload = kaspa_consensus_core::palw_carriage::encode_palw_carriage_v1(
            &kaspa_consensus_core::palw_carriage::PalwCarriageV1::Attestation(attestation),
        );
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));
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
            params.mergeset_size_limit(),
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
    use kaspa_txscript::caches::TxScriptCacheCounters;
    use kaspa_txscript::opcodes::codes;
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
            p.mergeset_size_limit(),
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

    /// ADR-0087 Decision 3, as the devnet drill found it: the market's sink output (`OP_RETURN
    /// "MSKMDL01" <line id>`) is consensus-legal ONLY on a network that declares the market; a
    /// dormant network refuses it exactly as before, and no other `OP_RETURN` form rides along.
    #[test]
    fn consensus_mode_admits_the_model_sink_only_where_the_market_is_declared() {
        use kaspa_consensus_core::palw_model_market_v1::palw_model_sink_spk_v1;
        let line = kaspa_consensus_core::Hash64::from_u64_word(7);
        let sink = tx_with_output(palw_model_sink_spk_v1(&line), SUBNETWORK_ID_NATIVE);
        let dormant = validator(PqEnforcementMode::Consensus);
        assert_eq!(dormant.check_transaction_pq_output_classes(&sink), Err(TxRuleError::NonPqStandardOutputClass(0)));
        let mut declared = validator(PqEnforcementMode::Consensus);
        declared.model_sink_outputs_allowed = true;
        assert!(
            declared.check_transaction_pq_output_classes(&sink).is_ok(),
            "the sink is the one unspendable output the market needs"
        );
        // a look-alike: an OP_RETURN with another tag is not the sink and stays refused
        let mut other = vec![codes::OpReturn, codes::OpData8];
        other.extend_from_slice(b"MSKMDL02");
        let not_sink = tx_with_output(ScriptPublicKey::new(0, other.into()), SUBNETWORK_ID_NATIVE);
        assert_eq!(declared.check_transaction_pq_output_classes(&not_sink), Err(TxRuleError::NonPqStandardOutputClass(0)));
        assert!(declared.check_transaction_pq_output_classes(&tx_with_output(pq_p2pkh_spk(), SUBNETWORK_ID_NATIVE)).is_ok());
    }

    /// **ADR-0087 Decision 6: a build that SCHEDULES the market must accept, below the activation,
    /// exactly what a build that does not carry the market accepts** (mainnet audit 2026-09-06,
    /// M-9).
    ///
    /// The rule is an EQUALITY between two arms, not an agreement with one error value: arm (a) is
    /// a dormant ruleset and arm (b) is the same ruleset with the fence scheduled at 9,000,000.
    /// Below the activation both must REFUSE the same sink transaction — (a) in isolation, (b) in
    /// header context, since isolation must stay permissive or no sink could ever be valid. At and
    /// above the activation (b) accepts. Arm (c), armed from genesis, accepts everywhere.
    ///
    /// Before the repair (b) accepted the sink at DAA 0, H blocks before any market existed.
    #[test]
    fn a_scheduled_market_admits_the_sink_only_at_its_own_activation() {
        use crate::processes::transaction_validator::tx_validation_in_header_context::LockTimeArg;
        use kaspa_consensus_core::config::params::ForkActivation;
        use kaspa_consensus_core::palw_model_market_v1::palw_model_sink_spk_v1;

        let line = kaspa_consensus_core::Hash64::from_u64_word(7);
        let sink = tx_with_output(palw_model_sink_spk_v1(&line), SUBNETWORK_ID_NATIVE);

        // The three arms differ only in the fence they carry; the isolation boolean is DERIVED
        // from it, exactly as `TransactionValidator::new` derives it.
        let arm = |fence: Option<ForkActivation>| {
            let mut tv = validator(PqEnforcementMode::Consensus);
            tv.palw_model_market_fence = fence;
            tv.model_sink_outputs_allowed = fence.is_some();
            tv
        };
        // The whole verdict for one arm at one score: isolation first, then header context.
        let verdict = |tv: &TransactionValidator, daa: u64| -> Result<(), TxRuleError> {
            tv.check_transaction_pq_output_classes(&sink)?;
            tv.validate_tx_in_header_context(&sink, LockTimeArg::Finalized, daa)
        };

        let dormant = arm(None);
        let scheduled = arm(Some(ForkActivation::new(9_000_000)));
        let from_genesis = arm(Some(ForkActivation::always()));

        // (a) and (b) agree below the activation — that equality IS Decision 6.
        assert!(verdict(&dormant, 8_999_999).is_err(), "a dormant ruleset has no sink output class");
        assert!(
            verdict(&scheduled, 8_999_999).is_err(),
            "scheduling the fence must not relax a consensus rule before the fence's own score"
        );
        assert!(verdict(&dormant, 0).is_err());
        assert!(verdict(&scheduled, 0).is_err());
        // ..and they disagree only where the decision says they must: at the activation.
        assert!(verdict(&scheduled, 9_000_000).is_ok(), "the market exists at its activation");
        assert!(verdict(&scheduled, u64::MAX).is_ok());
        assert!(verdict(&dormant, 9_000_000).is_err(), "a dormant ruleset never gets a market");

        // The refusals are named, and each half names itself: isolation refuses the CLASS on a
        // dormant ruleset, header context refuses the HEIGHT on a scheduled one.
        assert_eq!(dormant.check_transaction_pq_output_classes(&sink), Err(TxRuleError::NonPqStandardOutputClass(0)));
        assert!(scheduled.check_transaction_pq_output_classes(&sink).is_ok(), "isolation must stay permissive");
        assert_eq!(
            scheduled.validate_tx_in_header_context(&sink, LockTimeArg::Finalized, 8_999_999),
            Err(TxRuleError::ModelSinkBeforeMarketActivation(0, 8_999_999))
        );

        // (c) armed from genesis: both doors open at DAA 0.
        assert!(verdict(&from_genesis, 0).is_ok());

        // A non-sink transaction is untouched by the new door on every arm, at every score.
        let plain = tx_with_output(pq_p2pkh_spk(), SUBNETWORK_ID_NATIVE);
        for tv in [&dormant, &scheduled, &from_genesis] {
            assert!(tv.validate_tx_in_header_context(&plain, LockTimeArg::Finalized, 0).is_ok());
        }
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

    /// **The isolation cap must cover every output the ConsensusV2 coinbase builder can emit.**
    ///
    /// It did not. `expected_coinbase_transaction` appends the ADR-0018 §D inclusion bounty, the
    /// §E validator payouts and the ADR-0042 Decision 10 escrow releases on top of the mergeset
    /// payout, while this rule still allowed `ghostdag_k + 2` — the classic mergeset-only bound.
    /// On testnet-11 the first claim to reach `Final` released one escrowed reward, the coinbase
    /// reached 4 outputs against a limit of 3, and the node refused 112 consecutive blocks its own
    /// producer had built. Nothing in the suite related the two numbers, so nothing objected.
    ///
    /// The relationship, not the constant: a coinbase carrying the mergeset AND a full block's
    /// worth of every appended kind must pass, and one output beyond that must not.
    #[test]
    fn the_coinbase_cap_admits_every_output_a_consensus_v2_block_can_pay() {
        use kaspa_consensus_core::palw_state_v2::{
            PALW_V2_COINBASE_EXTRA_OUTPUTS, PALW_V2_MAX_PAYOUTS_PER_BLOCK, PALW_V2_MAX_VALIDATOR_PAYOUTS,
        };
        // This module is not the one at the top of the file and does not inherit its imports.
        use crate::params::MAINNET_PARAMS;
        use crate::processes::transaction_validator::{TransactionValidator, errors::TxRuleError};
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE;
        use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutput, scriptvec};
        use kaspa_core::assert_match;

        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.mergeset_size_limit(),
            Default::default(),
        );

        let coinbase_with = |n: usize| {
            Transaction::new(
                0,
                vec![],
                (0..n).map(|_| TransactionOutput { value: 1, script_public_key: ScriptPublicKey::new(0, scriptvec!(0x51)) }).collect(),
                0,
                SUBNETWORK_ID_COINBASE,
                0,
                vec![],
            )
        };

        // **What the builder can emit at its worst — one output per MERGESET block, not per blue**
        // (mainnet audit, 2026-09-05). This used to read `k + 1` blues plus "one aggregate for the
        // reds", which was the pre-ADR-0058 shape: since ADR-0058 an entitled in-window red is paid
        // to its OWN script (`expected_coinbase_transaction` pushes one output per such red), and
        // reds are bounded by the mergeset — 180 on every V2 preset — not by `ghostdag_k`. The old
        // arithmetic put the cap at 28 and the builder could emit ~180, so a wide mergeset made the
        // producer's own block fail its own isolation check: the 112-block halt this function's
        // comment records, reachable again and reachable on purpose.
        let worst_case =
            params.mergeset_size_limit() as usize + 1 + 1 + PALW_V2_MAX_VALIDATOR_PAYOUTS as usize + PALW_V2_MAX_PAYOUTS_PER_BLOCK;
        assert!(
            tv.check_coinbase_in_isolation(&coinbase_with(worst_case)).is_ok(),
            "a coinbase paying every mergeset block plus every appended kind must pass; the builder can emit {worst_case} outputs"
        );

        // And the cap is still a cap.
        assert_match!(tv.check_coinbase_in_isolation(&coinbase_with(worst_case + 1)), Err(TxRuleError::CoinbaseTooManyOutputs(_, _)));

        // The bound the old arithmetic gave, stated so the regression cannot come back quietly: a
        // mergeset of 30 entitled reds is an ordinary DAG on a 180-block mergeset limit, and it is
        // past every version of this cap that counted blues only.
        let old_cap = params.ghostdag_k() as usize + 2 + PALW_V2_COINBASE_EXTRA_OUTPUTS as usize;
        assert!(
            old_cap < params.mergeset_size_limit() as usize,
            "the premise: a cap counting blues only sits BELOW the mergeset bound ({old_cap} against {}), so a mergeset \
             wide enough to pass it is an ordinary DAG rather than an attack",
            params.mergeset_size_limit()
        );
        assert!(
            tv.check_coinbase_in_isolation(&coinbase_with(old_cap + 1)).is_ok(),
            "one output past the blues-only cap must be legal — that cap was the halt"
        );

        // The two constants are one statement, so a change to either has to move both.
        assert_eq!(
            PALW_V2_COINBASE_EXTRA_OUTPUTS,
            PALW_V2_MAX_PAYOUTS_PER_BLOCK as u64 + 1 + PALW_V2_MAX_VALIDATOR_PAYOUTS,
            "the extra allowance must be exactly the kinds the builder appends"
        );
    }
}
