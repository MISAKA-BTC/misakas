use crate::constants::{MAX_SOMPI, TX_VERSION};
use kaspa_consensus_core::config::params::PqEnforcementMode;
use kaspa_consensus_core::dns_finality::{
    DnsTxKind, dns_tx_kind, validate_slashing_evidence_tx, validate_stake_attestation_shard_payload, validate_stake_bond_tx,
    validate_stake_unbond_payload,
};
use kaspa_consensus_core::palw::validate_palw_overlay_tx;
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
        // ADR-0040 (AUTH-TXSHAPE) runs FIRST so the 0x38 canonical-shape violation is the reported
        // error rather than whichever generic rule an illegal input/output happens to trip on the way.
        check_palw_block_authorization_shape(tx)?;
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

    /// kaspa-pq ADR-0040 **ECON-02** — the coinbase output cap, decomposed against what
    /// `CoinbaseManager::expected_coinbase_transaction` actually pushes.
    ///
    /// The upstream constant is `ghostdag_k + 2`, which decomposes as **(k+1) blue-source outputs +
    /// 1 merged-red output**: GHOSTDAG bounds `mergeset_blues` at `k+1`
    /// (`processes/ghostdag/protocol.rs`), the hash-lane blue arm pushes exactly one output per blue
    /// source, and the red arm pushes one aggregated output for all reds.
    ///
    /// ADR-0040 PALW cap. An algo-4 blue source pushes up to three outputs:
    /// provider A, provider B, and the tx-fee Worker share (`processes/coinbase.rs`, the
    /// `WorkRewardClass::ReplicaPalw` arm). So the blue term becomes `3·(k+1)` and the cap must widen
    /// to `3·(k+1) + 1` or a legitimately-constructed PALW block is rejected by its own coinbase.
    ///
    /// Fence rationale. Widening the cap is a consensus relaxation, so it applies only to presets that
    /// define a PALW lane. Mainnet, testnet-10, simnet and devnet retain `ghostdag_k + 2`; the PALW
    /// presets use the widened cap.
    ///
    /// The fence is static; `palw_algo4_accept` is runtime-configurable. The cap is therefore based on
    /// `palw_activation_daa_score`, not `palw_algo4_accept`. A runtime flag must not alter coinbase
    /// validity for otherwise identical blocks.
    ///
    /// Taking the wide arm on the two PALW presets costs nothing while `palw_algo4_accept` is false:
    /// no algo-4 block can be accepted, so no coinbase carrying the wide PALW arm can exist, and the
    /// cap is simply not the binding constraint. Both presets are closed (seeder refusal,
    /// `--connect-peers`, `--archival`) and activate only by re-genesis.
    ///
    /// # Deliberately NOT fixed here: the pre-existing validator/bounty overflow
    ///
    /// Neither the old cap nor the widened one accounts for the outputs appended *after* the
    /// blue/red loops: `validator_reward_outputs` (ADR-0018 §E participation payouts plus the §E
    /// deferred quality-bonus outputs) and the §D inclusion bounty. The §E set is bounded only by the
    /// number of distinct `(bond, epoch)` pairs the block includes plus the epochs it finalizes
    /// (`virtual_processor/utxo_validation.rs::validator_reward_outputs_for_block`) — i.e. by block
    /// mass, not by any small constant. kaspa-pq therefore *already* has a latent worst-case
    /// coinbase-rejection cliff, independent of PALW and reachable on the current mainnet/testnet
    /// presets where the DNS carve is active from block 1.
    ///
    /// That is a separate finding with a separate blast radius: fixing it means relaxing the cap on
    /// **live** networks, which needs its own activation fence and its own audit. It is deliberately
    /// left alone here so that ECON-02 changes exactly the PALW term and nothing else — the widened
    /// arm reproduces the same (documented) gap one notch up rather than silently half-closing it.
    /// `coinbase_output_cap_ignores_validator_and_bounty_tail` pins the gap so it cannot be lost.
    fn coinbase_outputs_limit(&self) -> u64 {
        let blues = self.ghostdag_k as u64 + 1;
        if self.palw_lane_present {
            // 3 outputs per PALW blue source (provider A / provider B / fee-worker) + 1 merged red.
            3 * blues + 1
        } else {
            // Upstream: 1 output per blue source + 1 merged red.
            blues + 1
        }
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

        let outputs_limit = self.coinbase_outputs_limit();
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
        // kaspa-pq **ADR-0040 P1-6** — the per-block ticket authorization is block METADATA, not a
        // transfer, so like the coinbase it carries no inputs. The generic "every non-coinbase
        // transaction must spend something" rule therefore does not apply to it.
        //
        // This is an EXEMPTION FROM THIS RULE ONLY, not a licence to be shapeless: the input-less-ness
        // (and output-less-ness, and every other free field) is REQUIRED — not merely tolerated — by
        // `check_palw_block_authorization_shape` above, which is the authority on the 0x38 shape. An
        // earlier version of this comment asserted those properties as if something enforced them; from
        // ADR-0040 (AUTH-TXSHAPE) onwards something does.
        //
        // Why this is not a free-transaction surface. Two separate arguments, because clause 7 only
        // covers one of the two cases:
        //
        //  * On an ALGO-4 block, body validation clause 7 admits **at most one** such transaction, in
        //    the LAST position, and only when it carries a valid ML-DSA-87 signature by the leaf's
        //    declared ticket authority binding that block's header preimage and tx set.
        //  * On any OTHER block, clause 7 does not run at all (`check_palw_ticket` returns early unless
        //    `pow_algo_id == POW_ALGO_ID_PALW_REPLICA`), so neither the count nor the position of 0x38
        //    transactions is constrained. That is bounded instead by ORDINARY BLOCK MASS: the shape rule
        //    pins the DECLARED `mass()` (the storage-mass commitment) to 0, but `check_block_mass`
        //    derives `compute_mass` and `transient_mass` from the serialized transaction via
        //    `calc_non_contextual_masses`, and transient mass is proportional to byte length. So padding
        //    a block with authorizations costs block mass at the normal per-byte rate and cannot inflate
        //    a block beyond `max_block_mass`. Zero declared storage mass is correct on its own terms —
        //    the transaction has no outputs, so it creates no UTXO to charge for.
        //
        // In both cases it moves no value: zero inputs and zero outputs are required, not merely
        // tolerated.
        let is_ticket_authorization = tx.subnetwork_id == crate::processes::palw::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION;
        if !tx.is_coinbase() && !is_ticket_authorization && tx.inputs.is_empty() {
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

/// Validates the canonical PALW ticket-authorization transaction shape (subnetwork `0x38`).
///
/// # Consensus rationale
///
/// The authorization root excludes the authorization transaction to avoid self-reference. All other
/// transaction fields are therefore fixed so one authorization maps to one block serialization.
///
/// # Enumerated against every field of [`Transaction`]
///
/// * `version` — pinned to [`TX_VERSION`]. Also pinned globally by `check_transaction_version`;
///   restated here so the whole invariant reads in one place rather than depending on a rule that a
///   future version bump could widen.
/// * `inputs` — pinned EMPTY. `check_transaction_inputs_count` only *permits* this; nothing required
///   it before.
/// * `outputs` — pinned EMPTY, mirroring the `SlashingEvidence` precedent. An output here cannot mint
///   (an input-less transaction fails the UTXO-context value check and is silently skipped), but it is
///   freely chosen bytes in the merkle leaf, which is the whole defect.
/// * `lock_time` — pinned to 0. Nothing else in the pipeline constrains it: `check_tx_is_finalized`
///   iterates the input list, so a ZERO-INPUT transaction is vacuously finalized at ANY lock_time.
///   This field alone was a 2^64 twin-block multiplier.
/// * `subnetwork_id` — the discriminator that selects this rule; it is 0x38 by construction.
/// * `gas` — pinned to 0. Also pinned globally by `check_gas`; restated for the same reason as
///   `version`.
/// * `payload` — pinned canonical by the strict borsh round-trip in
///   `kaspa_consensus_core::palw::validate_block_authorization`, reached via
///   `check_transaction_subnetwork` → `validate_palw_overlay_payload`. Clause 7 restates it.
/// * `mass` — pinned to 0, mirroring `TxRuleError::CoinbaseNonZeroMassCommitment`. `tx::hash` folds
///   the storage-mass commitment in whenever it is non-zero, so a mass commitment is a merkle-leaf
///   axis; it is only ever *checked* in UTXO context, which this transaction never reaches.
/// * `id` — derived from all of the above, not an independent field.
///
/// # Design commitment
///
/// The authorization is assembled by the block producer AFTER the template is finalized and is never a
/// relayed mempool transaction. That is why pinning `mass` to 0 is safe: no template builder stamps a
/// storage-mass commitment on it. Any future design that routes an authorization through the mempool,
/// or that sorts/reorders a block's transactions, is incompatible with this rule and with clause 7's
/// last-position requirement.
///
/// This is a context-free rule, so it lives in isolation and therefore also covers the mempool and
/// block-template surfaces, not only block body validation.
fn check_palw_block_authorization_shape(tx: &Transaction) -> TxResult<()> {
    if tx.subnetwork_id != crate::processes::palw::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION {
        return Ok(());
    }
    let free_field = if tx.version != TX_VERSION {
        Some("version")
    } else if !tx.inputs.is_empty() {
        Some("inputs")
    } else if !tx.outputs.is_empty() {
        Some("outputs")
    } else if tx.lock_time != 0 {
        Some("lock_time")
    } else if tx.gas != 0 {
        Some("gas")
    } else if tx.mass() != 0 {
        Some("mass")
    } else {
        None
    };
    match free_field {
        Some(field) => Err(TxRuleError::NonCanonicalPalwAuthorizationTx(field)),
        None => Ok(()),
    }
}

fn check_gas(tx: &Transaction) -> TxResult<()> {
    // Revisit this validation if subnetworks are activated.
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
    } else if let Some(kind) = tx.subnetwork_id.palw_tx_kind() {
        // ADR-0039: PALW subnetworks are routed through a strict, context-free v1 decoder here.
        // Activation, beacon phase, active-bond lookup, and ML-DSA verification require a block POV
        // and therefore belong to contextual validation rather than this reusable isolation check.
        //
        // ADR-0040 ECON-03: the tx-level form additionally enforces the provider-bond VALUE LOCK —
        // output-0 must lock the declared `amount_sompi` to the owner's own key — putting it at the
        // same coordinate as the DNS `StakeBond` arm above, where a rejection actually rejects the
        // transaction. It cannot live in the overlay-effect arm, whose `Result` the caller discards.
        validate_palw_overlay_tx(kind, &tx.payload, &tx.outputs).map_err(TxRuleError::InvalidPalwOverlayPayload)
    } else if let Some(kind) = tx.subnetwork_id.palw_compute_registry_tx_kind() {
        // ADR-MA §17.1: Compute Set registry payloads (0x40-0x44) decode strictly + canonically
        // here — context-free SHAPE only. The activation fence is a block-DAA rule enforced in
        // `check_palw_overlay_activation`'s registry clause, and the §9/§10.3 admission rules run
        // at the acceptance fold — both exactly mirroring the PALW band's layering.
        kaspa_consensus_core::palw_compute_set::parse_palw_compute_registry(kind, &tx.payload)
            .map(|_| ())
            .map_err(TxRuleError::InvalidComputeRegistryPayload)
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

    /// ADR-MA §17.1: a transaction routed by a Compute Set registry subnetwork (0x40-0x44) is
    /// accepted when its payload is the strict canonical encoding, and rejected with
    /// `InvalidComputeRegistryPayload` (never the blanket `SubnetworksDisabled`) when it is not.
    /// Per-rule payload coverage lives in `kaspa_consensus_core::palw_compute_set`; this test
    /// confirms only the consensus-layer wiring.
    #[test]
    fn validate_compute_registry_subnetwork_tx() {
        use kaspa_consensus_core::palw_compute_set::{
            ComputeSetRegistryError, PALW_COMPUTE_SET_DESCRIPTOR_VERSION, PALW_COMPUTE_SET_PROPOSAL_PAYLOAD_VERSION,
            PalwComputeSetDescriptorV2, PalwComputeSetProposalV1,
        };
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL;
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

        let h = |b: u8| Hash64::from_bytes([b; 64]);
        let proposal = PalwComputeSetProposalV1 {
            version: PALW_COMPUTE_SET_PROPOSAL_PAYLOAD_VERSION,
            descriptor: PalwComputeSetDescriptorV2 {
                version: PALW_COMPUTE_SET_DESCRIPTOR_VERSION,
                compute_vm_id: h(1),
                model_family_id: h(2),
                model_artifact_root: h(3),
                model_manifest_root: h(4),
                tokenizer_root: h(5),
                chat_template_root: h(6),
                preprocessing_root: h(7),
                decode_policy_root: h(8),
                semantic_program_root: h(9),
                shape_table_root: h(10),
                shape_cost_table_root: h(11),
                arithmetic_rules_root: h(12),
                overflow_budget_root: h(13),
                lut_root: h(14),
                trace_policy_root: h(15),
                checkpoint_policy_root: h(16),
                conformance_vector_root: h(17),
                modality_mask: 1,
                resource_limits_root: h(18),
            },
            proposer_credential: h(0x50),
            proposal_bond_ref: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x51u8; 64]), index: 0 },
            artifact_distribution_root: h(0x52),
            independent_build_attestation_root: h(0x53),
            requested_shadow_activation_daa: 1_000,
        };

        // Well-formed canonical proposal → accepted.
        let mut tx = base.clone();
        tx.subnetwork_id = SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL;
        tx.payload = borsh::to_vec(&proposal).unwrap();
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        // Trailing byte breaks strict canonicality → InvalidComputeRegistryPayload.
        let mut padded = tx.clone();
        padded.payload.push(0);
        assert_match!(
            tv.validate_tx_in_isolation(&padded),
            Err(TxRuleError::InvalidComputeRegistryPayload(ComputeSetRegistryError::MalformedRegistryPayload(_)))
        );

        // Unsupported payload version is pinned at this coordinate too.
        let mut wrong_version = proposal.clone();
        wrong_version.version = 9;
        let mut tx_wrong = base.clone();
        tx_wrong.subnetwork_id = SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL;
        tx_wrong.payload = borsh::to_vec(&wrong_version).unwrap();
        assert_match!(
            tv.validate_tx_in_isolation(&tx_wrong),
            Err(TxRuleError::InvalidComputeRegistryPayload(ComputeSetRegistryError::UnsupportedRegistryPayloadVersion(
                "compute_set_proposal",
                9
            )))
        );

        // 0x45 sits above the band: still the blanket SubnetworksDisabled, fail-closed.
        let mut alien = base.clone();
        alien.subnetwork_id = SubnetworkId::from_byte(0x45);
        assert_match!(tv.validate_tx_in_isolation(&alien), Err(TxRuleError::SubnetworksDisabled(_)));
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

    #[test]
    fn validate_palw_overlay_subnetwork_tx() {
        use kaspa_consensus_core::dns_finality::STAKE_ATTESTATION_SIG_LEN;
        use kaspa_consensus_core::palw::{
            PALW_PAYLOAD_VERSION_V1, PalwBeaconCommitV1, PalwBeaconRevealV1, PalwTxError, beacon_commitment,
        };
        use kaspa_consensus_core::subnets::{SUBNETWORK_ID_PALW_BEACON_COMMIT, SUBNETWORK_ID_PALW_BEACON_REVEAL};
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
        let mut tx = Transaction::new(
            TX_VERSION,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x31; 64]), index: 0 },
                signature_script: vec![0; 64],
                sequence: u64::MAX,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 1_000, script_public_key: ScriptPublicKey::new(0, scriptvec!(0x76, 0xa9, 0x14)) }],
            0,
            SUBNETWORK_ID_PALW_BEACON_COMMIT,
            0,
            vec![],
        );
        let bond = TransactionOutpoint { transaction_id: Hash64::from_bytes([0x32; 64]), index: 4 };
        let random = [0x33; 64];
        let commit = PalwBeaconCommitV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            epoch: 12,
            bond_outpoint: bond,
            commitment: beacon_commitment(12, &random, &bond),
            signature: vec![0x44; STAKE_ATTESTATION_SIG_LEN],
        };
        tx.payload = borsh::to_vec(&commit).unwrap();
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        let reveal = PalwBeaconRevealV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            epoch: 12,
            bond_outpoint: bond,
            random_64: random,
            signature: vec![0x55; STAKE_ATTESTATION_SIG_LEN],
        };
        tx.subnetwork_id = SUBNETWORK_ID_PALW_BEACON_REVEAL;
        tx.payload = borsh::to_vec(&reveal).unwrap();
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        // Every frozen PALW byte is routed to the PALW validator (never the generic subnet error).
        // 0x37 (provider unbond) is included as of ADR-0040 ECON-03 leg 5: its acceptance is open,
        // so a malformed request is a DECODE failure here rather than `UnsupportedKind`.
        for kind in 0x30..=0x37 {
            tx.subnetwork_id = SubnetworkId::from_byte(kind);
            tx.payload = vec![0xff, 0x00];
            assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::InvalidPalwOverlayPayload(PalwTxError::Decode)));
        }
        // A well-formed 0x37 is ACCEPTED at this coordinate — and that means only that its SHAPE is
        // right. The signature below is 0x42 repeated, which no key produced; authorization is
        // `palw_provider_unbond_authorized` (processes/palw.rs), which needs a point of view this
        // validator does not have. Acceptance is safe only while an accepted 0x37 mutates nothing,
        // which holds because no `ProviderBondView` is composed during validation yet.
        {
            use kaspa_consensus_core::dns_finality::{STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN};
            use kaspa_consensus_core::palw::PalwProviderUnbondRequestV1;
            use kaspa_consensus_core::tx::TransactionOutpoint;
            let req = PalwProviderUnbondRequestV1 {
                version: PALW_PAYLOAD_VERSION_V1,
                bond_outpoint: TransactionOutpoint::new(tx.id(), 0),
                owner_public_key: vec![0x41; STAKE_VALIDATOR_PUBKEY_LEN],
                signature: vec![0x42; STAKE_ATTESTATION_SIG_LEN],
            };
            tx.subnetwork_id = SubnetworkId::from_byte(0x37);
            tx.payload = borsh::to_vec(&req).unwrap();
            assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

            // Shape is still enforced: a short signature is refused.
            let mut short = req;
            short.signature.pop();
            tx.payload = borsh::to_vec(&short).unwrap();
            assert_match!(
                tv.validate_tx_in_isolation(&tx),
                Err(TxRuleError::InvalidPalwOverlayPayload(PalwTxError::InvalidSignatureLen(_)))
            );
        }

        // kaspa-pq ADR-0040 ECON-03 — THE VALUE LOCK IS ENFORCED AT THIS COORDINATE.
        //
        // This is the test that fails if the `validate_palw_overlay_tx` call above is reverted to
        // `validate_palw_overlay_payload`. Without it, the lock rule would exist in consensus-core and
        // be reachable by nobody — which is exactly the failure mode this ADR keeps shipping.
        {
            use kaspa_consensus_core::dns_finality::STAKE_VALIDATOR_PUBKEY_LEN;
            use kaspa_consensus_core::palw::{PalwProviderBondPayloadV1, provider_bond_lock_spk};
            let owner_public_key = vec![0x41u8; STAKE_VALIDATOR_PUBKEY_LEN];
            let bond_payload = PalwProviderBondPayloadV1 {
                version: PALW_PAYLOAD_VERSION_V1,
                owner_public_key: owner_public_key.clone(),
                operator_group_id: Hash64::from_bytes([0x01; 64]),
                runtime_classes: vec![Hash64::from_bytes([0x02; 64])],
                capacity_by_shape: vec![(1, 10)],
                reward_key_root: Hash64::from_bytes([0x04; 64]),
                amount_sompi: 1_000,
                unbond_delay_epochs: 10,
            };
            let mut bond_tx = tx.clone();
            bond_tx.subnetwork_id = SubnetworkId::from_byte(0x30);
            bond_tx.payload = borsh::to_vec(&bond_payload).unwrap();

            // A bond with NO BACKING — the payload is perfectly well-formed, and before ECON-03 this
            // transaction was VALID while declaring collateral it did not have.
            bond_tx.outputs =
                vec![TransactionOutput { value: 1_000, script_public_key: ScriptPublicKey::new(0, scriptvec!(0x76, 0xa9, 0x14)) }];
            assert_match!(
                tv.validate_tx_in_isolation(&bond_tx),
                Err(TxRuleError::InvalidPalwOverlayPayload(PalwTxError::ProviderBondOutputScriptMismatch))
            );

            // No output-0 at all.
            bond_tx.outputs = vec![];
            assert_match!(
                tv.validate_tx_in_isolation(&bond_tx),
                Err(TxRuleError::InvalidPalwOverlayPayload(PalwTxError::MissingProviderBondOutput))
            );

            // A REAL bond: output-0 locks the declared amount to the owner's own key.
            bond_tx.outputs = vec![TransactionOutput { value: 1_000, script_public_key: provider_bond_lock_spk(&owner_public_key) }];
            assert_match!(tv.validate_tx_in_isolation(&bond_tx), Ok(()));
        }

        let mut bad = commit;
        bad.signature.pop();
        tx.subnetwork_id = SUBNETWORK_ID_PALW_BEACON_COMMIT;
        tx.payload = borsh::to_vec(&bad).unwrap();
        assert_match!(
            tv.validate_tx_in_isolation(&tx),
            Err(TxRuleError::InvalidPalwOverlayPayload(PalwTxError::InvalidSignatureLen(_)))
        );
    }

    /// kaspa-pq **ADR-0040 (AUTH-TXSHAPE)** — the 0x38 ticket-authorization transaction has exactly
    /// ONE legal serialization per payload.
    ///
    /// # The attack
    ///
    /// Algo-4 blocks do NO proof-of-work, and clause 7's bound merkle root EXCLUDES this transaction
    /// (an authorization cannot commit to a root containing itself). So every byte of this transaction
    /// that is hashed into the REAL `hash_merkle_root` but not determined by the payload is a free
    /// axis: an observer lifts one honest authorization, varies that byte, recomputes the root, and
    /// emits another fully valid zero-work block. `lock_time` alone gave 2^64 of them, because a
    /// zero-input transaction is vacuously finalized at any lock_time.
    ///
    /// # Coverage
    ///
    /// One REJECT arm per field of [`Transaction`] that this rule pins, plus the ACCEPT arm — without
    /// which every reject arm would be satisfied by a rule that simply banned the subnetwork. The two
    /// remaining fields are not free: `subnetwork_id` is the discriminator, and `id` is derived.
    /// `payload` canonicality is covered by the `palw_stateless_payload_validator_*` tests in
    /// consensus-core; the end-to-end block-level halves live in `virtual_processor::tests`.
    #[test]
    fn palw_block_authorization_tx_canonical_shape() {
        use kaspa_consensus_core::dns_finality::{STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN};
        use kaspa_consensus_core::palw::{PALW_PAYLOAD_VERSION_V1, PalwBlockAuthorizationV1};
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION;
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

        let auth = PalwBlockAuthorizationV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            batch_id: Hash64::from_bytes([0x41; 64]),
            leaf_index: 7,
            ticket_nullifier: Hash64::from_bytes([0x42; 64]),
            header_preimage_commitment: Hash64::from_bytes([0x43; 64]),
            authority_public_key: vec![0x44; STAKE_VALIDATOR_PUBKEY_LEN],
            signature: vec![0x45; STAKE_ATTESTATION_SIG_LEN],
        };
        let payload = borsh::to_vec(&auth).unwrap();
        let canonical = || Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION, 0, payload.clone());

        // ACCEPT: the shape both in-tree producers emit.
        assert_match!(tv.validate_tx_in_isolation(&canonical()), Ok(()));

        // REJECT — `version`. Pinned locally as well as globally, so a future version widening cannot
        // silently reopen this axis.
        let mut tx = canonical();
        tx.version = TX_VERSION + 1;
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::NonCanonicalPalwAuthorizationTx("version")));

        // REJECT — `inputs`. `check_transaction_inputs_count` merely PERMITS the empty input list on
        // this subnetwork; this is the rule that REQUIRES it. A signature_script is arbitrary bytes in
        // the merkle leaf under the FULL tx encoding.
        let mut tx = canonical();
        tx.inputs = vec![TransactionInput {
            previous_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x46; 64]), index: 0 },
            signature_script: vec![0xab; 32],
            sequence: u64::MAX,
            sig_op_count: 0,
        }];
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::NonCanonicalPalwAuthorizationTx("inputs")));

        // REJECT — `outputs`. An output here cannot MINT (an input-less transaction fails the UTXO
        // value check and is silently skipped), but it is freely chosen bytes in the merkle leaf, which
        // is the entire defect. Mirrors the `SlashingEvidence` no-outputs precedent.
        let mut tx = canonical();
        tx.outputs =
            vec![TransactionOutput { value: 1_000, script_public_key: ScriptPublicKey::new(0, scriptvec!(0x76, 0xa9, 0x14)) }];
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::NonCanonicalPalwAuthorizationTx("outputs")));

        // REJECT — `lock_time`, the 2^64 multiplier. NOTHING else in the pipeline constrains it:
        // `check_tx_is_finalized` iterates the input list, which is empty.
        let mut tx = canonical();
        tx.lock_time = 0xDEAD_BEEF;
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::NonCanonicalPalwAuthorizationTx("lock_time")));

        // REJECT — `gas`. Pinned globally too; restated so the invariant reads in one place.
        let mut tx = canonical();
        tx.gas = 1;
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::NonCanonicalPalwAuthorizationTx("gas")));

        // REJECT — `mass`. `tx::hash` folds the storage-mass commitment in whenever it is non-zero, so
        // it is a merkle-leaf axis; it is otherwise only ever CHECKED in UTXO context, which this
        // transaction never reaches. Mirrors `CoinbaseNonZeroMassCommitment`.
        let tx =
            Transaction::new_with_mass(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION, 0, payload.clone(), 1);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::NonCanonicalPalwAuthorizationTx("mass")));

        // Non-0x38 transactions are untouched by this rule: a native transfer keeps its inputs,
        // outputs and lock_time.
        let mut native = Transaction::new(
            TX_VERSION,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: TransactionId::from_slice(&[0x47; 64]), index: 0 },
                signature_script: vec![0; 64],
                sequence: u64::MAX,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 1_000, script_public_key: ScriptPublicKey::new(0, scriptvec!(0x76, 0xa9, 0x14)) }],
            0xDEAD_BEEF,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        assert_match!(tv.validate_tx_in_isolation(&native), Ok(()));
        native.lock_time = 0;
        assert_match!(tv.validate_tx_in_isolation(&native), Ok(()));
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

