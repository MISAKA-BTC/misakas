//! Signer-boundary controls over real public source-session constructors.
//! Float identity fixtures do not assert Float arithmetic or native acceptance.
use super::*;
use kaspa_consensus_core::{
    Hash64,
    palw_backend::PalwExecutionBackendV1,
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_flat_constraint_stream_v1::{
        PalwFlatConstraintActionV1, PalwFlatConstraintSourceAnswerV1, PalwFlatConstraintSourceSessionV1,
    },
    palw_float_constraint_source_v1::{
        PalwFloatConstraintSourceSessionV1, PalwFloatSourceActionV1, PalwFloatSourceAnswerV1, PalwFloatSourceCursorV1,
    },
    palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
    palw_staged_bundle_v1::PalwSelectedSourceContextV1,
};
use std::{cell::Cell, collections::BTreeSet};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(n), 0))
}
fn binding() -> (kaspa_consensus_core::palw_step_leg::PalwStepBindingV2, kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3)
{
    let backend = crate::palw_panel::seat_s_tests::floor_backend();
    let (job, prompt) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
    let run = backend.execute_free_prompt(&job, &prompt.into_iter().map(|id| id as usize).collect::<Vec<_>>()).unwrap();
    (misaka_palw_base0::produce::base0_material_decode_v1(&run.outcome.material).unwrap().0, job)
}
fn flat() -> (Flat, Move, PalwBondKeyV2) {
    let (binding, job) = binding();
    let signer = bond(1);
    let owner = bond(9);
    let source = PalwFlatConstraintSourceSessionV1::new_v1(
        h(10),
        owner,
        100,
        h(11),
        signer,
        BTreeSet::from([signer]),
        &binding,
        0,
        None,
        None,
        100_000,
        200_000,
        PalwStagedPublicCourtConfigV1::network_v1().prices,
    )
    .unwrap();
    let answer = PalwFlatConstraintSourceAnswerV1 {
        answerer: signer,
        source_id: source.source_id,
        cursor: source.slot_cursor_v1(&signer).unwrap(),
        action: PalwFlatConstraintActionV1::RowBegin { row: 0, width: binding.shape_profile.vocab_size },
    };
    (
        Flat {
            snapshot_block: h(12),
            now_daa: 101,
            session: Box::new(source).into(),
            context: Box::new(PalwSelectedSourceContextV1 { binding: Box::new(binding), job: Box::new(job) }).into(),
        },
        Move {
            version: 1,
            claim: h(10),
            reserver: owner,
            reserved_daa: 100,
            seed_id: h(11),
            phase: PalwConstraintStreamPhaseV1::PrefixCourt,
            action: Action::FlatSourceAnswer(answer),
            signature: vec![],
        },
        signer,
    )
}
#[test]
fn response_presign_flat_actual_cursor_transition_and_deadline_gate_signer() {
    let (old, movement, signer) = flat();
    let calls = Cell::new(0);
    let sign = || {
        calls.set(calls.get() + 1);
        Some(vec![1])
    };
    let mut fresh = old.clone();
    fresh.now_daa += 1;
    fresh.snapshot_block = h(13);
    assert!(sign_current_v1(Ok(Some(fresh.clone())), |p| flat_current_v1(&old, p, &movement, signer), sign).unwrap().is_some());
    assert_eq!(calls.get(), 1);
    let mut source = (*fresh.session).clone();
    let Action::FlatSourceAnswer(answer) = &movement.action else { unreachable!() };
    source.answer_v1(fresh.now_daa, answer).unwrap();
    fresh.session = Box::new(source).into();
    assert!(sign_current_v1(Ok(Some(fresh)), |p| flat_current_v1(&old, p, &movement, signer), sign).unwrap().is_none());
    for changed in 0..6 {
        let mut fresh = old.clone();
        match changed {
            0 => fresh.now_daa = fresh.session.response_deadline_v1(&signer).unwrap(),
            1 => {
                let mut s = (*fresh.session).clone();
                s.source_id = h(80);
                fresh.session = Box::new(s).into();
            }
            2 => {
                let mut s = (*fresh.session).clone();
                s.owner = bond(80);
                fresh.session = Box::new(s).into();
            }
            3 => {
                let mut s = (*fresh.session).clone();
                s.bundle_id = h(80);
                fresh.session = Box::new(s).into();
            }
            4 => {
                let mut c = (*fresh.context).clone();
                c.job.job_nonce[0] ^= 1;
                fresh.context = Box::new(c).into();
            }
            _ => fresh.now_daa = old.now_daa - 1,
        }
        assert!(sign_current_v1(Ok(Some(fresh)), |p| flat_current_v1(&old, p, &movement, signer), sign).unwrap().is_none());
    }
    assert_eq!(calls.get(), 1, "superseded prepared packets never reach actual signer callback");
    assert!(sign_current_v1::<Flat>(Ok(None), |_| panic!("no native state"), sign).unwrap().is_none());
    assert_eq!(
        sign_current_v1::<Flat>(Err("native read/capacity unavailable".into()), |_| panic!("read fault is not semantic"), sign)
            .unwrap_err(),
        "native read/capacity unavailable"
    );
    assert_eq!(calls.get(), 1);
}
#[test]
fn response_presign_flat_movement_scope_and_literal_actor_are_not_replaceable() {
    let (old, movement, signer) = flat();
    for change in 0..5 {
        let mut m = movement.clone();
        match change {
            0 => m.claim = h(70),
            1 => m.reserver = bond(70),
            2 => m.reserved_daa += 1,
            3 => m.seed_id = h(70),
            _ => {
                let Action::FlatSourceAnswer(a) = &mut m.action else { unreachable!() };
                a.answerer = bond(70);
            }
        }
        assert!(!flat_current_v1(&old, &old, &m, signer));
    }
    assert!(!flat_current_v1(&old, &old, &movement, movement.reserver), "voluntary outside owner is not literal paid custody");
}
#[test]
fn response_presign_float_constructor_scope_binding_and_exclusive_clock() {
    use kaspa_consensus_core::{palw_step_refute::PalwDecodeTokensV1, palw_v2::*};
    let (mut binding, mut job) = binding();
    // Recommit a Float identity fixture using public binding hashing; no
    // Float arithmetic/approval or migration of the original integer claim.
    binding.shape_profile.lane = kaspa_consensus_core::palw_step::PalwStepLaneV1::Float32;
    binding.job_context.shape_profile_id = binding.shape_profile.shape_profile_id();
    job.class_id = binding.job_context.shape_profile_id;
    binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
    let signer = bond(1);
    let owner = bond(9);
    let source = PalwFloatConstraintSourceSessionV1::new_v1(
        h(10),
        owner,
        100,
        h(11),
        signer,
        BTreeSet::from([signer]),
        &binding,
        &job,
        100_000,
        200_000,
        PalwStagedPublicCourtConfigV1::network_v1().prices,
    )
    .unwrap();
    let answer = PalwFloatSourceAnswerV1 {
        answerer: signer,
        source_id: source.source_id,
        cursor: PalwFloatSourceCursorV1::Ids,
        action: PalwFloatSourceActionV1::Ids(PalwDecodeTokensV1 {
            summary: PalwTraceSummaryV2 {
                vocab_size: binding.shape_profile.vocab_size,
                logits_dtype: PalwLogitsDtypeV2::F32Le,
                declared_prefill_tokens: binding.job_context.declared_prefill_tokens,
                exact_decode_tokens: binding.job_context.exact_decode_tokens,
                event_count: binding.job_context.exact_decode_tokens,
                first_event_kind: PalwTracePhaseV2::Prefill,
                last_event_kind: PalwTracePhaseV2::Decode,
                output_token_ids_hash: h(50),
                stop_reason: PalwStopReasonV2::ExactBudgetReached,
            },
            ordered_event_commitment: h(51),
            generated_token_ids: vec![0],
        }),
    };
    let old = Float {
        snapshot_block: h(12),
        now_daa: 101,
        session: Box::new(source).into(),
        context: Box::new(PalwSelectedSourceContextV1 { binding: Box::new(binding), job: Box::new(job) }).into(),
    };
    let movement = Move {
        version: 1,
        claim: h(10),
        reserver: owner,
        reserved_daa: 100,
        seed_id: h(11),
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::FloatSourceAnswer(answer),
        signature: vec![],
    };
    queued_source_packet_controls_v1(crate::palw_panel::court_outbox::SourceResponseSnapshotV1::Float(old.clone()), movement.clone());
    let calls = Cell::new(0);
    let sign = || {
        calls.set(calls.get() + 1);
        Some(vec![1])
    };
    let mut fresh = old.clone();
    fresh.now_daa += 1;
    assert!(sign_current_v1(Ok(Some(fresh)), |p| float_current_v1(&old, p, &movement, signer), sign).unwrap().is_some());
    for change in 0..4 {
        let mut fresh = old.clone();
        match change {
            0 => fresh.now_daa = fresh.session.response_deadline_v1(&signer).unwrap(),
            1 => {
                let mut s = (*fresh.session).clone();
                s.owner = bond(80);
                fresh.session = Box::new(s).into();
            }
            2 => {
                let mut s = (*fresh.session).clone();
                s.source_id = h(80);
                fresh.session = Box::new(s).into();
            }
            _ => {
                let mut c = (*fresh.context).clone();
                c.binding.full_logits_trace_root = h(80);
                fresh.context = Box::new(c).into();
            }
        }
        assert!(sign_current_v1(Ok(Some(fresh)), |p| float_current_v1(&old, p, &movement, signer), sign).unwrap().is_none());
    }
    assert_eq!(calls.get(), 1);
}

