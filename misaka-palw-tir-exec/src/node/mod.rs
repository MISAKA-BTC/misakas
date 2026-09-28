//! **The executor as a node runs a class** (feature `node`; RFC-0002 Phase F step F9): a
//! PALWTIR1 artifact mapped in place ([`artifact`]), and a job executed into the step leg and the
//! roots Phase F defines ([`run`]). The consensus types these are stated in live in
//! `kaspa-consensus-core`; the executor itself does not need them, so this layer is a feature.

pub mod artifact;
pub mod mapped;
pub mod run;

pub use artifact::TirArtifactV1;
pub use run::{TirClassRunnerV1, TirJobRunV1, TirLeafOutV1};
