use super::*;
#[test]
fn cold_receipt_walks_inner_borsh_after_its_route_version_and_preserves_actor() {
    let bond = PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(17), 1));
    let receipt = PaidClaimReceiptV2 {
        version: 2,
        enrollment: [1; 64],
        verification_class: [2; 64],
        assignment: [3; 64],
        source_context: [4; 64],
        sample_seed: [5; 64],
        evidence_root: [6; 64],
        transcript_root: [7; 64],
        signer: palw_kernel_bond_id_v1(&bond),
        signed_daa: 11,
        complete: true,
        round: 0,
        message_root: [0; 64],
    };
    let bytes = KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::RoundReceipt { receipt: receipt.clone() } }.encode();
    let mut object = PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature: vec![] };
    assert_eq!(receipt_from_saved(&object, bond).unwrap(), receipt);
    let other = PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(17), 2));
    assert!(receipt_from_saved(&object, other).is_err());
    let PalwConsensusObjectV2::KernelRouteV1 { bytes, .. } = &mut object else { unreachable!() };
    bytes[0] = 255;
    assert!(receipt_from_saved(&object, bond).is_err(), "scanner of the inner enum cannot bypass the actual route version");
}

#[test]
fn accepted_demands_are_archived_in_order_and_a_crash_between_renames_reuses_the_index() {
    let bond = PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(17), 1));
    let demand = KernelRouteObjectV1::FileDemand { demander: palw_kernel_bond_id_v1(&bond), claim: [9; 64], stage: 3, position: 4 };
    let object = PalwConsensusObjectV2::KernelRouteV1 { bytes: demand.encode(), signer: bond, signature: vec![] };
    assert!(saved_is_file_demand(&object, bond));
    let receipt = PalwConsensusObjectV2::KernelRouteV1 {
        bytes: KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::RoundReceipt {
                receipt: PaidClaimReceiptV2 {
                    version: 2,
                    enrollment: [1; 64],
                    verification_class: [2; 64],
                    assignment: [3; 64],
                    source_context: [4; 64],
                    sample_seed: [5; 64],
                    evidence_root: [6; 64],
                    transcript_root: [7; 64],
                    signer: palw_kernel_bond_id_v1(&bond),
                    signed_daa: 11,
                    complete: true,
                    round: 0,
                    message_root: [0; 64],
                },
            },
        }
        .encode(),
        signer: bond,
        signature: vec![],
    };
    assert!(!saved_is_file_demand(&receipt, bond));

    let dir = tempfile::tempdir().unwrap();
    let outbox = dir.path();
    let (object_path, tx_path) = (outbox.join("receipt.borsh"), outbox.join("transaction.borsh"));
    assert_eq!(archived_demands(outbox).unwrap(), 0);
    fs::write(&object_path, b"o0").unwrap();
    fs::write(&tx_path, b"t0").unwrap();
    archive_demand_wal(outbox, &object_path, &tx_path, 0).unwrap();
    assert!(!object_path.exists() && !tx_path.exists());
    assert_eq!(archived_demands(outbox).unwrap(), 1);
    // A crash after the transaction moved but before the object did: the count is unchanged, the object re-archives at index 1.
    fs::write(&object_path, b"o1").unwrap();
    fs::write(outbox.join("demand-00001-transaction.borsh"), b"t1").unwrap();
    assert_eq!(archived_demands(outbox).unwrap(), 1);
    archive_demand_wal(outbox, &object_path, &tx_path, 1).unwrap();
    assert_eq!(archived_demands(outbox).unwrap(), 2);
    assert_eq!(fs::read(outbox.join("demand-00001.borsh")).unwrap(), b"o1");
    assert_eq!(fs::read(outbox.join("demand-00001-transaction.borsh")).unwrap(), b"t1");
}
