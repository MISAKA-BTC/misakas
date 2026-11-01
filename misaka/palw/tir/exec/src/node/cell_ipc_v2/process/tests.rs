//! Actual child/native stdio ownership tests, not a model-execution helper.
//! All source/expected/captured/Command owners coexist under the declared test
//! external inventory; it is a conservative local declaration, not measured RSS.
use super::super::{
    codec::{CellParamBlobV1, CellRequestWideV2 as Request, CellResponseWideV2 as Response},
    deadline_unix::{
        WideUnixDeadlineV2, WideUnixReadV2, WideUnixWriteV2, unix_stdio_adapter_backing_v2, unix_stdio_deadline_adapters_v2,
    },
};
use super::*;
use crate::{Buf, ParamData};
use misaka_palw_tir::{DType, Ref, TirProgramV1, builder::ProgramBuilder, program::LOGITS_I64_Q24_SCHEME_V2};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use tempfile::TempDir;

type Process = WideCellProcessV2<WideUnixReadV2, WideUnixWriteV2, WideUnixDeadlineV2>;
const EXTERNAL: u64 = 16 << 20;
const SOURCE: u64 = 1 << 20;

// Constant shell program: peer payloads are literal octal printf bytes only.
// No program/parameter/user bytes are interpolated into shell source. Path and
// numeric lengths arrive as environment data and every expansion is quoted.
// PID text is local lifecycle metadata, not protocol/model response data.
const RAW_PEER: &str = r#"
printf '%s\n' "$$" > "$A3_CAPTURE_DIR/pid"
dd bs=1 count=13 of="$A3_CAPTURE_DIR/hello" 2>/dev/null
if [ "$A3_MODE" = wrong-ready ]; then
  printf '\021\000\000\000\000\000\000\000\000\001\000\000\000\003\000\000\000\000\000\000\000\162\141\167\000'
  exec cat >/dev/null
fi
printf '\021\000\000\000\000\000\000\000\000\002\000\000\000\003\000\000\000\000\000\000\000\162\141\167\000'
dd bs=1 count="$A3_OPEN_N" of="$A3_CAPTURE_DIR/open1" 2>/dev/null
if [ "$A3_MODE" = refuse-open ]; then
  printf '\024\000\000\000\000\000\000\000\005\013\000\000\000\000\000\000\000\162\141\167\055\162\145\146\165\163\141\154'
  exec cat >/dev/null
fi
if [ "$A3_MODE" = wrong-open-tag ]; then
  printf '\001\000\000\000\000\000\000\000\004'
  exec cat >/dev/null
fi
printf '\001\000\000\000\000\000\000\000\001'
dd bs=1 count=9 of="$A3_CAPTURE_DIR/close1" 2>/dev/null
printf '\001\000\000\000\000\000\000\000\004'
dd bs=1 count="$A3_OPEN_N" of="$A3_CAPTURE_DIR/open2" 2>/dev/null
printf '\001\000\000\000\000\000\000\000\001'
dd bs=1 count=9 of="$A3_CAPTURE_DIR/close2" 2>/dev/null
printf '\001\000\000\000\000\000\000\000\004'
exec cat >/dev/null
"#;

