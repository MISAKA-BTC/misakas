//! Factory for the unpublished private registry/bootstrap resource probe.
//!
//! Every added registration has a distinct valid salt-derived ML-DSA key and a
//! funded output. Funds are carved out of the existing salted main output. No
//! capability, class admission, beacon, useful work or release approval is added.
use super::{
    drill::{
        PalwDrillKeyRoleV1, PalwDrillKeyV1, PalwDrillSaltV1, palw_chain_params_v1, palw_drill_network_v1,
        palw_t12_drill_genesis_utxos_v1, palw_t12_drill_premine_outpoint_v1,
    },
    params::{PALW_T12_GENESIS_BONDS, Params},
    premine::{MAIN_PREMINE_INDEX, MISAKA_PREMINE_CAP_SOMPI},
};
use crate::{
    header::Header,
    mldsa87_primitives::p2pkh_mldsa87_spk,
    muhash::MuHashExtensions,
    palw_mode_v2::PalwConsensusMode,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use kaspa_muhash::MuHash;

pub const PRIVATE_POPULATION_MAX_BONDS_V1: u32 = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivatePopulationV1 {
    total_bonds: u32,
}

impl PrivatePopulationV1 {
    pub fn new(total_bonds: u32) -> Result<Self, String> {
        if total_bonds <= PALW_T12_GENESIS_BONDS.len() as u32 || total_bonds > PRIVATE_POPULATION_MAX_BONDS_V1 {
            return Err(format!(
                "private total bonds must be greater than {} and at most {PRIVATE_POPULATION_MAX_BONDS_V1}",
                PALW_T12_GENESIS_BONDS.len()
            ));
        }
        Ok(Self { total_bonds })
    }

    pub fn total_bonds(self) -> u32 {
        self.total_bonds
    }
}

pub struct PrivatePopulationGenesisV1 {
    pub params: Params,
    pub utxos: UtxoCollection,
}

/// The same immutable salt/size factory supplies Config and the actual initial
/// UTXO loader. M-07 then compares the independently loaded commitment/hash.
/// There is no arbitrary initial-set override or public startup flag.
pub fn derive_private_population_v1(
    salt: &PalwDrillSaltV1,
    profile: PrivatePopulationV1,
) -> Result<PrivatePopulationGenesisV1, String> {
    let started = std::time::Instant::now();
    let mut params = palw_chain_params_v1(palw_drill_network_v1(), Some(salt))?;
    let mut utxos = palw_t12_drill_genesis_utxos_v1(salt);
    let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
        return Err("private population requires the salted native V2 registry".into());
    };
    let base_count = bundle.genesis_objects.iter().filter(|row| matches!(row, PalwConsensusObjectV2::BondRegistered { .. })).count();
    if base_count != PALW_T12_GENESIS_BONDS.len() {
        return Err("private population base registry differs from the shipped salted shape".into());
    }
    let extra = profile.total_bonds.checked_sub(base_count as u32).ok_or("private population count below its base")?;
    let collateral = bundle.state.min_collateral_sompi();
    let carved = collateral.checked_mul(u64::from(extra)).ok_or("private population collateral overflow")?;
    let main = palw_t12_drill_premine_outpoint_v1(salt, MAIN_PREMINE_INDEX);
    let main_entry = utxos.get_mut(&main).ok_or("salted main output is absent")?;
    main_entry.amount = main_entry
        .amount
        .checked_sub(carved)
        .filter(|amount| *amount > 0)
        .ok_or("private population collateral exceeds the existing main output")?;

    let mut txid_subject = Vec::with_capacity(36);
    txid_subject.extend_from_slice(salt.as_bytes());
    txid_subject.extend_from_slice(&profile.total_bonds.to_le_bytes());
    let txid = blake2b_512_keyed(b"rfc12-private-population-outputs/v1", &txid_subject);
    bundle.genesis_objects.reserve(extra as usize);
    for index in 0..extra {
        // Outside the ordinary drill's finite signer keyring; private keys are
        // dropped here. Distinct operator IDs are resource rows, not evidence
        // that the operators are independent economic parties.
        let key_index = super::drill::PALW_DRILL_KEYRING_SPAN_V1.checked_add(index).ok_or("private key index overflow")?;
        let key = PalwDrillKeyV1::derive(salt, PalwDrillKeyRoleV1::Bond, key_index);
        let bond = PalwBondKeyV2(TransactionOutpoint::new(txid, index));
        let entry = UtxoEntry::new(collateral, p2pkh_mldsa87_spk(&key.payload), 0, false);
        if utxos.insert(bond.0, entry).is_some() {
            return Err("private factory generated a duplicate monetary outpoint".into());
        }
        bundle.genesis_objects.push(PalwConsensusObjectV2::BondRegistered {
            bond,
            operator_pubkey: key.pubkey.clone(),
            pubkey: key.pubkey,
            collateral,
            payout_payload: Hash64::from_bytes(key.payload),
            capable_classes: Default::default(),
            signature: Vec::new(), // exact genesis registrations use the native genesis loader
        });
    }
    let supply = utxos.values().try_fold(0u64, |sum, row| sum.checked_add(row.amount)).ok_or("private factory supply overflow")?;
    if supply != MISAKA_PREMINE_CAP_SOMPI {
        return Err("private factory changed the canonical premine supply".into());
    }
    let mut multiset = MuHash::new();
    for (outpoint, entry) in &utxos {
        multiset.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = multiset.finalize();
    params.genesis.hash = Header::from(&params.genesis).hash;
    eprintln!(
        "[rfc12-private-population] total_bonds={} funded_rows={} supply_sompi={} factory_ms={}",
        profile.total_bonds,
        utxos.len(),
        supply,
        started.elapsed().as_millis()
    );
    Ok(PrivatePopulationGenesisV1 { params, utxos })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn private_population_is_exact_salt_size_and_funded_supply_conserved() {
        let salt = PalwDrillSaltV1::from_bytes([0x81; 32]).unwrap();
        let profile = PrivatePopulationV1::new(17).unwrap();
        let first = derive_private_population_v1(&salt, profile).unwrap();
        let again = derive_private_population_v1(&salt, profile).unwrap();
        assert_eq!(first.params.genesis.hash, again.params.genesis.hash);
        assert_eq!(first.utxos, again.utxos);
        let PalwConsensusMode::ConsensusV2(bundle) = &first.params.palw_consensus_mode else { unreachable!() };
        let bonds: Vec<_> = bundle
            .genesis_objects
            .iter()
            .filter_map(|object| match object {
                PalwConsensusObjectV2::BondRegistered { bond, pubkey, collateral, .. } => Some((bond, pubkey, collateral)),
                _ => None,
            })
            .collect();
        assert_eq!(bonds.len(), 17);
        assert_eq!(bonds.iter().map(|(_, key, _)| key.to_vec()).collect::<HashSet<_>>().len(), 17);
        assert!(bonds.iter().all(|(bond, key, collateral)| key.len() == 2592 && first.utxos[&bond.0].amount == **collateral));
        assert_eq!(first.utxos.values().map(|entry| entry.amount).sum::<u64>(), MISAKA_PREMINE_CAP_SOMPI);
        assert_ne!(
            derive_private_population_v1(&salt, PrivatePopulationV1::new(18).unwrap()).unwrap().params.genesis.hash,
            first.params.genesis.hash
        );
        let other = PalwDrillSaltV1::from_bytes([0x82; 32]).unwrap();
        assert_ne!(derive_private_population_v1(&other, profile).unwrap().params.genesis.hash, first.params.genesis.hash);
        assert!(PrivatePopulationV1::new(PALW_T12_GENESIS_BONDS.len() as u32).is_err());
        assert!(PrivatePopulationV1::new(PRIVATE_POPULATION_MAX_BONDS_V1 + 1).is_err());
    }
}
