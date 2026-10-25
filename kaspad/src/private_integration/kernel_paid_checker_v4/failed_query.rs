//! Rebind exact reader provenance to the same own native provider generation.
use super::{Digest, Hash64, PalwBondKeyV2, PalwKernelMaterialUnitQueryV4};

pub(super) fn select_failed_material_query_v4(
    base: Digest,
    consent: Option<Hash64>,
    provider: PalwBondKeyV2,
    transport: Option<PalwKernelMaterialUnitQueryV4>,
    invalid_unit: Option<(u8, u32)>,
) -> Option<PalwKernelMaterialUnitQueryV4> {
    let consent = consent?;
    let root = Hash64::from_bytes(base);
    let query = match transport {
        Some(query) => query,
        None => {
            let (stage, position) = invalid_unit?;
            PalwKernelMaterialUnitQueryV4 { version: 4, root, consent, dependency: root, provider, stage, position }
        }
    };
    // A different generation/provider/root cannot borrow an old acquisition's
    // failure, even if the coordinate is the same. Native query authority is
    // checked again by the caller before the existing funded Challenge is signed.
    if query.version != 4 || query.root != root || query.dependency != root || query.consent != consent || query.provider != provider {
        return None;
    }
    query.validate().ok()?;
    Some(query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    fn provider(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
    }
    #[test]
    fn decoded_unit_failure_keeps_current_scope_and_existing_transport_retry() {
        let base = [11; 64];
        let generation = Hash64::from_u64_word(12);
        let provider = provider(13);
        let q = select_failed_material_query_v4(base, Some(generation), provider, None, Some((0x40, 0))).unwrap();
        assert_eq!(q.root, Hash64::from_bytes(base));
        assert_eq!(q.dependency, q.root);
        assert_eq!(q.consent, generation);
        assert_eq!(q.provider, provider);
        assert_eq!((q.stage, q.position), (0x40, 0));
        assert_eq!(select_failed_material_query_v4(base, Some(generation), provider, Some(q.clone()), None), Some(q));
    }
    #[test]
    fn failure_from_another_generation_provider_or_claim_cannot_authorize_a_query() {
        let base = [11; 64];
        let generation = Hash64::from_u64_word(12);
        let p = provider(13);
        let original = select_failed_material_query_v4(base, Some(generation), p, None, Some((0, 2))).unwrap();
        for changed in 0..5 {
            let mut q = original.clone();
            match changed {
                0 => q.consent = Hash64::from_u64_word(15),
                1 => q.provider = provider(16),
                2 => q.root = Hash64::from_u64_word(17),
                3 => q.dependency = Hash64::from_u64_word(18),
                _ => q.version = 3,
            }
            assert!(select_failed_material_query_v4(base, Some(generation), p, Some(q), Some((0, 2))).is_none());
        }
        assert!(select_failed_material_query_v4(base, None, p, Some(original), Some((0, 2))).is_none());
        assert!(select_failed_material_query_v4(base, Some(generation), p, None, None).is_none());
    }
}
