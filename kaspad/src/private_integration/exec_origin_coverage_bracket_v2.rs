//! Generation bracket for private historical-header observations.
pub(super) fn bracketed<S: Eq, T>(
    mut stamp: impl FnMut() -> Result<S, String>,
    read: impl FnOnce() -> Result<T, String>,
) -> Result<(S, T), String> {
    let before = stamp()?;
    let value = read()?;
    if before != stamp()? {
        return Err("coverage point/clock/import changed".into());
    }
    Ok((before, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn census_requires_one_generation() {
        let original = ("a", 100, 7);
        assert_eq!(bracketed(|| Ok(original), || Ok(42)).unwrap().1, 42);
        for changed in [("b", 100, 7), ("a", 101, 7), ("a", 100, 8)] {
            let mut reads = [Ok(original), Ok(changed)].into_iter();
            assert!(bracketed(|| reads.next().unwrap(), || Ok(42)).is_err());
        }
    }

    #[test]
    fn census_does_not_survive_missing_stamps_or_failed_reads() {
        assert!(bracketed(|| Err::<(), _>("import unavailable".into()), || -> Result<(), String> { panic!("must not read") }).is_err());
        let mut reads = [Ok(("a", 100, 7)), Err("import active".into())].into_iter();
        assert!(bracketed(|| reads.next().unwrap(), || Ok(42)).is_err());
        assert!(bracketed(|| Ok(("a", 100, 7)), || Err::<(), _>("header read failed".into())).is_err());
    }
}