#[test]
fn response_presign_tir_public_duty_ordinal_header_seed_actor_and_expiry() {
    use kaspa_consensus_core::{
        palw_rendering_resources_v1::PalwRenderingClaimTermsV1, palw_step_leg::*, palw_tir_class_v1::*, palw_tir_prefix_owned_v1::*,
        palw_tir_prefix_state_v1::PalwTirPrefixStateStreamV1, palw_tir_step_v1::PalwTirStepSpaceV1,
    };
    use misaka_palw_tir::{
        builder::ProgramBuilder,
        program::{HISTORY_BOUND_V1_SMALL, Ref},
        types::{DType, TensorType},
    };
    // Public program/layout constructors define a real finite prefix duty.
    // Committed zero boundary bytes below are an identity/opening fixture,
    // not an assertion that these bytes are an honest model execution.
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, 4], false);
    let w = pb.param("w", DType::I8, &[4, 4], true);
    let head = pb.param("head", DType::I8, &[8, 4], false);
    let sum = pb.fixed_state("sum", DType::I32, &[4], -1000, 1000, true);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let s = b.add(Ref::State(sum), Ref::CarryIn(0), DType::I64);
        let s = b.state_write(sum, s);
        let s = b.reshape_fixed(s, &[4, 1]);
        let y = b.matmul(w, s, DType::I32);
        let y = b.reshape_fixed(y, &[4]);
        b.finish(&[y])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut program = pb.finish(pre, vec![layer], post, logits);
    program.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 4,
            checkpoint_interval: 2,
            h_tile: 1,
            commit_tiles: vec![4, 4, 4096],
            state_tiles: vec![4],
        },
        tokenizer_id: h(5),
    };
    let class_id = class.class_id(&h(6));
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let (base, _) = binding();
    let mut context = base.job_context;
    context.shape_profile_id = class_id;
    context.declared_prefill_tokens = 3;
    context.exact_decode_tokens = 2;
    context.max_context_tokens = 4;
    let maximum = 1 << 20;
    let count = space.leaf_count_capped(&context, maximum).unwrap();
    let leaf = space.prefix_state_leaf_v1(&context, 2, 0).unwrap();
    let values = vec![0; leaf.value_count as usize * 4];
    let preimage = PalwStepTileLeafV1 {
        version: kaspa_consensus_core::palw_tir_step_v1::PALW_TIR_STEP_LEAF_VERSION_V1,
        coord: leaf.coord,
        value_count: leaf.value_count,
        values_le: values.clone(),
    };
    let mut hashes = (0..count).map(h).collect::<Vec<_>>();
    hashes[leaf.index as usize] = step_tile_leaf_hash_v1(&context.context_hash(), &class_id, &preimage);
    let root = step_merkle_root_v1(&hashes).unwrap();
    let opening = step_opening_v1(&hashes, leaf.index).unwrap();
    let template = PalwTirPrefixStateStreamV1::new_v1(&space, class_id, &context, root, count, maximum, 2, h(7)).unwrap();
    let config = PalwStagedPublicCourtConfigV1::network_v1();
    let plan = tir_prefix_resource_plan_v1(template.total_v1(), 64, 8, 2, config.prices).unwrap();
    let terms =
        PalwRenderingClaimTermsV1::derive(100, plan, config.acquisition_daa, config.collection_daa, config.reorg_retention_daa)
            .unwrap();
    let signer = bond(1);
    let owner = bond(9);
    let row = PalwOwnedTirPrefixCourtV1::admit_v1(
        h(11),
        h(10),
        owner,
        100,
        h(8),
        terms,
        101,
        u64::MAX,
        signer,
        BTreeSet::from([signer]),
        template,
        terms.material_until_daa,
        terms.bond_exit_after_daa,
        &[terms.bond_exit_after_daa],
    )
    .unwrap();
    let old = Tir {
        snapshot_block: h(12),
        now_daa: 101,
        row: Box::new(row).into(),
        header: Box::new(kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
            version: 1,
            job_context: context.clone(),
            artifact_root: h(6),
            full_logits_trace_root: h(30),
            step_leaf_count: count,
            step_merkle_root: root,
            committed_execution_root: h(8),
        }),
    };
    let answer = PalwTirPrefixLeafAnswerV1 { answerer: signer, seed_id: h(11), ordinal: 0, values_le: values, opening };
    let movement = Move {
        version: 1,
        claim: h(10),
        reserver: owner,
        reserved_daa: 100,
        seed_id: h(11),
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action: Action::NativeTirPrefixAnswer(answer),
        signature: vec![],
    };
    queued_source_packet_controls_v1(crate::palw_panel::court_outbox::SourceResponseSnapshotV1::Tir(old.clone()), movement.clone());
    let calls = Cell::new(0);
    let sign = || {
        calls.set(calls.get() + 1);
        Some(vec![1])
    };
    let mut fresh = old.clone();
    fresh.now_daa += 1;
    fresh.snapshot_block = h(13);
    assert!(sign_current_v1(Ok(Some(fresh.clone())), |p| tir_current_v1(&old, p, &movement, signer), sign).unwrap().is_some());
    let mut advanced = (*fresh.row).clone();
    let Action::NativeTirPrefixAnswer(answer) = &movement.action else { unreachable!() };
    advanced.answer_v1(fresh.now_daa, &space, &context, answer).unwrap();
    fresh.row = Box::new(advanced).into();
    assert!(sign_current_v1(Ok(Some(fresh)), |p| tir_current_v1(&old, p, &movement, signer), sign).unwrap().is_none());
    for change in 0..5 {
        let mut fresh = old.clone();
        let mut m = movement.clone();
        match change {
            0 => fresh.now_daa = fresh.row.respond_until_daa,
            1 => fresh.header.step_merkle_root = h(80),
            2 => fresh.header.job_context.execution_seed[0] ^= 1,
            3 => m.seed_id = h(80),
            _ => {
                let Action::NativeTirPrefixAnswer(a) = &mut m.action else { unreachable!() };
                a.answerer = bond(80);
            }
        }
        assert!(sign_current_v1(Ok(Some(fresh)), |p| tir_current_v1(&old, p, &m, signer), sign).unwrap().is_none());
    }
    assert_eq!(calls.get(), 1, "advanced/expired/substituted TIR duty never reaches the signer");
}

