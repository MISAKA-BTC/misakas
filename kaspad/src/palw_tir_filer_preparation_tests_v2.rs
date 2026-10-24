//! Opaque fixture controls of the actual preparation/build/enqueue path; no
//! native claim authority or model/court campaign is supplied by this fixture.
use super::*;
use kaspa_consensus_core::{
    palw_state_v2::PalwConsensusObjectV2, palw_tir_public_carriage_v1::*, palw_tir_public_claim_v1::PalwTirPublicFrameViewV1,
};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicUsize, Ordering},
};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(n), 0))
}
fn view() -> PalwTirPublicFrameViewV1 {
    let header = kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
        version: 1,
        job_context: kaspa_consensus_core::palw_v2::PalwJobContextV2 {
            version: 2,
            network_id: b"testnet-12".to_vec(),
            job_id: h(1),
            job_nullifier: h(2),
            assignment_id: h(3),
            execution_seed: [0u8; 32],
            model_profile_id: h(4),
            runtime_manifest_hash: h(5),
            runtime_class_id: h(6),
            shape_profile_id: h(7),
            trace_scheme_id: h(8),
            cu_ruleset_id: h(9),
            tokenizer_id: h(10),
            prompt_token_ids_hash: h(11),
            declared_prefill_tokens: 3,
            exact_decode_tokens: 2,
            max_context_tokens: 64,
        },
        artifact_root: h(12),
        full_logits_trace_root: h(13),
        step_leaf_count: 19,
        step_merkle_root: h(14),
        committed_execution_root: h(15),
    };
    PalwTirPublicFrameViewV1 {
        claim: h(100),
        allocation_id: h(101),
        source_id: h(102),
        producer: bond(1),
        pledge_id: h(103),
        liable: BTreeSet::from([bond(1), bond(2)]),
        drawn_panel: BTreeSet::from([bond(2), bond(3)]),
        accepted_daa: 100,
        collect_until_daa: 200,
        material_until_daa: 300,
        conviction_rent_sompi: 777,
        maximum_witness_bytes: 1 << 20,
        selection: Some(kaspa_consensus_core::palw_tir_court_v1::PalwTirSelectionRuleV1::Keyed {
            sampling: kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
        }),
        owners_opened: 0,
        custodian_load: kaspa_consensus_core::palw_tir_public_claim_v1::PalwTirPublicCustodianLoadV1 {
            owners: 8,
            demands_per_custodian: 64,
            answer_bytes_per_custodian: 1 << 20,
        },
        header,
    }
}

