use super::*;

#[test]
fn producer_context_hold_does_not_authorize_new_execution() {
    let parent = Hash64::from_u64_word(1);
    let other = Hash64::from_u64_word(2);
    let view = (parent, 100);
    let run = std::cell::Cell::new(0);
    let attempt = |facts, template, epoch, allowed, active, context| {
        let checked = palw_producer_view_guard_v1(facts, template, epoch, allowed, active, context);
        if checked.is_ok() {
            run.set(run.get() + 1);
        }
        checked
    };
    assert!(attempt((other, 100), view, None, true, true, Ok(7)).is_err());
    assert!(attempt((parent, 99), view, None, true, true, Ok(7)).is_err());
    for missing in ["own bootstrap selected-chain history is unavailable", "native state missing", "import incomplete"] {
        assert!(attempt(view, view, None, true, true, Err(missing.into())).is_err());
    }
    assert!(attempt(view, view, Some(7), true, true, Ok(8)).is_err());
    assert!(attempt(view, view, Some(7), false, true, Ok(7)).is_err());
    assert!(attempt(view, view, Some(7), true, false, Ok(7)).is_err());
    assert_eq!(run.get(), 0, "every hold prevents starting inference");
    assert_eq!(attempt(view, view, Some(7), true, true, Ok(7)), Ok(7));
    assert_eq!(run.get(), 1, "coherent restored state resumes work without a sticky hold");
}

#[test]
fn producer_publication_allows_parent_progress_but_rejects_replaced_import() {
    // Publication guard deliberately has no current-parent input: ordinary
    // advancement during a multi-minute draw does not discard a valid result.
    assert_eq!(palw_producer_epoch_guard_v1(Some(7), true, true, Ok(7)), Ok(7));
    assert!(palw_producer_epoch_guard_v1(Some(7), true, true, Ok(8)).is_err());
    assert!(palw_producer_epoch_guard_v1(Some(7), true, true, Err("import incomplete".into())).is_err());
    assert!(palw_producer_epoch_guard_v1(Some(7), true, false, Ok(7)).is_err());
    assert!(palw_producer_epoch_guard_v1(Some(7), false, true, Ok(7)).is_err());
}