#[test]
fn response_presign_owned_reader_keeps_exact_host_budget_until_blocking_worker_drops() {
    use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    struct Reader {
        ledger: Arc<PalwMemoryLedgerV1>,
        dropped: Arc<AtomicBool>,
    }
    impl Drop for Reader {
        fn drop(&mut self) {
            assert_eq!(self.ledger.reserved_bytes(), 128, "actual reader drops before its reservation");
            self.dropped.store(true, Ordering::SeqCst);
        }
    }
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(128), || None);
    let key = PalwMemoryReservationKeyV1 { role: "response-reader-lifetime-test", class_id: h(1), job: h(2) };
    let memory = ledger.reserve(key, 128).unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let book_owner =
        OwnedSourceReaderV1::new(Box::new(Reader { ledger: Arc::clone(&ledger), dropped: Arc::clone(&dropped) }), memory).unwrap();
    let worker_owner = Arc::clone(&book_owner);
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let guard = worker_owner.reader.lock().unwrap();
        ready_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        drop(guard);
        drop(worker_owner);
    });
    ready_rx.recv().unwrap();
    drop(book_owner);
    assert_eq!(ledger.reserved_bytes(), 128, "book eviction/canceled waiter cannot detach reader reservation");
    assert!(ledger.reserve(PalwMemoryReservationKeyV1 { role: "replacement-response-reader", class_id: h(3), job: h(4) }, 1).is_err());
    assert!(!dropped.load(Ordering::SeqCst));
    release_tx.send(()).unwrap();
    worker.join().unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(ledger.reserved_bytes(), 0);
}

