//! Bounded inverse of the three new TFO3 admission rows. Claim/frame/book/
//! escrow writes belong to the enclosing original checkpoint, not this one.
use super::*;
use crate::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1},
    palw_positive_contract_v2::{PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, PUBLIC_TFP1_LINK_TABLE_V1},
    palw_tfp1_original_history_v1::PUBLIC_TFP1_HISTORY_TABLE_V1,
};

pub(super) struct Tfp1OriginalInverseV1 {
    first: usize,
    original: [u8; 64],
    claim: [u8; 64],
    lookup: (u8, Vec<u8>),
    _memory: Option<NativeDemandLeaseV1>,
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

    // Inject raw writer effects solely to exercise inverse ownership and its
    // journal checks. These bytes are not admitted TFO3 rows or a valid delta.
    fn insertion(b: &mut TransitionBuilder<'_>, table: u8, key: [u8; 64]) {
        let bytes = vec![table; 16];
        assert!(b.state.kernel_route.as_mut().unwrap().aux.insert((table, key.to_vec()), bytes.clone()).is_none());
        b.entries.push(PalwDeltaEntryV2::KernelRouteRow { table, key: key.to_vec(), old: None, new: Some(bytes) });
    }

    #[test]
    fn insertion_inverse_restores_every_partial_prefix_without_allocating_at_rollback() {
        let p = parameters();
        let extras = PalwTransitionExtrasV1::default();
        let initial = state();
        for count in 0..=3 {
            let mut b = TransitionBuilder::new(&initial, &p, false, false, false, false, &extras);
            insertion(&mut b, 197, [3; 64]);
            let before = b.state.clone();
            let previous = b.entries.clone();
            let inverse = Tfp1OriginalInverseV1::prepare_v1(&b, [4; 64], [5; 64]).unwrap();
            for (table, key) in [
                (PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, [4; 64]),
                (PUBLIC_TFP1_LINK_TABLE_V1, [5; 64]),
                (PUBLIC_TFP1_HISTORY_TABLE_V1, [5; 64]),
            ]
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
            assert_eq!(b.entries, previous);
            assert_eq!(checker.outstanding(), (0, 0));
        }
    }

    #[test]
    fn inverse_refuses_unowned_or_changed_effects_before_removing_any_row() {
        let p = parameters();
        let extras = PalwTransitionExtrasV1::default();
        let initial = state();
        for invalid in 0..5 {
            let mut b = TransitionBuilder::new(&initial, &p, false, false, false, false, &extras);
            let inverse = Tfp1OriginalInverseV1::prepare_v1(&b, [4; 64], [5; 64]).unwrap();
            insertion(&mut b, PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, [4; 64]);
            insertion(&mut b, PUBLIC_TFP1_LINK_TABLE_V1, [5; 64]);
            match invalid {
                0 => b.entries.swap(0, 1),
                1 => {
                    b.state.kernel_route.as_mut().unwrap().aux.get_mut(&(PUBLIC_TFP1_LINK_TABLE_V1, vec![5; 64])).unwrap()[0] ^= 1;
                }
                2 => insertion(&mut b, PUBLIC_TFP1_HISTORY_TABLE_V1, [6; 64]),
                3 => {
                    let PalwDeltaEntryV2::KernelRouteRow { old, .. } = &mut b.entries[1] else { unreachable!() };
                    *old = Some(vec![1]);
                }
                _ => {
                    b.state.kernel_route.as_mut().unwrap().aux.insert((PUBLIC_TFP1_HISTORY_TABLE_V1, vec![5; 64]), vec![1]);
                }
            }
            let before = b.state.clone();
            let entries = b.entries.clone();
            assert!(matches!(inverse.restore_v1(&mut b), Err(PalwStateV2Error::NativeResource(_))));
            assert_eq!(b.state, before);
            assert_eq!(b.entries, entries);
        }
        let mut b = TransitionBuilder::new(&initial, &p, false, false, false, false, &extras);
        insertion(&mut b, PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, [4; 64]);
        assert!(matches!(Tfp1OriginalInverseV1::prepare_v1(&b, [4; 64], [5; 64]), Err(PalwStateV2Error::NativeResource(_))));
    }
}