fn output(count: usize) -> PalwTirPublicConvictionEvidenceV1 {
    PalwTirPublicConvictionEvidenceV1::Output { rows_root: h(20), ids: vec![3; count] }
}
fn build(prepared: &PreparedFilingV2, view: &PalwTirPublicFrameViewV1, signs: &AtomicUsize) -> PalwConsensusObjectV2 {
    let sign = |_: &[u8], _: &[u8]| {
        signs.fetch_add(1, Ordering::SeqCst);
        vec![7; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]
    };
    if prepared.staged {
        palw_tir_public_witness_part_object_v1(sign, h(99), view, bond(9), 150, &prepared.encoded, 0).unwrap()
    } else {
        palw_tir_public_conviction_object_v1(sign, h(99), view, bond(9), 150, prepared.evidence.clone()).unwrap()
    }
}
#[test]
fn tir_filer_preparation_staged_uses_exact_bytes_without_widening_ordinary_control() {
    let view = view();
    let signs = AtomicUsize::new(0);
    let evidence = output(PALW_TIR_PUBLIC_CONVICTION_MAX_IDS_V1 + 1);
    assert!(evidence.check_v1().is_err());
    evidence.check_staged_v1().unwrap();
    let prepared = PreparedFilingV2::new(&evidence, &view, bond(9), h(99)).unwrap();
    assert!(prepared.staged);
    assert_eq!(prepared.encoded, borsh::to_vec(&evidence).unwrap());
    assert!(borsh::from_slice::<PalwTirPublicConvictionEvidenceV1>(&prepared.encoded).is_err());
    let object = build(&prepared, &view, &signs);
    let PalwConsensusObjectV2::TirPublicControlV1 { control } = object else { panic!("actual control") };
    control.check_v1().unwrap();
    let PalwTirPublicControlActionV1::WitnessPart(part) = control.action else { panic!("staged control") };
    assert_eq!(part.bytes, prepared.encoded[..PALW_TIR_PUBLIC_WITNESS_PART_BYTES_V1]);
    assert_eq!(part.total_bytes, prepared.encoded.len() as u64);
    assert_eq!(signs.load(Ordering::SeqCst), 1);
    let small = PreparedFilingV2::new(&output(2), &view, bond(9), h(99)).unwrap();
    assert!(!small.staged);
    let object = build(&small, &view, &signs);
    let PalwConsensusObjectV2::TirPublicControlV1 { control } = object else { panic!("actual control") };
    assert!(matches!(control.action, PalwTirPublicControlActionV1::Convict(_)));
    control.check_v1().unwrap();
    let mut short = view;
    short.maximum_witness_bytes = 10;
    assert!(matches!(PreparedFilingV2::new(&evidence, &short, bond(9), h(99)), Err(FilerPreparationFaultV2::Semantic(_))));
    assert_eq!(signs.load(Ordering::SeqCst), 2, "bad frame ceiling never calls signer");
}
const TEST_FILER_BYTES: u64 = 64 << 20;
fn headroom() -> u64 {
    use kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1;
    let (mut low, mut high) = (0, TEST_FILER_BYTES);
    while low < high {
        let n = low + (high - low + 1) / 2;
        match reserve_native_requests_v1(n) {
            Ok(lease) => {
                drop(lease);
                low = n
            }
            Err(_) => high = n - 1,
        }
    }
    low
}
#[test]
fn tir_filer_preparation_capacity_refusal_preserves_finding_and_send_then_retries() {
    const CHILD: &str = "MISAKA_TIR_FILER_PREP_QUOTA_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let full = format!("{}::tir_filer_preparation_capacity_refusal_preserves_finding_and_send_then_retries", module_path!());
        let name = full.split_once("::").unwrap().1;
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    use kaspa_consensus_core::palw_native_resource_v1::*;
    let view = view();
    let mut book = PalwTirPublicFilerBookV1::default();
    assert!(book.queue_finding_v1(view.claim, output(20_001), 100));
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: TEST_FILER_BYTES, database_bytes: 0 }));
    let full = reserve_native_requests_v1(TEST_FILER_BYTES).unwrap();
    let signs = AtomicUsize::new(0);
    assert!(matches!(
        PreparedFilingV2::new(&book.filings[&view.claim].evidence, &view, bond(9), h(99)),
        Err(FilerPreparationFaultV2::Resource(_))
    ));
    assert_eq!(book.len(), 1);
    assert_eq!(book.filings[&view.claim].sends, 0);
    assert_eq!(signs.load(Ordering::SeqCst), 0);
    drop(full);
    let prepared = PreparedFilingV2::new(&book.filings[&view.claim].evidence, &view, bond(9), h(99)).unwrap();
    let object = build(&prepared, &view, &signs);
    let mut outbox = CourtOutboxV1::default();
    let key = (view.claim, PALW_TIR_PUBLIC_CONVICTION_ROUND_V1, false);
    let blocker = reserve_native_requests_v1(headroom()).unwrap();
    assert!(book.enqueue_filing_v2(view.claim, &object, 150, 199, &mut outbox).is_err());
    assert!(outbox.pending().is_empty());
    assert!(outbox.submitted_at(&key).is_none());
    assert_eq!(book.filings[&view.claim].sends, 0);
    drop(blocker);
    book.enqueue_filing_v2(view.claim, &object, 150, 199, &mut outbox).unwrap();
    assert_eq!(outbox.pending().len(), 1);
    assert_eq!(outbox.submitted_at(&key), Some(&150));
    assert_eq!(book.filings[&view.claim].sends, 1);
    assert_eq!(signs.load(Ordering::SeqCst), 1, "destination retry reuses exact signed object");
}
#[tokio::test]
async fn tir_filer_preparation_host_only_signed_copy_survives_retry_and_cancel_until_payload_drop() {
    use crate::palw_memory_ledger::host_ledger_v1;
    assert!(
        kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(0).unwrap().is_none(),
        "host-only control uses native quotas disabled"
    );
    let mut view = view();
    view.claim = h(0xCAFE_7711);
    let prepared = PreparedFilingV2::new(&output(2), &view, bond(9), h(99)).unwrap();
    let signs = AtomicUsize::new(0);
    let object = build(&prepared, &view, &signs);
    let mut book = PalwTirPublicFilerBookV1::default();
    assert!(book.queue_finding_v1(view.claim, output(2), 100));
    let mut outbox = CourtOutboxV1::default();
    book.enqueue_filing_v2(view.claim, &object, 150, 199, &mut outbox).unwrap();
    drop(object);
    drop(prepared);
    let held =
        || host_ledger_v1().snapshot().rows.iter().any(|r| r.key.role == "public-tir-filer-court-payload" && r.key.job == view.claim);
    assert!(held(), "actual destination clone retains host payload/node ownership after source drops");
    let mut retry = outbox.try_retry_buffer().unwrap();
    for packet in outbox.try_take_priority().unwrap() {
        retry.push(packet);
    }
    assert!(held());
    outbox.finish_flush(retry);
    assert!(held());
    let scratch = outbox.try_retry_buffer().unwrap();
    let batch = outbox.try_take_priority().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _actual_batch = batch;
        tx.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    rx.await.unwrap();
    task.abort();
    assert!(matches!(task.await,Err(why) if why.is_cancelled()));
    assert!(held(), "canceled carrier dropped its payload; ticket conservatively survives until flush cleanup");
    drop(scratch);
    let empty = outbox.try_retry_buffer().unwrap();
    assert!(!held(), "next actual flush prunes abandoned payload ticket");
    outbox.finish_flush(empty);
    assert!(outbox.pending().is_empty());
}