fn limits() -> WideCellLimitsV2 {
    WideCellLimitsV2 {
        max_frame_bytes: 1 << 20,
        max_decoded_bytes: 1 << 20,
        max_operation_bytes: 1 << 29,
        max_total_read_bytes: 16 << 20,
        max_total_write_bytes: 16 << 20,
        max_messages: 64,
        max_steps: 8,
        max_program_bytes: 1 << 18,
        max_params: 8,
        max_param_bytes: 1 << 20,
        max_values: 16,
        max_lanes: 1 << 16,
        max_carry_vectors: 8,
        max_rank: 4,
        max_name_bytes: 128,
        max_workspace_bytes: 0,
        max_work: 1 << 40,
        external_retained_bytes: EXTERNAL,
    }
}
struct Fixture {
    bytes: Vec<u8>,
    plan: TirPlan,
    params: TirParams<'static>,
}
impl Fixture {
    fn new() -> Self {
        let mut p = ProgramBuilder::new(2, 1 << 18);
        let signed = p.param("signed", DType::I8, &[2], false);
        let index = p.param("index", DType::Idx, &[2], false);
        let logits = p.param("logits", DType::I64, &[2], false);
        let mut pre = p.block("pre", vec![]);
        let signed = pre.reshape_fixed(signed, &[2]);
        pre.commit(signed);
        let index = pre.reshape_fixed(index, &[2]);
        pre.commit(index);
        let pre = pre.finish(&[]);
        let mut post = p.block("post", vec![]);
        let logits = post.reshape_fixed(logits, &[2]);
        post.commit(logits);
        let Ref::Node(logits) = logits else { unreachable!() };
        let post = post.finish(&[]);
        let mut program = p.finish(pre, vec![], post, logits);
        program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
        // Independently declared local normal-form source, not registered-model
        // provenance or child compute. The peer never decodes/evaluates it.
        let bytes = program.encode();
        let program = TirProgramV1::decode_canonical(&bytes).unwrap();
        let plan = TirPlan::compile(&program).unwrap();
        let mut params: TirParams<'static> = TirParams::new(&plan);
        for (j, buf) in
            [(0, Buf::I8(vec![i8::MIN, i8::MAX])), (1, Buf::Idx(vec![0, u32::MAX])), (2, Buf::I64(vec![i64::MIN, i64::MAX]))]
        {
            params.insert(&plan, j, None, ParamData::from_buf(buf).unwrap()).unwrap();
        }
        Self { bytes, plan, params }
    }
    fn source(&self) -> ResidentWideOpenV2<'_> {
        ResidentWideOpenV2::new(&self.bytes, &self.plan, &self.params, SOURCE).unwrap()
    }
    fn manual_open(&self) -> Request {
        Request::Open {
            program: self.bytes.clone(),
            occ: (0, 2),
            params: vec![
                CellParamBlobV1 { param: 0, layer: None, bytes: vec![0x80, 0x7f] },
                CellParamBlobV1 { param: 1, layer: None, bytes: vec![0, 0, 0, 0, 255, 255, 255, 255] },
                CellParamBlobV1 {
                    param: 2,
                    layer: None,
                    bytes: vec![0, 0, 0, 0, 0, 0, 0, 128, 255, 255, 255, 255, 255, 255, 255, 127],
                },
            ],
        }
    }
    fn build_work(&self) -> (u64, u64) {
        // Literal fixture: 3 existing parameter memberships, 2 occurrences,
        // 3 None-layer payload headers (11 each), native payload bytes2+8+16.
        let program = self.bytes.len() as u64;
        let metadata = (program + 4096) * (3 + 1) * (2 + 1) * 128;
        let body = 21 + program + 3 * 11 + 2 + 8 + 16;
        let copy = (body + 4096) * 128;
        (metadata, copy)
    }
}
fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = (body.len() as u64).to_le_bytes().to_vec();
    out.extend_from_slice(body);
    out
}
fn literal_open(message: &Request) -> Vec<u8> {
    let Request::Open { program, occ, params } = message else { panic!("test Open") };
    let mut body = vec![1];
    body.extend_from_slice(&(program.len() as u64).to_le_bytes());
    body.extend_from_slice(program);
    body.extend_from_slice(&occ.0.to_le_bytes());
    body.extend_from_slice(&occ.1.to_le_bytes());
    body.extend_from_slice(&(params.len() as u32).to_le_bytes());
    for param in params {
        assert_eq!(param.layer, None);
        body.extend_from_slice(&param.param.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&(param.bytes.len() as u64).to_le_bytes());
        body.extend_from_slice(&param.bytes);
    }
    frame(&body)
}

