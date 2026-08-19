//! The fixture tag family is a property of the NETWORK, not of the process.
//!
//! `MISAKA_PALW_POW_FIXTURE=1` is process-global because environment variables are, but what it
//! selects is a rule set, and rule sets belong to networks. Until this was enforced where the tag
//! is computed, the confinement lived in kaspad's startup rail as a process-wide `exit(1)` — which
//! had two consequences worth a test each:
//!
//! * a library consumer of [`palw_l1_tag`] (a miner, a harness) was never subject to the
//!   confinement at all, because it never ran the rail;
//! * one process could not host a devnet consensus and a non-PALW consensus at the same time, so
//!   the integration suite could not be run in a single invocation whatever the variable was set
//!   to — the devnet half needs it, the simnet half aborted on it.
//!
//! Its own binary because it sets a process-global environment variable, and it sets it rather
//! than reading it so the assertions mean the same thing in a CI run that exports it and a
//! developer run that does not.

use kaspa_consensus_core::pow_layer0::{PowLayer0Error, palw_fixture_l1_tag_v1, palw_pow_seed_v1};
use kaspa_hashes::Hash64;
use kaspa_pow::palw::{fixture_permitted_on, fixture_requested, palw_l1_tag};

const TS: u64 = 1_700_000_000;
const NONCE: u64 = 5;

fn hash() -> Hash64 {
    Hash64::from_bytes([0x31; 64])
}

fn select_the_fixture_tag_family() {
    // SAFETY: this integration test is its own binary, and every test below reads the variable
    // only through the tag calls, after this has run.
    unsafe { std::env::set_var("MISAKA_PALW_POW_FIXTURE", "1") };
}

/// Devnet, suffixed devnet, and nothing else. `kaspa-devnet` is the case that matters: a substring
/// test would admit it, and it is not a network this codebase names — the ids here are the
/// `NetworkId` display form the consensus layer passes down (`params.net.to_string()`).
#[test]
fn only_devnet_permits_the_fixture_family() {
    assert!(fixture_permitted_on(b"devnet"));
    assert!(fixture_permitted_on(b"devnet-3"));

    for other in [&b"mainnet"[..], b"testnet", b"testnet-10", b"testnet-11", b"simnet", b"kaspa-devnet", b""] {
        assert!(!fixture_permitted_on(other), "{} must not run fixture rules", String::from_utf8_lossy(other));
    }
}

/// The property the suite needs: with the variable exported once, a devnet tag is the fixture and
/// a non-PALW network is untouched by it.
///
/// `simnet` stands for every network whose `pow_palw_activation` is `never()` — it computes no
/// PALW tag in production at all, which is precisely why aborting a process on its behalf was
/// over-broad. Asked for one anyway, it refuses for the honest reason (no worker configured) and
/// NOT with a fixture tag.
#[test]
fn one_process_can_hold_a_fixture_network_and_a_real_one() {
    select_the_fixture_tag_family();
    assert!(fixture_requested(), "the variable is what the whole test is about");

    let devnet = palw_l1_tag(hash(), TS, NONCE, b"devnet").expect("devnet honors the fixture");
    let expected = palw_fixture_l1_tag_v1(&palw_pow_seed_v1(hash(), TS, NONCE, b"devnet"));
    assert_eq!(devnet, expected, "devnet must compute the fixture tag for its own seed");

    // A worker in the developer's environment would answer the simnet call for real, which is a
    // different (also correct) outcome; the assertion that survives either way is the one that
    // matters — whatever simnet computed, it was not the fixture.
    let simnet = palw_l1_tag(hash(), TS, NONCE, b"simnet");
    match simnet {
        Err(PowLayer0Error::PalwUnavailable(msg)) => {
            assert!(msg.contains("PALW_WORKER"), "the refusal must name the knob, got: {msg}")
        }
        Ok(tag) => {
            let simnet_fixture = palw_fixture_l1_tag_v1(&palw_pow_seed_v1(hash(), TS, NONCE, b"simnet"));
            assert_ne!(tag, simnet_fixture, "a real worker answered, but simnet must never be given fixture rules");
        }
        Err(other) => panic!("unexpected simnet outcome: {other:?}"),
    }
}

/// The seed binds the network, so the two tags differ even where both are fixtures — a devnet-3
/// block is not replayable as a devnet block. Cheap to state, and it is the reason confining by
/// network is sound rather than merely tidy: the families do not overlap.
#[test]
fn a_fixture_tag_is_bound_to_the_network_that_asked_for_it() {
    select_the_fixture_tag_family();
    let a = palw_l1_tag(hash(), TS, NONCE, b"devnet").expect("devnet");
    let b = palw_l1_tag(hash(), TS, NONCE, b"devnet-3").expect("suffixed devnet");
    assert_ne!(a, b, "the seed binds the network id");
}
