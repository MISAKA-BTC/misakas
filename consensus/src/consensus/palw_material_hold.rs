//! A material read is also a file-deletion authorization. Unknown reads must pin.

pub(super) fn material_owed_from_snapshot_v1<G: PartialEq, T, E: std::fmt::Display>(
    stable: bool,
    expected_generation: G,
    snapshot: Result<Option<(G, T)>, E>,
    read_owed: impl FnOnce(&T) -> bool,
) -> Result<bool, String> {
    if !stable {
        return Err("material GC import/branch generation changed".into());
    }
    match snapshot {
        Ok(Some((generation, state))) if generation == expected_generation => Ok(read_owed(&state)),
        Ok(Some(_)) => Err("material GC snapshot generation does not match selected parent".into()),
        Ok(None) => Err("material GC accepted snapshot is unavailable".into()),
        Err(error) => Err(format!("material GC snapshot read failed: {error}")),
    }
}

pub(super) fn material_hold_from_snapshot_v1<G: PartialEq, T, E>(
    active: bool,
    stable: bool,
    expected_generation: G,
    snapshot: Result<Option<(G, T)>, E>,
    read_hold: impl FnOnce(T) -> Option<u64>,
) -> Option<u64> {
    if !active {
        return None;
    }
    if !stable {
        return Some(u64::MAX);
    }
    match snapshot {
        Ok(Some((generation, state))) if generation == expected_generation => read_hold(state),
        _ => Some(u64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_cold_missing_or_changed_reads_never_authorize_material_gc() {
        for snapshot in [Err("native allocation refused"), Err("DB read failed"), Ok(None), Ok(Some((8, false)))] {
            assert!(material_owed_from_snapshot_v1(true, 9, snapshot, |_| panic!("unknown state cannot judge a claim")).is_err());
        }
        assert!(material_owed_from_snapshot_v1::<_, _, &str>(false, 9, Ok(Some((9, false))), |_| panic!("unstable import")).is_err());
    }
    #[test]
    fn healthy_absent_or_discharged_claims_can_be_collected_after_recovery() {
        struct BorrowedOnly {
            owed: bool,
        }
        // Deliberately not Clone: authorization borrows the source projection.
        for owed in [false, true] {
            assert_eq!(
                material_owed_from_snapshot_v1::<_, _, &str>(true, 9, Ok(Some((9, BorrowedOnly { owed }))), |state| state.owed),
                Ok(owed)
            );
        }
    }

    #[test]
    fn unavailable_or_other_generation_material_snapshot_cannot_discharge_retention() {
        for snapshot in [Err("read failed"), Ok(None), Ok(Some((8, None)))] {
            assert_eq!(material_hold_from_snapshot_v1(true, true, 9, snapshot, |hold| hold), Some(u64::MAX));
        }
        for stable in [false, true] {
            assert_eq!(
                material_hold_from_snapshot_v1::<_, Option<u64>, ()>(false, stable, 9, Ok(Some((8, None))), |_| panic!(
                    "dormant read"
                )),
                None
            );
        }
    }

    #[test]
    fn matching_complete_snapshot_preserves_exact_absence_and_physical_payment_clock() {
        for hold in [None, Some(500), Some(u64::MAX)] {
            assert_eq!(material_hold_from_snapshot_v1::<_, _, ()>(true, true, 9, Ok(Some((9, hold))), |hold| hold), hold);
            assert_eq!(
                material_hold_from_snapshot_v1::<_, _, ()>(true, false, 9, Ok(Some((9, hold))), |_| panic!("unstable import read")),
                Some(u64::MAX)
            );
        }
    }
}
