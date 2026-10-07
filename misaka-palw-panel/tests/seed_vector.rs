use kaspa_hashes::Hash64;
use misaka_palw_panel::{BondIdV1, ClaimSealV1, PanelSnapshotV1, panel_seed_v3};

#[test]
fn seed_v3_matches_independently_encoded_python_blake2b_vector() {
    let h = |v| Hash64::from_bytes([v; 64]);
    let seal = ClaimSealV1 {
        id: h(3),
        accepted_block: h(0),
        accepted_daa: 0,
        accepted_height: 0,
        acceptance_order: 0,
        occurrence_index: 0,
        checkpoint: h(0),
        checkpoint_daa: 0,
        checkpoint_height: 0,
        anchor_slot: 5,
        beacon_epoch: 6,
    };
    let snapshot = PanelSnapshotV1 {
        root: h(4),
        checkpoint: h(0),
        checkpoint_daa: 0,
        checkpoint_height: 0,
        policy_id: h(0),
        class_id: h(0),
        excluded_bond: BondIdV1 { transaction: h(0), index: 0 },
        excluded_operator: h(0),
        excluded_key: h(0),
        candidates: vec![],
    };
    // Independently generated with hashlib.blake2b(struct.pack('<I', len(domain)) + domain +
    // bytes([1])*64 + bytes([2])*64 + bytes([3])*64 + struct.pack('<Q', 5) + bytes([4])*64 +
    // struct.pack('<Q', 6) + bytes([7])*64, digest_size=64), without Borsh or the Rust helper.
    assert_eq!(
        panel_seed_v3(h(1), h(2), &seal, &snapshot, h(7)).to_string(),
        "8fe01aa1a74402b07e2932468fe023fa3a171849b3faf35229feef5f6e003f23e7c15042640240b1f1ba0c843eba936cbb2a8a0b9fa338dfb6e9864ad81587f9"
    );
    assert_ne!(panel_seed_v3(h(1), h(2), &seal, &snapshot, h(7)), panel_seed_v3(h(2), h(1), &seal, &snapshot, h(7)));
}
