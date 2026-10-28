// Proposed regression: append INSIDE the existing #[cfg(test)] mod in
// consensus/core/src/palw_rfc0016_v1/capacity.rs (uses its h/attestation helpers).
// Not compiled or executed in this audit. On the inspected source, the source-level
// arithmetic yields 1000 CCU, while joint resources allow at most 3 * 10 = 30 CCU.
#[test]
fn v8_supply_keeps_nonzero_da_and_court_bottlenecks() {
    let mut a = attestation(1, 1, 1_000);
    a.capability = PalwCapacityResourcesV1 {
        compute_ccu_per_span: 1_000,
        memory_bytes: 100,
        da_bytes_per_span: 50,
        court_permille_per_span: 3_000,
    };
    let load = PalwClaimLoadV1 {
        compute_ccu: 10,
        memory_bytes: 100,
        da_bytes: 10,
        court_permille: 1_000,
    };
    let joint_claims = palw_capacity_claims_per_span_v1(&a.capability, &load);
    assert_eq!(joint_claims, 3);
    let supply = palw_capacity_supply_from_attestations_v1(&[a], 500, |_| Some(load));
    let demand: BTreeMap<Hash64, u128> = [(h(1), 10 * load.compute_ccu)].into_iter().collect();
    let allocated = palw_waterfill_capacity_v1(&supply, &demand);
    assert!(allocated[&h(1)] <= joint_claims * load.compute_ccu,
        "court/DA constraints disappeared in the attestation-to-supply conversion");
    assert!(!palw_capacity_fits_v1(allocated[&h(1)], demand[&h(1)]));
}
