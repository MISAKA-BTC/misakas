//! **PALW-TIR v1 on a GPU: an exact integer backend** (RFC-0002 §7 F-6, Phase G; design:
//! `docs/design/palw/tir/gpu-integer-backend.md`).
//!
//! Every PALW-TIR value is an integer, every lossy site is a named primitive with one rounding
//! rule, and every exact sum is order-free inside a proved range (PALW-TIR-24). So a GPU computes
//! the SAME function as the reference evaluator — byte for byte, at every node — provided it
//! reproduces each lossy site exactly and never lets an accumulator wrap where the proof does not
//! cover it. There is no tolerance and no "reproducibility mode": a backend that differs in one bit
//! is a defect the conformance suite catches (F-4), and on the chain it convicts whoever ran it.
//!
//! * **API: wgpu** ([`device`]) — Metal on macOS, Vulkan on Linux, one WGSL source for both.
//! * **`i64` is native** (`SHADER_INT64`, required of the device); **`i128`** is two `u64` words
//!   with an exact library of its own ([`wgsl::INTLIB128`]); only checked `i128` arithmetic (a
//!   result that may not even fit `i128`) runs on the CPU executor's kernel.
//! * **The plan decides, not the backend.** A kernel takes the CPU executor's refined
//!   [`NodePlan`](misaka_palw_tir_exec::plan::NodePlan) — the working type, the accumulation proof
//!   (`Acc`), the checks that can still fire — and computes under exactly that proof; a node whose
//!   plan it cannot honour is not run here.
//! * **Two executors.** [`GpuExecutor`] steps one position at a time, as `TirExecutor` does;
//!   [`BatchReplay`] replays a whole job a layer at a time over all of its positions (a projection
//!   one GEMM over the positions, causal attention one masked batched product), for a seat that
//!   holds every token before it starts.
//!
//! Node software, never a consensus dependency, and an isolated cargo workspace: nothing in the
//! root workspace depends on this crate or sees its dependencies.

pub mod batch;
pub mod cell;
pub mod device;
pub mod exec;
pub mod fixtures;
pub mod kernels;
pub mod tensor;
pub mod wgsl;

pub use batch::{BatchReplay, BatchStats, ReplayOut, ReplaySink};
pub use cell::{GpuCellStepper, GpuDeviceV1};
pub use device::{DeviceError, GpuDevice};
pub use exec::{ExecStats, GpuExecutor};
pub use kernels::{DeviceFailure, Ragged, Recorder, Unsupported};
pub use tensor::{DevTensor, Form};
