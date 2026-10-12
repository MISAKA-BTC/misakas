//! **A device's cells (RFC-0006): the cell stepper over the GPU executor.**
//!
//! [`GpuCellStepper`] is [`GpuExecutor`] as a `misaka_palw_tir_exec::TirCellStepperV1`: the node's one cell verifier
//! (`node::verify_cell_stepping_v1`, in a binary that links consensus) runs over it unchanged, so a device's verdict is the CPU's by
//! construction — and the executor underneath is held byte-exact to the CPU executor by the conformance suite
//! (`tests/cell.rs` holds the cell grain: every committed value, carry, logits row, `Fixed` state and history tile of a cell equal
//! to `CpuCellStepperV1`'s).
//!
//! [`GpuDeviceV1`] is the `TirDeviceV1` a node registers (`node::register_device_v1`). **It refuses what it cannot run, and a
//! refusal is always correct** (`gpu-integer-backend.md` §8, B-1): a cell whose params the device cannot hold, a history the device
//! holds too few rows of for a tile. A cell of a segment after the first never reaches it (the node adapter refuses it: the device
//! keeps no committed boundary state to resume from). The node then runs the CPU executor on the same cell.
//!
//! **Why this crate does not register itself.** It is an isolated workspace: wgpu 30 needs `js-sys ^0.3.104` and consensus-core's
//! wasm stack pins `js-sys =0.3.77`, so no lock holds both. A node binary that carries a device does so through a plug-in that
//! builds in this workspace and speaks the two traits of `misaka_palw_tir_exec::cellstep` (no consensus type crosses).

use std::ops::Range;

use misaka_palw_tir::{RunState, TirResult};
use misaka_palw_tir_exec::elem::Buf;
use misaka_palw_tir_exec::params::TirParams;
use misaka_palw_tir_exec::plan::TirPlan;
use misaka_palw_tir_exec::{StepSink, TirCellStepperV1, TirDeviceV1, buf_lanes_le_v1};

use crate::device::GpuDevice;
use crate::exec::GpuExecutor;

/// The device executor as a cell stepper.
pub struct GpuCellStepper<'a>(pub GpuExecutor<'a>);

impl TirCellStepperV1 for GpuCellStepper<'_> {
    fn step_cell(
        &mut self,
        token: u32,
        occ: Range<usize>,
        carry_in: &[Vec<i128>],
        sink: &mut dyn StepSink,
    ) -> TirResult<Vec<Vec<i128>>> {
        self.0.step_cell(token, occ, carry_in, sink)
    }

    fn logits_lanes(&self) -> Vec<i32> {
        self.0.logits().1.to_i128s().into_iter().map(|v| v as i32).collect()
    }

    fn fixed_lanes(&self, state: u16, layer: Option<u16>, first: usize, n: usize, out: &mut Vec<u8>) -> Result<(), String> {
        let buf: Buf = self.0.fixed_value(state, layer).ok_or_else(|| format!("no Fixed instance {:?}", (state, layer)))?;
        buf_lanes_le_v1(&buf, first, n, out);
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
        let rows = self
            .0
            .hist_tail_rows(state, layer, h_tile)
            .ok_or_else(|| format!("history {:?}: the device holds fewer than {h_tile} rows", (state, layer)))?;
        for row in rows {
            for x in &row[first_lane..first_lane + row_lanes] {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        Ok(())
    }

    fn resume(
        &mut self,
        _st: &RunState,
        _keep: &dyn Fn(u16, Option<u16>) -> bool,
        _tails: &[(u16, Option<u16>, Vec<Vec<i32>>)],
    ) -> Result<(), String> {
        Err("a device does not restore a segment's committed state".to_string())
    }
}

/// The device: one wgpu device, opened once.
pub struct GpuDeviceV1 {
    dev: GpuDevice,
    name: String,
    capacity: Option<u64>,
}

impl GpuDeviceV1 {
    /// Open the machine's device, or say why not.
    pub fn open() -> Result<Self, String> {
        let dev = GpuDevice::new().map_err(|e| e.to_string())?;
        let name = format!("wgpu/{}", dev.describe());
        // A unified-memory adapter's bytes ARE the host's: no separate device pool (§9).
        let capacity = (dev.info.device_type != wgpu::DeviceType::IntegratedGpu).then_some(dev.limits.max_buffer_size);
        Ok(Self { dev, name, capacity })
    }
}

impl TirDeviceV1 for GpuDeviceV1 {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn capacity_bytes(&self) -> Option<u64> {
        self.capacity
    }

    fn cell_stepper<'a>(
        &'a self,
        plan: &'a TirPlan,
        params: &'a TirParams<'a>,
        occ: Range<usize>,
    ) -> Result<Box<dyn TirCellStepperV1 + 'a>, String> {
        let exec = GpuExecutor::new_cell(&self.dev, plan, params, occ).map_err(|e| e.to_string())?;
        Ok(Box::new(GpuCellStepper(exec)))
    }
}
