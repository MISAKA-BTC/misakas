//! Single-use thread-local test callback, not a per-instance production hook.
use super::Consensus;
use crate::pipeline::virtual_processor::VirtualStateProcessor;
thread_local! {
    static AFTER_PROJECTION: std::cell::Cell<Option<fn(&VirtualStateProcessor)>> = const { std::cell::Cell::new(None) };
}
pub(super) fn invoke(consensus: &Consensus) {
    AFTER_PROJECTION.with(|callback| {
        if let Some(callback) = callback.take() {
            callback(&consensus.virtual_processor);
        }
    });
}
impl Consensus {
    pub(crate) fn observe_evm_view_projection_v2(&self, callback: Option<fn(&VirtualStateProcessor)>) {
        AFTER_PROJECTION.with(|slot| slot.set(callback));
    }
}