fn mismatch(why: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Tfp1OriginalInverseV1>("TFP1 original inverse", why))
}

impl Tfp1OriginalInverseV1 {
    /// The only scratch allocation is one fixed lookup key. The writer must
    /// keep these tables free of derived-index side effects, checked before
    /// its actual mutation. A whole-state clone is never needed for these rows.
    pub(super) fn prepare_v1(b: &TransitionBuilder<'_>, original: [u8; 64], claim: [u8; 64]) -> Result<Self, PalwStateV2Error> {
        let mut quote = super::native_writers::NativeWriteQuoteV1::new("TFP1 original inverse lookup");
        quote.exact::<u8>(64)?;
        let memory = quote.reserve_scratch_v1()?;
        let mut key = Vec::new();
        key.try_reserve_exact(64).map_err(|_| mismatch("lookup allocation refused"))?;
        if key.capacity() != 64 {
            return Err(mismatch("lookup allocation differs from reserved size"));
        }
        key.extend_from_slice(&original);
        let mut lookup = (PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, key);
        let route = b.state.kernel_route.as_ref().ok_or_else(|| mismatch("missing native route"))?;
        for (table, key) in
            [(PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, original), (PUBLIC_TFP1_LINK_TABLE_V1, claim), (PUBLIC_TFP1_HISTORY_TABLE_V1, claim)]
        {
            lookup.0 = table;
            lookup.1.copy_from_slice(&key);
            if route.aux.contains_key(&lookup) {
                return Err(mismatch("inverse cannot own a preexisting row"));
            }
        }
        if let Some(memory) = &memory {
            memory.materialized_v1();
        }
        Ok(Self { first: b.entries.len(), original, claim, lookup, _memory: memory })
    }

    pub(super) fn restore_v1(mut self, b: &mut TransitionBuilder<'_>) -> Result<(), PalwStateV2Error> {
        let entries = b.entries.get(self.first..).ok_or_else(|| mismatch("journal precedes inverse boundary"))?;
        if entries.len() > 3 {
            return Err(mismatch("inverse encountered more than three insertions"));
        }
        let route = b.state.kernel_route.as_ref().ok_or_else(|| mismatch("native route disappeared during insertion"))?;
        // Check the entire suffix before changing any row. A changed writer
        // must extend this inverse; an unexpected effect is a local failure,
        // not a reason to discard a peer object and keep a partial candidate.
        for (index, (table, key)) in [
            (PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, &self.original),
            (PUBLIC_TFP1_LINK_TABLE_V1, &self.claim),
            (PUBLIC_TFP1_HISTORY_TABLE_V1, &self.claim),
        ]
        .into_iter()
        .enumerate()
        {
            self.lookup.0 = table;
            self.lookup.1.copy_from_slice(key);
            let Some(entry) = entries.get(index) else {
                if route.aux.contains_key(&self.lookup) {
                    return Err(mismatch("insertion has no journal entry"));
                }
                continue;
            };
            let PalwDeltaEntryV2::KernelRouteRow { table: actual, key: actual_key, old: None, new: Some(new) } = entry else {
                return Err(mismatch("inverse encountered a non-insertion"));
            };
            if *actual != table || actual_key.as_slice() != key {
                return Err(mismatch("inverse encountered an unrelated insertion"));
            }
            if route.aux.get(&self.lookup) != Some(new) {
                return Err(mismatch("inserted row differs from its journal"));
            }
        }
        let route = b.state.kernel_route.as_mut().expect("route checked above");
        while b.entries.len() > self.first {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, .. } = b.entries.pop().expect("checked journal suffix") else {
                unreachable!("checked insertion journal")
            };
            // Move the already charged journal key; rollback allocates no
            // replacement rows and never clones a state or encoded payload.
            route.aux.remove(&(table, key));
        }
        // Writer reservations remain charged until the enclosing builder is
        // dropped, including its possibly enlarged journal buffer. A retry
        // cannot reclaim capacity still physically owned by that buffer.
        Ok(())
    }
}
