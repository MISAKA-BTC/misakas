//! Small direct OPV mechanism controls, never K3 or campaign evidence.
use super::*;
#[path = "../../../misaka/palw/kernel/tests/common/mod.rs"]
mod fixture;
fn point(n: u64) -> TransactionOutpoint {
    TransactionOutpoint::new(Hash64::from_u64_word(n), 0)
}

#[test]
fn actual_direct_replay_filing_convicts_but_dismissal_cannot_authorize_publication() {
    use fixture::ledger_world::{World, policy};
    let policy = policy();
    let opv = fixture::opv_world::opv_example();
    let mut world = World::with_opv(policy, opv);
    let job = world.post_job(2, &[3], 2, 1);
    let (_, produced) = world.lying(&job, 2);
    let claim = produced.claim.id();
    world.block(4, vec![produced.tx]);
    assert!(world.l.claims.contains_key(&claim));
    let goal = misaka_palw_remote::pursuit::kernel_check_program_v1(&world.l, &claim, &world.params, [0; 64]).unwrap().unwrap();
    let misaka_palw_remote::pursuit::KernelGoalV1::Prove(proof) = goal else {
        panic!("public direct fault must localize without a demand")
    };
    let checker = PalwBondKeyV2(point(19));
    let action = KernelRouteObjectV1::FileProof { accuser: palw_kernel_bond_id_v1(&checker), claim, proof };
    let mut route = kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::new(policy, Some(opv), Default::default());
    route.header.scalars = world.l.scalars();
    route.rows = world.l.to_rows();
    let ledger = route.ledger_checked_v1().unwrap();
    let root = ledger.root();
    assert!(convicts(&ledger, &action, 1000, 4).unwrap());
    assert!(!convicts(&ledger, &action, 0, 4).unwrap(), "insufficient collateral signs nothing");
    let invalid = KernelRouteObjectV1::FileProof {
        accuser: palw_kernel_bond_id_v1(&checker),
        claim,
        proof: ProsecutionV1::Kernel(vec![1, 2, 3]),
    };
    assert!(!convicts(&ledger, &invalid, 1000, 4).unwrap(), "accepted dismissal is not conviction authority");
    // A later observation can already contain the conviction even in the
    // same import generation. Recheck the new ledger, not the earlier boolean.
    let mut latest = ledger.try_clone_v1().unwrap();
    latest.begin_block_v1(5).unwrap();
    latest.try_sync_bond_v1(palw_kernel_bond_id_v1(&checker), 1000).unwrap();
    let events = latest.apply_object_reserved_v1(&action, &AuthV1 { signer_bond: palw_kernel_bond_id_v1(&checker) }).unwrap().unwrap();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LedgerEventV1::Convicted { claim: found, .. } if found.as_slice() == claim.as_slice()))
    );
    assert!(!convicts(&latest, &action, 1000, 5).unwrap(), "latest duplicate cannot reuse an earlier conviction preview");
    assert_eq!(ledger.root(), root, "preview cannot mutate native observation");
}

#[test]
fn signed_refutation_wal_binds_exact_proof_actor_domain_and_survives_cold_carrier() {
    let checker = PalwBondKeyV2(point(20));
    let funding = point(21);
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed([31; 32]);
    let domain = Hash64::from_u64_word(22);
    let action = KernelRouteObjectV1::FileProof {
        accuser: palw_kernel_bond_id_v1(&checker),
        claim: [4; 64],
        proof: ProsecutionV1::Kernel(vec![5]),
    };
    let bytes = action.encode();
    let object = PalwConsensusObjectV2::KernelRouteV1 {
        signer: checker,
        signature: key
            .sign_with_context(
                palw_kernel_route_message_v1(domain, &checker, &bytes).as_byte_slice(),
                PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            )
            .to_vec(),
        bytes: bytes.clone(),
    };
    validate_object(&object, &bytes, checker, key.public_key(), domain).unwrap();
    let cold: PalwConsensusObjectV2 = borsh::from_slice(&borsh::to_vec(&object).unwrap()).unwrap();
    validate_object(&cold, &bytes, checker, key.public_key(), domain).unwrap();
    assert!(validate_object(&cold, &bytes, PalwBondKeyV2(point(23)), key.public_key(), domain).is_err());
    assert!(validate_object(&cold, &bytes, checker, key.public_key(), Hash64::from_u64_word(24)).is_err());
    let mut altered = bytes.clone();
    altered.push(0);
    assert!(validate_object(&cold, &altered, checker, key.public_key(), domain).is_err());
    let mut badsig = cold.clone();
    if let PalwConsensusObjectV2::KernelRouteV1 { signature, .. } = &mut badsig {
        signature[0] ^= 1;
    }
    assert!(validate_object(&badsig, &bytes, checker, key.public_key(), domain).is_err());

    let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([32; 32]);
    let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
        &kaspa_hashes::blake2b_512_address_payload(payer.verification_key.as_ref()).as_bytes(),
    );
    let entry = kaspa_consensus_core::tx::UtxoEntry::new(100_000_000, script.clone(), 0, false);
    let config = kaspa_consensus_core::config::Config::new(kaspa_consensus_core::config::params::Params::from(
        kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 12),
    ));
    let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(&config, &payer, &object, funding, &entry, &[], None, 4)
        .unwrap();
    validate_carrier(&tx, funding, &entry, &script, 100_000_000).unwrap();
    assert!(validate_carrier(&tx, point(25), &entry, &script, 100_000_000).is_err());
    assert!(validate_carrier(&tx, funding, &entry, &script, 1).is_err());
    let raw = borsh::to_vec(&tx).unwrap();
    let (decoded, _owner) = super::super::super::recovery_ack_carrier_v4::decode_carrier(&raw, &object).unwrap();
    assert_eq!(decoded.id(), tx.id());
    validate_carrier(&decoded, funding, &entry, &script, 100_000_000).unwrap();
    assert!(super::super::super::recovery_ack_carrier_v4::decode_carrier(&raw, &badsig).is_err());
}

#[test]
fn refutation_authority_never_reuses_approval_or_bond_input_or_implicit_cap() {
    let checker = PalwBondKeyV2(point(30));
    for (input, cap) in [(point(30), 100), (point(31), 100), (point(32), 0)] {
        let authority =
            ObjectiveRefutationFeeV1 { funding_outpoint: format!("{}:{}", input.transaction_id, input.index), maximum_fee_sompi: cap };
        assert!(authority.funding(checker, point(31)).is_err());
    }
    let authority = ObjectiveRefutationFeeV1 { funding_outpoint: format!("{}:0", point(32).transaction_id), maximum_fee_sompi: 100 };
    assert_eq!(authority.funding(checker, point(31)).unwrap(), point(32));
    assert!(serde_json::from_str::<ObjectiveRefutationFeeV1>(r#"{"funding_outpoint":"x"}"#).is_err());
}