/// kaspa-pq ADR-0040 **ECON-02** — the coinbase output cap and its PALW fence. Drives the private
/// `coinbase_outputs_limit` / `check_coinbase_in_isolation` directly so the cap arithmetic is isolated
/// from the other in-isolation checks.
#[cfg(test)]
mod coinbase_output_cap_tests {
    use super::TransactionValidator;
    use kaspa_consensus_core::KType;
    use kaspa_consensus_core::config::params::MAINNET_PARAMS;
    use kaspa_consensus_core::errors::tx::TxRuleError;
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE;
    use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutput};
    use smallvec::smallvec;

    fn empty_spk() -> ScriptPublicKey {
        ScriptPublicKey::new(0, smallvec![])
    }

    fn coinbase_with_outputs(n: usize) -> Transaction {
        Transaction::new(
            0,
            vec![],
            (0..n).map(|_| TransactionOutput::new(1, empty_spk())).collect(),
            0,
            SUBNETWORK_ID_COINBASE,
            0,
            vec![],
        )
    }

    fn cap_validator(ghostdag_k: KType, palw_lane_present: bool) -> TransactionValidator {
        let params = MAINNET_PARAMS.clone();
        let mut tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            ghostdag_k,
            Default::default(),
        );
        tv.palw_lane_present = palw_lane_present;
        tv
    }

    /// The fence holds in the direction that matters: on every VALUE-BEARING network the cap is still
    /// the upstream `ghostdag_k + 2`, so no live node's view of coinbase validity moved. The two PALW
    /// presets take the widened arm, which is safe because the fence is a per-preset CONSTANT — every
    /// node on such a network computes the same cap, so there is nothing to disagree about.
    #[test]
    fn coinbase_output_cap_is_unchanged_on_every_value_bearing_preset() {
        use kaspa_consensus_core::config::params::{
            DEVNET_PALW_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, TESTNET_PALW_PARAMS, TESTNET_PARAMS,
        };
        for (name, params, expect_wide) in [
            ("mainnet", MAINNET_PARAMS, false),
            ("testnet", TESTNET_PARAMS, false),
            ("simnet", SIMNET_PARAMS, false),
            ("devnet", DEVNET_PARAMS, false),
            ("testnet-palw", TESTNET_PALW_PARAMS, true),
            ("devnet-palw", DEVNET_PALW_PARAMS, true),
        ] {
            let lane_present = params.palw_activation_daa_score != u64::MAX;
            assert_eq!(lane_present, expect_wide, "{name}: unexpected PALW lane presence");
            let k = params.ghostdag_k();
            let tv = cap_validator(k, lane_present);
            let expected = if expect_wide { 3 * (k as u64 + 1) + 1 } else { k as u64 + 2 };
            assert_eq!(tv.coinbase_outputs_limit(), expected, "{name}: coinbase output cap");
        }
    }

    /// The fence must never be the runtime-mutable acceptance lever. `--palw-enable-algo4` flips
    /// `palw_algo4_accept` in-process (kaspad/src/args.rs), so fencing a rule that governs EVERY
    /// coinbase on it would let two operators on one network disagree about an ordinary algo-3 block.
    /// This pins the two levers as INDEPENDENT: lane presence is a preset constant, acceptance is not.
    #[test]
    fn coinbase_output_cap_fence_is_static_not_the_acceptance_lever() {
        use kaspa_consensus_core::config::params::{DEVNET_PALW_PARAMS, MAINNET_PARAMS as MN};
        // Both acceptance polarities produce the same widened cap on a PALW preset.
        assert!(DEVNET_PALW_PARAMS.palw_activation_daa_score != u64::MAX, "devnet-palw must have the lane");
        for accept in [false, true] {
            let mut params = DEVNET_PALW_PARAMS;
            params.palw_algo4_accept = accept;
            let k = params.ghostdag_k();
            assert_eq!(
                cap_validator(k, params.palw_activation_daa_score != u64::MAX).coinbase_outputs_limit(),
                3 * (k as u64 + 1) + 1,
                "the PALW cap depends only on lane presence (accept={accept})"
            );
        }
        // A non-PALW preset likewise stays narrow at both acceptance polarities.
        for accept in [false, true] {
            let mut params = MN;
            params.palw_algo4_accept = accept;
            let k = params.ghostdag_k();
            assert_eq!(
                cap_validator(k, params.palw_activation_daa_score != u64::MAX).coinbase_outputs_limit(),
                k as u64 + 2,
                "the non-PALW cap depends only on lane presence (accept={accept})"
            );
        }
    }

    /// With algo-4 accepted the blue term triples, because a PALW blue source pushes provider A,
    /// provider B and the fee-worker output instead of one miner output.
    #[test]
    fn coinbase_output_cap_widens_to_three_per_blue_on_a_palw_lane() {
        for k in [1u8, 18, 124] {
            let k = k as KType;
            assert_eq!(cap_validator(k, false).coinbase_outputs_limit(), k as u64 + 2);
            assert_eq!(cap_validator(k, true).coinbase_outputs_limit(), 3 * (k as u64 + 1) + 1);
        }
    }

    /// A coinbase carrying the worst-case PALW blue shape (3 outputs per blue source + 1 merged red)
    /// is REJECTED under the shipped cap and ACCEPTED once algo-4 is accepted. This is the concrete
    /// block ECON-02 was about: without the widening, a correctly-constructed PALW block fails on its
    /// own coinbase.
    #[test]
    fn worst_case_palw_coinbase_needs_the_widened_cap() {
        let k: KType = 18;
        let n = 3 * (k as usize + 1) + 1;
        let cb = coinbase_with_outputs(n);
        assert_eq!(
            cap_validator(k, false).check_coinbase_in_isolation(&cb),
            Err(TxRuleError::CoinbaseTooManyOutputs(n, k as u64 + 2)),
            "the shipped cap rejects the PALW blue shape — this is exactly ECON-02"
        );
        assert!(cap_validator(k, true).check_coinbase_in_isolation(&cb).is_ok());
    }

    /// Records the current cap boundary: validator rewards and the inclusion bounty are appended after
    /// the blue/red loops and are bounded by block mass rather than `coinbase_outputs_limit`.
    #[test]
    fn coinbase_output_cap_ignores_validator_and_bounty_tail() {
        let k: KType = 18;
        // One output per blue source + 1 red is already at the cap ...
        let at_cap = k as usize + 2;
        assert_eq!(cap_validator(k, false).coinbase_outputs_limit(), at_cap as u64);
        // ... so a single §E validator payout on top of it overflows, pre-PALW.
        let cb = coinbase_with_outputs(at_cap + 1);
        assert_eq!(
            cap_validator(k, false).check_coinbase_in_isolation(&cb),
            Err(TxRuleError::CoinbaseTooManyOutputs(at_cap + 1, at_cap as u64)),
            "pre-existing, separately-tracked: the §E/§D tail is not covered by the cap"
        );
    }
}
