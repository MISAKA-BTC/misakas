//! Actual consensus mass inputs for the separately versioned native carriage
//! reservation. Bytes alone do not establish transaction/block capacity.
use crate::{
    config::params::Params,
    constants::TRANSIENT_BYTE_TO_MASS_FACTOR,
    mass::{MassCalculator, transaction_estimated_serialized_size},
    subnets::SUBNETWORK_ID_SIZE,
    tx::{Transaction, VerifiableTransaction},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};

/// Consumer-resolved consensus constants, never supplied by an action frame.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCarriageMassParamsV22 {
    pub version: u16,
    pub max_block_mass: u64,
    pub mass_per_tx_byte: u64,
    pub mass_per_script_pub_key_byte: u64,
    pub mass_per_sig_op: u64,
    pub storage_mass_parameter: u64,
    pub transient_byte_to_mass_factor: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::params::TESTNET_PARAMS,
        mldsa87_primitives::p2pkh_mldsa87_spk,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{PopulatedTransaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    };
    use kaspa_hashes::Hash64;
    fn envelope(payload: u64) -> NativeCarrierEnvelopeV22 {
        NativeCarrierEnvelopeV22 {
            max_payload_bytes: payload,
            max_inputs: 1,
            max_outputs: 1,
            max_signature_script_bytes: 8192,
            max_output_script_bytes: 69,
            max_sig_ops: 1,
            max_storage_mass: 100_000,
        }
    }
    fn carrier(payload: usize, output: u64) -> (Transaction, Vec<UtxoEntry>) {
        let spk = p2pkh_mldsa87_spk(&[1; 64]);
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_bytes([3; 64]), 0), vec![0; 8192], 0, 1)],
            vec![TransactionOutput::new(output, spk.clone())],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![0; payload],
        );
        (tx, vec![UtxoEntry::new(100_000_000, spk, 1, false)])
    }
    #[test]
    fn complete_source_packet_uses_actual_mass_not_the_inner_byte_cap() {
        let p = NativeCarriageMassParamsV22::from_consensus_params(&TESTNET_PARAMS);
        assert!(p.packet_mass(&envelope(128 << 10)).is_err(), "128KiB plus carrier overhead exceeds actual500000 mass");
        let e = envelope(70 << 10);
        let (tx, entries) = carrier(e.max_payload_bytes as usize, 99_000_000);
        let calc = MassCalculator::new_with_consensus_params(&TESTNET_PARAMS);
        let expected = calc.calc_non_contextual_masses(&tx);
        let populated = PopulatedTransaction::new(&tx, entries);
        let actual = calc.calc_contextual_masses(&populated).unwrap().max(expected);
        assert_eq!(p.check_transaction(&e, &populated).unwrap(), actual);
        assert_eq!(p.packet_mass(&e).unwrap(), actual, "all actual field sizes fill the committed envelope");
        let mut changed = p.clone();
        changed.mass_per_tx_byte += 1;
        assert_ne!(changed.root(), p.root());
        let mut huge = e.clone();
        huge.max_signature_script_bytes = u64::MAX;
        assert!(p.packet_mass(&huge).is_err());
        let mut forged = p.clone();
        forged.transient_byte_to_mass_factor = 1;
        assert!(forged.packet_mass(&e).is_err());
    }
    #[test]
    fn original_source_packet_refuses_contextual_storage_and_signature_growth() {
        let p = NativeCarriageMassParamsV22::from_consensus_params(&TESTNET_PARAMS);
        let e = envelope(70 << 10);
        let (tx, entries) = carrier(100, 1);
        assert!(
            p.check_transaction(&e, &PopulatedTransaction::new(&tx, entries)).is_err(),
            "small bytes cannot hide dust storage mass"
        );
        let (mut tx, entries) = carrier(100, 99_000_000);
        tx.inputs[0].signature_script.push(0);
        assert!(p.check_transaction(&e, &PopulatedTransaction::new(&tx, entries.clone())).is_err());
        tx.inputs[0].signature_script.pop();
        tx.inputs[0].sig_op_count = 2;
        assert!(p.check_transaction(&e, &PopulatedTransaction::new(&tx, entries)).is_err());
    }
}

/// Explicit original carrier contract. Input/output/script and contextual
/// storage limits are part of the promise; a bare payload-byte limit is not.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCarrierEnvelopeV22 {
    pub max_payload_bytes: u64,
    pub max_inputs: u32,
    pub max_outputs: u32,
    pub max_signature_script_bytes: u64,
    pub max_output_script_bytes: u64,
    pub max_sig_ops: u64,
    pub max_storage_mass: u64,
}

