//! Explicit Hello2 data transport for pre-admitted resident CPU plans.
//!
//! This namespace does not register a kernel/device/native backend. V1 DTOs,
//! message meanings and i32 wire readers/encoders remain unchanged. Generic
//! model-artifact startup/Open and bounded program admission are later work;
//! a preloaded toy plan is not generic registered-model IPC completion.
pub(super) mod budget;
pub mod codec;
pub mod cpu;

pub use budget::{WideCellLimitsV2, WideCellSessionQuoteV2, WideCellUsageV2, quote_client, quote_server};
pub use codec::{CELL_WIRE_VERSION_V2, CellParamBlobV1, CellRequestWideV2, CellResponseWideV2, CellValueV1};
pub use cpu::{CpuWideCellStepperV2, PreparedWideCellV2};
