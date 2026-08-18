//! The resident agent is an accelerator: turning it on must not change what a caller sees.
//!
//! This is the model-free half of that claim, and it pins the boundary where it is easiest to
//! break — the case where the agent CANNOT run at all. With `MISAKA_PALW_AGENT=1` and a worker
//! that does not exist, `palw_l1_tag` must produce exactly the `PalwUnavailable` an agent-less
//! node produces, still naming the worker path, rather than an agent-specific error, a hang, or a
//! panic. The equivalence of the two SUCCESS paths needs the 1.2 GB pinned model and lives in
//! `palw_agent_equivalence.rs`.

use kaspa_consensus_core::pow_layer0::PowLayer0Error;
use kaspa_hashes::Hash64;
use kaspa_pow::palw::palw_l1_tag;

#[test]
fn an_unspawnable_agent_falls_back_to_the_one_shot_error() {
    const MISSING: &str = "/nonexistent/palw-worker-that-is-not-here";
    // SAFETY: this integration test is its own binary and holds exactly one test, so nothing else
    // in the process reads or writes the environment while this runs.
    unsafe {
        std::env::set_var("MISAKA_PALW_AGENT", "1");
        std::env::set_var("PALW_WORKER", MISSING);
        // `devnet` pins no determinism class, so the calibration probe is a no-op — but the
        // fixture family would answer without any subprocess at all and make this vacuous.
        std::env::remove_var("MISAKA_PALW_POW_FIXTURE");
    }

    let err = palw_l1_tag(Hash64::from_bytes([7u8; 64]), 1, 1, b"devnet").expect_err("no worker exists to answer");
    match err {
        PowLayer0Error::PalwUnavailable(msg) => {
            assert!(msg.contains(MISSING), "an operator whose worker is missing must still be told which path was tried, got: {msg}");
        }
        other => panic!("enabling the agent changed the failure a missing worker produces: {other:?}"),
    }
}
