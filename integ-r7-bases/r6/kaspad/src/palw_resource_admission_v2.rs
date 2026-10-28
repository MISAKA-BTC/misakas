//! Explicit node-local capacity policy. It does not change consensus, prices,
//! supported graphs, or legacy operator behaviour when the capability is off.
use std::sync::OnceLock;
static STRICT: OnceLock<bool> = OnceLock::new();
pub fn arm(enabled: bool) {
    let _ = STRICT.set(enabled);
}
pub fn enabled() -> bool {
    *STRICT.get().unwrap_or(&false)
}
/// Missing holding measurements have no defensible zero-byte interpretation.
pub fn unknown_holding_terms() -> (u64, u64) {
    if enabled() { (u64::MAX, 0) } else { (0, 0) }
}
/// A saturated demand and a missing local capacity cannot authorize work.
pub fn admission_capacity(capacity: Option<u64>, demand: u64) -> Option<u64> {
    admission_capacity_for(enabled(), capacity, demand)
}
fn admission_capacity_for(strict: bool, capacity: Option<u64>, demand: u64) -> Option<u64> {
    if strict && (capacity.is_none() || demand == u64::MAX) { Some(0) } else { capacity }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_capacity_and_saturated_demand_never_authorize_strict_work() {
        assert_eq!(admission_capacity_for(true, None, 1), Some(0));
        assert_eq!(admission_capacity_for(true, Some(u64::MAX), u64::MAX), Some(0));
        assert_eq!(admission_capacity_for(true, Some(4096), 2048), Some(4096));
        assert_eq!(admission_capacity_for(false, None, u64::MAX), None);
        assert_eq!(admission_capacity_for(false, Some(4096), 2048), Some(4096));
    }
}
