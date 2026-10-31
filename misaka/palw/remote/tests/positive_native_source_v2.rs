//! Run the native-source public API fixtures with the Miner's exact dependency
//! feature set, without building the unrelated full core unit-test harness.
#![cfg(feature = "positive-miner")]

#[path = "../../../../consensus/core/tests/positive_source_v2.rs"]
mod native_source;
