// Shape-only compatibility diagnostic, not accepted-state/funding authority.
use borsh::BorshDeserialize;
use kaspa_consensus_core::{
    palw_kernel_route_v1::{PalwKernelRouteHeaderV1, PalwKernelRouteStateV1},
    palw_native_resource_v1::preflight_native_value_v1,
    palw_security_certification_v1::{PalwClaimCertificationRowV1, PalwKernelCertifiedRewardHoldV1},
};
fn scan(label: &str, prefix: Option<u8>, value: &[u8]) -> bool {
    let header = PalwKernelRouteHeaderV1::deserialize_reader(&mut std::io::Cursor::new([0; 4096])).unwrap();
    let mut route = PalwKernelRouteStateV1::with_header(header);
    if let Some(p) = prefix {
        let mut key = vec![p]; key.extend_from_slice(&[1;64]);
        route.aux.insert((136, key), value.to_vec());
    }
    let bytes = borsh::to_vec(&route).unwrap();
    let result = preflight_native_value_v1::<PalwKernelRouteStateV1>(bytes.as_slice(),bytes.len() as u64);
    println!("{label}: success={} error={:?}",result.is_ok(),result.as_ref().err());
    result.is_ok()
}
fn main() {
    let claim = PalwClaimCertificationRowV1 {
        claim:[1;64],class_id:[2;64],producer_bond:[3;64],producer_principal:[4;64],attempt:true,
        final_daa:1,committed_daa:1,committed_span:1,anchor:None,draws:vec![],drawn_daa:0,
        deadline_daa:10,attestations:vec![],held_reward:1,payout_payload:[5;64],outcome:None,
    };
    let hold=PalwKernelCertifiedRewardHoldV1 { version:1,claim:[1;64],producer_bond:[3;64],
        producer_payload:[5;64],poster_bond:[6;64],poster_payload:[7;64],amount:1,held_daa:1 };
    let bytes=borsh::to_vec(&claim).unwrap();
    assert!(scan("empty control",None,&[]));
    assert!(scan("legacy S",Some(b'S'),&bytes));
    assert!(scan("legacy V",Some(b'V'),&bytes));
    let k=scan("legacy K",Some(b'K'),&bytes);
    let h=scan("legacy H",Some(b'H'),&borsh::to_vec(&hold).unwrap());
    assert!(!k && !h,"this pre-fix diagnostic expects the source-confirmed omission");
}
