//! Local producer-generated test archive only; NOT a new peer codec/native row.
//! The complete claims record is never materialized in RAM in these tests.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    palw_logits_q24_v2::WideLogitsEventV2,
    palw_positive_check_v2::full_relation::wide_v2::{StepTargetV2, stream::*},
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1},
    palw_step_refute::PalwStepInputRowV1,
    palw_tir_court_v1::{
        PalwTirParamOpeningV1,
        source_only::wide_v2::{TirSourceOnlyMaterialV2, WideGeneratedIdsPinV2},
    },
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

// These are local test files made by this test's producer. Generic Borsh below
// is deliberately not exported as a production untrusted material decoder.
#[derive(BorshSerialize, BorshDeserialize)]
struct Frame {
    operands: PalwStepInputRowV1,
    params: PalwTirParamOpeningV1,
    prompt: Vec<u32>,
    pin: Option<(Hash64, Vec<u32>)>,
    target: Option<(PalwStepOpeningV1, PalwStepTileLeafV1, Option<WideLogitsEventV2>)>,
}
const FRAME_CAP: usize = 1 << 20;
const ARCHIVE_CAP: u64 = 1 << 24;
fn store(file: &mut File, frame: &Frame) {
    let bytes = borsh::to_vec(frame).unwrap();
    assert!(bytes.len() <= FRAME_CAP);
    assert!(file.stream_position().unwrap() + 4 + bytes.len() as u64 <= ARCHIVE_CAP);
    file.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
    file.write_all(&bytes).unwrap();
}
fn load(file: &mut File) -> Frame {
    let mut count = [0; 4];
    file.read_exact(&mut count).unwrap();
    let n = u32::from_le_bytes(count) as usize;
    assert!(n <= FRAME_CAP);
    let mut bytes = vec![0; n];
    file.read_exact(&mut bytes).unwrap();
    borsh::from_slice(&bytes).unwrap()
}
fn stream_limits() -> StreamRecordLimitsV2 {
    StreamRecordLimitsV2 {
        max_record_bytes: 1 << 28,
        max_claim_values: 1 << 14,
        max_read_bytes: 1 << 30,
        max_read_calls: 1 << 24,
        max_extra_heap_bytes: 1 << 31,
        metadata_auth_bytes: 1 << 29,
        signature_workspace_bytes: 1 << 20,
        external_retained_bytes: 1 << 28,
    }
}
struct RunGuard(Arc<AtomicUsize>);
impl Drop for RunGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[test]
fn detached_disk_claims_and_materials_finish_real_node_after_producer_run_is_released() {
    for window in [1, 3] {
        let live = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(AtomicUsize::new(0));
        let count = live.clone();
        let seen_inner = seen.clone();
        let reservation: TirMemoryReserverV1 = Arc::new(move |role, _, _, bytes| {
            assert!(bytes > 0);
            if role == "positive-wide-panel-capture-v2" || role == "tir-wide-q24-v2-required" {
                count.fetch_add(1, Ordering::SeqCst);
                seen_inner.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(RunGuard(count.clone())) as Box<dyn Send + Sync>)
            } else {
                Ok(Box::new(()) as Box<dyn Send + Sync>)
            }
        });
        // Independent export/preparation/archive owners acquired BEFORE copies.
        // This test guard covers bounded local archive serialization buffers,
        // metadata clones, held parameter inventory and one loaded frame.
        let archive_owner =
            reservation("test-only-material-archive", Hash64::from_u64_word(0), Hash64::from_u64_word(0), 1 << 28).unwrap();
        let archive_disk_owner = disk_reserve()(Hash64::from_u64_word(0), Hash64::from_u64_word(0), ARCHIVE_CAP).unwrap();
        let mut detached = None;
        run_wide_common::with_fixture(window, |f, _| {
            let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
            let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.class.class_id(&f.artifact)).unwrap();
            let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
            let capture = capture_one_pass_v2(
                CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
                &runner,
                run_bounds(),
                &work,
                tempfile::tempfile().unwrap(),
                &reservation,
            )
            .unwrap();
            let signed = SignedMiner::new(f, false);
            let result = signed.authenticate();
            let prep = signed.preparation(f);
            let inventory = TirHeldInventoryV1::build(f.program, &TirParamsSourceV1(f.params)).unwrap();
            let builder =
                WidePanelBuilderV2::prepare(&capture, &prep, &result, &signed.rooted, &inventory, provider(), owners(), &reservation)
                    .unwrap();
            let mut tape = builder.claims_tape(tempfile::tempfile().unwrap()).unwrap();
            let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([96; 32]);
            let record = tape
                .freeze_record(
                    &prep,
                    capture.binding().unwrap(),
                    capture.run().generated().to_vec(),
                    principal(panel.verification_key.as_ref()),
                    stream_limits(),
                    &reservation,
                )
                .unwrap();
            let signature =
                *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
                    .unwrap()
                    .as_ref();
            tape.rewind();
            let node_reserver = |bytes| {
                reservation("test-stream-node", f.job.shape_profile_id, f.job.context_hash(), bytes)
                    .map_err(|_| FullRelationError::Budget)
            };
            let node = signed
                .preparation(f)
                .begin_stream(
                    record.binding().clone(),
                    record.generated().to_vec(),
                    record.author(),
                    record.signing_digest(),
                    tape,
                    *panel.verification_key.as_ref(),
                    signature,
                    stream_limits(),
                    &node_reserver,
                )
                .unwrap();
            // Material proofs are generated from this SAME observed run, then
            // bounded-spooled once. Node does not retain a model or a builder.
            let mut archive = tempfile::tempfile().unwrap();
            let mut frontier = 0;
            for i in 0..prep.plan().summary().relations {
                let relation = prep.plan().relation_at(i).unwrap();
                assert_eq!(relation.legacy_frontier, frontier);
                let material = builder.material(i, frontier).unwrap();
                let m = material.as_borrowed();
                let t = material.target();
                store(
                    &mut archive,
                    &Frame {
                        operands: m.operands.clone(),
                        params: m.params.clone(),
                        prompt: m.prompt_token_ids.to_vec(),
                        pin: m.decode_tokens.map(|p| (p.rows_root, p.generated_token_ids.clone())),
                        target: t.map(|t| (t.opening.clone(), t.preimage.clone(), t.trace.cloned())),
                    },
                );
                if matches!(relation.kind, ExpandedRelationKind::Legacy(_)) {
                    frontier += 1;
                }
            }
            assert!(archive.metadata().unwrap().len() <= ARCHIVE_CAP);
            archive.seek(SeekFrom::Start(0)).unwrap();
            detached = Some((node, archive));
            assert!(live.load(Ordering::SeqCst) > 0);
        });
        assert_eq!(seen.load(Ordering::SeqCst), 2);
        assert_eq!(live.load(Ordering::SeqCst), 0, "CPU run/capture leases must be released before Node checking");
        let (mut node, mut archive) = detached.unwrap();
        let n = node.preparation().plan().summary().relations;
        for i in 0..n {
            let frame = load(&mut archive);
            let pin = frame.pin.as_ref().map(|p| WideGeneratedIdsPinV2 { rows_root: p.0, generated_token_ids: p.1.clone() });
            let material = TirSourceOnlyMaterialV2 {
                operands: &frame.operands,
                params: &frame.params,
                prompt_token_ids: &frame.prompt,
                prompt_ids_openings: &[],
                decode_tokens: pin.as_ref(),
            };
            let target = frame.target.as_ref().map(|t| StepTargetV2 { opening: &t.0, preimage: &t.1, trace: t.2.as_ref() });
            let step = node.submit(&material, target).unwrap();
            assert_eq!(node.checked_relations(), i + 1);
            assert_eq!(step, if i + 1 == n { StreamSubmitV2::NeedEof } else { StreamSubmitV2::Continue });
        }
        let mut eof = [0];
        assert_eq!(archive.read(&mut eof).unwrap(), 0);
        assert_eq!(node.finish().unwrap(), CheckOutcome::Matched);
        assert_eq!(node.read_meter().relations_read, n);
        assert_eq!(node.finish(), Err(FullRelationError::Closed));
        drop(archive_owner);
        drop(archive_disk_owner);
    }
}
