//! A saved signature names one immutable receipt. Current native-view changes
//! cannot license changing its assignment, context or seed and cannot make a
//! retryable mismatch permanently stop the private checker.
use super::{Digest, PaidClaimReceiptV2, outbox::CheckerOutboxErrorV4};
use kaspa_consensus_core::palw_claim_challenge_v2::PalwPaidClaimLockV2;

pub(super) fn validate(
    receipt: &PaidClaimReceiptV2,
    enrollment: Digest,
    round: u32,
    signer: Digest,
    current: &PalwPaidClaimLockV2,
) -> Result<(), CheckerOutboxErrorV4> {
    // Configuration/WAL identity is independent of the current snapshot. A
    // different actor/enrollment/round remains a hard refusal, even when its
    // source fields also differ.
    if receipt.enrollment != enrollment || receipt.round != round || receipt.signer != signer {
        return Err(CheckerOutboxErrorV4::Outbox("checker receipt WAL enrollment/round/actor differs".into()));
    }
    if receipt.assignment != current.assignment || receipt.source_context != current.context || receipt.sample_seed != current.seed {
        return Err(CheckerOutboxErrorV4::NativeView(
            "checker receipt WAL belongs to another source/assignment; retained without rebinding".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        Hash64,
        palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
        tx::{TransactionId, TransactionOutpoint},
    };
    use misaka_palw_kernel::{claim_paid_v2::PaidClaimObjectV2, route::KernelRouteObjectV1};

    fn fixture() -> (PaidClaimReceiptV2, PalwPaidClaimLockV2, PalwBondKeyV2) {
        let actor = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([9; 64]), 1));
        let receipt = PaidClaimReceiptV2 {
            version: 2,
            enrollment: [1; 64],
            verification_class: [2; 64],
            assignment: [3; 64],
            source_context: [4; 64],
            sample_seed: [5; 64],
            evidence_root: [6; 64],
            transcript_root: [7; 64],
            signer: kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&actor),
            signed_daa: 100,
            complete: true,
            round: 0,
            message_root: [0; 64],
        };
        let lock = PalwPaidClaimLockV2 {
            enrollment: receipt.enrollment,
            context: receipt.source_context,
            anchor: [8; 64],
            seed: receipt.sample_seed,
            locked_daa: 90,
            assignment: receipt.assignment,
            seats: vec![actor],
            quorum: 1,
            source_valid: true,
        };
        (receipt, lock, actor)
    }

    #[test]
    fn changed_native_binding_holds_the_same_signature_until_its_original_snapshot_returns() {
        let (receipt, lock, actor) = fixture();
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([0x35; 32]);
        let domain = Hash64::from_bytes([0x42; 64]);
        for inner in [
            PaidClaimObjectV2::Receipt { receipt: receipt.clone() },
            PaidClaimObjectV2::RoundReceipt { receipt: receipt.clone() },
            PaidClaimObjectV2::ReceiptApprovedBootstrapV6 { receipt: receipt.clone() },
        ] {
            // Synthetic receipt statement, real codec/signature. This creates
            // no native assignment/source or accepted approval.
            let object = misaka_palw_sdk::claim_paid_v2::sign_paid_claim_object_v2(
                &key,
                domain,
                actor,
                &KernelRouteObjectV1::PaidClaimV2 { object: inner },
            )
            .unwrap();
            let original = borsh::to_vec(&object).unwrap();
            let saved = super::super::receipt_from_saved(&object, actor).unwrap();
            assert!(validate(&saved, receipt.enrollment, 0, receipt.signer, &lock).is_ok());
            for field in 0..3 {
                let mut changed = lock.clone();
                match field {
                    0 => changed.assignment = [19; 64],
                    1 => changed.context = [20; 64],
                    _ => changed.seed = [21; 64],
                }
                assert!(matches!(
                    validate(&saved, receipt.enrollment, 0, receipt.signer, &changed),
                    Err(CheckerOutboxErrorV4::NativeView(_))
                ));
                assert_eq!(borsh::to_vec(&object).unwrap(), original, "source mismatch must not replace immutable WAL");
            }
            assert!(validate(&saved, receipt.enrollment, 0, receipt.signer, &lock).is_ok(), "rollback retries original statement");
            let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature } = &object else { unreachable!() };
            let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(domain, signer, bytes);
            assert!(
                kaspa_txscript::verify_mldsa87_with_context(
                    key.public_key(),
                    message.as_byte_slice(),
                    signature,
                    kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn fixed_wal_identity_mismatch_is_hard_even_when_native_binding_also_changed() {
        let (receipt, mut lock, _) = fixture();
        lock.seed = [22; 64];
        for (enrollment, round, signer) in
            [([23; 64], 0, receipt.signer), (receipt.enrollment, 1, receipt.signer), (receipt.enrollment, 0, [24; 64])]
        {
            assert!(matches!(validate(&receipt, enrollment, round, signer, &lock), Err(CheckerOutboxErrorV4::Outbox(_))));
        }
    }
}
