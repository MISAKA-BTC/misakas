//! ADR-0044 FP-08: the finding docs/palw-fp-wiring-atomicity.md records, measured rather than
//! asserted — both V2-lineage algo ids are absent from the finalizer today, so a network
//! demanding one cannot validate a block, and `skip_proof_of_work` does not change that (the
//! error returns before the skip is consulted).
//!
//! When Unit A / Unit B land, THIS test is the one that must change — deliberately, in the same
//! commit as the arm it describes.

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::pow_layer0::{
    PowLayer0Error, POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3,
};
use kaspa_hashes::Hash64;

fn header_with_algo(algo_id: u8) -> Header {
    let mut header = Header::from_precomputed_hash(Hash64::from_u64_word(0xB10C), vec![Hash64::from_u64_word(0xBEEF)]);
    header.pow_algo_id = algo_id;
    header
}

/// Both V2-lineage ids reach the finalizer's unknown-id arm — a returned error, never a panic
/// (the remote-crash P0's fix), and never a silent pass.
#[test]
fn the_v2_lineage_algo_ids_have_no_finalizer_arm_yet() {
    for algo in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3] {
        let header = header_with_algo(algo);
        let state = kaspa_pow::StateLayer0::new(&header, b"fp-probe");
        let result = state.check_pow_layer0(header.nonce);
        assert_eq!(
            result.unwrap_err(),
            PowLayer0Error::UnknownAlgoId(algo),
            "algo {algo} must answer UnknownAlgoId until its arm lands with its whole unit"
        );
    }

    // …while a shipped id still computes, so this is a statement about the two new ids and not
    // about a broken finalizer.
    let header = header_with_algo(POW_ALGO_ID_KHEAVYHASH);
    let state = kaspa_pow::StateLayer0::new(&header, b"fp-probe");
    assert!(state.check_pow_layer0(header.nonce).is_ok(), "the hash lane still computes");
}

/// The block-level entry maps that error to a FAILED PoW rather than a panic — which is what
/// makes the absence safe to ship, and what a wiring PR must preserve for unknown ids.
#[test]
fn an_absent_arm_is_a_failed_pow_not_a_panic() {
    for algo in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3, 200u8] {
        let header = header_with_algo(algo);
        let (level, passed) = kaspa_pow::calc_block_level_check_pow_layer0(&header, b"fp-probe", 255);
        assert!(!passed, "algo {algo} must not pass PoW");
        assert_eq!(level, 0, "an unverifiable header carries no block level");
    }
}
