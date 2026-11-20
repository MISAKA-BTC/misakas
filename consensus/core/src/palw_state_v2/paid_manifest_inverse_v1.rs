//! Bounded rollback for the paired pending/paid ManifestRequest insertions.
//! Fee candidates are owned by the caller; this inverse restores native rows.
use super::*;
use crate::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1},
    palw_positive_contract_v2::{
        PUBLIC_MANIFEST_PAID_TABLE_V1, PUBLIC_PENDING_MANIFEST_TABLE_V1, tfp1_original::manifest_paid::PaidManifestRequestRowV1,
    },
};
use misaka_palw_positive_contract::public_one_honest::PendingManifestCaseRecordV1;

pub(super) struct PaidManifestInverseV1 {
    first: usize,
    pending: [u8; PendingManifestCaseRecordV1::KEY_BYTES],
    paid: [u8; PaidManifestRequestRowV1::KEY_BYTES],
    lookup: (u8, Vec<u8>),
    _memory: Option<NativeDemandLeaseV1>,
}

fn mismatch(why: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PaidManifestInverseV1>("paid manifest inverse", why))
}

impl PaidManifestInverseV1 {
    pub(super) fn prepare_v1(
        b: &TransitionBuilder<'_>,
        pending: [u8; PendingManifestCaseRecordV1::KEY_BYTES],
        paid: [u8; PaidManifestRequestRowV1::KEY_BYTES],
    ) -> Result<Self, PalwStateV2Error> {
        let size = pending.len().max(paid.len());
        let mut quote = super::native_writers::NativeWriteQuoteV1::new("paid manifest inverse lookup");
        quote.exact::<u8>(size as u64)?;
        let memory = quote.reserve_scratch_v1()?;
        let mut key = Vec::new();
        key.try_reserve_exact(size).map_err(|_| mismatch("lookup allocation refused"))?;
        if key.capacity() != size {
            return Err(mismatch("lookup allocation differs from reserved size"));
        }
        let mut lookup = (0, key);
        let route = b.state.kernel_route.as_ref().ok_or_else(|| mismatch("missing native route"))?;
        for (table, key) in [(PUBLIC_PENDING_MANIFEST_TABLE_V1, pending.as_slice()), (PUBLIC_MANIFEST_PAID_TABLE_V1, paid.as_slice())]
        {
            lookup.0 = table;
            lookup.1.clear();
            lookup.1.extend_from_slice(key);
            if route.aux.contains_key(&lookup) {
                return Err(mismatch("inverse cannot own a preexisting row"));
            }
        }
        if let Some(memory) = &memory {
            memory.materialized_v1();
        }
        Ok(Self { first: b.entries.len(), pending, paid, lookup, _memory: memory })
    }