fn copy_source_snapshot_v1(
    p: &crate::palw_panel::court_outbox::SourceResponseSnapshotV1,
) -> crate::palw_panel::court_outbox::SourceResponseSnapshotV1 {
    use crate::palw_panel::court_outbox::SourceResponseSnapshotV1 as S;
    match p {
        S::Flat(p) => S::Flat(p.clone()),
        S::Float(p) => S::Float(p.clone()),
        S::Tir(p) => S::Tir(p.clone()),
    }
}
fn queued_source_packet_controls_v1(snapshot: crate::palw_panel::court_outbox::SourceResponseSnapshotV1, mut movement: Move) {
    use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
    use crate::palw_panel::court_outbox::{CourtOutboxV1, SourceResponseFaultV1 as F, SourceResponseSnapshotV1 as S};
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as O;
    use std::future::Future;
    movement.signature = vec![7; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
    let signer = match &movement.action {
        Action::FlatSourceAnswer(a) => a.answerer,
        Action::FloatSourceAnswer(a) => a.answerer,
        Action::NativeTirPrefixAnswer(a) => a.answerer,
        _ => panic!("source packet"),
    };
    let key = (movement.seed_id, 42, true);
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(128), || None);
    let memory =
        ledger.reserve(PalwMemoryReservationKeyV1 { role: "queued-source-test", class_id: h(1), job: movement.claim }, 128).unwrap();
    let object = O::PublicCourtStreamMoveV1 { movement: Box::new(movement) };
    let mut outbox = CourtOutboxV1::default();
    outbox.try_push_source_response(key, &object, 110, snapshot, memory, None).unwrap();
    assert_eq!(ledger.reserved_bytes(), 128);
    let mut retry = outbox.try_retry_buffer().unwrap();
    let flight = outbox.try_take_priority().unwrap();
    let reads = Cell::new(0);
    let signers = Cell::new(0);
    let submits = Cell::new(0);
    // A late read failure at the actual common operation gate must leave the
    // carrier signer uncalled, and the same exact packet/guard requeued.
    assert!(matches!(
        outbox.current_source_then_for_tests(
            &flight[0].3,
            |_| {
                reads.set(reads.get() + 1);
                Err("native read capacity unavailable".into())
            },
            || signers.set(signers.get() + 1)
        ),
        Err(F::Retry(_))
    ));
    assert_eq!(signers.get(), 0);
    for item in flight {
        retry.push(item);
    }
    outbox.finish_flush(retry);
    assert_eq!(ledger.reserved_bytes(), 128);
    let mut retry = outbox.try_retry_buffer().unwrap();
    let flight = outbox.try_take_priority().unwrap();
    outbox
        .current_source_then_for_tests(&flight[0].3, |p| Ok(Some(copy_source_snapshot_v1(p))), || signers.set(signers.get() + 1))
        .unwrap();
    assert!(matches!(
        outbox.current_source_then_for_tests(
            &flight[0].3,
            |_| Err("late native transport refusal".into()),
            || submits.set(submits.get() + 1)
        ),
        Err(F::Retry(_))
    ));
    assert_eq!((signers.get(), submits.get()), (1, 0));
    // Exact packet identity is independent of shared debounce keys. A changed
    // signed packet cannot borrow this original snapshot or trigger any read.
    let mut foreign = object.clone();
    let O::PublicCourtStreamMoveV1 { movement } = &mut foreign else { unreachable!() };
    movement.signature[0] ^= 1;
    assert!(matches!(
        outbox.current_source_then_for_tests(
            &foreign,
            |_| panic!("foreign packet must not read under original owner"),
            || panic!("must not submit")
        ),
        Err(F::Stale)
    ));
    for variant in [0, 1] {
        let refusal = outbox.current_source_then_for_tests(
            &object,
            |p| {
                let mut fresh = copy_source_snapshot_v1(p);
                match &mut fresh {
                    S::Flat(p) => {
                        if variant == 0 {
                            p.now_daa = p.session.response_deadline_v1(&signer).unwrap();
                        } else {
                            let mut c = (*p.context).clone();
                            c.binding.full_logits_trace_root = h(80);
                            p.context = Box::new(c).into();
                        }
                    }
                    S::Float(p) => {
                        if variant == 0 {
                            p.now_daa = p.session.response_deadline_v1(&signer).unwrap();
                        } else {
                            let mut c = (*p.context).clone();
                            c.binding.full_logits_trace_root = h(80);
                            p.context = Box::new(c).into();
                        }
                    }
                    S::Tir(p) => {
                        if variant == 0 {
                            p.now_daa = p.row.respond_until_daa;
                        } else {
                            p.header.step_merkle_root = h(80);
                        }
                    }
                }
                Ok(Some(fresh))
            },
            || submits.set(submits.get() + 1),
        );
        assert!(matches!(refusal, Err(F::Stale)));
    }
    assert_eq!(submits.get(), 0);
    for item in flight {
        retry.push(item);
    }
    outbox.finish_flush(retry);
    // Poll and cancel an actual future owning the drained production batch.
    // The retained exact packet enables one recovery on the next real flush.
    let mut canceled = Box::pin(async {
        let _retry = outbox.try_retry_buffer().unwrap();
        let batch = outbox.try_take_priority().unwrap();
        std::future::pending::<()>().await;
        drop(batch);
    });
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(canceled.as_mut().poll(&mut cx).is_pending());
    drop(canceled);
    assert!(outbox.pending().is_empty());
    assert_eq!(ledger.reserved_bytes(), 128);
    let recovery = outbox.try_retry_buffer().unwrap();
    assert_eq!(outbox.pending().len(), 1);
    drop(recovery);
    let recovery = outbox.try_retry_buffer().unwrap();
    assert_eq!(outbox.pending().len(), 1, "next flush does not recover twice");
    drop(recovery);
    // A successfully completed packet is marked before any later cancellation;
    // it must never be resurrected by the next flush.
    let mut canceled = Box::pin(async {
        let batch = outbox.try_take_priority().unwrap();
        outbox
            .current_source_then_for_tests(&batch[0].3, |p| Ok(Some(copy_source_snapshot_v1(p))), || submits.set(submits.get() + 1))
            .unwrap();
        outbox.finish_source_packet(&batch[0].3);
        std::future::pending::<()>().await;
        drop(batch);
    });
    assert!(canceled.as_mut().poll(&mut cx).is_pending());
    drop(canceled);
    let recovery = outbox.try_retry_buffer().unwrap();
    assert!(outbox.pending().is_empty());
    assert_eq!(submits.get(), 1);
    assert_eq!(ledger.reserved_bytes(), 0, "completed packet snapshots release after canceled payload retires");
    drop(recovery);
    assert!(outbox.submitted_at(&key).is_none(), "guard disposal does not invent a successful submission stamp");
}
#[test]
fn response_presign_queued_flat_packet_exact_scope_retry_cancel_and_lifetime() {
    let (old, movement, _) = flat();
    queued_source_packet_controls_v1(crate::palw_panel::court_outbox::SourceResponseSnapshotV1::Flat(old), movement);
}

