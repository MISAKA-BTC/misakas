#![cfg(feature = "node")]
//! Local synthetic regression declarations, not native registration or credit.
//! First startup/forward constructs NO full trace binding. Dense execution below
//! is a separately requested regression oracle after ordinary forward completes.
mod run_wide_common;
use kaspa_consensus_core::{
    Hash64, palw_logits_q24_v2,
    palw_native_resource_v1::{LocalScanWorkBudgetV1, SourceProgramLocalLimitsV1, scan_source_program_local_v1},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_step_refute::{flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_step_v1::PalwTirStepSpaceV2,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir_artifact::{
    bounded_v1::{PalwTirHeaderLimitsV1, read_header_bounded_v1},
    write_container_v1,
};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirHeldInventoryV1, TirParamsSourceV1,
        memory::TirMemoryReserverV1,
        owned_artifact_params_v2::{OwnedParamLimitsV2, forward::*},
        run::TirClassRunnerV1,
        run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2},
    },
    plan::{TirPlanCompileLimitsV2, prepare_tir_plan_compile_v2},
};
use run_wide_common::{EXPECTED_GENERATED, MAX_CONTEXT, PROMPT};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};
const EXTERNAL: u64 = 1 << 30;
fn header_limits() -> PalwTirHeaderLimitsV1 {
    PalwTirHeaderLimitsV1 {
        max_file_bytes: 1 << 24,
        max_header_bytes: 1 << 20,
        max_program_bytes: 1 << 20,
        max_layout_bytes: 1 << 20,
        max_tensors: 1024,
        max_meta_bytes: 4096,
        max_native_bytes: 1 << 22,
        max_copy_bytes: 1 << 22,
        max_work: 1 << 40,
        max_operation_bytes: 1 << 34,
        external_retained_bytes: EXTERNAL,
    }
}
fn load_limits() -> LocalForwardLoadLimitsV2 {
    LocalForwardLoadLimitsV2 {
        params: OwnedParamLimitsV2 {
            max_tensor_bytes: 1 << 24,
            max_payload_bytes: 1 << 25,
            max_chunk_bytes: 32768,
            max_metadata_bytes: 1 << 24,
            max_native_bytes: 1 << 26,
            max_inventory_leaves: 1 << 20,
            max_work_per_attempt: 1 << 42,
            max_attempts: 4,
            max_total_read_bytes: 1 << 28,
            max_total_inventory_leaves: 1 << 22,
            max_total_work: 1 << 46,
            max_operation_bytes: 1 << 35,
            external_retained_bytes: 0,
        },
        max_new_owner_bytes: 1 << 28,
        max_operation_bytes: 1 << 35,
        max_retained_requests: 1 << 32,
        max_provider_calls_per_attempt: 1 << 20,
        max_total_provider_calls: 1 << 24,
    }
}
fn run_limits() -> LocalWideForwardLimitsV2 {
    LocalWideForwardLimitsV2 {
        max_positions: MAX_CONTEXT,
        max_generated: 16,
        max_logits_lanes: 64,
        max_work_per_attempt: 1 << 44,
        max_requested_bytes: 1 << 32,
        max_operation_bytes: 1 << 36,
        max_attempts: 4,
        max_total_work: 1 << 48,
        max_retained_requests: 1 << 35,
        external_retained_bytes: EXTERNAL + (1 << 28),
    }
}
struct Guard(Arc<AtomicU64>, u64);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_sub(self.1, Ordering::SeqCst);
    }
}
fn reserver(held: Arc<AtomicU64>, calls: Arc<AtomicUsize>) -> TirMemoryReserverV1 {
    Arc::new(move |_, _, _, n| {
        assert!(n > 0);
        calls.fetch_add(1, Ordering::SeqCst);
        held.fetch_add(n, Ordering::SeqCst);
        Ok(Box::new(Guard(held.clone(), n)))
    })
}
// This finite external ticket/guard is acquired before header/source/compile
// construction. Independent literal oracles are test-only caller inputs; no
// allocation/RSS success is inferred from this counting test callback.
fn fixture(
    window: u32,
    empty: bool,
    body: impl FnOnce(&mut File, LocalForwardInputsV2<'_>, &TirPlan, &TirParams<'_>, &TirMemoryReserverV1),
) {
    fixture_case(Case::Wide, window, empty, body)
}
#[derive(Clone, Copy)]
enum Case {
    Wide,
    Tiled,
    Flat,
    I8,
    I16,
    Idx,
    Unknown,
    SelectTopK,
    Abort,
    LongNarrow,
    LongWide,
}
fn case_program(case: Case, window: u32) -> (misaka_palw_tir::TirProgramV1, misaka_palw_tir::MapParams) {
    use misaka_palw_tir::{Cmp, DType, Ref, Tensor, builder::ProgramBuilder};
    if matches!(case, Case::LongNarrow | Case::LongWide) {
        let wide = matches!(case, Case::LongWide);
        let dtype = if wide { DType::I64 } else { DType::I32 };
        let mut b = ProgramBuilder::new(4, 1 << 21);
        let data = b.param("long.logits", dtype, &[4], false);
        let pre = b.block("pre", vec![]).finish(&[]);
        let mut post = b.block("post", vec![]);
        let out = post.reshape_fixed(data, &[4]);
        post.commit(out);
        let Ref::Node(logits) = out else { unreachable!() };
        let post = post.finish(&[]);
        let mut p = b.finish(pre, vec![], post, logits);
        p.logits_scheme_id =
            if wide { misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2 } else { tiled_logits_scheme_id_v1().as_bytes() };
        let mut map = misaka_palw_tir::MapParams::default();
        map.tensors.insert((0, None), Tensor::new(dtype, vec![4], vec![1, 3, 3, 0]).unwrap());
        return (p, map);
    }
    if matches!(case, Case::SelectTopK | Case::Abort) {
        let mut b = ProgramBuilder::new(4, 1 << 18);
        if matches!(case, Case::SelectTopK) {
            let table = b.param("selector.table", DType::I32, &[4, 4], false);
            let threshold = b.scalar(DType::I32, 4);
            let when_true = b.konst(DType::I32, &[2], &[0, 7]);
            let when_false = b.konst(DType::I32, &[2], &[8, 1]);
            let pre = b.block("pre", vec![]).finish(&[]);
            let mut post = b.block("post", vec![]);
            let row = post.gather(table, Ref::Input(0), 0, 0);
            let ids = post.topk(row, 0, 2);
            let picked = post.gather(row, ids, 0, 0);
            // Token1 has TopK values3: false->[8,1]->ID0. Token0 has
            // TopK values9: true->[0,7]->ID1. Both branch choice and the
            // generated-token recurrence therefore affect the next output.
            let condition = post.compare(picked, threshold, Cmp::Gt);
            let out = post.select(condition, when_true, when_false, DType::I32);
            post.commit(out);
            let Ref::Node(logits) = out else { unreachable!() };
            let post = post.finish(&[]);
            let mut p = b.finish(pre, vec![], post, logits);
            p.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
            let mut map = misaka_palw_tir::MapParams::default();
            map.tensors.insert(
                (0, None),
                Tensor::new(DType::I32, vec![4, 4], vec![1, 9, 9, 0, -2, 3, 1, 3, 4, 1, 2, 4, 0, 0, 0, 0]).unwrap(),
            );
            return (p, map);
        }
        let a = b.param("a", DType::I32, &[4], false);
        let c = b.param("b", DType::I32, &[4], false);
        let state = b.fixed_state("last", DType::I32, &[4], i64::from(i32::MIN), i64::from(i32::MAX), false);
        let mut pre = b.block("pre", vec![]);
        pre.state_write(state, a);
        let pre = pre.finish(&[]);
        let mut post = b.block("post", vec![]);
        let out = post.add(a, c, DType::I32);
        post.commit(out);
        let Ref::Node(logits) = out else { unreachable!() };
        let post = post.finish(&[]);
        let mut p = b.finish(pre, vec![], post, logits);
        p.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
        let mut map = misaka_palw_tir::MapParams::default();
        map.tensors.insert((0, None), Tensor::new(DType::I32, vec![4], vec![i32::MAX as i128, 0, 0, 0]).unwrap());
        map.tensors.insert((1, None), Tensor::new(DType::I32, vec![4], vec![1, 0, 0, 0]).unwrap());
        return (p, map);
    }
    let (mut p, mut map) = run_wide_common::program(matches!(case, Case::Wide), window);
    match case {
        Case::Flat => p.logits_scheme_id = flat_logits_scheme_id_v1().as_bytes(),
        Case::Unknown => p.logits_scheme_id = [225; 64],
        Case::I8 | Case::I16 | Case::Idx => {
            let dtype = match case {
                Case::I8 => DType::I8,
                Case::I16 => DType::I16,
                _ => DType::Idx,
            };
            p.params[1].dtype = dtype;
            let post = p.schedule.post as usize;
            p.blocks[post].nodes[p.logits as usize].out.dtype = dtype;
            let data = map.tensors.get_mut(&(1, None)).unwrap();
            data.dtype = dtype;
            if dtype == DType::Idx {
                for v in &mut data.data {
                    *v += 3;
                }
            }
        }
        _ => {}
    }
    (p, map)
}
fn fixture_case(
    case: Case,
    window: u32,
    empty: bool,
    body: impl FnOnce(&mut File, LocalForwardInputsV2<'_>, &TirPlan, &TirParams<'_>, &TirMemoryReserverV1),
) {
    let external = Arc::new(AtomicU64::new(0));
    let r = reserver(external.clone(), Arc::new(AtomicUsize::new(0)));
    let ticket = r("test-forward-external", Hash64::from_u64_word(0), Hash64::from_u64_word(0), EXTERNAL).unwrap();
    // program() only declares literal parameters and nodes; it does NOT execute
    // the model, record node values, compute a trace root or create a binding.
    let (program, map) = case_program(case, window);
    let long = matches!(case, Case::LongNarrow | Case::LongWide);
    let long_prompt = if long { Some(vec![0u32; (1 << 21) - 1]) } else { None };
    let declared_prompt = long_prompt.as_deref().unwrap_or(PROMPT);
    let context_max = if long { 1 << 21 } else { MAX_CONTEXT };
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let inventory = TirHeldInventoryV1::build(&program, &TirParamsSourceV1(&params)).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: context_max,
            checkpoint_interval: 3,
            h_tile: if window == 1 { 1 } else { 2 },
            commit_tiles: program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).map(|_| 4).collect(),
            state_tiles: vec![4; program.states.len()],
        },
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let zero = Hash64::from_u64_word(0);
    let class_id = class.class_id(&artifact);
    let ctx = PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_u64_word(41),
        job_nullifier: zero,
        assignment_id: zero,
        execution_seed: [2; 32],
        model_profile_id: zero,
        runtime_manifest_hash: zero,
        runtime_class_id: zero,
        shape_profile_id: class_id,
        trace_scheme_id: match case {
            Case::Wide | Case::LongWide => palw_logits_q24_v2::scheme_id(),
            Case::Flat => kaspa_consensus_core::palw_v2::trace_scheme_id_v2(),
            _ => tiled_logits_scheme_id_v1(),
        },
        cu_ruleset_id: zero,
        tokenizer_id: zero,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, declared_prompt).unwrap(),
        declared_prefill_tokens: declared_prompt.len() as u32,
        exact_decode_tokens: if long { 1 } else { 6 },
        max_context_tokens: context_max,
    };
    let execution = ctx.context_hash();
    let wide_space = if program.wide_logits_q24() { Some(PalwTirStepSpaceV2::new(&class).unwrap()) } else { None };
    let narrow_space = if program.wide_logits_q24() { None } else { Some(LocalNarrowForwardSpaceV2::new(&class, &ticket).unwrap()) };
    let space = match (&wide_space, &narrow_space) {
        (Some(s), _) => LocalForwardSpaceV2::Wide(s),
        (_, Some(s)) => LocalForwardSpaceV2::Narrow(s),
        _ => unreachable!(),
    };
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("forward.palwtir");
    write_container_v1(
        &path,
        &program,
        if empty { vec![] } else { borsh::to_vec(&class.layout).unwrap() },
        class.tokenizer_id.as_bytes(),
        "local-only".into(),
        &mut |j, l| Ok(params.get(j, l).unwrap().to_le_bytes()),
    )
    .unwrap();
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
    let len = file.metadata().unwrap().len();
    let header = read_header_bounded_v1(&mut file, len, header_limits()).unwrap();
    let mut scan = LocalScanWorkBudgetV1::new(1 << 40).unwrap();
    let source = scan_source_program_local_v1(
        &header.header().program,
        SourceProgramLocalLimitsV1 {
            max_program_bytes: 1 << 20,
            max_scan_steps: 1 << 36,
            max_nf_items: 1 << 32,
            max_native_requests: 1 << 30,
        },
        &mut scan,
    )
    .expect("requires audited local Source build; unsupported is NOT a forward pass")
    .decode()
    .unwrap();
    let checked = header.validate_with_program(source.program(), header_limits()).unwrap();
    let compiled = prepare_tir_plan_compile_v2(
        &source,
        TirPlanCompileLimitsV2 {
            max_quote_visits: 1 << 36,
            max_retained_bytes: 1 << 30,
            max_scratch_bytes: 1 << 30,
            max_operation_bytes: 1 << 34,
            external_retained_bytes: EXTERNAL,
        },
    )
    .unwrap()
    .compile()
    .unwrap();
    body(
        &mut file,
        LocalForwardInputsV2 {
            checked: &checked,
            source: &source,
            compiled: &compiled,
            class: &class,
            artifact,
            expected_class: class_id,
            space,
            original_context: &ctx,
            expected_execution: execution,
            external_retained_bytes: EXTERNAL,
            external_ticket: &ticket,
        },
        &plan,
        &params,
        &r,
    );
}
#[test]
fn first_trace_free_startup_and_plain_forward_match_original_dense_for_fixed_hist_windows() {
    for window in [1, 3] {
        for empty in [false, true] {
            fixture(window, empty, |file, input, plan, params, r| {
                let mut loads = LocalForwardLoadBudgetV2::new(load_limits()).unwrap();
                let loaded = load_local_wide_forward_v2(file, input, &mut loads, r).unwrap();
                // The verified OWNED native values do not depend on later disk
                // mutation. No execution/trace binding is involved in loading.
                file.set_len(0).unwrap();
                assert_eq!(loaded.original_context(), input.original_context);
                assert_eq!(loaded.original_context().max_context_tokens, MAX_CONTEXT);
                assert_eq!(loaded.original_context().tokenizer_id, Hash64::from_u64_word(0));
                assert_ne!(input.class.tokenizer_id, loaded.original_context().tokenizer_id);
                let mut runs = LocalWideForwardBudgetV2::new(run_limits()).unwrap();
                let output = loaded.forward_local(PROMPT, &mut runs, r).unwrap();
                assert_eq!(output.generated(), EXPECTED_GENERATED);
                assert_eq!(output.execution(), input.expected_execution);
                assert_eq!(output.quote().row_bytes, output.quote().vocabulary * 8);
                assert_eq!(output.quote().generated_bytes, 6 * 4);
                assert_eq!(runs.usage().attempts, 1);
                assert_eq!(runs.usage().work, output.quote().work);
                // Separate regression oracle only: production ordinary entry never calls
                // this full runner, and its startup above never took a trace binding.
                let old = TirClassRunnerWideQ24V2::new(input.space.wide_space().unwrap(), plan, params, input.expected_class)
                    .unwrap()
                    .run(
                        input.original_context,
                        PROMPT,
                        input.expected_execution,
                        TirWideRunLimitsV2 {
                            max_positions: MAX_CONTEXT,
                            max_step_leaves: 1 << 16,
                            max_row_lanes: 64,
                            max_work: 1 << 50,
                            max_requested_bytes: 1 << 34,
                        },
                        &mut |_| {},
                    )
                    .unwrap();
                assert_eq!(output.generated(), old.generated());
                assert_eq!(output.output_root(), old.output_root());
            });
        }
    }
}
#[test]
fn output_owns_required_guard_and_failed_prompt_or_guard_attempts_do_not_refund() {
    fixture(1, false, |file, input, _, _, _| {
        let held = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let r = reserver(held.clone(), calls.clone());
        let loaded = load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), &r).unwrap();
        let load_bytes = held.load(Ordering::SeqCst);
        let mut runs = LocalWideForwardBudgetV2::new(run_limits()).unwrap();
        let output = loaded.forward_local(PROMPT, &mut runs, &r).unwrap();
        let run_bytes = output.quote().requested_bytes;
        assert_eq!(held.load(Ordering::SeqCst), load_bytes + run_bytes);
        drop(loaded);
        assert_eq!(held.load(Ordering::SeqCst), run_bytes);
        drop(output);
        assert_eq!(held.load(Ordering::SeqCst), 0);
        let loaded = load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), &r).unwrap();
        let mut l = run_limits();
        l.max_attempts = 1;
        let mut runs = LocalWideForwardBudgetV2::new(l).unwrap();
        assert_eq!(loaded.forward_local(&[0, 0], &mut runs, &r).err(), Some(LocalForwardErrorV2::Pins));
        let spent = runs.usage();
        assert!(spent.work > 0 && spent.retained_requests > 0);
        assert_eq!(loaded.forward_local(PROMPT, &mut runs, &r).err(), Some(LocalForwardErrorV2::Limit));
        assert_eq!(runs.usage(), spent);
        let deny: TirMemoryReserverV1 = Arc::new(|_, _, _, _| Err("injected denied guard".into()));
        let mut runs = LocalWideForwardBudgetV2::new(l).unwrap();
        assert_eq!(loaded.forward_local(PROMPT, &mut runs, &deny).err(), Some(LocalForwardErrorV2::Guard));
        let spent = runs.usage();
        assert!(spent.work > 0 && spent.retained_requests > 0);
        assert_eq!(loaded.forward_local(PROMPT, &mut runs, &r).err(), Some(LocalForwardErrorV2::Limit));
        assert_eq!(runs.usage(), spent);
    });
}
#[test]
fn one_below_load_and_run_caps_refuse_before_payload_or_cpu_guards() {
    fixture(3, true, |file, input, _, _, r| {
        let loaded = load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).unwrap();
        let lq = loaded.load_quote();
        let mut ll = load_limits();
        ll.max_new_owner_bytes = lq.requested_bytes - 1;
        let mut budget = LocalForwardLoadBudgetV2::new(ll).unwrap();
        let at = file.stream_position().unwrap();
        let fail: TirMemoryReserverV1 = Arc::new(|_, _, _, _| panic!("refusal must precede guard"));
        assert_eq!(load_local_wide_forward_v2(file, input, &mut budget, &fail).err(), Some(LocalForwardErrorV2::Limit));
        assert_eq!(file.stream_position().unwrap(), at);
        assert!(budget.usage().params.attempts > 0);
        let mut l = run_limits();
        let q = loaded.forward_quote(l).unwrap();
        l.max_requested_bytes = q.requested_bytes - 1;
        let mut budget = LocalWideForwardBudgetV2::new(l).unwrap();
        assert_eq!(loaded.forward_local(PROMPT, &mut budget, &fail).err(), Some(LocalForwardErrorV2::Limit));
        assert_eq!(budget.usage().attempts, 1);
        assert!(budget.usage().work > 0);
        let mut bad = input;
        bad.expected_execution = Hash64::from_u64_word(999);
        assert_eq!(
            load_local_wide_forward_v2(file, bad, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).err(),
            Some(LocalForwardErrorV2::Pins)
        );
        let mut bad = input;
        bad.artifact = Hash64::from_u64_word(998);
        assert_eq!(
            load_local_wide_forward_v2(file, bad, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).err(),
            Some(LocalForwardErrorV2::Pins)
        );
    });
}
struct Probe<'a> {
    file: &'a mut File,
    mode: u8,
    calls: Arc<AtomicUsize>,
}
impl Read for Probe<'_> {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            1 => Err(std::io::Error::from(std::io::ErrorKind::Interrupted)),
            2 => {
                let n = b.len().min(1);
                self.file.read(&mut b[..n])
            }
            _ => self.file.read(b),
        }
    }
}
impl Seek for Probe<'_> {
    fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.mode == 3 { Err(std::io::Error::other("injected seek failure")) } else { self.file.seek(p) }
    }
}
impl LocalForwardReaderV2 for Probe<'_> {
    fn retained_request_bytes(&self) -> Option<u64> {
        Some(std::mem::size_of::<Self>() as u64)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
#[test]
fn bounded_short_read_interrupt_seek_truncation_and_root_failure_never_return_partial_owner() {
    for mode in [0, 1, 2, 3, 4] {
        fixture(1, false, |file, input, _, _, r| {
            if mode == 0 {
                let e = &input.checked.header().tensors[0];
                file.seek(SeekFrom::Start(e.offset)).unwrap();
                file.write_all(&[255]).unwrap();
            }
            if mode == 4 {
                let e = input.checked.header().tensors.last().unwrap();
                file.set_len(e.offset + e.bytes - 1).unwrap();
            }
            let calls = Arc::new(AtomicUsize::new(0));
            let mut probe = Probe { file, mode, calls: calls.clone() };
            let mut l = load_limits();
            if mode == 1 {
                l.max_provider_calls_per_attempt = 3;
            }
            let mut budget = LocalForwardLoadBudgetV2::new(l).unwrap();
            let result = load_local_wide_forward_v2(&mut probe, input, &mut budget, r);
            if mode == 2 {
                assert!(result.is_ok());
            } else {
                assert_eq!(result.err(), Some(LocalForwardErrorV2::Params));
            }
            let spent = budget.usage();
            assert!(spent.params.read_bytes > 0 && spent.params.work > 0 && spent.retained_requests > 0);
            assert_eq!(spent.provider_calls_reserved, l.max_provider_calls_per_attempt);
            if mode == 1 {
                assert_eq!(calls.load(Ordering::SeqCst), 3);
            }
        });
    }
}
struct Unknown;
impl Read for Unknown {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("unknown provider read")
    }
}
impl Seek for Unknown {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
        panic!("unknown provider seek")
    }
}
impl LocalForwardReaderV2 for Unknown {}
#[test]
fn unknown_provider_quote_is_denied_before_guard_or_io() {
    fixture(1, false, |_, input, _, _, _| {
        let fail: TirMemoryReserverV1 = Arc::new(|_, _, _, _| panic!("unknown guard"));
        let mut b = LocalForwardLoadBudgetV2::new(load_limits()).unwrap();
        assert_eq!(load_local_wide_forward_v2(&mut Unknown, input, &mut b, &fail).err(), Some(LocalForwardErrorV2::Limit));
        assert_eq!(b.usage(), LocalForwardLoadUsageV2::default());
    });
}

