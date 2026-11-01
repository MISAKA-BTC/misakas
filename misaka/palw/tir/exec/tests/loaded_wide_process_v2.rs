//! OS-main, test-only actual File/Loaded/CPU/stdio child integration.
//! Requires the audited Source build profile; no unknown-profile success marker.
//! Never counts as a default Rust test-thread-stack pass or native activation.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
    //! Test-only OS-main harness. No libtest stdout, SDK/native registration,
    //! computation certification or Rust default-test-stack pass is claimed.
    //! Real child File->Loaded->ordinary CPU->Hello2 stdio, never canned responses.
    use kaspa_consensus_core::{
        Hash64,
        palw_artifact::{PalwArtifactMerkleFrontierV1, artifact_leaf_parts_v1},
        palw_native_resource_v1::{SourceProgramLocalLimitsV1, require_source_program_local_profile_v1},
        palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
        palw_tir_step_v1::{
            PalwTirStepBindingV1, PalwTirStepSpaceV2, PalwTirVerifiedBindingV2, palw_tir_execution_root_v1, verify_tir_binding_v2,
        },
        palw_v2::PalwJobContextV2,
    };
    use misaka_palw_tir::{DType, Ref, TirProgramV1, builder::ProgramBuilder, program::LOGITS_I64_Q24_SCHEME_V2};
    use misaka_palw_tir_artifact::{bounded_v1::PalwTirHeaderLimitsV1, write_container_v1};
    use misaka_palw_tir_exec::{
        node::{
            cell_ipc_v2::{
                CellRequestWideV2 as Request, CellResponseWideV2 as Response, CellValueV1, WideCellLimitsV2, WideCellUsageV2, codec,
                deadline_unix::{
                    WideUnixDeadlineV2, WideUnixReadV2, WideUnixWriteV2, unix_stdio_adapter_backing_v2,
                    unix_stdio_deadline_adapters_v2,
                },
                process::{WideCellProcessV2, WideProcessOwnerLimitsV2},
            },
            memory::TirMemoryReserverV1,
            owned_artifact_params_v2::{
                OwnedParamLayoutOriginV2, OwnedParamLimitsV2,
                loaded::{
                    LoadedWideArtifactV2, LoadedWideInputsV2, LoadedWideLimitsV2, LoadedWideStartupBudgetV2, load_wide_artifact_v2,
                },
            },
        },
        plan::TirPlanCompileLimitsV2,
    };
    use std::{
        fs::{self, File},
        io::{self, Read},
        path::{Path, PathBuf},
        process::{Command, ExitCode},
        sync::{
            Arc,
            atomic::{AtomicU64, AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };
    const EXTERNAL: u64 = 8 << 20;
    const ALLOWANCE: u64 = 32 << 20;
    const CYCLES: u64 = 2;
    const CHILD_MARKER: &str = "a3-loaded-wide-v2-os-main";
    const RECORD_MAGIC: u64 = 0x41334c4f41443231;
    const RECORD_WORDS: usize = 32;
    const RECORD_BYTES: usize = RECORD_WORDS * 8;
    const CONTROL_FILE_CAP: u64 = 4096;
    const TIMEOUT: Duration = Duration::from_secs(10);
    type Process = WideCellProcessV2<WideUnixReadV2, WideUnixWriteV2, WideUnixDeadlineV2>;

    // Finite external TEST inventory includes independent pins/program/layout,
    // native fixture literals, command/environment/TempDir paths, PID+256B control
    // record, counters/callbacks, source/response copies, stdio/runtime metadata and
    // std stdin/stdout buffers. This is not allocator/RSS or total-process accounting.
    struct Pins {
        program: TirProgramV1,
        binding: PalwTirStepBindingV1,
        verified: PalwTirVerifiedBindingV2,
        ctx: PalwJobContextV2,
    }
    impl Pins {
        fn literal() -> Self {
            let mut b = ProgramBuilder::new(2, 1 << 18);
            let signed = b.param("signed", DType::I8, &[2], false);
            let index = b.param("index", DType::Idx, &[2], false);
            let logits = b.param("logits", DType::I64, &[2], false);
            let mut pre = b.block("pre", vec![]);
            let signed = pre.reshape_fixed(signed, &[2]);
            pre.commit(signed);
            let index = pre.reshape_fixed(index, &[2]);
            pre.commit(index);
            let pre = pre.finish(&[]);
            let mut post = b.block("post", vec![]);
            let logits = post.reshape_fixed(logits, &[2]);
            post.commit(logits);
            let Ref::Node(logits) = logits else { unreachable!() };
            let post = post.finish(&[]);
            let mut p = b.finish(pre, vec![], post, logits);
            p.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
            // Independent input literals are pinned BEFORE candidate file writing.
            let data = [
                vec![128, 127],
                vec![0, 0, 0, 0, 255, 255, 255, 255],
                vec![0, 0, 0, 0, 0, 0, 0, 128, 255, 255, 255, 255, 255, 255, 255, 127],
            ];
            let mut frontier = PalwArtifactMerkleFrontierV1::new();
            for (j, raw) in data.iter().enumerate() {
                frontier.push(artifact_leaf_parts_v1(&p.params[j].name, None, 0, raw));
            }
            let artifact = frontier.root().unwrap();
            let class = PalwTirClassV1 {
                version: 1,
                program: p.encode(),
                layout: PalwTirLayoutV1 {
                    version: 1,
                    max_context: 16,
                    checkpoint_interval: 1,
                    h_tile: 1,
                    commit_tiles: vec![4; 3],
                    state_tiles: vec![],
                },
                tokenizer_id: Hash64::from_u64_word(18),
            };
            let zero = Hash64::from_u64_word(0);
            let ctx = PalwJobContextV2 {
                version: 2,
                network_id: b"testnet-12".to_vec(),
                job_id: Hash64::from_u64_word(42),
                job_nullifier: zero,
                assignment_id: zero,
                execution_seed: [3; 32],
                model_profile_id: zero,
                runtime_manifest_hash: zero,
                runtime_class_id: zero,
                shape_profile_id: class.class_id(&artifact),
                trace_scheme_id: Hash64::from_bytes(LOGITS_I64_Q24_SCHEME_V2),
                cu_ruleset_id: zero,
                tokenizer_id: zero,
                prompt_token_ids_hash: zero,
                declared_prefill_tokens: 1,
                exact_decode_tokens: 1,
                max_context_tokens: 16,
            };
            let count = PalwTirStepSpaceV2::new(&class).unwrap().leaf_count_capped(&ctx, 1024).unwrap();
            let trace = Hash64::from_u64_word(61);
            let step = Hash64::from_u64_word(62);
            let binding = PalwTirStepBindingV1 {
                version: 1,
                job_context: ctx.clone(),
                class: class.clone(),
                artifact_root: artifact,
                full_logits_trace_root: trace,
                step_leaf_count: count,
                step_merkle_root: step,
                committed_execution_root: palw_tir_execution_root_v1(&ctx.context_hash(), &trace, &ctx.shape_profile_id, count, &step),
            };
            let verified = verify_tir_binding_v2(&binding, 1024).unwrap();
            Self { program: p, binding, verified, ctx }
        }
        fn inputs(&self) -> LoadedWideInputsV2<'_> {
            LoadedWideInputsV2 {
                expected: &self.binding,
                verified: &self.verified,
                original_context: &self.ctx,
                external_retained_bytes: EXTERNAL,
                external_ticket: self,
            }
        }
        fn write_original(&self, path: &Path) {
            let data = [
                vec![128, 127],
                vec![0, 0, 0, 0, 255, 255, 255, 255],
                vec![0, 0, 0, 0, 0, 0, 0, 128, 255, 255, 255, 255, 255, 255, 255, 127],
            ];
            write_container_v1(
                path,
                &self.program,
                vec![],
                self.binding.class.tokenizer_id.as_bytes(),
                "independent-literal-process-test".into(),
                &mut |j, _| Ok(data[j as usize].clone()),
            )
            .unwrap();
        }
    }
    fn startup_limits() -> LoadedWideLimitsV2 {
        LoadedWideLimitsV2 {
            header: PalwTirHeaderLimitsV1 {
                max_file_bytes: 1 << 20,
                max_header_bytes: 1 << 16,
                max_program_bytes: 1 << 18,
                max_layout_bytes: 1 << 16,
                max_tensors: 32,
                max_meta_bytes: 1024,
                max_native_bytes: 1 << 16,
                max_copy_bytes: 1 << 16,
                max_work: 1 << 32,
                max_operation_bytes: 1 << 30,
                external_retained_bytes: 0,
            },
            source: SourceProgramLocalLimitsV1 {
                max_program_bytes: 1 << 18,
                max_scan_steps: 1 << 32,
                max_nf_items: 1 << 20,
                max_native_requests: 1 << 20,
            },
            compile: TirPlanCompileLimitsV2 {
                max_quote_visits: 1 << 24,
                max_retained_bytes: 1 << 22,
                max_scratch_bytes: 1 << 24,
                max_operation_bytes: 1 << 30,
                external_retained_bytes: 0,
            },
            params: OwnedParamLimitsV2 {
                max_tensor_bytes: 1 << 20,
                max_payload_bytes: 1 << 20,
                max_chunk_bytes: 32768,
                max_metadata_bytes: 1 << 20,
                max_native_bytes: 1 << 20,
                max_inventory_leaves: 1024,
                max_work_per_attempt: 1 << 32,
                max_attempts: 4,
                max_total_read_bytes: 1 << 22,
                max_total_inventory_leaves: 4096,
                max_total_work: 1 << 36,
                max_operation_bytes: 1 << 30,
                external_retained_bytes: 0,
            },
            max_new_owner_bytes: ALLOWANCE,
            max_operation_bytes: 1 << 30,
            max_attempts: 4,
            max_total_read_bytes: 1 << 24,
            max_total_work: 1 << 40,
            max_total_source_steps: 1 << 36,
            max_retained_claims: 4 * ALLOWANCE,
        }
    }
    fn cell_limits(external: u64) -> WideCellLimitsV2 {
        WideCellLimitsV2 {
            max_frame_bytes: 1 << 20,
            max_decoded_bytes: 1 << 20,
            max_operation_bytes: 1 << 30,
            max_total_read_bytes: 1 << 24,
            max_total_write_bytes: 1 << 24,
            max_messages: 32,
            max_steps: 4,
            max_program_bytes: 1 << 18,
            max_params: 32,
            max_param_bytes: 1 << 20,
            max_values: 32,
            max_lanes: 1024,
            max_carry_vectors: 8,
            max_rank: 8,
            max_name_bytes: 256,
            max_workspace_bytes: 512 << 20,
            max_work: 1 << 48,
            external_retained_bytes: external,
        }
    }
    struct HeldGuard(Arc<AtomicU64>, u64);
    impl Drop for HeldGuard {
        fn drop(&mut self) {
            self.0.fetch_sub(self.1, Ordering::SeqCst);
        }
    }
    fn host(held: Arc<AtomicU64>, calls: Arc<AtomicUsize>) -> TirMemoryReserverV1 {
        Arc::new(move |role, _, _, bytes| {
            assert_eq!(role, "tir-loaded-wide-artifact-v2");
            assert_eq!(bytes, ALLOWANCE);
            calls.fetch_add(1, Ordering::SeqCst);
            held.fetch_add(bytes, Ordering::SeqCst);
            Ok(Box::new(HeldGuard(held.clone(), bytes)))
        })
    }
    fn request_bytes(program: u64) -> u64 {
        // Hello13; Open prefix8+tag1+program(u64+P)+occ8+count4+3*(param2+None1+bytesLen8)+26;
        // Step prefix8+tag1+token4+occ8+emptyCarryCount4=25; Close prefix8+tag1=9.
        13 + CYCLES * (program + 88 + 25 + 9)
    }
    fn response_bytes() -> u64 {
        // Ready8+1+4+8+20+1=42; Opened9; Closed9. Stepped body204:
        // tag1+count4+3*(slot4+block1+None1+node2+dtype1+rank4+dim4+lanesLen8+lanes32)
        // +emptyCarryCount4+logitsLen8+twoI64*8. The whole frame is212.
        42 + CYCLES * (9 + 212 + 9)
    }
    fn build_work(program: u64) -> u64 {
        let metadata = (program + 4096) * 4 * 3 * 128;
        let body = program + 80;
        metadata + (body + 4096) * 128
    }
    fn expected_step() -> Response {
        Response::Stepped {
            values: vec![
                CellValueV1 {
                    slot: 0,
                    block: 0,
                    layer: None,
                    node: 0,
                    dtype: DType::I8.tag(),
                    shape: vec![2],
                    lanes: vec![-128, 127],
                },
                CellValueV1 {
                    slot: 1,
                    block: 0,
                    layer: None,
                    node: 1,
                    dtype: DType::Idx.tag(),
                    shape: vec![2],
                    lanes: vec![0, u32::MAX as i128],
                },
                CellValueV1 {
                    slot: 2,
                    block: 1,
                    layer: None,
                    node: 0,
                    dtype: DType::I64.tag(),
                    shape: vec![2],
                    lanes: vec![i64::MIN as i128, i64::MAX as i128],
                },
            ],
            carry: vec![],
            logits: vec![i64::MIN, i64::MAX],
        }
    }
    fn poll_fixed<const N: usize>(path: &Path, deadline: Instant) -> io::Result<[u8; N]> {
        assert!(N <= CONTROL_FILE_CAP as usize);
        loop {
            match File::open(path) {
                Ok(mut file) => {
                    let bytes = file.metadata()?.len();
                    if bytes > CONTROL_FILE_CAP || bytes != N as u64 {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    let mut out = [0u8; N];
                    file.read_exact(&mut out)?;
                    let mut extra = [0u8; 1];
                    if file.read(&mut extra)? != 0 {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    return Ok(out);
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn publish_record(directory: &Path, words: [u64; RECORD_WORDS]) -> io::Result<()> {
        let mut bytes = [0u8; RECORD_BYTES];
        for (value, raw) in words.iter().zip(bytes.chunks_exact_mut(8)) {
            raw.copy_from_slice(&value.to_le_bytes());
        }
        // Publish only after complete bounded data, no reader sees a partial record.
        fs::write(directory.join("record.tmp"), bytes)?;
        fs::rename(directory.join("record.tmp"), directory.join("record.bin"))
    }
    fn read_record(directory: &Path) -> [u64; RECORD_WORDS] {
        let bytes = poll_fixed::<RECORD_BYTES>(&directory.join("record.bin"), Instant::now() + TIMEOUT).unwrap();
        let mut out = [0u64; RECORD_WORDS];
        for (value, raw) in out.iter_mut().zip(bytes.chunks_exact(8)) {
            *value = u64::from_le_bytes(raw.try_into().unwrap());
        }
        assert_eq!(out[0], RECORD_MAGIC, "missing complete child source marker");
        out
    }
    fn record_budget(out: &mut [u64; RECORD_WORDS], budget: &LoadedWideStartupBudgetV2) {
        let u = budget.usage();
        let source = budget.source_usage();
        let params = budget.params_usage();
        out[4] = u.attempts;
        out[5] = u.charged_read_bytes;
        out[6] = u.charged_work;
        out[7] = u.retained_claims;
        out[8] = source.0;
        out[9] = source.1;
        out[10] = source.2;
        out[11] = params.attempts;
        out[12] = params.read_bytes;
        out[13] = params.leaves;
        out[14] = params.work;
    }
    fn record_session(out: &mut [u64; RECORD_WORDS], u: WideCellUsageV2) {
        out[15] = u.read_bytes;
        out[16] = u.written_bytes;
        out[17] = u.messages;
        out[18] = u.steps;
        out[19] = u.work;
        out[20] = u.exported_bytes;
    }
    fn child_record(path: &Path) -> [u64; RECORD_WORDS] {
        let mut r = [0u64; RECORD_WORDS];
        r[0] = RECORD_MAGIC;
        r[31] = 1; // OS-main stack, not libtest-thread stack.
        if require_source_program_local_profile_v1().is_err() {
            r[2] = 5;
            return r;
        }
        r[1] = 1;
        // Independently declared ORIGINAL literal pins, never read from the file.
        let pins = Pins::literal();
        assert_eq!(pins.binding.class.tokenizer_id, Hash64::from_u64_word(18));
        assert_eq!(pins.ctx.tokenizer_id, Hash64::from_u64_word(0));
        assert_eq!(pins.ctx.max_context_tokens, 16);
        let mut budget = LoadedWideStartupBudgetV2::new(startup_limits()).unwrap();
        let held = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let reserve = host(held.clone(), calls.clone());
        // Exactly ONE artifact File open in this child. The File moves into Loaded;
        // header, Source and native values are read through that same descriptor.
        let file = match File::open(path) {
            Ok(file) => {
                r[27] = 1;
                file
            }
            Err(_) => {
                r[2] = 4;
                return r;
            }
        };
        let owner = match load_wide_artifact_v2(file, pins.inputs(), &mut budget, Some(&reserve)) {
            Ok(owner) => owner,
            Err(e) => {
                r[2] = 1;
                r[3] = e as u64 + 1;
                record_budget(&mut r, &budget);
                r[21] = calls.load(Ordering::SeqCst) as u64;
                r[22] = held.load(Ordering::SeqCst);
                return r;
            }
        };
        r[28] = 1;
        assert_eq!(owner.artifact_root(), pins.binding.artifact_root);
        assert_eq!(owner.original_context(), &pins.ctx);
        assert_eq!(owner.layout_origin(), OwnedParamLayoutOriginV2::IndependentlyExpectedOnly);
        assert_eq!(held.load(Ordering::SeqCst), ALLOWANCE);
        let p = pins.binding.class.program.len() as u64;
        let mut limits = cell_limits(owner.quote().operation_retained_bytes);
        limits.max_total_read_bytes = request_bytes(p);
        limits.max_total_write_bytes = response_bytes();
        limits.max_steps = CYCLES;
        limits.max_messages = 2 + 6 * CYCLES;
        let stdin = io::stdin();
        let stdout = io::stdout();
        // This is a finite cap on REAL input bytes, no skipped harness/junk bytes.
        // Exactly two real Open/Step/Close cycles end at the cap; only after the last
        // actual Close does Take report EOF, permitting ordinary serve to return.
        let mut reader = stdin.lock().take(request_bytes(p));
        let mut writer = stdout.lock();
        r[29] = 1;
        match owner.serve(&mut reader, &mut writer, limits) {
            Ok(usage) => {
                record_session(&mut r, usage);
                if usage.read_bytes != request_bytes(p)
                    || usage.written_bytes != response_bytes()
                    || usage.steps != CYCLES
                    || usage.messages != 2 + 6 * CYCLES
                {
                    r[2] = 3;
                }
            }
            Err(e) => {
                r[2] = 2;
                r[3] = e.kind as u64 + 1;
                record_session(&mut r, e.usage);
            }
        }
        r[30] = u64::from(reader.limit() == 0);
        record_budget(&mut r, &budget);
        r[23] = u64::from(owner.original_context().max_context_tokens);
        r[24] = u64::from(pins.binding.class.tokenizer_id == Hash64::from_u64_word(18)) * 18;
        r[25] = u64::from(owner.original_context().tokenizer_id != Hash64::from_u64_word(0));
        r[26] = CYCLES;
        drop(owner);
        r[21] = calls.load(Ordering::SeqCst) as u64;
        r[22] = held.load(Ordering::SeqCst);
        if r[21] != 1 || r[22] != 0 || r[30] != 1 {
            r[2] = 3;
        }
        r
    }
    // Only inspect this trusted helper's specific positive direct-child PID. On
    // failure kill is authorized ONLY after waitpid(WNOHANG)==0 establishes it is
    // still our live child. Never signal a recycled/nonchild PID. OS kill/wait and
    // process-group/wall-time guarantees remain outside this source test.
    struct PidCleanup(libc::pid_t);
    impl PidCleanup {
        fn assert_reaped(&self) {
            let mut status = 0;
            loop {
                // SAFETY: inspect only our specific direct child, never any PID.
                let got = unsafe { libc::waitpid(self.0, &mut status, libc::WNOHANG) };
                if got < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                assert_eq!(got, -1, "process owner must reap; positive means test reaped it, zero means live");
                assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ECHILD));
                break;
            }
        }
    }
    impl Drop for PidCleanup {
        fn drop(&mut self) {
            let mut status = 0;
            loop {
                // SAFETY: specific direct-child PID only, nonblocking inspection.
                let got = unsafe { libc::waitpid(self.0, &mut status, libc::WNOHANG) };
                if got < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if got == 0 {
                    // SAFETY: immediately preceding wait established our live child.
                    let _ = unsafe { libc::kill(self.0, libc::SIGKILL) };
                    loop {
                        // SAFETY: wait only for this same direct child after kill.
                        let waited = unsafe { libc::waitpid(self.0, &mut status, 0) };
                        if waited < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        break;
                    }
                }
                break;
            }
        }
    }
    fn parent() {
        let started = Instant::now();
        let pins = Pins::literal();
        let directory = tempfile::TempDir::new().unwrap();
        let artifact = directory.path().join("original.palwtir");
        pins.write_original(&artifact);
        let original_ctx = pins.ctx.clone();
        let original_class = pins.binding.class.clone();
        let held = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let reserve = host(held.clone(), calls.clone());
        let mut startup = LoadedWideStartupBudgetV2::new(startup_limits()).unwrap();
        let loaded: LoadedWideArtifactV2<'_> =
            load_wide_artifact_v2(File::open(&artifact).unwrap(), pins.inputs(), &mut startup, Some(&reserve)).unwrap();
        assert_eq!(loaded.artifact_root(), pins.binding.artifact_root);
        assert_eq!(loaded.layout_origin(), OwnedParamLayoutOriginV2::IndependentlyExpectedOnly);
        assert_eq!(loaded.original_context(), &original_ctx);
        assert_eq!(pins.binding.class, original_class);
        assert_eq!(pins.binding.class.tokenizer_id, Hash64::from_u64_word(18));
        assert_eq!(loaded.original_context().tokenizer_id, Hash64::from_u64_word(0));
        assert_eq!(loaded.original_context().max_context_tokens, 16);
        assert_eq!(held.load(Ordering::SeqCst), ALLOWANCE);
        {
            let source = loaded.resident_open().unwrap();
            let p = pins.binding.class.program.len() as u64;
            let one_build = build_work(p);
            let mut l = cell_limits(loaded.quote().operation_retained_bytes);
            l.max_total_read_bytes = response_bytes();
            l.max_total_write_bytes = request_bytes(p);
            l.max_steps = CYCLES;
            l.max_messages = 2 + 6 * CYCLES;
            let owner_limits = WideProcessOwnerLimitsV2 {
                max_open_build_work: CYCLES * one_build,
                adapter_retained_bytes: unix_stdio_adapter_backing_v2().shared_heap_bytes,
            };
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .arg("--a3-loaded-wide-child-v2")
                .env("MISAKA_A3_LOADED_CHILD", CHILD_MARKER)
                .env("MISAKA_A3_LOADED_FILE", &artifact)
                .env("MISAKA_A3_LOADED_CONTROL", directory.path());
            // Production stderr-null policy is preserved, no pre_exec/dup2. Child
            // diagnostics are ONLY its bounded fixed-size PID/completion records.
            let mut cleanup = None;
            let spawned = Process::spawn_with_deadline_adapters(&mut command, l, owner_limits, |out, input| {
                let adapters = unix_stdio_deadline_adapters_v2(out, input, TIMEOUT).map_err(|e| e.to_string())?;
                let raw = poll_fixed::<8>(&directory.path().join("pid.bin"), Instant::now() + TIMEOUT).map_err(|e| e.to_string())?;
                let pid = libc::pid_t::try_from(u64::from_le_bytes(raw)).map_err(|_| "child PID exceeds platform range")?;
                if pid <= 0 {
                    return Err("child PID must be positive".into());
                }
                cleanup = Some(PidCleanup(pid));
                Ok(adapters)
            });
            let cleanup = cleanup.expect("true helper must publish bounded PID before Hello2");
            let mut process = match spawned {
                Ok(process) => process,
                Err(error) => {
                    let record = read_record(directory.path());
                    cleanup.assert_reaped();
                    panic!(
                        "real Loaded child failed: fixed_kind={} source_profile={} startup_attempts={} client_usage={:?} cleanup={:?}",
                        record[2], record[1], record[4], error.usage, error.cleanup_error
                    );
                }
            };
            let initial = process.usage();
            assert_eq!(initial.client.written_bytes, 13);
            assert_eq!(initial.client.read_bytes, 42);
            assert_eq!(initial.client.messages, 2);
            assert_eq!(initial.client.steps, 0);
            assert_eq!(initial.open_build_work, 0);
            let ready = Response::Ready { version: 2, name: "cpu-prepared-wide-v2".into(), capacity: None };
            assert_eq!(codec::encoded_length_response(&ready).unwrap() + 8, 42);
            let mut exports = codec::native_backing_response(&ready).unwrap();
            assert_eq!(initial.client.exported_bytes, exports);
            let step = expected_step();
            assert_eq!(codec::encoded_length_response(&step).unwrap(), 204);
            let mut prior = initial;
            for cycle in 1..=CYCLES {
                assert_eq!(process.open_resident(&source, 0..2).unwrap(), Response::Opened);
                exports += codec::native_backing_response(&Response::Opened).unwrap();
                let response = process.request(&Request::Step { token: 0, occ: (0, 2), carry_in: vec![] }).unwrap();
                assert_eq!(response, step); // Actual child Loaded CPU produced all native values/logits.
                exports += codec::native_backing_response(&response).unwrap();
                assert_eq!(process.request(&Request::Close).unwrap(), Response::Closed);
                exports += codec::native_backing_response(&Response::Closed).unwrap();
                let now = process.usage();
                assert_eq!(now.client.steps, cycle);
                assert_eq!(now.client.messages, 2 + 6 * cycle);
                assert_eq!(now.client.written_bytes, 13 + cycle * (p + 122));
                assert_eq!(now.client.read_bytes, 42 + cycle * 230);
                assert_eq!(now.client.exported_bytes, exports);
                assert_eq!(now.open_build_work, cycle * one_build);
                assert_eq!(now.total_work, now.client.work + now.open_build_work);
                assert!(now.total_work <= l.max_work);
                assert!(now.client.work > prior.client.work);
                assert!(now.client.exported_bytes > prior.client.exported_bytes);
                assert_eq!(held.load(Ordering::SeqCst), ALLOWANCE);
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                assert_eq!(startup.usage().attempts, 1);
                assert_eq!(startup.params_usage().read_bytes, 26);
                prior = now;
            }
            assert!(!process.poisoned());
            // Completion is observed BEFORE terminate/Drop: real Close+bounded EOF let
            // child serve return and drop its Loaded host lease normally, not via kill.
            let record = read_record(directory.path());
            assert_eq!(record[1], 1, "missing audited source profile marker");
            assert_eq!(record[2], 0, "child fixed failure kind");
            assert_eq!(record[4], 1);
            assert!(record[5] >= 26);
            assert!(record[6] > 0);
            assert_eq!(record[7], ALLOWANCE);
            assert_eq!(record[8], 1);
            assert!(record[9] >= record[10]);
            assert!(record[10] > 0);
            assert_eq!(record[11], 1);
            assert_eq!(record[12], 26);
            assert_eq!(record[13], 3);
            assert!(record[14] > 0);
            assert_eq!(record[15], request_bytes(p));
            assert_eq!(record[16], response_bytes());
            assert_eq!(record[17], 2 + 6 * CYCLES);
            assert_eq!(record[18], CYCLES);
            assert!(record[19] > 0);
            assert_eq!(record[20], 0);
            assert_eq!(record[21], 1);
            assert_eq!(record[22], 0);
            assert_eq!(record[23], 16);
            assert_eq!(record[24], 18);
            assert_eq!(record[25], 0);
            assert_eq!(record[26], CYCLES);
            assert_eq!(&record[27..], [1, 1, 1, 1, 1]);
            assert_eq!(process.usage(), prior);
            drop(process);
            cleanup.assert_reaped();
            assert_eq!(held.load(Ordering::SeqCst), ALLOWANCE);
        }
        drop(loaded);
        assert_eq!(held.load(Ordering::SeqCst), 0);
        assert_eq!(pins.ctx, original_ctx);
        assert_eq!(pins.binding.class, original_class);
        assert_eq!(startup.usage().retained_claims, ALLOWANCE); // no refund after owner Drop.
        eprintln!(
            "A3_LOADED_PROCESS_V2_PASS profile=audited-source-v1 actual_file=1 loaded=1 cpu_i64_endpoints=1 cycles=2 client_steps=2 server_steps=2 params_read=26 child_guard_released=1 reaped=1 stack=os-main elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    fn child() -> ExitCode {
        let Some(directory) = std::env::var_os("MISAKA_A3_LOADED_CONTROL").map(PathBuf::from) else {
            return ExitCode::FAILURE;
        };
        let Some(path) = std::env::var_os("MISAKA_A3_LOADED_FILE").map(PathBuf::from) else {
            return ExitCode::FAILURE;
        };
        // Fixed local control bytes only, no harness/stdout model protocol output.
        let pid = u64::from(std::process::id()).to_le_bytes();
        if fs::write(directory.join("pid.tmp"), pid)
            .and_then(|_| fs::rename(directory.join("pid.tmp"), directory.join("pid.bin")))
            .is_err()
        {
            return ExitCode::FAILURE;
        }
        let record = child_record(&path);
        let okay = record[1] == 1 && record[2] == 0 && record[22] == 0 && record[30] == 1;
        if publish_record(&directory, record).is_err() {
            return ExitCode::FAILURE;
        }
        if okay { ExitCode::SUCCESS } else { ExitCode::FAILURE }
    }
    pub fn run() -> ExitCode {
        let arg = std::env::args_os().nth(1);
        let marker = std::env::var_os("MISAKA_A3_LOADED_CHILD");
        if arg.as_deref() == Some(std::ffi::OsStr::new("--a3-loaded-wide-child-v2"))
            && marker.as_deref() == Some(std::ffi::OsStr::new(CHILD_MARKER))
        {
            return child();
        }
        if arg.is_some() || marker.is_some() {
            eprintln!("A3_LOADED_PROCESS_V2_REFUSED unexpected-dispatch stack=os-main");
            return ExitCode::FAILURE;
        }
        if require_source_program_local_profile_v1().is_err() {
            eprintln!("A3_LOADED_PROCESS_V2_UNSUPPORTED no_success_marker stack=os-main");
            return ExitCode::from(77);
        }
        parent();
        ExitCode::SUCCESS
    }
}
fn main() -> std::process::ExitCode {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        supported::run()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        eprintln!("A3_LOADED_PROCESS_V2_UNSUPPORTED platform no_success_marker stack=os-main");
        std::process::ExitCode::from(77)
    }
}
