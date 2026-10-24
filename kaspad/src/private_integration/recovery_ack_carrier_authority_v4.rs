//! Current local authority for both newly funded and cold-restored recovery
//! carriers. A failed observation retains the immutable WAL; it never funds a
//! replacement, retires material, or decides consensus guilt.
use kaspa_consensus_core::{
    Hash64,
    api::ConsensusApi,
    config::params::Params,
    mass::MassCalculator,
    palw_producer_v2::PalwObjectRehearsalV1,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::{ScriptPublicKey, Transaction, UtxoEntry},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CarrierPointV4 {
    sink: Hash64,
    daa: u64,
    import_epoch: u64,
}
fn point(api: &dyn ConsensusApi) -> Result<CarrierPointV4, String> {
    let (before, stable) = api.pruning_import_status().map_err(|e| e.to_string())?;
    if !stable || before.active.is_some() {
        return Err("recovery carrier import is incomplete".into());
    }
    let result = CarrierPointV4 { sink: api.get_sink(), daa: api.get_virtual_daa_score(), import_epoch: before.epoch };
    let (after, stable) = api.pruning_import_status().map_err(|e| e.to_string())?;
    if !stable
        || after.active.is_some()
        || after.epoch != before.epoch
        || api.get_sink() != result.sink
        || api.get_virtual_daa_score() != result.daa
    {
        return Err("recovery carrier native point moved".into());
    }
    Ok(result)
}

/// Borrowed fee/payer check; the live UTXO comes from this node, not the WAL.
fn funding(
    params: &Params,
    tx: &Transaction,
    object: &PalwConsensusObjectV2,
    payer: &ScriptPublicKey,
    entry: &UtxoEntry,
    maximum_fee: u64,
    daa: u64,
    is_bond: bool,
) -> Result<(), String> {
    if tx.inputs.len() != 1
        || tx.outputs.len() != 1
        || tx.outputs[0].value == 0
        || entry.script_public_key != *payer
        || tx.outputs[0].script_public_key != *payer
    {
        return Err("recovery carrier payer/change differs from current fee authority".into());
    }
    if is_bond
        || (entry.is_coinbase
            && !kaspa_consensus_core::dns_finality::coinbase_spend_settled(
                entry.block_daa_score,
                daa,
                params.coinbase_spend_maturity(),
                None,
            ))
    {
        return Err("recovery carrier funding is bonded or immature".into());
    }
    let fee = entry.amount.checked_sub(tx.outputs[0].value).ok_or("recovery carrier change exceeds live funding")?;
    let mass = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    )
    .calc_non_contextual_masses(tx)
    .compute_mass;
    let minimum = kaspa_pq_validator_core::relay_fee_for_compute_mass(mass)
        .checked_add(crate::palw_panel::palw_carrier_fees::palw_lifecycle_carrier_rent_v2(params, object, daa))
        .ok_or("recovery carrier current fee overflow")?;
    if fee < minimum || fee > maximum_fee {
        return Err("recovery carrier fee is outside current rent/relay minimum or configured cap".into());
    }
    Ok(())
}

