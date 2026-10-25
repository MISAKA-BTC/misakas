//! Production disposition/queue ownership controls. Shape-only TIR data below
//! is deliberately not a valid arithmetic allegation or accepted native claim.
use super::*;
use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
use kaspa_consensus_core::{
    palw_backend::{PalwTirOwnStepSourceInfoV1, PalwTirPublicConeReadyV2},
    palw_step::PalwStepCoordinateV1,
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_court_v1::{PalwTirConeRefutationV1, PalwTirParamOpeningV1},
};
use std::sync::atomic::{AtomicUsize, Ordering};
struct Held(Arc<AtomicUsize>);
impl Drop for Held {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
struct SearchSource;
impl PalwTirOwnStepSourceV1 for SearchSource {
    fn info_v1(&self) -> &PalwTirOwnStepSourceInfoV1 {
        panic!("ownership probe has no model result")
    }
    fn node_v1(&self, _: u8, _: u64) -> Result<Hash64, String> {
        Err("unused".into())
    }
    fn frontier_v1(&self, _: u8, _: u64) -> Result<Vec<Hash64>, String> {
        Err("unused".into())
    }
    fn leaf_hashes_v1(&self, _: u64, _: u32) -> Result<Vec<Hash64>, String> {
        Err("unused".into())
    }
    fn row_node_v1(&self, _: u8, _: u64) -> Result<Hash64, String> {
        Err("unused".into())
    }
    fn row_frontier_v1(&self, _: u8, _: u64) -> Result<Vec<Hash64>, String> {
        Err("unused".into())
    }
}
pub(super) fn case() -> (OwnerCaseV1, Arc<PalwMemoryLedgerV1>) {
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(64), || None);
    let memory = ledger
        .reserve(PalwMemoryReservationKeyV1 { role: "cone-case-test", class_id: Default::default(), job: Default::default() }, 64)
        .unwrap();
    let own = OwnedTirReplayV2::open_reserved(memory, || Ok(Arc::new(SearchSource) as Arc<dyn PalwTirOwnStepSourceV1>)).unwrap();
    (
        OwnerCaseV1 {
            claim: Hash64::from_u64_word(1),
            allocation_id: Default::default(),
            source_id: Default::default(),
            suspicion: SuspicionV1::Output,
            own,
            prompt: vec![],
            replay_ids: vec![],
            steps_differ: true,
            acquisition: None,
            units: vec![],
            ids: None,
            harvested: Default::default(),
            leaf: Some(3),
            cone_want: None,
            cone_needs_ids: false,
            demands: 0,
            last_move: None,
            done: false,
        },
        ledger,
    )
}
fn shape_only_ready(held: Arc<AtomicUsize>) -> PalwTirPublicConeReadyV2 {
    let backend = crate::palw_panel::seat_s_tests::floor_backend();
    let (job, _) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
    let context = backend.fp_job_context_v1(&job).unwrap();
    let zero = Hash64::default();
    let binding = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1 {
        version: 1,
        job_context: context,
        class: PalwTirClassV1 {
            version: 1,
            program: vec![],
            layout: PalwTirLayoutV1 {
                version: 1,
                max_context: 1,
                checkpoint_interval: 1,
                h_tile: 1,
                commit_tiles: vec![],
                state_tiles: vec![],
            },
            tokenizer_id: zero,
        },
        artifact_root: zero,
        full_logits_trace_root: zero,
        step_leaf_count: 1,
        step_merkle_root: zero,
        committed_execution_root: zero,
    };
    let proof = PalwCourtVerdictProofV2::TirCone {
        refutation: Box::new(PalwTirConeRefutationV1 {
            binding,
            output_opening: PalwStepOpeningV1 { leaf_index: 0, leaf_hash: zero, siblings: vec![] },
            output_preimage: PalwStepTileLeafV1 {
                version: 1,
                coord: PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
                value_count: 0,
                values_le: vec![],
            },
            operands: kaspa_consensus_core::palw_step_refute::PalwStepInputRowV1 { preimages: vec![], run_siblings: vec![] },
            params: PalwTirParamOpeningV1::None,
            prompt_token_ids: vec![],
            prompt_ids_openings: vec![],
            decode_tokens: None,
        }),
    };
    held.fetch_add(1, Ordering::SeqCst);
    PalwTirPublicConeReadyV2::from_reserved_v2(proof, vec![Box::new(Held(held))], Default::default()).unwrap()
}
#[test]
fn tir_cone_owned_consumer_resource_fault_keeps_case_and_configured_refusal_retires_locally() {
    let (mut case, ledger) = case();
    let mut filer = PalwTirPublicFilerBookV1::default();
    let claim = case.claim;
    for why in
        [PalwTirPublicConeFaultV2::LocalResource("host quota".into()), PalwTirPublicConeFaultV2::Unavailable("read deferred".into())]
    {
        assert!(apply_owner_cone_result_v2(&mut case, &mut filer, claim, 100, Err(why)).is_err());
        assert!(!case.done);
        assert_eq!(ledger.reserved_bytes(), 64);
        assert_eq!(filer.len(), 0);
    }
    apply_owner_cone_result_v2(&mut case, &mut filer, claim, 100, Ok(ConeOutcomeV1::Refused("configured copy cap".into()))).unwrap();
    assert!(case.done);
    assert_eq!(filer.len(), 0);
    drop(case);
    assert_eq!(ledger.reserved_bytes(), 0);
}
#[test]
fn tir_cone_owned_consumer_pending_filer_keeps_actual_output_guards_until_drop() {
    let held = Arc::new(AtomicUsize::new(0));
    let ready = shape_only_ready(held.clone());
    let (mut case, _ledger) = case();
    let claim = case.claim;
    let mut filer = PalwTirPublicFilerBookV1::default();
    apply_owner_cone_result_v2(&mut case, &mut filer, claim, 100, Ok(ConeOutcomeV1::Ready(ready))).unwrap();
    assert!(case.done);
    assert_eq!(filer.len(), 1);
    assert_eq!(held.load(Ordering::SeqCst), 1);
    drop(case);
    assert_eq!(held.load(Ordering::SeqCst), 1);
    drop(filer);
    assert_eq!(held.load(Ordering::SeqCst), 0);
}
#[test]
fn tir_cone_owned_consumer_output_capacity_does_not_mark_done_or_queue() {
    const CHILD: &str = "MISAKA_TIR_CONE_OUTPUT_QUOTA_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let full = format!("{}::tir_cone_owned_consumer_output_capacity_does_not_mark_done_or_queue", module_path!());
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
    let held = Arc::new(AtomicUsize::new(0));
    let ready = shape_only_ready(held.clone());
    let (mut case, ledger) = case();
    let claim = case.claim;
    kaspa_consensus_core::palw_native_resource_v1::arm_native_logical_limits_v1(Some(
        kaspa_consensus_core::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 },
    ));
    let mut filer = PalwTirPublicFilerBookV1::default();
    assert!(matches!(
        apply_owner_cone_result_v2(&mut case, &mut filer, claim, 100, Ok(ConeOutcomeV1::Ready(ready))),
        Err(PalwTirPublicConeFaultV2::LocalResource(_))
    ));
    assert!(!case.done);
    assert_eq!(filer.len(), 0);
    assert_eq!(ledger.reserved_bytes(), 64);
    assert_eq!(held.load(Ordering::SeqCst), 0);
}

