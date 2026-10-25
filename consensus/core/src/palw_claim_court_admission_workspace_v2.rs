//! Temporary funded membership for native claim admission, before insertion.
//! F qualifying escrow rows bound D distinct claims (D <= F); streaming insertion
//! avoids FromIterator's extra bulk buffers. The builder construction owner is
//! retained through the last lookup and beyond the local Set's destruction.
use super::super::native_writers::NativeWriteQuoteV1;
use super::*;

pub(super) fn quote(state: &PalwChainStateV2) -> Result<NativeWriteQuoteV1, PalwStateV2Error> {
    let mut q = NativeWriteQuoteV1::new("native claim admission funded membership");
    let n = funded_court_reserve_claims_borrowed_v2(state).count() as u64;
    q.member::<Hash64, ()>(n)?;
    Ok(q)
}

// Read-only phase instrumentation: the request and refusal remain production
// code. It cannot supply an error, change capacity, or alter the borrowed state.
#[cfg(test)]
thread_local! {
    static OBSERVER: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(super) fn set_observer(observer: Option<Box<dyn Fn()>>) {
    OBSERVER.with(|slot| *slot.borrow_mut() = observer);
}
#[cfg(test)]
pub(super) fn observe_before_admission() {
    OBSERVER.with(|slot| {
        if let Some(observer) = slot.borrow().as_ref() {
            observer();
        }
    });
}