fn legacy_space<'a>(input: LocalForwardInputsV2<'a>) -> &'a kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1 {
    match input.space {
        LocalForwardSpaceV2::Narrow(s) => s.legacy_space(),
        _ => panic!("expected narrow metadata"),
    }
}
#[test]
fn flat_and_tiled_i32_ordinary_match_legacy_without_changing_context_or_row_width() {
    for case in [Case::Flat, Case::Tiled] {
        for window in [1, 3] {
            fixture_case(case, window, false, |file, input, plan, params, r| {
                let loaded =
                    load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).unwrap();
                assert_eq!(
                    loaded.profile(),
                    if matches!(case, Case::Flat) { LocalForwardProfileV2::FlatI32 } else { LocalForwardProfileV2::TiledI32 }
                );
                let output = loaded.forward_local(PROMPT, &mut LocalWideForwardBudgetV2::new(run_limits()).unwrap(), r).unwrap();
                let dense = TirClassRunnerV1::new(legacy_space(input), plan, params, input.expected_class)
                    .unwrap()
                    .run(input.original_context, PROMPT, 1 << 16, true, &mut |_| {})
                    .unwrap();
                assert_eq!(output.generated(), EXPECTED_GENERATED);
                assert_eq!(output.generated(), dense.generated.as_slice());
                assert_eq!(output.output_root(), dense.output_root);
                assert_eq!(output.quote().row_bytes, 4 * output.quote().vocabulary);
                assert_eq!(loaded.original_context(), input.original_context);
                assert_eq!(loaded.original_context().max_context_tokens, MAX_CONTEXT);
                let mut wrong = (*input.original_context).clone();
                wrong.trace_scheme_id = if matches!(case, Case::Flat) {
                    tiled_logits_scheme_id_v1()
                } else {
                    kaspa_consensus_core::palw_v2::trace_scheme_id_v2()
                };
                let bad = LocalForwardInputsV2 { original_context: &wrong, expected_execution: wrong.context_hash(), ..input };
                assert_eq!(
                    load_local_wide_forward_v2(file, bad, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).err(),
                    Some(LocalForwardErrorV2::UnsupportedProfile)
                );
            });
        }
    }
}
#[test]
fn declared_i8_i16_idx_and_unknown_scheme_are_not_reinterpreted_as_i32() {
    for case in [Case::I8, Case::I16, Case::Idx, Case::Unknown] {
        fixture_case(case, 1, false, |file, input, _, _, r| {
            let mut budget = LocalForwardLoadBudgetV2::new(load_limits()).unwrap();
            assert_eq!(load_local_wide_forward_v2(file, input, &mut budget, r).err(), Some(LocalForwardErrorV2::UnsupportedProfile));
            assert!(budget.usage().retained_requests > 0);
        });
    }
}
#[test]
fn dynamic_gather_select_and_topk_follow_the_same_ordinary_and_legacy_execution() {
    fixture_case(Case::SelectTopK, 1, false, |file, input, plan, params, r| {
        let loaded = load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).unwrap();
        let output = loaded.forward_local(PROMPT, &mut LocalWideForwardBudgetV2::new(run_limits()).unwrap(), r).unwrap();
        let dense = TirClassRunnerV1::new(legacy_space(input), plan, params, input.expected_class)
            .unwrap()
            .run(input.original_context, PROMPT, 1 << 16, true, &mut |_| {})
            .unwrap();
        // Literal fixture expectation + ordinary-versus-full parity; this is
        // not an independent validation of the shared Select/TopK algorithm.
        assert_eq!(output.generated(), &[0, 1, 0, 1, 0, 1]);
        assert_eq!(dense.logits_rows, vec![vec![8, 1], vec![0, 7], vec![8, 1], vec![0, 7], vec![8, 1], vec![0, 7]]);
        assert_eq!(output.generated(), dense.generated.as_slice());
        assert_eq!(output.output_root(), dense.output_root);
    });
}
#[test]
fn checked_arithmetic_abort_returns_no_output_and_spends_each_attempt_like_the_legacy_runner() {
    fixture_case(Case::Abort, 1, false, |file, input, plan, params, r| {
        let loaded = load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).unwrap();
        let mut budget = LocalWideForwardBudgetV2::new(run_limits()).unwrap();
        assert_eq!(loaded.forward_local(PROMPT, &mut budget, r).err(), Some(LocalForwardErrorV2::Execute));
        let first = budget.usage();
        assert_eq!(loaded.forward_local(PROMPT, &mut budget, r).err(), Some(LocalForwardErrorV2::Execute));
        assert_eq!(budget.usage().attempts, 2);
        assert_eq!(budget.usage().work, 2 * first.work);
        assert_eq!(budget.usage().retained_requests, 2 * first.retained_requests);
        assert!(
            TirClassRunnerV1::new(legacy_space(input), plan, params, input.expected_class)
                .unwrap()
                .run(input.original_context, PROMPT, 1 << 16, true, &mut |_| {})
                .is_err()
        );
    });
}

