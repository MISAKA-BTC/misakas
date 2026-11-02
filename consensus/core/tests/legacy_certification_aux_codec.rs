//! Legacy codec compatibility only; no new P subject or accepted-state authority.
use borsh::BorshDeserialize;
use kaspa_consensus_core::{
    palw_kernel_route_v1::{PalwKernelRouteHeaderV1, PalwKernelRouteStateV1},
    palw_native_resource_v1::{NativeHeapV1, preflight_native_value_v1},
    palw_security_certification_v1::*,
};
fn claim() -> PalwClaimCertificationRowV1 {
    PalwClaimCertificationRowV1 {
        claim: [1; 64],
        class_id: [2; 64],
        producer_bond: [3; 64],
        producer_principal: [4; 64],
        attempt: true,
        final_daa: 1,
        committed_daa: 1,
        committed_span: 1,
        anchor: Some((2, [8; 64])),
        draws: vec![[9; 64]; 3],
        drawn_daa: 2,
        deadline_daa: 10,
        attestations: vec![],
        held_reward: 1,
        payout_payload: [5; 64],
        outcome: None,
    }
}
fn hold() -> PalwKernelCertifiedRewardHoldV1 {
    PalwKernelCertifiedRewardHoldV1 {
        version: PALW_KERNEL_CERTIFIED_REWARD_HOLD_VERSION_V1,
        claim: [1; 64],
        producer_bond: [3; 64],
        producer_payload: [5; 64],
        poster_bond: [6; 64],
        poster_payload: [7; 64],
        amount: 1,
        held_daa: 1,
    }
}
fn key(prefix: u8) -> Vec<u8> {
    let mut k = vec![prefix];
    k.extend_from_slice(&[1; 64]);
    k
}
fn route(k: Vec<u8>, value: Vec<u8>) -> PalwKernelRouteStateV1 {
    let header = PalwKernelRouteHeaderV1::deserialize_reader(&mut std::io::Cursor::new([0; 4096])).unwrap();
    let mut r = PalwKernelRouteStateV1::with_header(header);
    r.aux.insert((PALW_SECURITY_CERTIFICATION_TABLE_V1, k), value);
    r
}
fn accepts(k: Vec<u8>, value: Vec<u8>) -> bool {
    let r = route(k, value);
    let bytes = borsh::to_vec(&r).unwrap();
    preflight_native_value_v1::<PalwKernelRouteStateV1>(bytes.as_slice(), bytes.len() as u64).is_ok()
}
#[test]
fn existing_s_v_k_h_rows_roundtrip_without_changing_their_canonical_bytes() {
    let c = borsh::to_vec(&claim()).unwrap();
    let h = borsh::to_vec(&hold()).unwrap();
    assert_eq!(h.len(), 338);
    for (prefix, value) in [(b'S', c.clone()), (b'V', c.clone()), (b'K', c), (b'H', h)] {
        let r = route(key(prefix), value);
        let bytes = borsh::to_vec(&r).unwrap();
        let q = preflight_native_value_v1::<PalwKernelRouteStateV1>(bytes.as_slice(), bytes.len() as u64).unwrap();
        assert!(q.logical_requests_v1() >= r.native_heap_v1("legacy row shape").bytes().unwrap());
        let decoded: PalwKernelRouteStateV1 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded, r);
        assert_eq!(borsh::to_vec(&decoded).unwrap(), bytes);
    }
    assert_eq!(hold().native_heap_v1("fixed legacy hold").bytes(), Some(0));
}
#[test]
fn malformed_key_extent_unknown_p_and_noncanonical_h_bodies_still_refuse() {
    let c = borsh::to_vec(&claim()).unwrap();
    let h = borsh::to_vec(&hold()).unwrap();
    for prefix in [b'P', b'k', b'h', 0] {
        assert!(!accepts(key(prefix), c.clone()));
    }
    for prefix in [b'K', b'H'] {
        for n in [0, 1, 64, 66] {
            let mut k = key(prefix);
            k.resize(n, 0);
            assert!(!accepts(k, c.clone()));
        }
    }
    for n in 0..h.len() {
        assert!(!accepts(key(b'H'), h[..n].to_vec()));
    }
    let mut extra = h.clone();
    extra.push(0);
    assert!(!accepts(key(b'H'), extra));
    let mut extra = c.clone();
    extra.push(0);
    assert!(!accepts(key(b'K'), extra));
    let mut unknown = hold();
    unknown.version += 1;
    assert!(!accepts(key(b'H'), borsh::to_vec(&unknown).unwrap()));
    assert!(!accepts(key(b'H'), c));
    assert!(!accepts(key(b'K'), h));
}

#[test]
fn impossible_nested_k_draw_count_is_rejected_by_streaming_preflight() {
    let mut c = borsh::to_vec(&claim()).unwrap();
    // Four identities, bool, three clocks, Some(span, anchor), then draw count.
    let count_offset = 4 * 64 + 1 + 3 * 8 + 1 + 8 + 64;
    assert_eq!(u32::from_le_bytes(c[count_offset..count_offset + 4].try_into().unwrap()), 3);
    c[count_offset..count_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(!accepts(key(b'K'), c));
}
