//! Raw-data/body separation tests. The duty below is a local test capability,
//! not a substitute for A4 accepted native history or a ServiceReceipt.
use super::*;
use misaka_palw_positive_contract::identity::ResultId;
use std::{cell::Cell, collections::BTreeMap};

struct QuotedDuty {
    result: ResultId,
    request: SourceRequest,
    bounds: DutyBoundsV2,
    units: BTreeMap<u64, SourceUnitBoundsV2>,
    status_calls: Cell<u64>,
    bound_calls: Cell<u64>,
}
impl CanonicalDataDutyV2 for QuotedDuty {
    fn request(&self) -> &SourceRequest {
        &self.request
    }
    fn authorize(&self, result: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2> {
        if result.id() != self.result {
            return Err(MinerErrorV2::Binding);
        }
        Ok(self.bounds)
    }
    fn status(&self, index: u64) -> Result<MinerDataStatus, MinerErrorV2> {
        self.status_calls.set(self.status_calls.get() + 1);
        Ok(if (self.request.first..self.request.end).contains(&index) {
            MinerDataStatus::Pending
        } else {
            MinerDataStatus::NotRequested
        })
    }
    fn unit_bounds(&self, index: u64) -> Result<Option<SourceUnitBoundsV2>, MinerErrorV2> {
        self.bound_calls.set(self.bound_calls.get() + 1);
        self.units.get(&index).copied().map(Some).ok_or(MinerErrorV2::Binding)
    }
}
struct CountedProvider<'a> {
    inner: &'a dyn StoredProviderV2,
    reads: Cell<u64>,
}
impl StoredProviderV2 for CountedProvider<'_> {
    fn unit(&self, kind: SourceKind, source: Digest, index: u64) -> Result<&StoredUnitV2, String> {
        self.reads.set(self.reads.get() + 1);
        self.inner.unit(kind, source, index)
    }
}
fn limits() -> ProcessLimitsV2 {
    ProcessLimitsV2 {
        max_units_per_request: 8,
        max_tracked_units: 8,
        max_provider_reads: 8,
        max_status_checks: 32,
        max_prepared_bytes: u64::MAX,
    }
}
fn fixture(unique: u64, body: u64) -> super::tests::Fixture {
    let mut f = super::tests::Fixture::new();
    f.contract.budget.unique_source_bytes = unique;
    f.contract.budget.transmitted_bytes = body;
    f.contract.budget.retained_bytes = body;
    f.result.service_contract = f.contract.id();
    let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
    f.signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &f.result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    f
}
fn duty(f: &super::tests::Fixture, kind: SourceKind, first: u64, units: &[SourceUnitBoundsV2]) -> QuotedDuty {
    QuotedDuty {
        result: f.result.id(),
        request: f.request(kind, first, first + units.len() as u64),
        bounds: DutyBoundsV2 {
            source_units: 3,
            max_unit_bytes: units.iter().map(|x| x.body_bytes).max().unwrap(),
            work_per_read: 5,
            budget: f.contract.budget,
            now: 1,
            cutoff: 90,
        },
        units: units.iter().copied().enumerate().map(|(i, q)| (first + i as u64, q)).collect(),
        status_calls: Cell::new(0),
        bound_calls: Cell::new(0),
    }
}
fn responder<'a>(f: &super::tests::Fixture, providers: &'a [&'a dyn StoredProviderV2]) -> MinerResponderV2<'a, QuotedDuty> {
    MinerResponderV2::new(&f.admission, f.result.clone(), f.contract, 1, &f.key, &f.signature, providers, limits()).unwrap()
}

#[test]
fn exact_raw_budget_accepts_opening_overhead_without_charging_it_as_unique_data() {
    let f = fixture(16, 178);
    let counted = CountedProvider { inner: &f.provider, reads: Cell::new(0) };
    let providers: [&dyn StoredProviderV2; 1] = [&counted];
    // Existing fixture: 16 raw bytes + 34 bytes coordinate framing + 2*64 path.
    let duty = duty(&f, SourceKind::ArtifactInventory, 0, &[SourceUnitBoundsV2 { data_bytes: 16, body_bytes: 178 }]);
    let mut miner = responder(&f, &providers);
    let reply = miner.respond(&duty).unwrap();
    assert_eq!(reply.units.len(), 1);
    assert_eq!(miner.meter().unique_source_bytes, 16);
    assert_eq!(miner.meter().prepared_bytes, 178);
    assert_eq!(counted.reads.get(), 1);
    assert_eq!(duty.status_calls.get(), 1);
    assert_eq!(duty.bound_calls.get(), 1);
    let before = miner.meter();
    // Explicit retry reuses the same union but still needs new body allowance.
    assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Budget));
    assert_eq!(counted.reads.get(), 1);
    assert_eq!(miner.meter().unique_source_bytes, before.unique_source_bytes);
}