use misaka_palw_tir_exec::node::owned_artifact_params_v2::forward::measurement::*;
fn dense_limits() -> DenseMeasurementLimitsV2 {
    DenseMeasurementLimitsV2 {
        runner: TirWideRunLimitsV2 {
            max_positions: MAX_CONTEXT,
            max_step_leaves: 1 << 16,
            max_row_lanes: 64,
            max_work: 1 << 54,
            max_requested_bytes: 1 << 34,
        },
        max_extra_bytes: 1 << 34,
        max_combined_peak_bytes: 1 << 35,
        max_operation_bytes: 1 << 40,
        max_geometry_visits: 1 << 48,
        max_work_per_attempt: 1 << 56,
        max_attempts: 2,
        max_total_work: 1 << 60,
        max_retained_requests: 1 << 38,
        external_retained_bytes: 1 << 38,
    }
}
#[test]
fn measurement_borrows_same_owned_params_after_file_is_gone_and_holds_both_guard_scopes() {
    for case in [Case::Flat, Case::Tiled, Case::Wide] {
        fixture_case(case, 1, false, |file, input, _, _, _| {
            let held = Arc::new(AtomicU64::new(0));
            let calls = Arc::new(AtomicUsize::new(0));
            let r = reserver(held.clone(), calls.clone());
            let owner =
                load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), &r).unwrap();
            file.set_len(0).unwrap(); // Any hidden reopen/second payload load would fail.
            let ordinary = owner.forward_local(PROMPT, &mut LocalWideForwardBudgetV2::new(run_limits()).unwrap(), &r).unwrap();
            let prior = held.load(Ordering::SeqCst);
            let previous_calls = calls.load(Ordering::SeqCst);
            let mut budget = DenseMeasurementBudgetV2::new(dense_limits()).unwrap();
            let prepared = owner.prepare_dense_measurement(&ordinary, &mut budget).unwrap();
            let q = prepared.quote();
            assert_eq!(budget.usage().attempts, 1);
            assert_eq!(budget.usage().work, q.work);
            assert_eq!(budget.usage().retained_requests, q.combined_peak_bytes);
            let dense = prepared.run(PROMPT, &r).unwrap();
            assert!(dense.matches_ordinary());
            assert_eq!(dense.generated(), ordinary.generated());
            assert_eq!(dense.output_root(), ordinary.output_root());
            assert_eq!(dense.leaf_count(), q.leaves);
            assert_eq!(calls.load(Ordering::SeqCst), previous_calls + 2); // extra +explicit required run; no global fallback.
            let kept = if matches!(case, Case::Wide) { q.combined_peak_bytes } else { q.extra_requested_bytes };
            assert_eq!(held.load(Ordering::SeqCst), prior + kept);
            drop(dense);
            assert_eq!(held.load(Ordering::SeqCst), prior);
        });
    }
}
#[test]
fn denied_measurement_guards_and_undersized_extra_caps_preserve_budget_without_partial_comparison() {
    fixture(1, false, |file, input, _, _, r| {
        let owner = load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).unwrap();
        let ordinary = owner.forward_local(PROMPT, &mut LocalWideForwardBudgetV2::new(run_limits()).unwrap(), r).unwrap();
        let mut l = dense_limits();
        l.max_attempts = 1;
        let mut budget = DenseMeasurementBudgetV2::new(l).unwrap();
        let prepared = owner.prepare_dense_measurement(&ordinary, &mut budget).unwrap();
        let q = prepared.quote();
        let deny: TirMemoryReserverV1 = Arc::new(|_, _, _, _| Err("injected extra denial".into()));
        assert_eq!(prepared.run(PROMPT, &deny).err(), Some(DenseMeasurementErrorV2::Guard));
        let spent = budget.usage();
        assert_eq!(owner.prepare_dense_measurement(&ordinary, &mut budget).err(), Some(DenseMeasurementErrorV2::Limit));
        assert_eq!(budget.usage(), spent);
        let mut l = dense_limits();
        l.max_extra_bytes = q.extra_requested_bytes - 1;
        let mut budget = DenseMeasurementBudgetV2::new(l).unwrap();
        assert_eq!(owner.prepare_dense_measurement(&ordinary, &mut budget).err(), Some(DenseMeasurementErrorV2::Limit));
        assert!(budget.usage().attempts > 0 && budget.usage().work > 0);
    });
}
#[test]
fn explicit_required_executor_denial_releases_extra_without_fallback_or_partial_comparison() {
    for case in [Case::Tiled, Case::Wide] {
        fixture_case(case, 1, false, |file, input, _, _, _| {
            let held = Arc::new(AtomicU64::new(0));
            let calls = Arc::new(AtomicUsize::new(0));
            let actual = reserver(held.clone(), calls.clone());
            let owner =
                load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), &actual).unwrap();
            let ordinary = owner.forward_local(PROMPT, &mut LocalWideForwardBudgetV2::new(run_limits()).unwrap(), &actual).unwrap();
            let before = held.load(Ordering::SeqCst);
            let delegate = actual.clone();
            let runtime_calls = Arc::new(AtomicUsize::new(0));
            let observed = runtime_calls.clone();
            let deny_runtime: TirMemoryReserverV1 = Arc::new(move |role, class, execution, bytes| {
                if role == "tir-dense-measurement-extra-v2" {
                    delegate(role, class, execution, bytes)
                } else {
                    observed.fetch_add(1, Ordering::SeqCst);
                    Err("injected explicit executor denial".into())
                }
            });
            let mut budget = DenseMeasurementBudgetV2::new(dense_limits()).unwrap();
            let prepared = owner.prepare_dense_measurement(&ordinary, &mut budget).unwrap();
            // Execute is a generic runner rejection, not Miner/arithmetic blame.
            assert_eq!(prepared.run(PROMPT, &deny_runtime).err(), Some(DenseMeasurementErrorV2::Execute));
            assert_eq!(runtime_calls.load(Ordering::SeqCst), 1);
            assert_eq!(held.load(Ordering::SeqCst), before);
            assert!(budget.usage().work > 0 && budget.usage().retained_requests > 0);
        });
    }
}

