use crate::{
    native_bundle::{MANIFEST_FILE, Manifest, VERSION, action_file},
    native_run::read_bounded,
};
use kaspa_consensus_core::{
    palw_court_service_v1::ServiceCost,
    palw_tokenizer_registration_carriage_v1::{
        TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1, TokenizerRegistrationCandidateWorkV1, TokenizerRegistrationSourceWorkV1,
        scan_tokenizer_registration_work_v1,
    },
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::Transaction,
};
use serde_json::{Value, json};
use std::{path::Path, time::Instant};

fn dimensions(cost: ServiceCost) -> Value {
    json!({"bytes": cost.bytes, "work": cost.work, "io": cost.io, "controls": cost.controls})
}

pub fn run(dir: &Path, source_count: u64) {
    assert!((1..=10_000).contains(&source_count), "measurement source count is out of range");
    let manifest: Manifest = borsh::from_slice(&read_bounded(&dir.join(MANIFEST_FILE), 256 * 1024)).unwrap();
    assert_eq!(manifest.version, VERSION);
    let source_budget = manifest.policy.source_budget_v1().unwrap();
    assert_eq!(source_budget.controls, 1, "this workload fills a one-control source partition");
    let mut policy = manifest.policy;
    let sweep = policy.bookkeeping_work_v1().unwrap().sweep;
    let scale = |per_source: u64| per_source.checked_mul(source_count).unwrap();
    policy.maximum_source_blocks = source_count;
    policy.service_budget = ServiceCost {
        bytes: scale(source_budget.bytes),
        work: scale(source_budget.work).checked_add(sweep).unwrap(),
        io: scale(source_budget.io),
        controls: scale(source_budget.controls),
    };
    assert_eq!(policy.source_budget_v1().unwrap(), source_budget);
    let maximum_finish = policy.maximum_finish_cost_v1().unwrap();
    let policy_borsh_hex: String = borsh::to_vec(&policy).unwrap().iter().map(|byte| format!("{byte:02x}")).collect();

    let mut malformed = TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1.to_vec();
    malformed.push(0xff);
    let unknown_finish = read_bounded(&dir.join(action_file(manifest.snapshot_before as usize)), 96 * 1024);
    assert_eq!(unknown_finish.get(4), Some(&2));
    for (case, payload) in [("malformed_duplicate", malformed), ("unresolved_finish_duplicate", unknown_finish)] {
        let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_LIFECYCLE, 0, payload);
        // This copy has no accepted parent authorization under the scanner's
        // measurement policy. It must still consume the physical fallback.
        let cost = scan_tokenizer_registration_work_v1(&tx, &policy, |_| None).unwrap().unwrap().cost;
        assert_eq!(cost, maximum_finish);

        let start = Instant::now();
        for _ in 0..source_count {
            let mut source = TokenizerRegistrationSourceWorkV1::new(policy).unwrap();
            source.charge_v1(&tx).unwrap();
            assert_eq!(source.used_v1(), cost);
            assert!(source.charge_v1(&tx).is_err());
            assert_eq!(source.used_v1(), cost);
        }
        let source_check_ns = start.elapsed().as_nanos();

        let start = Instant::now();
        let mut candidate = TokenizerRegistrationCandidateWorkV1::new(policy, true).unwrap();
        for _ in 0..source_count {
            candidate.charge_v1(&tx, |_| None).unwrap();
        }
        let total = candidate.used_v1();
        assert_eq!(
            total,
            ServiceCost {
                bytes: scale(cost.bytes),
                work: scale(cost.work).checked_add(sweep).unwrap(),
                io: scale(cost.io),
                controls: scale(cost.controls),
            }
        );
        assert!(candidate.charge_v1(&tx, |_| None).is_err());
        assert_eq!(candidate.used_v1(), total);
        let candidate_check_ns = start.elapsed().as_nanos();

        println!(
            "{}",
            json!({
                "mode": "physical_scanner_only", "case": case, "bundleCase": manifest.case,
                "policyId": policy.id_v1().unwrap().to_string(), "policyBorshHex": policy_borsh_hex,
                "modeledSources": source_count,
                "physicalCarriersCharged": source_count, "parserInvocations": 0,
                "sourceBudget": dimensions(source_budget), "carrierCost": dimensions(cost),
                "candidateBudget": dimensions(policy.service_budget), "candidateUsed": dimensions(total),
                "sourceCheckNsIncludingRejectedDuplicate": source_check_ns,
                "candidateCheckNsIncludingRejectedOverflow": candidate_check_ns,
            })
        );
    }
}
