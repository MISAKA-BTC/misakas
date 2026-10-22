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
