//! Run the same public fence/identity tests as one bounded test target.
use kaspa_consensus_core::config::params::{ForkActivation, PositiveCourtFenceV2, PositiveCourtPolicyV2};
pub use kaspa_consensus_core::{Hash, Hash64, config, palw_lifecycle_objects_v2, subnets, tx};
use misaka_palw_positive_contract::{messages::CheckProfile, tir};

#[path = "../src/config/params/positive_court_v2_tests.rs"]
mod checks;