#[test]
fn long_prefill_one_post_leaf_quote_covers_bridge_prompt_before_any_runner_lease() {
    for case in [Case::LongNarrow, Case::LongWide] {
        fixture_case(case, 1, false, |file, input, _, _, r| {
            let owner =
                load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).unwrap();
            let mut l = dense_limits();
            l.runner.max_positions = input.original_context.max_context_tokens;
            // Metadata-only projection: no ordinary output is fabricated and no
            // millions-position CPU loop is run just to inspect the required quote.
            let q = owner.quote_dense_measurement(l).unwrap();
            assert_eq!(q.leaves, 1);
            let encoded = 4 + 4 * u64::from(input.original_context.declared_prefill_tokens);
            assert!(encoded >= (8 << 20) - 4);
            assert!(q.bridge_prompt_writer_bytes >= encoded);
            assert!(q.extra_requested_bytes >= q.bridge_prompt_writer_bytes + q.constructor_bytes);
            l.max_extra_bytes = q.bridge_prompt_writer_bytes - 1;
            assert_eq!(owner.quote_dense_measurement(l).err(), Some(DenseMeasurementErrorV2::Limit));
        });
    }
}
#[test]
fn narrow_runner_work_cap_is_enforced_independently_of_larger_aggregate_cap() {
    fixture_case(Case::Tiled, 1, false, |file, input, _, _, r| {
        let owner = load_local_wide_forward_v2(file, input, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), r).unwrap();
        let mut l = dense_limits();
        let q = owner.quote_dense_measurement(l).unwrap();
        assert!(q.runner_work > 0);
        l.runner.max_work = q.runner_work - 1;
        assert!(l.max_work_per_attempt > q.work);
        assert_eq!(owner.quote_dense_measurement(l).err(), Some(DenseMeasurementErrorV2::Limit));
    });
}
