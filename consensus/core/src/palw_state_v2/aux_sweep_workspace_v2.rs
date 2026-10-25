//! Borrowed caller scratch for the existing pre-object auxiliary sweeps.
//! Row writers separately admit their mutations and retained journal outputs.
use super::native_writers::NativeWriteQuoteV1;
use super::*;

fn slots<T>(quote: &mut NativeWriteQuoteV1, count: usize, path: &'static str) -> Result<(), PalwStateV2Error> {
    if count != 0 {
        // Keep the unchanged collectors' minimum capacity and geometric growth,
        // including old/new buffer coexistence, inside a conservative envelope.
        let count = (count as u64).checked_mul(4).ok_or_else(|| {
            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Vec<T>>(path))
        })?;
        quote.exact::<T>(count.max(8))?;
    }
    Ok(())
}

impl TransitionBuilder<'_> {
    pub(super) fn reserve_aux_sweep_vec_v2<T>(&mut self, count: usize, path: &'static str) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        let mut quote = NativeWriteQuoteV1::new(path);
        slots::<T>(&mut quote, count, path)?;
        self.admit_native_write_v1(quote)
    }

    pub(super) fn reserve_panel_sweep_workspace_v2(&mut self) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        type Lock = (PalwBondKeyV2, Hash64);
        type GroupEntry = (Lock, bool);
        let n = self.state.slashable_locks.len();
        let mut quote = NativeWriteQuoteV1::new("panel obligation grouped sweep scratch");
        // At most one group per actual lock. Eight entries per lock cover the
        // minimum allocation of many singleton groups and growth of large ones.
        // Group removals/iterators transfer buffers; they do not deep-clone them.
        quote.member::<Hash64, Vec<GroupEntry>>(n as u64)?;
        let groups = (n as u64).checked_mul(8).ok_or_else(|| {
            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<GroupEntry>(
                "panel obligation grouped sweep scratch",
            ))
        })?;
        quote.exact::<GroupEntry>(groups)?;
        slots::<Lock>(&mut quote, n, "panel obligation pruned locks")?;
        slots::<Hash64>(&mut quote, self.state.panel_liabilities.len(), "panel obligation pruned liabilities")?;
        self.admit_native_write_v1(quote)
    }
}
