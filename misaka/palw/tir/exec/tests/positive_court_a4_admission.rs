//! Real A4/A5 admission tests through the public core library API. Keeping the
//! test driver on the Node side avoids a core -> Node dependency cycle.
#![cfg(feature = "node")]
use kaspa_consensus_core::palw_positive_court_v2::coordinator::admission::*;
use misaka_palw_positive_contract::{
    ContractError,
    ports::{AdmissionIntent, AtomicCourtStore},
};
#[path = "../../../../../consensus/core/src/palw_positive_court_v2/coordinator/admission_tests.rs"]
mod tests;
