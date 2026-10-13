//! A material read is also a file-deletion authorization. Unknown reads must pin.

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