pub(super) fn verify_current(
    api: &dyn ConsensusApi,
    params: &Params,
    tx: &Transaction,
    object: &PalwConsensusObjectV2,
    payer: &ScriptPublicKey,
    maximum_fee: u64,
) -> Result<CarrierPointV4, String> {
    let before = point(api)?;
    if tx.inputs.len() != 1 {
        return Err("recovery carrier funding input differs".into());
    }
    let outpoint = tx.inputs[0].previous_outpoint;
    if kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&outpoint.transaction_id) {
        return Err("recovery carrier cannot use public genesis funding".into());
    }
    let entry = api.get_virtual_utxo_entry(outpoint).ok_or("saved recovery funding is not currently unspent")?;
    funding(
        params,
        tx,
        object,
        payer,
        &entry,
        maximum_fee,
        before.daa,
        api.palw_bond_payout_payload_v2(PalwBondKeyV2(outpoint)).is_some(),
    )?;
    if !matches!(api.palw_object_rehearsal_v1(object), Some(PalwObjectRehearsalV1::Accepted)) {
        return Err("recovery carrier action is not currently admitted by own native fold".into());
    }
    if point(api)? != before {
        return Err("recovery carrier authority changed during validation".into());
    }
    Ok(before)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION},
        errors::pruning::PruningImportResult,
        network::{NetworkId, NetworkType},
        palw_monetary_material_v1::PalwMonetaryMaterialActionV1,
        palw_native_resource_v1::NativeCoverageV1,
        palw_state_v2::PalwStateV2Error,
        pruning::{PruningImportSessionStateV1, PruningImportSessionV1},
        subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        tx::{TransactionInput, TransactionOutpoint, TransactionOutput},
    };
    use std::sync::Mutex;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    struct Facts {
        entry: Option<UtxoEntry>,
        daa: u64,
        epoch: u64,
        stable: bool,
        active: bool,
        bond: bool,
        verdict: u8,
        move_during_read: bool,
    }
    struct Native(Mutex<Facts>);
    impl ConsensusApi for Native {
        fn get_sink(&self) -> Hash64 {
            h(1)
        }
        fn get_virtual_daa_score(&self) -> u64 {
            self.0.lock().unwrap().daa
        }
        fn pruning_import_status(&self) -> PruningImportResult<(PruningImportSessionStateV1, bool)> {
            let s = self.0.lock().unwrap();
            Ok((
                PruningImportSessionStateV1 {
                    epoch: s.epoch,
                    active: s.active.then_some(PruningImportSessionV1 { point: h(1), epoch: s.epoch }),
                    utxos_started: false,
                    utxos_imported: false,
                },
                s.stable,
            ))
        }
        fn get_virtual_utxo_entry(&self, _: TransactionOutpoint) -> Option<UtxoEntry> {
            self.0.lock().unwrap().entry.clone()
        }
        fn palw_bond_payout_payload_v2(&self, _: PalwBondKeyV2) -> Option<Hash64> {
            self.0.lock().unwrap().bond.then_some(h(9))
        }
        fn palw_object_rehearsal_v1(&self, _: &PalwConsensusObjectV2) -> Option<PalwObjectRehearsalV1> {
            let mut s = self.0.lock().unwrap();
            if s.move_during_read {
                s.epoch += 1;
            }
            match s.verdict {
                0 => Some(PalwObjectRehearsalV1::Accepted),
                1 => None,
                _ => Some(PalwObjectRehearsalV1::Refused(PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Self>(
                    "fixture",
                    "local capacity unavailable",
                )))),
            }
        }
    }
    fn fixture() -> (Params, Transaction, PalwConsensusObjectV2, ScriptPublicKey, Native) {
        // Fee/authority seam only. Dummy signatures and Accepted here are not
        // native admission; the existing decoder suite uses real signed builders.
        let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        let payer = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&[7; 64]);
        let object = PalwConsensusObjectV2::MonetaryMaterialActionV1 {
            action: Box::new(PalwMonetaryMaterialActionV1::Release { root: h(2), consent: h(3), tail_block: h(4) }),
            signer: PalwBondKeyV2(TransactionOutpoint::new(h(5), 0)),
            signature: vec![1],
        };
        let tx = Transaction::new(
            TX_VERSION,
            vec![TransactionInput::new(TransactionOutpoint::new(h(6), 0), vec![1], MAX_TX_IN_SEQUENCE_NUM, 1)],
            vec![TransactionOutput::new(99_000_000, payer.clone())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            borsh::to_vec(&(super::super::PALW_LIFECYCLE_TX_VERSION_V2, &object)).unwrap(),
        );
        let native = Native(Mutex::new(Facts {
            entry: Some(UtxoEntry::new(100_000_000, payer.clone(), 0, false)),
            daa: 10,
            epoch: 1,
            stable: true,
            active: false,
            bond: false,
            verdict: 0,
            move_during_read: false,
        }));
        (params, tx, object, payer, native)
    }
    #[test]
    fn cold_carrier_requires_current_payer_change_fee_maturity_and_unbonded_funding() {
        let (params, tx, object, payer, native) = fixture();
        let entry = native.0.lock().unwrap().entry.clone().unwrap();
        let verify = |tx: &Transaction, entry: &UtxoEntry, cap, bond| funding(&params, tx, &object, &payer, entry, cap, 10, bond);
        verify(&tx, &entry, 2_000_000, false).unwrap();
        assert!(verify(&tx, &entry, 999_999, false).is_err());
        assert!(verify(&tx, &entry, 2_000_000, true).is_err());
        let mut wrong = entry.clone();
        wrong.script_public_key = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&[8; 64]);
        assert!(verify(&tx, &wrong, 2_000_000, false).is_err());
        wrong = entry.clone();
        wrong.is_coinbase = true;
        wrong.block_daa_score = 10;
        assert!(verify(&tx, &wrong, 2_000_000, false).is_err());
        let mut changed = tx.clone();
        changed.outputs[0].script_public_key = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&[8; 64]);
        assert!(verify(&changed, &entry, 2_000_000, false).is_err());
        changed = tx.clone();
        changed.outputs[0].value = entry.amount;
        assert!(verify(&changed, &entry, 2_000_000, false).is_err(), "zero fee is below live relay/rent");
        changed.outputs[0].value += 1;
        assert!(verify(&changed, &entry, 2_000_000, false).is_err());
    }
    #[test]
    fn stale_missing_and_resource_refused_authority_holds_the_immutable_carrier() {
        let (params, tx, object, payer, native) = fixture();
        let bytes = borsh::to_vec(&tx).unwrap();
        let verify = || verify_current(&native, &params, &tx, &object, &payer, 2_000_000);
        let before = verify().unwrap();
        {
            native.0.lock().unwrap().epoch += 1;
        }
        assert_ne!(verify().unwrap(), before, "async caller must discard its preceding point");
        {
            native.0.lock().unwrap().stable = false;
        }
        assert!(verify().is_err());
        {
            let mut s = native.0.lock().unwrap();
            s.stable = true;
            s.active = true;
        }
        assert!(verify().is_err());
        {
            let mut s = native.0.lock().unwrap();
            s.active = false;
            s.verdict = 1;
        }
        assert!(verify().is_err(), "unavailable rehearsal cannot authorize relay");
        {
            native.0.lock().unwrap().verdict = 2;
        }
        assert!(verify().is_err(), "local NativeResource is a hold, never accepted or guilt");
        {
            let mut s = native.0.lock().unwrap();
            s.verdict = 0;
            s.move_during_read = true;
        }
        assert!(verify().is_err(), "import epoch changed while authority was read");
        {
            let mut s = native.0.lock().unwrap();
            s.move_during_read = false;
            s.entry = None;
        }
        assert!(verify().is_err(), "a reorg-spent or unavailable funding input cannot be regenerated");
        assert_eq!(borsh::to_vec(&tx).unwrap(), bytes);
    }
}
