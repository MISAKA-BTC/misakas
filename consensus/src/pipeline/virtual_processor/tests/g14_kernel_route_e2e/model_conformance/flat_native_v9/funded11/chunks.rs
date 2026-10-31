use super::*;
use kaspa_consensus_core::palw_kernel_route_v1::{PalwKernelChunkTargetV1, PalwKernelChunkV1};

#[tokio::test]
async fn execution11_native_unknown_seal_chunk64_drops_before_allocation_with_or_without_signature_and_own_fence() {
    for armed in [false, true] {
        let mut net = Net::over_cfg(if armed { native_config() } else { config() }, TestConsensus::new);
        net.beat_to(1).await;
        let chunk = PalwKernelChunkV1 {
            opener: net.bond(7),
            group: Hash64::from_u64_word(0x22_113),
            target: PalwKernelChunkTargetV1::FundedProofV11 { seal: [7; 64], actor: net.kid(7) },
            index: 0,
            count: 64,
            bytes: vec![0; 32 << 10],
        };
        let signed = signed_flat_chunk(&mut net, 7, chunk);
        let mut unsigned = signed.clone();
        let Obj::KernelRouteChunkV1 { signature, .. } = &mut unsigned else { unreachable!() };
        signature.clear();
        let before = net.api().map(|r| r.aux).unwrap_or_default();
        let collateral = net.collateral(7);
        // An unsigned chunk never reaches a block: the ride rule refuses it at the mempool
        // (anyone could otherwise open a group as any bond). Only the signed one is carried.
        let saved = net.funding[7].clone();
        let tx = net.carrier(7, &unsigned);
        assert!(net.mempool(&tx).is_err(), "an unsigned native chunk is refused before any fold");
        net.funding[7] = saved;
        net.send(vec![(7, signed)]).await;
        let after = net.api();
        let after_aux = after.as_ref().map(|r| r.aux.clone()).unwrap_or_default();
        for table in [EXECUTION_QUOTA_TABLE_V11, EXECUTION_SEAL_TABLE_V11, EXECUTION_DEMAND_TABLE_V11] {
            assert_eq!(
                before.iter().filter(|((t, _), _)| *t == table).collect::<Vec<_>>(),
                after_aux.iter().filter(|((t, _), _)| *t == table).collect::<Vec<_>>(),
                "unknown original seal cannot allocate/debit table{table}"
            );
        }
        assert_eq!(net.collateral(7), collateral);
        assert_eq!(
            after.as_ref().map(|r| r.chunk_groups_of_v1(&net.bond(7))).unwrap_or(0),
            0,
            "native lane cannot fill the ordinary groups"
        );
        let replay = net.replay().await;
        net.assert_same(&replay, "unknown native seal/ownfence rejected before quota, part allocation and signatures");
    }
}
