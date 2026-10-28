//! **The cell stepper seam (RFC-0006): what a cell verifier asks of an executor, and of a device.**
//!
//! The node's one cell verifier (`node::verify_cell_stepping_v1`) runs over any [`TirCellStepperV1`]: the CPU executor
//! ([`CpuCellStepperV1`]) or a device's. A device is a [`TirDeviceV1`]: it builds a stepper for the occurrences of a cell from
//! the cell's params. Nothing here depends on consensus types, so a device crate built outside the node's lock (the GPU
//! backend is its own cargo workspace) implements these two traits and nothing more; the node adapts a registered device to
//! its `KernelBackendV1` (`node::register_device_v1`) and falls back to the CPU on any refusal.

use std::ops::Range;

use misaka_palw_tir::RunState;

use crate::elem::{Buf, Elem, Slice};
use crate::exec::{StepSink, TirExecutor};
use crate::params::TirParams;
use crate::plan::TirPlan;

/// **What the cell verifier asks of an executor** — the CPU executor ([`CpuCellStepperV1`]) or a device's (`misaka-palw-tir-gpu`).
/// A backend implements the stepping and the reading of the state it holds, and refuses (`resume`) what it cannot do; the
/// verifier ([`verify_cell_stepping_v1`]) is one function over both, so a device's verdict is the CPU's by construction.
pub trait TirCellStepperV1 {
    /// One position of the cell (see [`TirExecutor::step_cell`]): the committed values reach `sink`, the last occurrence's
    /// carry-out is returned (empty at `post`).
    fn step_cell(
        &mut self,
        token: u32,
        occ: std::ops::Range<usize>,
        carry_in: &[Vec<i128>],
        sink: &mut dyn StepSink,
    ) -> misaka_palw_tir::TirResult<Vec<Vec<i128>>>;
    /// The last step's logits row, as `i32` lanes (the last shard's).
    fn logits_lanes(&self) -> Vec<i32>;
    /// `n` lanes of the `Fixed` instance of `(state, layer)` from `first`, appended little-endian (`i32`/`u32`, PALW-TIR-5).
    fn fixed_lanes(&self, state: u16, layer: Option<u16>, first: usize, n: usize, out: &mut Vec<u8>) -> Result<(), String>;
    /// The newest `h_tile` rows of the history instance of `(state, layer)`, lanes `first_lane .. first_lane + row_lanes` of
    /// each, oldest row first, appended little-endian.
    fn hist_tile_lanes(
        &self,
        state: u16,
        layer: Option<u16>,
        h_tile: usize,
        first_lane: usize,
        row_lanes: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), String>;
    /// Resume at the start of position `st.pos` for the instances `keep` names (a segment after the first). A backend that
    /// cannot resume refuses; the caller then verifies the cell on the CPU.
    fn resume(
        &mut self,
        _st: &RunState,
        _keep: &dyn Fn(u16, Option<u16>) -> bool,
        _tails: &[(u16, Option<u16>, Vec<Vec<i32>>)],
    ) -> Result<(), String> {
        Err("this backend does not resume a segment".to_string())
    }
}

/// The CPU executor as a cell stepper.
pub struct CpuCellStepperV1<'a>(pub TirExecutor<'a>);

impl TirCellStepperV1 for CpuCellStepperV1<'_> {
    fn step_cell(
        &mut self,
        token: u32,
        occ: std::ops::Range<usize>,
        carry_in: &[Vec<i128>],
        sink: &mut dyn StepSink,
    ) -> misaka_palw_tir::TirResult<Vec<Vec<i128>>> {
        self.0.step_cell(token, occ, carry_in, sink)
    }
    fn logits_lanes(&self) -> Vec<i32> {
        self.0.logits().1.to_i128s().into_iter().map(|v| v as i32).collect()
    }
    fn fixed_lanes(&self, state: u16, layer: Option<u16>, first: usize, n: usize, out: &mut Vec<u8>) -> Result<(), String> {
        let v = self.0.fixed_value(state, layer).ok_or_else(|| format!("no Fixed instance {:?}", (state, layer)))?;
        lanes_le_of_v1(v, first, n, out);
        Ok(())
    }
    fn hist_tile_lanes(
        &self,
        state: u16,
        layer: Option<u16>,
        h_tile: usize,
        first_lane: usize,
        row_lanes: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), String> {
        let tail = self.0.hist_tail(state, layer).ok_or_else(|| format!("no history instance {:?}", (state, layer)))?;
        if tail.len() < h_tile {
            return Err(format!("history {:?}: {} rows kept for a tile of {h_tile}", (state, layer), tail.len()));
        }
        for row in tail.iter().skip(tail.len() - h_tile) {
            for x in &row[first_lane..first_lane + row_lanes] {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        Ok(())
    }
    fn resume(
        &mut self,
        st: &RunState,
        keep: &dyn Fn(u16, Option<u16>) -> bool,
        tails: &[(u16, Option<u16>, Vec<Vec<i32>>)],
    ) -> Result<(), String> {
        self.0.import_cell_state(st, keep).map_err(|e| e.to_string())?;
        for (j, layer, rows) in tails {
            self.0.set_hist_tail_rows(*j, *layer, rows).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// **A device a node may run cells on**: one place values live and kernels run. It builds the stepper of one cell from the cell's
/// occurrences' params (a shard seat's own layers, and nothing else) or refuses; a refusal is always correct (`gpu-integer-backend.md` B-1).
pub trait TirDeviceV1: Send + Sync {
    /// "wgpu/Apple M1 Max (Metal)" — logs and the gate.
    fn name(&self) -> String;
    /// The device memory a node's ledger may arm a device pool with; `None` when the device's memory is the host's.
    fn capacity_bytes(&self) -> Option<u64>;
    /// The stepper of a cell over `occ`, with `params` uploaded for those occurrences only; a stepper that starts at position 0
    /// (a device does not resume a segment).
    fn cell_stepper<'a>(
        &'a self,
        plan: &'a TirPlan,
        params: &'a TirParams<'a>,
        occ: Range<usize>,
    ) -> Result<Box<dyn TirCellStepperV1 + 'a>, String>;
}

/// A buffer's lanes `first .. first + n` as the little-endian words a leaf commits (`idx` as `u32`, everything else `i32`,
/// PALW-TIR-5) — for a backend whose state lives elsewhere and reads back to a [`Buf`].
pub fn buf_lanes_le_v1(buf: &Buf, first: usize, n: usize, out: &mut Vec<u8>) {
    lanes_le_of_v1(buf.slice(), first, n, out);
}

/// [`buf_lanes_le_v1`] over a borrowed slice.
pub fn lanes_le_of_v1(data: Slice<'_>, from: usize, n: usize, out: &mut Vec<u8>) {
    use misaka_palw_tir::DType;
    fn typed<T: Elem>(v: &[T], out: &mut Vec<u8>) {
        if T::DTYPE == DType::Idx {
            for x in v {
                out.extend_from_slice(&(x.to_i64() as u32).to_le_bytes());
            }
        } else {
            for x in v {
                out.extend_from_slice(&(x.to_i64() as i32).to_le_bytes());
            }
        }
    }
    crate::with_slice!(data, v => typed(&v[from..from + n], out));
}