#[test]
fn response_presign_source_queue_admission_and_canceled_recovery_capacity_are_atomic() {
    use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
    use crate::palw_panel::court_outbox::{CourtOutboxV1, SourceResponseSnapshotV1 as S};
    use kaspa_consensus_core::{
        palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1},
        palw_state_v2::PalwConsensusObjectV2 as O,
    };
    const MODE: &str = "MISAKA_SOURCE_QUEUE_CAPACITY_MODE";
    let Ok(mode) = std::env::var(MODE) else {
        let name = format!("{}::response_presign_source_queue_admission_and_canceled_recovery_capacity_are_atomic", module_path!());
        let name = name.split_once("::").unwrap().1;
        for mode in ["enqueue", "recover"] {
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", name, "--nocapture", "--test-threads=1"])
                    .env(MODE, mode)
                    .status()
                    .unwrap()
                    .success(),
                "{mode}"
            );
        }
        return;
    };
    let (old, mut movement, _) = flat();
    movement.signature = vec![7; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
    let key = (movement.seed_id, 42, true);
    let object = O::PublicCourtStreamMoveV1 { movement: Box::new(movement) };
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(128), || None);
    let memory =
        ledger.reserve(PalwMemoryReservationKeyV1 { role: "source-queue-refusal-test", class_id: h(1), job: h(2) }, 128).unwrap();
    let mut outbox = CourtOutboxV1::default();
    if mode == "recover" {
        outbox.try_push_source_response(key, &object, 110, S::Flat(old), memory, None).unwrap();
        let retry = outbox.try_retry_buffer().unwrap();
        drop(outbox.try_take_priority().unwrap()); // canceled raw batch
        drop(retry);
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
        assert!(outbox.try_retry_buffer().is_err());
        assert!(outbox.pending().is_empty());
        assert_eq!(ledger.reserved_bytes(), 128, "whole original packet/duty guard survives failed recovery");
        assert!(outbox.try_retry_buffer().is_err());
        assert_eq!(ledger.reserved_bytes(), 128, "repeat refusal never prunes or fabricates authority");
        drop(outbox);
        assert_eq!(ledger.reserved_bytes(), 0);
    } else {
        // The shared retained clone+due API succeeds normally and must preserve
        // both existing logical entries on a later typed destination refusal.
        let mut destination = CourtOutboxV1::default();
        destination.try_push_clone_with_due(key, &object, 110).unwrap();
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
        let other = (h(99), 42, true);
        assert!(destination.try_push_clone_with_due(other, &object, 120).is_err());
        assert_eq!(destination.pending().len(), 1);
        assert_eq!(destination.forget_due(&key), Some(110));
        assert_eq!(destination.forget_due(&other), None);
        assert!(outbox.try_push_source_response(key, &object, 110, S::Flat(old), memory, None).is_err());
        assert!(outbox.pending().is_empty());
        assert_eq!(outbox.forget_due(&key), None);
        assert_eq!(ledger.reserved_bytes(), 0, "refused destination releases snapshots before their tickets");
    }
}
