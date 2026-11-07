//! Checked symbolic §8 replay costs. These count the specification's cost vector,
//! not physical instructions, elapsed time, checkpoint copies, acquisition or fees.
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1;
use misaka_palw_tir::admit::{CostV1, node_cost};
use std::cell::Cell;

pub(crate) struct TirReplayCostBudgetV1 {
    position: CostV1,
    limit: CostV1,
    used: Cell<CostV1>,
}
impl TirReplayCostBudgetV1 {
    pub(crate) fn new(space: &PalwTirStepSpaceV1, positions: u32) -> Result<Self, String> {
        let position = tir_replay_position_cost_v1(space)?;
        let limit = scale_cost(position, u64::from(positions)).ok_or("the TIR whole-move symbolic replay cost overflows")?;
        Ok(Self { position, limit, used: Cell::new(CostV1::default()) })
    }
    pub(crate) fn requested(&self, positions: u64) -> Option<CostV1> {
        scale_cost(self.position, positions)
    }
    pub(crate) fn admit(&self, positions: u64) -> Option<()> {
        let total = add_cost(self.used.get(), self.requested(positions)?)?;
        if !fits_cost(total, self.limit) {
            return None;
        }
        self.used.set(total);
        Some(())
    }
    pub(crate) fn used(&self) -> CostV1 {
        self.used.get()
    }
    pub(crate) fn reset(&self) {
        self.used.set(CostV1::default());
    }
    pub(crate) fn set_limit(&mut self, limit: CostV1) {
        self.limit = limit;
        self.reset();
    }
    pub(crate) fn set_positions(&mut self, positions: u32) {
        // An overflowing tool override fails closed at zero, rather than silently
        // using saturated costs. Actual constructors return an explicit error.
        self.set_limit(scale_cost(self.position, u64::from(positions)).unwrap_or_default());
    }
}

pub(crate) fn add_cost(a: CostV1, b: CostV1) -> Option<CostV1> {
    Some(CostV1 {
        macs: a.macs.checked_add(b.macs)?,
        elementwise: a.elementwise.checked_add(b.elementwise)?,
        transcendentals: a.transcendentals.checked_add(b.transcendentals)?,
        bytes_read: a.bytes_read.checked_add(b.bytes_read)?,
        bytes_written: a.bytes_written.checked_add(b.bytes_written)?,
    })
}
pub(crate) fn scale_cost(a: CostV1, n: u64) -> Option<CostV1> {
    Some(CostV1 {
        macs: a.macs.checked_mul(n)?,
        elementwise: a.elementwise.checked_mul(n)?,
        transcendentals: a.transcendentals.checked_mul(n)?,
        bytes_read: a.bytes_read.checked_mul(n)?,
        bytes_written: a.bytes_written.checked_mul(n)?,
    })
}
pub(crate) fn fits_cost(need: CostV1, limit: CostV1) -> bool {
    need.macs <= limit.macs
        && need.elementwise <= limit.elementwise
        && need.transcendentals <= limit.transcendentals
        && need.bytes_read <= limit.bytes_read
        && need.bytes_written <= limit.bytes_written
}

/// Sum every scheduled occurrence at its worst legal H = block.window. Including
/// post at every position is conservative. Repeated blocks are charged per occurrence.
/// A saturated legacy node cost is refused, never silently treated as a finite bound.
pub fn tir_replay_position_cost_v1(space: &PalwTirStepSpaceV1) -> Result<CostV1, String> {
    let mut total = CostV1::default();
    for &(block, _) in space.occurrences() {
        let block = block as usize;
        let h = u64::from(space.info.blocks[block].window.unwrap_or(1));
        for node in 0..space.program.blocks[block].nodes.len() {
            let cost = node_cost(&space.program, block, node, h);
            if [cost.macs, cost.elementwise, cost.transcendentals, cost.bytes_read, cost.bytes_written].contains(&u64::MAX) {
                return Err("a TIR node's symbolic replay cost is saturated".into());
            }
            total = add_cost(total, cost).ok_or("the TIR occurrence replay cost overflows")?;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_costs_refuse_sum_scale_overflow_and_each_dimension() {
        let one = CostV1 { macs: 1, elementwise: 2, transcendentals: 3, bytes_read: 4, bytes_written: 5 };
        assert_eq!(scale_cost(one, 2), add_cost(one, one));
        assert!(add_cost(CostV1 { macs: u64::MAX, ..one }, one).is_none());
        assert!(scale_cost(CostV1 { bytes_written: u64::MAX, ..one }, 2).is_none());
        assert!(fits_cost(one, one));
        for lower in [
            CostV1 { macs: 0, ..one },
            CostV1 { elementwise: 1, ..one },
            CostV1 { transcendentals: 2, ..one },
            CostV1 { bytes_read: 3, ..one },
            CostV1 { bytes_written: 4, ..one },
        ] {
            assert!(!fits_cost(one, lower));
        }
    }
}
