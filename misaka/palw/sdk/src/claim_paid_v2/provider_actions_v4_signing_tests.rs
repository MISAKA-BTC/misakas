//! Signing/codec controls only: these payloads grant no native assignment,
//! approved source, receipt acceptance or activation.
use super::*;
use kaspa_consensus_core::{
    palw_kernel_route_v1::{PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1},
    tx::{TransactionId, TransactionOutpoint},
};
use kaspa_pq_validator_core::{MessageSigner, ValidatorKey};
use misaka_palw_kernel::claim_paid_v2::{PaidClaimObjectV2, PaidClaimReceiptV2};

fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_bytes([n; 64]), index: u32::from(n) })
}
fn receipt(signer: PalwBondKeyV2) -> PaidClaimReceiptV2 {
    PaidClaimReceiptV2 {
        version: 2,
        enrollment: [1; 64],
        verification_class: [2; 64],
        assignment: [3; 64],
        source_context: [4; 64],
        sample_seed: [5; 64],
        evidence_root: [6; 64],
        transcript_root: [7; 64],
        signer: palw_kernel_bond_id_v1(&signer),
        signed_daa: 101,
        complete: true,
        round: 0,
        message_root: [8; 64],
    }
}
fn action(object: PaidClaimObjectV2) -> ProviderObjectiveActionV4 {
    ProviderObjectiveActionV4 { object: KernelRouteObjectV1::PaidClaimV2 { object }, _construction: None }
}

#[test]
fn all_receipt_namespaces_sign_the_exact_existing_native_frame() {
    let actor = bond(9);
    let key = ValidatorKey::from_seed([0x31; 32]);
    let network = Hash64::from_bytes([0x42; 64]);
    let receipt = receipt(actor);
    let mut round_receipt = receipt.clone();
    round_receipt.round = 1;
    let objects = [
        PaidClaimObjectV2::Receipt { receipt: receipt.clone() },
        PaidClaimObjectV2::RoundReceipt { receipt: round_receipt },
        PaidClaimObjectV2::ReceiptApprovedBootstrapV6 { receipt },
    ];
    let mut frames: Vec<Vec<u8>> = Vec::new();
    for inner in objects {
        let action = action(inner);
        let signed = sign_provider_objective_action_v4(&key, network, actor, &action).unwrap();
        let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature } = &*signed else { panic!("native carrier expected") };
        assert_eq!(*signer, actor);
        assert_eq!(bytes, &action.object.encode(), "signed namespace must not be rewritten");
        assert_eq!(KernelRouteObjectV1::decode(bytes).unwrap(), action.object);
        let verifies = |network, bytes: &[u8]| {
            kaspa_txscript::verify_mldsa87_with_context(
                key.public_key(),
                palw_kernel_route_message_v1(network, signer, bytes).as_byte_slice(),
                signature,
                PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            )
            .unwrap_or(false)
        };
        assert!(verifies(network, bytes));
        assert!(!verifies(Hash64::from_bytes([0x43; 64]), bytes));
        for prior in &frames {
            assert!(!verifies(network, prior), "signature cannot migrate receipt namespaces");
        }
        frames.push(bytes.clone());
    }
    assert_ne!(frames[0], frames[2]);
}

struct NoSigning;
impl MessageSigner for NoSigning {
    fn public_key(&self) -> &[u8] {
        &[]
    }
    fn sign_message(&self, _: &[u8], _: &[u8]) -> Result<Vec<u8>, String> {
        panic!("role mismatch must refuse before signing")
    }
}

#[test]
fn every_receipt_namespace_refuses_a_different_actor_before_signing() {
    let named = receipt(bond(9));
    for inner in [
        PaidClaimObjectV2::Receipt { receipt: named.clone() },
        PaidClaimObjectV2::RoundReceipt { receipt: named.clone() },
        PaidClaimObjectV2::ReceiptApprovedBootstrapV6 { receipt: named },
    ] {
        let refused = sign_provider_objective_action_v4(&NoSigning, Hash64::from_bytes([0x42; 64]), bond(10), &action(inner));
        assert!(refused.is_err());
    }
}