// A guard is registered before Hello2 connect can fail, using the trusted raw
// peer's own PID file. Only waitpid==0 establishes our still-live direct child
// and permits kill. ECHILD/other PID status never authorizes killing any PID.
struct PidCleanup(libc::pid_t);
impl PidCleanup {
    fn assert_reaped(&self) {
        let mut status = 0;
        loop {
            // SAFETY: this is the specific positive PID obtained from our helper.
            let result = unsafe { libc::waitpid(self.0, &mut status, libc::WNOHANG) };
            if result < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            assert_eq!(result, -1, "owner did not reap its direct child (0=live; positive=reaped here)");
            assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ECHILD));
            return;
        }
    }
}
impl Drop for PidCleanup {
    fn drop(&mut self) {
        let mut status = 0;
        loop {
            // SAFETY: waitpid only inspects/reaps this specific possible child.
            let result = unsafe { libc::waitpid(self.0, &mut status, libc::WNOHANG) };
            if result < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if result == 0 {
                // SAFETY: waitpid just established that PID is our live child.
                let _ = unsafe { libc::kill(self.0, libc::SIGKILL) };
                loop {
                    // SAFETY: only wait for that same direct child after kill.
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
fn pid_from_file(path: &Path) -> Result<PidCleanup, String> {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        match fs::read_to_string(path) {
            Ok(text) => {
                // The write can become visible before its decimal digits are complete.
                if text.ends_with('\n') {
                    let pid: libc::pid_t = text.trim().parse().map_err(|_| "test helper PID is not decimal")?;
                    if pid > 0 {
                        return Ok(PidCleanup(pid));
                    }
                    return Err("test helper PID is not positive".into());
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        if Instant::now() >= until {
            return Err("test helper PID setup deadline".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn spawn(
    directory: &TempDir,
    mode: &'static str,
    open_n: usize,
    build_work: u64,
) -> (Result<Process, WideProcessErrorV2>, PidCleanup) {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", RAW_PEER]).env("A3_CAPTURE_DIR", directory.path()).env("A3_MODE", mode).env("A3_OPEN_N", open_n.to_string());
    let backing = unix_stdio_adapter_backing_v2();
    let owner = WideProcessOwnerLimitsV2 { max_open_build_work: build_work, adapter_retained_bytes: backing.shared_heap_bytes };
    let mut guard = None;
    let result = Process::spawn_with_deadline_adapters(&mut command, limits(), owner, |out, input| {
        let adapters = unix_stdio_deadline_adapters_v2(out, input, Duration::from_secs(3)).map_err(|e| e.to_string())?;
        guard = Some(pid_from_file(&directory.path().join("pid"))?);
        Ok(adapters)
    });
    (result, guard.expect("trusted helper must publish PID before responding"))
}
fn captured(directory: &TempDir, name: &str) -> Vec<u8> {
    fs::read(directory.path().join(name)).unwrap()
}

#[test]
fn actual_process_builder_frames_native_bytes_exports_work_and_drop_reap() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let manual = fixture.manual_open();
    let expected_open = literal_open(&manual);
    let (metadata, copy) = fixture.build_work();
    let one_build = metadata + copy;
    let directory = TempDir::new().unwrap();
    let (result, pid) = spawn(&directory, "happy", expected_open.len(), 2 * one_build);
    let mut process = result.unwrap();
    let shared_child = process.child.clone(); // Test-only observation, independent of last-Arc Drop.
    let initial = process.usage();
    assert_eq!(initial.client.messages, 2);
    assert_eq!(initial.client.written_bytes, 13);
    assert_eq!(initial.client.read_bytes, 25); // literal Ready body17 + prefix8.
    for _ in 0..2 {
        assert_eq!(process.open_resident(&source, 0..2).unwrap(), Response::Opened);
        assert_eq!(process.request(&Request::Close).unwrap(), Response::Closed);
    }
    assert_eq!(captured(&directory, "hello"), frame(&[0, 2, 0, 0, 0]));
    assert_eq!(captured(&directory, "open1"), expected_open);
    assert_eq!(captured(&directory, "open2"), expected_open);
    assert_eq!(captured(&directory, "close1"), frame(&[5]));
    assert_eq!(captured(&directory, "close2"), frame(&[5]));
    let payload = &captured(&directory, "open1")[8..];
    assert_eq!(Request::decode_bounded(payload, limits()).unwrap(), manual);
    let source_quote = source.quote_admitted(&0..2, limits(), metadata).unwrap();
    let expected_native =
        size_of::<Request>() as u64 + fixture.bytes.len() as u64 + 3 * size_of::<CellParamBlobV1>() as u64 + 2 + 8 + 16;
    assert_eq!(source_quote.request_bytes, expected_native);
    assert_eq!(source_quote.body_bytes + 8, expected_open.len() as u64);
    assert_eq!(source_quote.metadata_work, metadata);
    assert_eq!(source_quote.copy_work, copy);
    let usage = process.usage();
    assert_eq!(usage.open_build_work, 2 * one_build); // Exact prepaid allowance succeeds across Close.
    assert_eq!(usage.client.messages, 10);
    assert_eq!(usage.client.steps, 0);
    let written = 13 + 2 * expected_open.len() as u64 + 2 * 9;
    let read = 25 + 4 * 9;
    assert_eq!(usage.client.written_bytes, written);
    assert_eq!(usage.client.read_bytes, read);
    // Every returned Ready/Opened/Closed header is conservatively exported;
    // Ready alone additionally retains its exact three-byte name capacity.
    assert_eq!(usage.client.exported_bytes, 5 * size_of::<Response>() as u64 + 3);
    let l = limits();
    let header_work = (l.max_params + l.max_carry_vectors + l.max_values * (l.max_rank + 1) + 128) * 128;
    assert_eq!(usage.client.work, 5 * header_work + (written + read) * 128);
    assert_eq!(usage.total_work, usage.client.work + 2 * one_build);
    assert!(usage.total_work <= l.max_work);
    assert!(!process.poisoned());
    drop(process);
    assert!(shared_child.lock().unwrap().child.is_none());
    pid.assert_reaped();
}

#[test]
fn prepaid_builder_one_below_and_cumulative_reopen_fail_before_open_wire() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let expected_open = literal_open(&fixture.manual_open());
    let (metadata, copy) = fixture.build_work();
    let one_build = metadata + copy;
    for second in [false, true] {
        let directory = TempDir::new().unwrap();
        let allowance = if second { 2 * one_build - 1 } else { one_build - 1 };
        let (result, pid) = spawn(&directory, "happy", expected_open.len(), allowance);
        let mut process = result.unwrap();
        let shared_child = process.child.clone();
        if second {
            assert_eq!(process.open_resident(&source, 0..2).unwrap(), Response::Opened);
            assert_eq!(process.request(&Request::Close).unwrap(), Response::Closed);
        }
        let before = process.usage();
        let error = process.open_resident(&source, 0..2).unwrap_err();
        assert!(process.poisoned());
        assert!(process.client.is_none());
        assert!(shared_child.lock().unwrap().child.is_none());
        assert_eq!(error.usage, process.usage());
        assert_eq!(error.usage.client, before.client); // No failing Open frame/reply/export was admitted.
        assert_eq!(error.usage.open_build_work, before.open_build_work + metadata);
        assert!(error.usage.open_build_work <= allowance);
        assert!(error.usage.total_work <= limits().max_work);
        let frozen = process.usage();
        assert!(process.request(&Request::Close).is_err());
        assert_eq!(process.usage(), frozen);
        pid.assert_reaped();
    }
}

#[test]
fn wrong_ready_constructor_refused_open_and_wrong_tag_poison_reap_owned_child() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let expected_open = literal_open(&fixture.manual_open());
    let (metadata, copy) = fixture.build_work();
    for mode in ["wrong-ready", "refuse-open", "wrong-open-tag"] {
        let directory = TempDir::new().unwrap();
        let (result, pid) = spawn(&directory, mode, expected_open.len(), 2 * (metadata + copy));
        if mode == "wrong-ready" {
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("Ready1 must not negotiate Hello2"),
            };
            assert!(error.message.contains("no V1 fallback"));
            assert_eq!(error.usage.client.messages, 2);
            assert_eq!(error.usage.client.written_bytes, 13);
            assert_eq!(error.usage.client.read_bytes, 25);
            assert_eq!(error.usage.client.exported_bytes, 0);
        } else {
            let mut process = result.unwrap();
            let shared_child = process.child.clone();
            let before = process.usage();
            let error = process.open_resident(&source, 0..2).unwrap_err();
            assert!(process.poisoned());
            assert!(process.client.is_none());
            assert!(shared_child.lock().unwrap().child.is_none());
            assert_eq!(error.usage, process.usage());
            assert_eq!(error.usage.open_build_work, metadata + copy);
            assert_eq!(error.usage.client.exported_bytes, before.client.exported_bytes);
            assert!(error.usage.client.read_bytes > before.client.read_bytes);
            assert!(error.usage.client.written_bytes > before.client.written_bytes);
            if mode == "refuse-open" {
                assert_eq!(error.message, "raw-refusal");
            } else {
                assert!(error.message.contains("response tag"));
            }
            let frozen = process.usage();
            assert!(process.request(&Request::Close).is_err());
            assert_eq!(process.usage(), frozen);
        }
        assert_eq!(captured(&directory, "hello"), frame(&[0, 2, 0, 0, 0]));
        pid.assert_reaped();
    }
}
