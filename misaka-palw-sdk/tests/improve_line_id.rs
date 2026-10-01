//! **The id the drill harness derives for a line before the chain exists** (`audit-improve/dmdrive.py`, `model_line_id`): a drill's
//! lying executor is told which line to spoil by a prefix of the line's id, and the id is a keyed hash of the class, the founding bond and
//! the name (`model_line_id_v1`) — all known before the first block. This pins the derivation the harness mirrors, against values the
//! harness's own implementation produced.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_model_lines_v1::model_line_id_v1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;

#[test]
fn the_line_id_the_harness_derives_offline_is_the_chains() {
    let class = Hash64::from_bytes([0xAB; 64]);
    let bond = |index| PalwBondKeyV2(TransactionOutpoint { transaction_id: Hash64::from_bytes([7; 64]), index });
    assert_eq!(
        model_line_id_v1(&class, &bond(1), b"T").to_string(),
        "17131be198686c865fff3483f36c895a686bd10d8d92c0a894920e2a7923450c89f8e8061afefc2b29ab5667516311de410d614e2811d1379e11cee4fa89dacd"
    );
    assert_eq!(
        model_line_id_v1(&class, &bond(3), b"W2").to_string(),
        "7d1e43e6c6cc6dadc5b93e262bf559252feacb19508696972ffba06980aa28975049a00df7c2626ee6d03eff66453e46a71c1782983fd178a3e2b721737546b2"
    );
}