impl NativeCarriageMassParamsV22 {
    pub fn from_consensus_params(p: &Params) -> Self {
        Self {
            version: 22,
            max_block_mass: p.max_block_mass,
            mass_per_tx_byte: p.mass_per_tx_byte,
            mass_per_script_pub_key_byte: p.mass_per_script_pub_key_byte,
            mass_per_sig_op: p.mass_per_sig_op,
            storage_mass_parameter: p.storage_mass_parameter,
            transient_byte_to_mass_factor: TRANSIENT_BYTE_TO_MASS_FACTOR,
        }
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/actual-consensus-carriage-mass/v22", self)
    }
    fn calculator(&self) -> Result<MassCalculator, String> {
        if self.version != 22 || self.max_block_mass == 0 || self.transient_byte_to_mass_factor != TRANSIENT_BYTE_TO_MASS_FACTOR {
            return Err("native carriage lacks original consensus mass constants".into());
        }
        Ok(MassCalculator::new(
            self.mass_per_tx_byte,
            self.mass_per_script_pub_key_byte,
            self.mass_per_sig_op,
            self.storage_mass_parameter,
        ))
    }
    /// Conservative common occupancy bound for all three physical mass axes.
    /// Its use for scheduling does not replace per-axis consensus validation.
    pub fn packet_mass(&self, e: &NativeCarrierEnvelopeV22) -> Result<u64, String> {
        self.calculator()?;
        if e.max_inputs == 0 || e.max_outputs == 0 || e.max_storage_mass > self.max_block_mass {
            return Err("native carriage has no bounded original funding envelope".into());
        }
        let overflow = || "native carrier mass envelope overflow".to_string();
        // This is the exact deterministic size formula used by consensus mass:
        // header, input outpoint/script/sequence and output value/script.
        let fixed = 2 + 8 + 8 + 8 + SUBNETWORK_ID_SIZE as u64 + 8 + kaspa_hashes::HASH_SIZE as u64 + 8;
        let input = (kaspa_hashes::HASH64_SIZE as u64 + 4 + 8 + 8).checked_add(e.max_signature_script_bytes).ok_or_else(overflow)?;
        let output = (8u64 + 2 + 8).checked_add(e.max_output_script_bytes).ok_or_else(overflow)?;
        let size = input
            .checked_mul(e.max_inputs as u64)
            .and_then(|n| output.checked_mul(e.max_outputs as u64).and_then(|m| n.checked_add(m)))
            .and_then(|n| n.checked_add(fixed))
            .and_then(|n| n.checked_add(e.max_payload_bytes))
            .ok_or_else(overflow)?;
        let output_scripts =
            e.max_output_script_bytes.checked_add(2).and_then(|n| n.checked_mul(e.max_outputs as u64)).ok_or_else(overflow)?;
        let compute = size
            .checked_mul(self.mass_per_tx_byte)
            .and_then(|n| output_scripts.checked_mul(self.mass_per_script_pub_key_byte).and_then(|m| n.checked_add(m)))
            .and_then(|n| e.max_sig_ops.checked_mul(self.mass_per_sig_op).and_then(|m| n.checked_add(m)))
            .ok_or_else(overflow)?;
        let transient = size.checked_mul(self.transient_byte_to_mass_factor).ok_or_else(overflow)?;
        let mass = compute.max(transient).max(e.max_storage_mass);
        if mass > self.max_block_mass {
            return Err("native complete carrier cannot fit the actual block mass".into());
        }
        Ok(mass)
    }
    /// A typed paid unit still needs its independent native authenticity check.
    /// This validates only that its complete transaction fulfills the original
    /// mass promise, including contextual UTXO storage rather than byte guesses.
    pub fn check_transaction(&self, e: &NativeCarrierEnvelopeV22, tx: &impl VerifiableTransaction) -> Result<u64, String> {
        let cap = self.packet_mass(e)?;
        let t: &Transaction = tx.tx();
        if t.is_coinbase()
            || t.payload.len() as u64 > e.max_payload_bytes
            || t.inputs.is_empty()
            || t.inputs.len() > e.max_inputs as usize
            || t.outputs.is_empty()
            || t.outputs.len() > e.max_outputs as usize
            || t.inputs.iter().any(|i| i.signature_script.len() as u64 > e.max_signature_script_bytes)
            || t.outputs.iter().any(|o| o.script_public_key.script().len() as u64 > e.max_output_script_bytes)
        {
            return Err("native carrier changes its original bounded transaction envelope".into());
        }
        let sigops =
            t.inputs.iter().try_fold(0u64, |a, i| a.checked_add(i.sig_op_count as u64)).ok_or("native carrier sigops overflow")?;
        if sigops > e.max_sig_ops {
            return Err("native carrier exceeds original signature work".into());
        }
        let calc = self.calculator()?;
        let non = calc.calc_non_contextual_masses(t);
        let storage = calc.calc_contextual_masses(tx).ok_or("native carrier lacks finite original UTXO storage mass")?;
        if storage.storage_mass > e.max_storage_mass {
            return Err("native carrier exceeds original contextual storage mass".into());
        }
        let actual = storage.max(non);
        if actual > cap || transaction_estimated_serialized_size(t) > u64::MAX / self.transient_byte_to_mass_factor {
            return Err("native carrier exceeds its original physical packet promise".into());
        }
        Ok(actual)
    }
}