#[test]
fn tir_cone_owned_consumer_full_local_queue_keeps_case_then_retries_after_space() {
    let (mut case, ledger) = case();
    case.claim = Hash64::from_u64_word(999);
    case.acquisition = Some((7, Hash64::from_u64_word(8)));
    let claim = case.claim;
    let mut filer = PalwTirPublicFilerBookV1::default();
    for n in 1..=16 {
        assert!(filer.queue_finding_v1(
            Hash64::from_u64_word(n),
            PalwTirPublicConvictionEvidenceV1::Output { rows_root: Hash64::from_u64_word(20), ids: vec![1] },
            100
        ));
    }
    let held = Arc::new(AtomicUsize::new(0));
    let ready = shape_only_ready(held.clone());
    assert!(matches!(
        apply_owner_cone_result_v2(&mut case, &mut filer, claim, 100, Ok(ConeOutcomeV1::Ready(ready))),
        Err(PalwTirPublicConeFaultV2::LocalResource(_))
    ));
    assert!(!case.done);
    assert_eq!(case.acquisition, Some((7, Hash64::from_u64_word(8))));
    assert_eq!(ledger.reserved_bytes(), 64);
    assert_eq!(filer.len(), 16);
    assert_eq!(held.load(Ordering::SeqCst), 0);
    filer.drop_v1(&Hash64::from_u64_word(1), "release one local queue slot");
    let ready = shape_only_ready(held.clone());
    apply_owner_cone_result_v2(&mut case, &mut filer, claim, 101, Ok(ConeOutcomeV1::Ready(ready))).unwrap();
    assert!(case.done);
    assert_eq!(filer.len(), 16);
    assert_eq!(held.load(Ordering::SeqCst), 1);
    drop(filer);
    assert_eq!(held.load(Ordering::SeqCst), 0);
}
