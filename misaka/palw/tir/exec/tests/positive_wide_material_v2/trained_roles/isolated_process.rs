//! Private diagnostic IPC, not a native/peer codec or authority. The sender is
//! this test's trusted framing producer. Borsh decoding here is NOT a general
//! adversarial peer decoder. Arithmetic, identities, all coverage and final
//! record authentication are still checked by the unchanged Core receiver.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    palw_logits_q24_v2::WideLogitsEventV2,
    palw_positive_check_v2::full_relation::wide_v2::{StepTargetV2, stream::ClaimsReaderV2},
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1},
    palw_step_refute::PalwStepInputRowV1,
    palw_tir_court_v1::{
        PalwTirParamOpeningV1,
        source_only::wide_v2::{TirSourceOnlyMaterialV2, WideGeneratedIdsPinV2},
    },
    palw_tir_step_v1::PalwTirStepBindingV1,
};
use std::{
    cell::RefCell,
    io::{self, Write},
    mem::size_of,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    os::unix::net::UnixStream,
    process::{Child, Command, Stdio},
    rc::Rc,
    time::Duration,
};
const CHILD_CAP: u64 = 2 << 30;
const ENCODE_OWNER: u64 = 128 << 20;
// Whole independent preparation/exports, valid-frame decoding, source auth and
// direct evaluation. Acquired before decoding. Core's record owner is additive.
const CHILD_EXTERNAL: u64 = (1 << 30) + (256 << 20) + (256 << 20) + (128 << 20);
const FRAME_CAP: usize = 16 << 20;
const HEADER_CAP: usize = 1 << 20;
const WIRE_CAP: u64 = 16 << 30;
const CHILD_TEST: &str = "trained_roles::isolated_process::node_child";
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Eq)]
struct Expected {
    profile: u8,
    class: Hash64,
    artifact: Hash64,
    registered_tokenizer: Hash64,
    job: PalwJobContextV2,
    miner_key: [u8; PUBKEY_BYTES],
}
fn worker_key() -> [u8; PUBKEY_BYTES] {
    *libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]).verification_key.as_ref()
}
fn trained_expected() -> Expected {
    Expected {
        profile: 1,
        class: hash(CLASS),
        artifact: hash(ROOT),
        registered_tokenizer: hash(TOKENIZER),
        job: original_job(),
        miner_key: worker_key(),
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
struct Header {
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    result: MinerResultCommit,
    miner_key: [u8; PUBKEY_BYTES],
    miner_signature: [u8; SIGNATURE_BYTES],
    panel_key: [u8; PUBKEY_BYTES],
    panel_signature: [u8; SIGNATURE_BYTES],
    commitment: Digest,
}
#[derive(BorshSerialize, BorshDeserialize)]
struct Frame {
    operands: PalwStepInputRowV1,
    params: PalwTirParamOpeningV1,
    prompt: Vec<u32>,
    openings: Vec<kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsOpeningV1>,
    pin: Option<(Hash64, Vec<u32>)>,
    target: Option<(PalwStepOpeningV1, PalwStepTileLeafV1, Option<WideLogitsEventV2>)>,
}
#[derive(BorshSerialize, BorshDeserialize, Debug)]
struct End {
    outcome: u8,
    checked: u64,
    submit_ns: u128,
    elements: u64,
    terms: u64,
    reads: u64,
    workspace_peak: u64,
    cpu_user_us: u64,
    cpu_system_us: u64,
    process_maxrss: i64,
    request_peak: u64,
    wire_received: u64,
    wire_sent: u64,
    model_read_denials: u32,
    pid: u32,
}
fn usage() -> (u64, u64, i64) {
    let mut u = std::mem::MaybeUninit::<libc::rusage>::uninit();
    assert_eq!(unsafe { libc::getrusage(libc::RUSAGE_SELF, u.as_mut_ptr()) }, 0);
    let u = unsafe { u.assume_init() };
    (
        (u.ru_utime.tv_sec as u64) * 1_000_000 + u.ru_utime.tv_usec as u64,
        (u.ru_stime.tv_sec as u64) * 1_000_000 + u.ru_stime.tv_usec as u64,
        u.ru_maxrss,
    )
}
fn emit(phase: &str, l: &Ledger, detail: serde_json::Value) {
    let (user, system, rss) = usage();
    println!(
        "ROLE_ISOLATED {}",
        serde_json::json!({"phase":phase,"pid":std::process::id(),"native_authority":false,
        "canonical_job":false,"full_context_execution":false,"node_process_isolated":true,"cpu_user_us":user,"cpu_system_us":system,
        "process_maxrss_native_units":rss,"logical_request_peak":l.peak.load(Ordering::SeqCst),"detail":detail})
    );
}
struct Endpoint {
    socket: UnixStream,
    received: u64,
    sent: u64,
    calls: u64,
}
#[derive(Clone)]
struct Channel(Rc<RefCell<Endpoint>>);
impl Channel {
    fn new(socket: UnixStream) -> io::Result<Self> {
        socket.set_read_timeout(Some(Duration::from_secs(60)))?;
        socket.set_write_timeout(Some(Duration::from_secs(60)))?;
        Ok(Self(Rc::new(RefCell::new(Endpoint { socket, received: 0, sent: 0, calls: 0 }))))
    }
    fn stop_write(&self) -> io::Result<()> {
        self.0.borrow().socket.shutdown(std::net::Shutdown::Write)
    }
}
impl Read for Channel {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut e = self.0.borrow_mut();
        let n = buf.len().min(usize::try_from(WIRE_CAP - e.received).unwrap());
        if n == 0 || e.calls >= 1 << 26 {
            return Err(io::Error::other("private IPC read budget"));
        }
        e.calls += 1;
        let n = e.socket.read(&mut buf[..n])?;
        e.received += n as u64;
        Ok(n)
    }
}
impl Write for Channel {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut e = self.0.borrow_mut();
        let n = buf.len().min(usize::try_from(WIRE_CAP - e.sent).unwrap());
        if n == 0 || e.calls >= 1 << 26 {
            return Err(io::Error::other("private IPC write budget"));
        }
        e.calls += 1;
        let n = e.socket.write(&buf[..n])?;
        e.sent += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().socket.flush()
    }
}
impl ClaimsReaderV2 for Channel {
    fn retained_request_bytes(&self) -> Option<u64> {
        Some((size_of::<Self>() + size_of::<Endpoint>() + 64) as u64)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
fn ioerr(e: impl std::fmt::Debug) -> String {
    format!("private IPC: {e:?}")
}
fn bytes(value: &impl BorshSerialize, cap: usize) -> R<Vec<u8>> {
    let n = borsh::object_length(value).map_err(ioerr)?;
    require(n <= cap, "private frame size before allocation")?;
    let mut b = exact(n)?;
    value.serialize(&mut b).map_err(ioerr)?;
    require(b.len() == n, "private frame length")?;
    Ok(b)
}
fn put(c: &mut Channel, b: &[u8], cap: usize) -> R<()> {
    require(b.len() <= cap, "private frame cap")?;
    c.write_all(&(b.len() as u32).to_le_bytes()).map_err(ioerr)?;
    c.write_all(b).map_err(ioerr)
}
fn get(c: &mut Channel, cap: usize) -> R<Vec<u8>> {
    let mut n = [0; 4];
    c.read_exact(&mut n).map_err(ioerr)?;
    let n = u32::from_le_bytes(n) as usize;
    require(n > 0 && n <= cap, "private frame cap before allocation")?;
    let mut b = exact(n)?;
    b.resize(n, 0);
    c.read_exact(&mut b).map_err(ioerr)?;
    Ok(b)
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn unhex(s: &str) -> R<Vec<u8>> {
    require(s.len() <= 32 << 10 && s.len().is_multiple_of(2), "independent configuration cap")?;
    s.as_bytes().chunks_exact(2).map(|p| u8::from_str_radix(std::str::from_utf8(p).map_err(ioerr)?, 16).map_err(ioerr)).collect()
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn denied(path: &str) -> R<()> {
    match File::open(path) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        _ => Err("cold child could read a denied source".into()),
    }
}
fn child_run() -> R<()> {
    let l = Arc::new(Ledger::default());
    let reserve = ledger_with_cap(l.clone(), CHILD_CAP);
    let external = reserve("cold-child-external", hash(CLASS), hash(CONTEXT), CHILD_EXTERNAL)?;
    child_scope(&l, &reserve)?;
    drop(external);
    require(l.live.load(Ordering::SeqCst) == 0, "cold child lease leak")?;
    Ok(())
}
fn child_scope(l: &Ledger, reserve: &TirMemoryReserverV1) -> R<()> {
    let e: Expected = borsh::from_slice(&unhex(&std::env::var("MISAKA_COLD_EXPECTED").map_err(ioerr)?)?).map_err(ioerr)?;
    if e.profile == 1 {
        require(e == trained_expected(), "trained independent configuration substituted")?;
    } else {
        require(
            e.profile == 0 && std::env::var("MISAKA_COLD_FIXTURE").as_deref() == Ok("synthetic-only"),
            "fixture configuration not authorized",
        )?;
    }
    let sentinel = std::env::var("MISAKA_COLD_SENTINEL").map_err(ioerr)?;
    denied(&sentinel)?;
    let denials = if e.profile == 1 {
        denied(ARTIFACT)?;
        denied(FIXTURE)?;
        3
    } else {
        1
    };
    cpu_limit(MeasurementWindow::ExtendedCost);
    emit("cold-node-source-denial", l, serde_json::json!({"denied_reads":denials,"no_model_or_fixture_loaded":true}));
    // This exact isolated test owns fd0. No parent model descriptor is passed.
    let mut channel = Channel::new(unsafe { UnixStream::from_raw_fd(0) }).map_err(ioerr)?;
    let raw = get(&mut channel, HEADER_CAP)?;
    let h: Header = borsh::from_slice(&raw).map_err(ioerr)?;
    require(
        h.binding.job_context == e.job
            && h.binding.artifact_root == e.artifact
            && h.binding.class.class_id(&e.artifact) == e.class
            && h.binding.class.tokenizer_id == e.registered_tokenizer
            && h.miner_key == e.miner_key,
        "independent metadata/header mismatch",
    )?;
    let rooted = AdmissionContext {
        scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
        expected_job: digest(e.job.job_id),
        expected_execution: digest(e.job.context_hash()),
        expected_miner: principal(&e.miner_key),
        expected_input: digest(e.job.prompt_token_ids_hash),
        registered_class: digest(e.class),
        artifact: digest(e.artifact),
        program: digest(h.binding.class.graph_ir_root()),
        tokenizer: digest(e.job.tokenizer_id),
        execution_rules: digest(e.job.cu_ruleset_id),
        selector: greedy_selector_id(),
        required_form: ClaimForm::PositiveResultV2,
    };
    let limits = if e.profile == 1 { MeasurementWindow::ExtendedCost.node_limits() } else { full_limits() };
    let prep = FullRelationPreparationV2::authenticate(
        &rooted,
        &h.binding.class,
        e.artifact,
        &e.job,
        PalwPromptIdsFormV1::Flat,
        limits,
        domains(),
        h.result,
        &h.miner_key,
        &h.miner_signature,
    )
    .map_err(ioerr)?;
    let relations = prep.plan().summary().relations;
    let mut rb = record_limits(CHILD_EXTERNAL);
    rb.metadata_auth_bytes = 64 << 20;
    rb.max_extra_heap_bytes = CHILD_CAP;
    let acquire = |whole| reserve_stream_increment(l, reserve, CHILD_EXTERNAL, whole, "cold-node-stream-increment");
    let mut node = prep
        .begin_stream(
            h.binding,
            h.generated,
            principal(&h.panel_key),
            h.commitment,
            channel.clone(),
            h.panel_key,
            h.panel_signature,
            rb,
            &acquire,
        )
        .map_err(ioerr)?;
    channel.write_all(&[0]).map_err(ioerr)?;
    let mut submit = 0u128;
    let (mut elements, mut terms, mut reads, mut peak) = (0u64, 0u64, 0u64, 0u64);
    for i in 0..relations {
        let raw = get(&mut channel, FRAME_CAP)?;
        let f: Frame = borsh::from_slice(&raw).map_err(ioerr)?;
        let pin = f.pin.map(|(rows_root, generated_token_ids)| WideGeneratedIdsPinV2 { rows_root, generated_token_ids });
        let m = TirSourceOnlyMaterialV2 {
            operands: &f.operands,
            params: &f.params,
            prompt_token_ids: &f.prompt,
            prompt_ids_openings: &f.openings,
            decode_tokens: pin.as_ref(),
        };
        let target = f.target.as_ref().map(|(opening, preimage, trace)| StepTargetV2 { opening, preimage, trace: trace.as_ref() });
        let mut diag = kaspa_consensus_core::palw_tir_court_v1::source_only::wide_v2::EvaluationDiagnosticsV2::default();
        let t = Instant::now();
        let result = node.submit_with_diagnostics(&m, target, &mut diag).map_err(ioerr)?;
        submit += t.elapsed().as_nanos();
        let w = diag.work.ok_or("cold evaluation work unavailable")?;
        elements = add(elements, w.elements)?;
        terms = add(terms, w.terms)?;
        reads = add(reads, diag.source_reads)?;
        peak = peak.max(diag.workspace_bytes.ok_or("cold workspace observation unavailable")?);
        let ack = if result == StreamSubmitV2::NeedEof { if i + 1 == relations { 1 } else { 2 } } else { 0 };
        channel.write_all(&[ack]).map_err(ioerr)?;
        if (i + 1) % 1024 == 0 {
            emit(
                "cold-node-progress",
                l,
                serde_json::json!({"checked":node.checked_relations(),"required":relations,"submit_ns":submit,"elements":elements,"terms":terms,"source_reads":reads,"workspace_peak":peak}),
            );
        }
        if ack != 0 {
            break;
        }
    }
    let outcome = node.finish().map_err(ioerr)?;
    let tag = match outcome {
        CheckOutcome::Matched => 1,
        CheckOutcome::OutputMismatch => 2,
        CheckOutcome::WitnessRejected => 3,
        _ => return Err("cold Node no authenticated final outcome".into()),
    };
    let (user, system, rss) = usage();
    let end = End {
        outcome: tag,
        checked: node.checked_relations(),
        submit_ns: submit,
        elements,
        terms,
        reads,
        workspace_peak: peak,
        cpu_user_us: user,
        cpu_system_us: system,
        process_maxrss: rss,
        request_peak: l.peak.load(Ordering::SeqCst),
        wire_received: channel.0.borrow().received,
        wire_sent: channel.0.borrow().sent,
        model_read_denials: denials,
        pid: std::process::id(),
    };
    put(&mut channel, &bytes(&end, 64 << 10)?, 64 << 10)?;
    emit(
        "cold-node-authenticated-end",
        l,
        serde_json::json!({"outcome":tag,"checked":end.checked,"required":relations,"submit_ns":submit,"wire_received":end.wire_received}),
    );
    drop(node);
    Ok(())
}
#[test]
#[ignore = "private exact subprocess entry; sandbox/independent configuration/socket required"]
fn node_child() {
    assert_eq!(std::env::var("MISAKA_COLD_CHILD").as_deref(), Ok("local-diagnostic-only"));
    child_run().unwrap();
}
fn transfer<Rd: Read>(
    e: Expected,
    header: impl FnOnce() -> Header,
    builder: &WidePanelBuilderV2<'_, '_>,
    prep: &FullRelationPreparationV2,
    claims: &mut Rd,
    l: &Ledger,
    reserve: &TirMemoryReserverV1,
) -> R<End> {
    let _encoding = reserve("isolated-parent-encoded-frame", hash(CLASS), hash(CONTEXT), ENCODE_OWNER)?;
    // Retained until exact child exit; its real 2-GiB subledger is additive to
    // every parent owner, under this SAME eight-GiB aggregate counter.
    let _child = reserve("isolated-child-whole-operation", hash(CLASS), hash(CONTEXT), CHILD_CAP)?;
    let h = header();
    let sentinel_dir = tempfile::TempDir::new().map_err(ioerr)?;
    let sentinel = sentinel_dir.path().join("denied-model-sentinel");
    std::fs::write(&sentinel, b"must not reach child").map_err(ioerr)?;
    let sentinel = sentinel.canonicalize().map_err(ioerr)?;
    let profile = format!(
        "(version 1)(allow default)(deny file-read* (subpath \"/Users/wata/Downloads/misaka-trained-llm-cohort-20261031\") (literal \"{}\"))",
        sentinel.display()
    );
    let (parent, child) = UnixStream::pair().map_err(ioerr)?;
    let mut channel = Channel::new(parent).map_err(ioerr)?;
    let fd: OwnedFd = child.into();
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command
        .args(["-p", &profile])
        .arg(std::env::current_exe().map_err(ioerr)?)
        .args(["--exact", CHILD_TEST, "--ignored", "--nocapture", "--test-threads=1"])
        .env("MISAKA_COLD_CHILD", "local-diagnostic-only")
        .env("MISAKA_COLD_FIXTURE", "synthetic-only")
        .env("MISAKA_COLD_EXPECTED", hex(&bytes(&e, 16 << 10)?))
        .env("MISAKA_COLD_SENTINEL", &sentinel)
        .stdin(Stdio::from(fd))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut child = ChildGuard(command.spawn().map_err(ioerr)?);
    // Command retains its configured stdin descriptor after spawn. Drop that
    // parent-side duplicate so a dead child produces EOF instead of a timeout.
    drop(command);
    put(&mut channel, &bytes(&h, HEADER_CAP)?, HEADER_CAP)?;
    let mut count = [0; 4];
    claims.read_exact(&mut count).map_err(ioerr)?;
    require(u32::from_le_bytes(count) as u64 == prep.plan().summary().relations, "local claims count")?;
    channel.write_all(&count).map_err(ioerr)?;
    let mut ack = [0];
    channel.read_exact(&mut ack).map_err(ioerr)?;
    require(ack == [0], "cold Node begin")?;
    let start = Instant::now();
    let mut frontier = 0;
    let mut material_ns = 0u128;
    let mut drain = false;
    for i in 0..prep.plan().summary().relations {
        let relation = prep.plan().relation_at(i).ok_or("local relation geometry")?;
        if !drain {
            let t = Instant::now();
            let material = builder.material(i, frontier)?;
            material_ns += t.elapsed().as_nanos();
            let m = material.as_borrowed();
            let target = material.target();
            let f = (
                m.operands,
                m.params,
                m.prompt_token_ids,
                m.prompt_ids_openings,
                m.decode_tokens.map(|p| (p.rows_root, &p.generated_token_ids)),
                target.map(|p| (p.opening, p.preimage, p.trace)),
            );
            put(&mut channel, &bytes(&f, FRAME_CAP)?, FRAME_CAP)?;
        }
        let mut n = [0; 4];
        claims.read_exact(&mut n).map_err(ioerr)?;
        let lanes = u32::from_le_bytes(n) as u64;
        let expected = match relation.kind {
            ExpandedRelationKind::Legacy(l) => l.value_count,
            ExpandedRelationKind::RecomputeNode { lane_count, .. } => lane_count,
        };
        require(lanes == u64::from(expected), "local claim lane count")?;
        channel.write_all(&n).map_err(ioerr)?;
        let mut left = mul(lanes, 16)?;
        let mut b = [0; 4096];
        while left > 0 {
            let n = left.min(b.len() as u64) as usize;
            claims.read_exact(&mut b[..n]).map_err(ioerr)?;
            channel.write_all(&b[..n]).map_err(ioerr)?;
            left -= n as u64;
        }
        if !drain {
            channel.read_exact(&mut ack).map_err(ioerr)?;
            require(ack[0] <= 2, "cold Node control signal")?;
            drain = ack[0] == 2;
        }
        if matches!(relation.kind, ExpandedRelationKind::Legacy(_)) {
            frontier += 1;
        }
        if (i + 1) % 1024 == 0 {
            emit(
                "isolated-parent-progress",
                l,
                serde_json::json!({"sent_relations":i+1,"required":prep.plan().summary().relations,"material_ns":material_ns,"wire_sent":channel.0.borrow().sent}),
            );
        }
    }
    let mut extra = [0];
    require(claims.read(&mut extra).map_err(ioerr)? == 0, "local claim tape trailing bytes")?;
    channel.stop_write().map_err(ioerr)?;
    let end: End = borsh::from_slice(&get(&mut channel, 64 << 10)?).map_err(ioerr)?;
    require(child.0.wait().map_err(ioerr)?.success(), "cold Node did not exit cleanly")?;
    emit(
        "isolated-parent-complete",
        l,
        serde_json::json!({"child_pid":end.pid,"child_outcome":end.outcome,"child_checked":end.checked,"material_ns":material_ns,
        "transfer_wall_ns":start.elapsed().as_nanos(),"wire_sent":channel.0.borrow().sent,"node_submit_ns":end.submit_ns,"child_cpu_user_us":end.cpu_user_us,"child_cpu_system_us":end.cpu_system_us,
        "child_request_peak":end.request_peak,"child_process_maxrss":end.process_maxrss,"model_read_denials":end.model_read_denials}),
    );
    Ok(end)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn send_trained(
    file: &File,
    builder: &WidePanelBuilderV2<'_, '_>,
    prep: &FullRelationPreparationV2,
    record: &kaspa_consensus_core::palw_positive_check_v2::full_relation::wide_v2::stream::StreamPanelRecordV2,
    tape: &mut misaka_palw_tir_exec::node::positive_wide_material_v2::claims_tape::WideClaimsTapeV2,
    signed: &SignedMiner,
    key: &[u8; PUBKEY_BYTES],
    signature: &[u8; SIGNATURE_BYTES],
    l: &Ledger,
    reserve: &TirMemoryReserverV1,
) -> R<()> {
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
    require(flags >= 0 && flags & libc::FD_CLOEXEC != 0, "model fd would be inherited")?;
    let end = transfer(
        trained_expected(),
        || Header {
            binding: record.binding().clone(),
            generated: record.generated().to_vec(),
            result: signed.result.clone(),
            miner_key: signed.key,
            miner_signature: signed.signature,
            panel_key: *key,
            panel_signature: *signature,
            commitment: record.signing_digest(),
        },
        builder,
        prep,
        tape,
        l,
        reserve,
    )?;
    require(
        end.outcome == 1 && end.checked == prep.plan().summary().relations && end.model_read_denials == 3,
        "trained cold Node incomplete",
    )?;
    require(l.panel_runs.load(Ordering::SeqCst) == 1, "extra producer numerical run")?;
    Ok(())
}
#[test]
#[ignore = "actual original Pythia, separate sandboxed cold Node, finite aggregate owners and outer monitor required"]
fn trained_parent() {
    assert_eq!(std::env::var("MISAKA_COLD_TRAINED").as_deref(), Ok("pythia70m-original-16-1"));
    assert_eq!(std::env::var("MISAKA_TRAINED_COST_WINDOW").as_deref(), Ok("1440s-8gib-16mi-calls"));
    run_mode_with_node(false, MeasurementWindow::ExtendedCost, true);
}

#[test]
fn sandboxed_node_checks_real_relations_and_separates_panel_and_miner_faults() {
    for variant in [0, 1, 2] {
        let l = Arc::new(Ledger::default());
        let reserve = ledger(l.clone());
        let baseline = reserve("isolated-fixture-and-export-owners", hash(CLASS), hash(CONTEXT), E).unwrap();
        run_wide_common::with_fixture(1, |f, _| {
            let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
            let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
            let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
            let capture = capture_one_pass_v2(
                CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
                &runner,
                run_bounds(),
                &work,
                tempfile::tempfile().unwrap(),
                &reserve,
            )
            .unwrap();
            let signed = SignedMiner::new(f, variant == 1);
            let auth = signed.authenticate();
            let prep = signed.preparation(f);
            let params = TirParamsSourceV1(f.params);
            let inventory = TirHeldInventoryV1::build(f.program, &params).unwrap();
            let builder =
                WidePanelBuilderV2::prepare(&capture, &prep, &auth, &signed.rooted, &inventory, provider(), owners(), &reserve)
                    .unwrap();
            let tape = builder.claims_tape(tempfile::tempfile().unwrap()).unwrap();
            let mut claims = Vec::new();
            tape.visit(&mut |_, v| {
                claims.push(v.to_vec());
                Ok(())
            })
            .unwrap();
            if variant == 2 {
                claims[0][0] += 1;
            }
            let binding = capture.binding().unwrap();
            let generated = capture.run().generated().to_vec();
            let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([97; 32]);
            let record = prep
                .freeze_record(binding.clone(), generated.clone(), principal(panel.verification_key.as_ref()), claims.clone())
                .unwrap();
            let sig =
                *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
                    .unwrap()
                    .as_ref();
            let mut canonical = std::io::Cursor::new(borsh::to_vec(&claims).unwrap());
            let expected = Expected {
                profile: 0,
                class: f.job.shape_profile_id,
                artifact: f.artifact,
                registered_tokenizer: f.class.tokenizer_id,
                job: f.job.clone(),
                miner_key: worker_key(),
            };
            let end = transfer(
                expected,
                || Header {
                    binding,
                    generated,
                    result: signed.result.clone(),
                    miner_key: signed.key,
                    miner_signature: signed.signature,
                    panel_key: *panel.verification_key.as_ref(),
                    panel_signature: sig,
                    commitment: record.signing_digest(),
                },
                &builder,
                &prep,
                &mut canonical,
                &l,
                &reserve,
            )
            .unwrap();
            assert_eq!(
                end.outcome,
                match variant {
                    0 => 1,
                    1 => 2,
                    _ => 3,
                }
            );
            if variant != 2 {
                assert_eq!(end.checked, prep.plan().summary().relations);
            }
            assert_eq!(end.model_read_denials, 1);
        });
        assert_eq!(l.live.load(Ordering::SeqCst), E);
        drop(baseline);
        assert_eq!(l.live.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn oversized_outer_frame_and_exhausted_io_refuse_before_body_allocation() {
    let (a, mut b) = UnixStream::pair().unwrap();
    let mut a = Channel::new(a).unwrap();
    b.write_all(&((FRAME_CAP + 1) as u32).to_le_bytes()).unwrap();
    assert!(get(&mut a, FRAME_CAP).unwrap_err().contains("before allocation"));
    a.0.borrow_mut().received = WIRE_CAP;
    assert!(a.read(&mut [0]).is_err());
    let used = a.0.borrow().calls;
    assert!(a.read(&mut [0]).is_err());
    assert_eq!(a.0.borrow().calls, used);
}