#[test]
fn ragged_prompt_uses_per_unit_raw_and_body_sums_instead_of_maximum_times_count() {
    let f = fixture(260, 908);
    let counted = CountedProvider { inner: &f.provider, reads: Cell::new(0) };
    let providers: [&dyn StoredProviderV2; 1] = [&counted];
    let quotes = [
        SourceUnitBoundsV2 { data_bytes: 128, body_bytes: 344 },
        SourceUnitBoundsV2 { data_bytes: 128, body_bytes: 344 },
        SourceUnitBoundsV2 { data_bytes: 4, body_bytes: 220 },
    ];
    let duty = duty(&f, SourceKind::Input, 0, &quotes);
    let mut miner = responder(&f, &providers);
    let reply = miner.respond(&duty).unwrap();
    assert_eq!(reply.units.len(), 3);
    assert_eq!(miner.meter().unique_source_bytes, 260);
    assert_eq!(miner.meter().prepared_bytes, 844); // promoted ragged tail uses one actual sibling.
    assert_eq!(counted.reads.get(), 3);
    assert_eq!(duty.status_calls.get(), 3);
    assert_eq!(duty.bound_calls.get(), 3);
}

#[test]
fn forged_exact_raw_or_body_bounds_cannot_release_larger_or_different_data() {
    for quote in [
        SourceUnitBoundsV2 { data_bytes: 15, body_bytes: 178 },
        SourceUnitBoundsV2 { data_bytes: 17, body_bytes: 178 },
        SourceUnitBoundsV2 { data_bytes: 16, body_bytes: 17 },
    ] {
        let f = fixture(32, 256);
        let counted = CountedProvider { inner: &f.provider, reads: Cell::new(0) };
        let providers: [&dyn StoredProviderV2; 1] = [&counted];
        let duty = duty(&f, SourceKind::ArtifactInventory, 0, &[quote]);
        let mut miner = responder(&f, &providers);
        assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Unavailable));
        assert_eq!(miner.meter().unique_source_bytes, 0);
        assert_eq!(miner.meter().prepared_bytes, 0);
        assert_eq!(counted.reads.get(), 1);
    }
    let f = fixture(32, 256);
    let counted = CountedProvider { inner: &f.provider, reads: Cell::new(0) };
    let providers: [&dyn StoredProviderV2; 1] = [&counted];
    let duty = duty(&f, SourceKind::ArtifactInventory, 0, &[SourceUnitBoundsV2 { data_bytes: 16, body_bytes: 15 }]);
    let mut miner = responder(&f, &providers);
    assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Binding));
    assert_eq!(counted.reads.get(), 0);
}

#[test]
fn raw_and_body_quote_overflow_is_refused_before_any_provider_read() {
    for raw_overflow in [false, true] {
        let f = fixture(u64::MAX, u64::MAX);
        let counted = CountedProvider { inner: &f.provider, reads: Cell::new(0) };
        let providers: [&dyn StoredProviderV2; 1] = [&counted];
        let one = SourceUnitBoundsV2 { data_bytes: if raw_overflow { u64::MAX } else { 1 }, body_bytes: u64::MAX };
        let duty = duty(&f, SourceKind::ArtifactInventory, 0, &[one, one]);
        let mut miner = responder(&f, &providers);
        assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Budget));
        assert_eq!(counted.reads.get(), 0);
        assert_eq!(duty.status_calls.get(), 2);
        assert_eq!(duty.bound_calls.get(), 2);
        assert_eq!(miner.meter().unique_source_bytes, 0);
        assert_eq!(miner.meter().prepared_bytes, 0);
    }
}