    pub(super) fn restore_v1(mut self, b: &mut TransitionBuilder<'_>) -> Result<(), PalwStateV2Error> {
        let entries = b.entries.get(self.first..).ok_or_else(|| mismatch("journal precedes inverse boundary"))?;
        if entries.len() > 2 {
            return Err(mismatch("inverse encountered more than two insertions"));
        }
        let route = b.state.kernel_route.as_ref().ok_or_else(|| mismatch("native route disappeared during insertion"))?;
        // Validate every effect before removing any row. Unexpected effects
        // are local failures: a caller must not keep a partly rolled-back state.
        for (index, (table, key)) in
            [(PUBLIC_PENDING_MANIFEST_TABLE_V1, self.pending.as_slice()), (PUBLIC_MANIFEST_PAID_TABLE_V1, self.paid.as_slice())]
                .into_iter()
                .enumerate()
        {
            self.lookup.0 = table;
            self.lookup.1.clear();
            self.lookup.1.extend_from_slice(key);
            let Some(entry) = entries.get(index) else {
                if route.aux.contains_key(&self.lookup) {
                    return Err(mismatch("insertion has no journal entry"));
                }
                continue;
            };
            let PalwDeltaEntryV2::KernelRouteRow { table: actual, key: actual_key, old: None, new: Some(new) } = entry else {
                return Err(mismatch("inverse encountered a non-insertion"));
            };
            if *actual != table || actual_key.as_slice() != key || route.aux.get(&self.lookup) != Some(new) {
                return Err(mismatch("inserted row differs from the owned journal"));
            }
        }
        let route = b.state.kernel_route.as_mut().expect("route checked above");
        while b.entries.len() > self.first {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, .. } = b.entries.pop().expect("checked journal suffix") else {
                unreachable!("checked insertion journal")
            };
            route.aux.remove(&(table, key));
        }
        // Keep writer reservations charged: the journal's allocation remains
        // owned by the enclosing builder, including after a failed attempt.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameters() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0).unwrap()
    }

    fn state() -> PalwChainStateV2 {
        let mut state = PalwChainStateV2::genesis();
        state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2)),
            None,
            misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
        ));
        state
    }

    // Raw effects test inverse ownership, not row admission or valid encoding.
    fn insertion(b: &mut TransitionBuilder<'_>, table: u8, key: &[u8]) {
        let bytes = vec![table; 16];
        assert!(b.state.kernel_route.as_mut().unwrap().aux.insert((table, key.to_vec()), bytes.clone()).is_none());
        b.entries.push(PalwDeltaEntryV2::KernelRouteRow { table, key: key.to_vec(), old: None, new: Some(bytes) });
    }

    #[test]
    fn paid_inverse_restores_each_partial_prefix_without_changing_prior_rows() {
        let params = parameters();
        let extras = PalwTransitionExtrasV1::default();
        let initial = state();
        for count in 0..=2 {
            let mut b = TransitionBuilder::new(&initial, &params, false, false, false, false, &extras);
            insertion(&mut b, 197, &[3; 64]);
            let before = b.state.clone();
            let journal = b.entries.clone();
            let inverse = PaidManifestInverseV1::prepare_v1(&b, [4; 128], [5; 64]).unwrap();
            for (table, key) in [(PUBLIC_PENDING_MANIFEST_TABLE_V1, &[4; 128][..]), (PUBLIC_MANIFEST_PAID_TABLE_V1, &[5; 64][..])]
                .into_iter()
                .take(count)
            {
                insertion(&mut b, table, key);
            }
            let checker = crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
                crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 },
            );
            b.native_reservation = crate::palw_native_resource_v1::NativeTransitionReservationV1::for_checker_v1(&checker);
            inverse.restore_v1(&mut b).unwrap();
            assert_eq!(b.state, before);
            assert_eq!(b.entries, journal);
            assert_eq!(checker.outstanding(), (0, 0));
        }
    }

    #[test]
    fn paid_inverse_refuses_foreign_effects_before_removing_either_row() {
        let params = parameters();
        let extras = PalwTransitionExtrasV1::default();
        let initial = state();
        for invalid in 0..6 {
            let mut b = TransitionBuilder::new(&initial, &params, false, false, false, false, &extras);
            let inverse = PaidManifestInverseV1::prepare_v1(&b, [4; 128], [5; 64]).unwrap();
            insertion(&mut b, PUBLIC_PENDING_MANIFEST_TABLE_V1, &[4; 128]);
            insertion(&mut b, PUBLIC_MANIFEST_PAID_TABLE_V1, &[5; 64]);
            match invalid {
                0 => b.entries.swap(0, 1),
                1 => {
                    b.state.kernel_route.as_mut().unwrap().aux.get_mut(&(PUBLIC_MANIFEST_PAID_TABLE_V1, vec![5; 64])).unwrap()[0] ^= 1;
                }
                2 => insertion(&mut b, 197, &[6; 64]),
                3 => {
                    let PalwDeltaEntryV2::KernelRouteRow { old, .. } = &mut b.entries[1] else { unreachable!() };
                    *old = Some(vec![1]);
                }
                4 => {
                    b.entries.pop();
                }
                _ => {
                    let PalwDeltaEntryV2::KernelRouteRow { key, .. } = &mut b.entries[1] else { unreachable!() };
                    key[0] ^= 1;
                }
            }
            let before = b.state.clone();
            let journal = b.entries.clone();
            assert!(matches!(inverse.restore_v1(&mut b), Err(PalwStateV2Error::NativeResource(_))));
            assert_eq!(b.state, before);
            assert_eq!(b.entries, journal);
        }
        for (table, key) in [(PUBLIC_PENDING_MANIFEST_TABLE_V1, &[4; 128][..]), (PUBLIC_MANIFEST_PAID_TABLE_V1, &[5; 64][..])] {
            let mut b = TransitionBuilder::new(&initial, &params, false, false, false, false, &extras);
            insertion(&mut b, table, key);
            assert!(matches!(PaidManifestInverseV1::prepare_v1(&b, [4; 128], [5; 64]), Err(PalwStateV2Error::NativeResource(_))));
        }
    }
}
