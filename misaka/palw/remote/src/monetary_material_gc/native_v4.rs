//! Own-native GC witness for a saved exact deletion tuple. A finalized recovery
//! receipt can replace pruned release history, but cannot rebind a saved intent
//! to another release block merely because its tail and generation still match.
use kaspa_consensus_core::{BlockHash, Hash64, api::ConsensusApi, palw_material_release_recovery_v2::PalwMaterialTerminalReceiptV2};

pub fn native_material_release_finalized_v4(
    native: &dyn ConsensusApi,
    root: Hash64,
    consent: Hash64,
    tail: Hash64,
    release_block: BlockHash,
) -> Result<bool, String> {
    release_with_v4(
        root,
        consent,
        tail,
        release_block,
        || native.palw_material_terminal_receipt_finalized_v2(root, consent),
        || native.palw_monetary_material_release_finalized_v1(root, consent, tail, release_block),
    )
}

/// Consumer-side saved intent binding; this does not create receipt authority.
pub(super) fn receipt_matches_gc_intent_v4(
    receipt: &PalwMaterialTerminalReceiptV2,
    root: Hash64,
    consent: Hash64,
    tail: Hash64,
    release_block: BlockHash,
) -> bool {
    receipt.covers(root, consent) && receipt.tail_block == tail && receipt.release_block == release_block
}

fn release_with_v4(
    root: Hash64,
    consent: Hash64,
    tail: Hash64,
    release_block: BlockHash,
    receipt: impl FnOnce() -> Result<Option<(BlockHash, PalwMaterialTerminalReceiptV2)>, String>,
    legacy: impl FnOnce() -> Result<bool, String>,
) -> Result<bool, String> {
    if let Some((_, receipt)) = receipt()? {
        // The native API establishes finality/current-chain authority and
        // generation history. Recheck the exact persisted tuple at this consumer.
        return Ok(receipt_matches_gc_intent_v4(&receipt, root, consent, tail, release_block));
    }
    legacy()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_material_release_recovery_v2::{
            PalwMaterialRecoveryContractV2 as Contract, PalwMaterialRecoveryGenerationV2 as Generation,
            PalwMaterialRecoveryTermsV2 as Terms,
        },
        palw_state_v2::PalwBondKeyV2,
        tx::TransactionOutpoint,
    };
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn receipt() -> PalwMaterialTerminalReceiptV2 {
        PalwMaterialTerminalReceiptV2 {
            contract: Contract {
                terms: Terms {
                    version: 2,
                    root: h(1),
                    initial_consent: h(2),
                    sponsor: PalwBondKeyV2(TransactionOutpoint::new(h(3), 0)),
                    nonce: h(4),
                    policy: h(5),
                    max_generations: 4,
                    all_generation_ack: true,
                },
                enrolled_daa: 1,
                generations: vec![Generation { consent: h(2), providers: vec![] }, Generation { consent: h(6), providers: vec![] }],
            },
            final_consent: h(6),
            tail_block: h(7),
            release_block: h(8),
            unused_native_session: false,
            acknowledged: Default::default(),
        }
    }
    #[test]
    fn finalized_recovery_binds_both_blocks_and_preserves_historical_generation() {
        for consent in [h(2), h(6)] {
            assert!(
                release_with_v4(h(1), consent, h(7), h(8), || Ok(Some((h(9), receipt()))), || panic!("no legacy fallback")).unwrap()
            );
        }
        for (root, consent, tail, block) in
            [(h(1), h(2), h(7), h(10)), (h(1), h(2), h(10), h(8)), (h(10), h(2), h(7), h(8)), (h(1), h(10), h(7), h(8))]
        {
            assert!(
                !release_with_v4(root, consent, tail, block, || Ok(Some((h(9), receipt()))), || panic!("mismatch cannot fallback"))
                    .unwrap()
            );
        }
    }
    #[test]
    fn read_or_capacity_failure_never_authorizes_gc_and_absence_preserves_legacy() {
        for error in ["native capacity unavailable", "native state unreadable"] {
            let result = release_with_v4(h(1), h(2), h(7), h(8), || Err(error.into()), || panic!("error cannot fallback"));
            assert_eq!(result, Err(error.into()));
        }
        for legacy in [Ok(false), Ok(true), Err("legacy read refused".into())] {
            assert_eq!(release_with_v4(h(1), h(2), h(7), h(8), || Ok(None), || legacy.clone()), legacy);
        }
    }
}
