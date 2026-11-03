//! DRAFT: actual Child/stdin/stdout owner for explicit Hello2.
//!
//! This is a caller-authenticated resident-source seam, not an artifact loader,
//! registry/backend/device or computation acceptance path. The child currently
//! still needs independently prepared matching resident model material.
//! Required deadline adapters own the actual pipes and their separately admitted
//! worker/buffer state. This module supplies no native blocking default and no
//! platform-independent wall-time guarantee for adapter Drop, kill or wait.
use std::{
    fmt,
    io::{self, Read, Write},
    mem::{align_of, size_of},
    ops::Range,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
};

use super::{
    budget::{WideCellLimitsV2, WideCellUsageV2, quote_client},
    codec::{self, CellParamBlobV1, CellRequestWideV2 as Request, CellResponseWideV2 as Response},
    session::{WideCellClientV2, WideCellIoErrorV2},
};
use crate::{Slice, TirParams, TirPlan};
use borsh::BorshSerialize;
use misaka_palw_tir::{DType, Ref};

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "Hello2 process size addition overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "Hello2 process size multiplication overflow".into())
}
fn count(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "Hello2 process index width".into())
}
fn cap(n: u64, limit: u64) -> Result<(), String> {
    if n > limit { Err("Hello2 process local ceiling".into()) } else { Ok(()) }
}
fn exact<T>(n: usize) -> Result<Vec<T>, String> {
    let mut out = Vec::new();
    out.try_reserve_exact(n).map_err(|_| "Hello2 process allocation refused")?;
    if out.capacity() != n {
        return Err("Hello2 process allocator exceeded exact request".into());
    }
    Ok(out)
}
fn empty_usage() -> WideCellUsageV2 {
    WideCellUsageV2 { read_bytes: 0, written_bytes: 0, messages: 0, steps: 0, work: 0, exported_bytes: 0 }
}

/// One exchange clock shared by the factory's reader and writer. The owner
/// calls this once before Hello2 connect and before every client.request, never
/// per syscall/partial transfer/retry. There is no no-deadline default.
pub trait WideExchangeControllerV2 {
    fn begin_exchange(&mut self) -> io::Result<()>;
}

/// Finite prepayment, not a second independent successful work allowance.
/// The client receives max_work minus this entire amount; all Open builds share
/// this amount cumulatively, including builds that later fail. The caller's
/// external reservation already covers all source/model/Command ownership.
#[derive(Clone, Copy, Debug)]
pub struct WideProcessOwnerLimitsV2 {
    pub max_open_build_work: u64,
    /// Adapter heap buffers, worker stacks/queues and transient copies, excluding
    /// inline R/W headers automatically counted in the process-owner header.
    pub adapter_retained_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideProcessUsageV2 {
    pub client: WideCellUsageV2,
    pub open_build_work: u64,
    pub total_work: u64,
}
#[derive(Debug)]
pub struct WideProcessErrorV2 {
    pub message: String,
    pub usage: WideProcessUsageV2,
    /// An attempted kill/wait is not reported as successful reaping on OS error.
    pub cleanup_error: Option<String>,
}
impl fmt::Display for WideProcessErrorV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for WideProcessErrorV2 {}

struct ChildState {
    child: Option<Child>,
}
impl ChildState {
    fn stop(&mut self) -> Result<(), String> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        // Always attempt wait, including when kill reports an already-exited child.
        let killed = child.kill().err();
        loop {
            match child.wait() {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    self.child = Some(child); // RAII cleanup may retry the wait.
                    return Err(format!("Hello2 child cleanup failed: kill={killed:?}, wait={e}"));
                }
            }
        }
    }
}
impl Drop for ChildState {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
type ChildOwner = Arc<Mutex<ChildState>>;
fn stop(owner: &ChildOwner) -> Result<(), String> {
    owner.lock().unwrap_or_else(|e| e.into_inner()).stop()
}

// Kill/reap before dropping a deadline adapter, whose worker join may otherwise
// wait on a still-running helper. Also protects client construction failures:
// WideCellClientV2 drops its consumed reader/writer on handshake refusal.
struct OwnedPipe<T> {
    inner: Option<T>,
    child: ChildOwner,
}
impl<T> Drop for OwnedPipe<T> {
    fn drop(&mut self) {
        let _ = stop(&self.child);
        self.inner.take();
    }
}
impl<R: Read> Read for OwnedPipe<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.as_mut().expect("owned read pipe").read(bytes)
    }
}
impl<W: Write> Write for OwnedPipe<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.as_mut().expect("owned write pipe").write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.as_mut().expect("owned write pipe").flush()
    }
}

/// Borrowed, already authenticated original program and resident native source.
/// Caller preconditions: original bytes belong to the independently resolved
/// class; plan is an unmodified TirPlan::compile result for those bytes; params
/// are stable matching resident weights from the independently admitted artifact.
/// retained_bytes includes the caller's actual plan/program/params/other source
/// backing (not just serialized lengths). No model qualification is performed.
/// A mapped/resident source can satisfy this without a full Tensor/i128 mirror.
pub struct ResidentWideOpenV2<'a> {
    original_program: &'a [u8],
    plan: &'a TirPlan,
    params: &'a TirParams<'a>,
    retained_bytes: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct WideOpenBuildQuoteV2 {
    pub selected_params: u64,
    pub param_bytes: u64,
    pub request_bytes: u64,
    pub body_bytes: u64,
    pub metadata_work: u64,
    pub copy_work: u64,
}
impl<'a> ResidentWideOpenV2<'a> {
    /// Does not validate/recompile public compiled fields or authenticate an
    /// artifact. Lazy sources are refused before any get can materialize them.
    pub fn new(original_program: &'a [u8], plan: &'a TirPlan, params: &'a TirParams<'a>, retained_bytes: u64) -> Result<Self, String> {
        if !plan.program.wide_logits_q24() {
            return Err("Hello2 Open requires the original explicit wide profile".into());
        }
        if params.row_source().is_some() {
            return Err("Hello2 Open requires a fully resident borrowed source".into());
        }
        Ok(Self { original_program, plan, params, retained_bytes })
    }
    fn scope(&self, occ: &Range<usize>) -> Result<(), String> {
        if occ.start >= occ.end || occ.end > self.plan.occurrences.len() {
            return Err("Hello2 Open scope".into());
        }
        u32::try_from(occ.start).map_err(|_| "Hello2 Open scope width")?;
        u32::try_from(occ.end).map_err(|_| "Hello2 Open scope width")?;
        Ok(())
    }
    fn needed(&self, occ: &Range<usize>, param: u16, layer: Option<u16>) -> bool {
        let declaration = &self.plan.program.params[param as usize];
        self.plan.occurrences[occ.clone()].iter().any(|(block, running_layer)| {
            (!declaration.per_layer || *running_layer == layer)
                && self.plan.program.blocks[*block as usize].nodes.iter().any(|node| node.inputs.contains(&Ref::Param(param)))
        })
    }
    // O(1) count/length checks and a closed-form allowance precede all metadata
    // scans. Repeated parameter-needed scans are covered conservatively here.
    fn metadata_work(&self, occ: &Range<usize>, limits: WideCellLimitsV2) -> Result<u64, String> {
        self.scope(occ)?;
        cap(count(self.original_program.len())?, limits.max_program_bytes)?;
        cap(self.retained_bytes, limits.external_retained_bytes)?;
        if self.params.row_source().is_some() {
            return Err("Hello2 Open refuses lazy sources".into());
        }
        mul(
            mul(add(count(self.original_program.len())?, 4096)?, add(count(self.plan.param_instances.len())?, 1)?)?,
            mul(add(count(occ.end - occ.start)?, 1)?, 128)?,
        )
    }
    fn data(&self, param: u16, layer: Option<u16>) -> Result<Slice<'_>, String> {
        // row_source() == None is checked before this first get. This is a
        // native borrowed lookup, not the row source's dense fallback.
        let data = self.params.get(param, layer).ok_or("Hello2 Open resident parameter absent")?;
        let declaration = &self.plan.program.params[param as usize];
        let lanes = declaration.shape.iter().try_fold(1u64, |n, d| mul(n, u64::from(*d)))?;
        if declaration.dtype == DType::I128 || data.dtype() != declaration.dtype || count(data.len())? != lanes {
            return Err("Hello2 Open resident parameter type/length differs".into());
        }
        Ok(data)
    }
    fn quote_admitted(
        &self,
        occ: &Range<usize>,
        limits: WideCellLimitsV2,
        metadata_work: u64,
    ) -> Result<WideOpenBuildQuoteV2, String> {
        let mut params = 0;
        let mut bytes = 0;
        let mut body = add(21, count(self.original_program.len())?)?; // tag+u64bytes+occ2*u32+u32count
        for &(param, layer) in &self.plan.param_instances {
            if !self.needed(occ, param, layer) {
                continue;
            }
            let data = self.data(param, layer)?;
            let size = mul(count(data.len())?, count(data.dtype().width())?)?;
            params = add(params, 1)?;
            bytes = add(bytes, size)?;
            cap(params, limits.max_params)?;
            cap(bytes, limits.max_param_bytes)?;
            body = add(body, add(add(11, if layer.is_some() { 2 } else { 0 })?, size)?)?;
        }
        u32::try_from(params).map_err(|_| "Hello2 Open param count width")?;
        let request_bytes = add(
            add(count(size_of::<Request>())?, count(self.original_program.len())?)?,
            add(mul(params, count(size_of::<CellParamBlobV1>())?)?, bytes)?,
        )?;
        cap(body, limits.max_frame_bytes)?;
        cap(request_bytes, limits.max_decoded_bytes)?;
        Ok(WideOpenBuildQuoteV2 {
            selected_params: params,
            param_bytes: bytes,
            request_bytes,
            body_bytes: body,
            metadata_work,
            copy_work: mul(add(body, 4096)?, 128)?,
        })
    }
    fn build_admitted(&self, occ: Range<usize>, quote: WideOpenBuildQuoteV2) -> Result<Request, String> {
        // Compare literal canonical bytes with a streaming writer, without
        // allocating another encoded program or accepting a delivered program.
        struct Compare<'a>(&'a [u8]);
        impl Write for Compare<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if !self.0.starts_with(bytes) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Open program differs"));
                }
                self.0 = &self.0[bytes.len()..];
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        if borsh::object_length(&self.plan.program).map_err(|_| "Open program length")? != self.original_program.len() {
            return Err("Hello2 Open original program differs".into());
        }
        let mut compare = Compare(self.original_program);
        self.plan.program.serialize(&mut compare).map_err(|_| "Hello2 Open original program differs")?;
        if !compare.0.is_empty() {
            return Err("Hello2 Open trailing original program bytes".into());
        }
        let mut program = exact(self.original_program.len())?;
        program.extend_from_slice(self.original_program);
        let mut params = exact(usize::try_from(quote.selected_params).map_err(|_| "Open parameter native count")?)?;
        for &(param, layer) in &self.plan.param_instances {
            if !self.needed(&occ, param, layer) {
                continue;
            }
            let data = self.data(param, layer)?;
            let n = mul(count(data.len())?, count(data.dtype().width())?)?;
            let mut bytes = exact(usize::try_from(n).map_err(|_| "Open parameter native bytes")?)?;
            // Preserve native signed/Idx bytes directly, including high-bit Idx.
            macro_rules! append {
                ($values:expr) => {
                    for value in $values {
                        bytes.extend_from_slice(&value.to_le_bytes());
                    }
                };
            }
            match data {
                Slice::I8(values) => append!(values),
                Slice::I16(values) => append!(values),
                Slice::I32(values) => append!(values),
                Slice::I64(values) => append!(values),
                Slice::Idx(values) => append!(values),
                Slice::I128(_) => return Err("Open parameter is never i128".into()),
            }
            if count(bytes.len())? != n || bytes.capacity() != bytes.len() {
                return Err("Open parameter backing differs from quote".into());
            }
            params.push(CellParamBlobV1 { param, layer, bytes });
        }
        let request = Request::Open {
            program,
            occ: (u32::try_from(occ.start).map_err(|_| "Open scope width")?, u32::try_from(occ.end).map_err(|_| "Open scope width")?),
            params,
        };
        if codec::native_backing_request(&request)? != quote.request_bytes
            || codec::encoded_length_request(&request)? != quote.body_bytes
        {
            return Err("Hello2 Open private backing/wire length differs from quote".into());
        }
        Ok(request)
    }
}

/// Actual process lifetime owner. Once adapters have successfully returned, a
/// protocol/connect failure, poison or owner Drop attempts kill+wait before
/// dropping those returned adapters. No V1 trait/backend registration.
/// Adapter-construction Err/unwind drops partial adapters inside the callback
/// before this owner can observe failure. Their bounded cancellation/Drop/join
/// is a callback precondition, not a cleanup guarantee supplied by this type.
/// A future explicit child-cancellation capability and partial-construction
/// failure/unwind tests are required before making a stronger guarantee.
/// Only Open constructed from the trusted resident source is exposed here.
pub struct WideCellProcessV2<R: Read, W: Write, C: WideExchangeControllerV2> {
    client: Option<WideCellClientV2<OwnedPipe<R>, OwnedPipe<W>>>,
    child: ChildOwner,
    controller: Option<C>,
    limits: WideCellLimitsV2,
    owner_limits: WideProcessOwnerLimitsV2,
    source_external_bytes: u64,
    build_work: u64,
    client_usage: WideCellUsageV2,
    open: bool,
    poisoned: bool,
}
impl<R: Read, W: Write, C: WideExchangeControllerV2> WideCellProcessV2<R, W, C> {
    /// Required adapter callback consumes actual ChildStdout/ChildStdin and owns
    /// them until Drop. It must implement finite per-operation deadlines and
    /// bounded cancellation/Drop/join, including on callback failure/panic.
    /// Partial adapter construction is still callback-owned: its Err/unwind
    /// cleanup can run before this owner observes failure and stops the child.
    /// This API currently supplies no child-cancellation token to that callback.
    /// All worker stacks/queues/buffers and transient duplicate data must fit
    /// adapter_retained_bytes; no detached threads or leaked handles are allowed.
    /// Merely supplying a callback is not evidence those preconditions hold.
    /// Child cleanup is for the direct child only, not a process-tree guarantee.
    pub fn spawn_with_deadline_adapters<F>(
        command: &mut Command,
        limits: WideCellLimitsV2,
        owner_limits: WideProcessOwnerLimitsV2,
        adapters: F,
    ) -> Result<Self, WideProcessErrorV2>
    where
        F: FnOnce(ChildStdout, ChildStdin) -> Result<(R, W, C), String>,
    {
        let usage = WideProcessUsageV2 { client: empty_usage(), open_build_work: 0, total_work: 0 };
        let preflight = (|| -> Result<WideCellLimitsV2, String> {
            let mut client_limits = limits;
            client_limits.max_work =
                limits.max_work.checked_sub(owner_limits.max_open_build_work).ok_or("Open build work allowance exceeds total")?;
            // Inline client/R/W/controller/Arc handle state and Arc/Mutex child state coexist.
            let guard =
                add(count(size_of::<Mutex<ChildState>>())?, count(2 * size_of::<usize>() + 2 * align_of::<Mutex<ChildState>>())?)?;
            // Pinned Rust1.93 macOS Mutex lazily boxes its pthread mutex;
            // Linux futex storage is inline. This is ChildState's box, distinct
            // from the adapter controller mutex included by its own quote.
            #[cfg(target_os = "macos")]
            let guard = add(guard, count(size_of::<libc::pthread_mutex_t>())?)?;
            let owner = add(count(size_of::<Self>())?, add(guard, owner_limits.adapter_retained_bytes)?)?;
            client_limits.external_retained_bytes = add(limits.external_retained_bytes, owner)?;
            quote_client(client_limits)?;
            Ok(client_limits)
        })();
        let client_limits = preflight.map_err(|message| WideProcessErrorV2 { message, usage, cleanup_error: None })?;
        // Stderr is not another unbounded pipe/worker queue owned by this layer.
        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| WideProcessErrorV2 { message: e.to_string(), usage, cleanup_error: None })?;
        let child = Arc::new(Mutex::new(ChildState { child: Some(child) }));
        let mut owner = Self {
            client: None,
            child: child.clone(),
            controller: None,
            limits: client_limits,
            owner_limits,
            source_external_bytes: limits.external_retained_bytes,
            build_work: 0,
            client_usage: empty_usage(),
            open: false,
            poisoned: false,
        };
        let pipes = {
            let mut guard = child.lock().unwrap_or_else(|e| e.into_inner());
            let process = guard.child.as_mut().expect("newly owned child");
            (process.stdout.take(), process.stdin.take())
        };
        let (stdout, stdin) = match pipes {
            (Some(r), Some(w)) => (r, w),
            _ => return Err(owner.fail("Hello2 child pipe missing")),
        };
        let (reader, writer, mut controller) = adapters(stdout, stdin).map_err(|e| owner.fail(e))?;
        if let Err(error) = controller.begin_exchange() {
            return Err(owner.fail(error.to_string()));
        }
        owner.controller = Some(controller);
        let client = WideCellClientV2::connect(
            OwnedPipe { inner: Some(reader), child: child.clone() },
            OwnedPipe { inner: Some(writer), child },
            client_limits,
        );
        match client {
            Ok(client) => {
                owner.client_usage = client.usage();
                owner.client = Some(client);
                Ok(owner)
            }
            Err(e) => {
                owner.client_usage = e.usage;
                Err(owner.fail(e.message))
            }
        }
    }
    pub fn usage(&self) -> WideProcessUsageV2 {
        // Prepaid partition invariants establish this addition fits the original
        // max_work. Still use checked arithmetic instead of wrapping a report.
        let total_work = self.client_usage.work.checked_add(self.build_work).expect("admitted partitioned work");
        WideProcessUsageV2 { client: self.client_usage, open_build_work: self.build_work, total_work }
    }
    pub fn poisoned(&self) -> bool {
        self.poisoned
    }
    fn fail(&mut self, message: impl Into<String>) -> WideProcessErrorV2 {
        self.poisoned = true;
        let cleanup_error = stop(&self.child).err();
        self.client.take(); // Drop adapters only after stopping the child.
        WideProcessErrorV2 { message: message.into(), usage: self.usage(), cleanup_error }
    }
    fn charge_build(&mut self, work: u64) -> Result<(), String> {
        let next = add(self.build_work, work)?;
        cap(next, self.owner_limits.max_open_build_work)?;
        self.build_work = next;
        Ok(())
    }
    fn ensure_peak(&self) -> Result<(), String> {
        let base = quote_client(self.limits)?;
        cap(add(base.operation_peak_bytes, self.client_usage.exported_bytes)?, self.limits.max_operation_bytes)
    }
    fn call(&mut self, request: &Request) -> Result<Response, WideProcessErrorV2> {
        if self.poisoned {
            return Err(self.fail("Hello2 process is poisoned"));
        }
        let begin = self.controller.as_mut().expect("live exchange controller").begin_exchange();
        if let Err(error) = begin {
            return Err(self.fail(error.to_string()));
        }
        let result = self.client.as_mut().expect("live unpoisoned client").request(request);
        match result {
            Ok(reply) => {
                self.client_usage = self.client.as_ref().expect("live client").usage();
                match request {
                    Request::Open { .. } => self.open = true,
                    Request::Close => self.open = false,
                    _ => {}
                }
                Ok(reply)
            }
            Err(WideCellIoErrorV2 { message, usage }) => {
                self.client_usage = usage;
                Err(self.fail(message))
            }
        }
    }
    /// The builder's required borrowed source must fit the caller inventory
    /// declared at spawn. A larger/new concurrent source requires a new owner.
    pub fn open_resident(&mut self, source: &ResidentWideOpenV2<'_>, occ: Range<usize>) -> Result<Response, WideProcessErrorV2> {
        if self.poisoned || self.open {
            return Err(self.fail("Hello2 process cannot Open in this state"));
        }
        let request = (|| -> Result<Request, String> {
            self.ensure_peak()?;
            cap(source.retained_bytes, self.source_external_bytes)?;
            let work = source.metadata_work(&occ, self.limits)?;
            self.charge_build(work)?; // Before repeated metadata scans or identity comparison.
            let quote = source.quote_admitted(&occ, self.limits, work)?;
            self.charge_build(quote.copy_work)?; // Before native parameter byte iteration/copy.
            source.build_admitted(occ, quote)
        })()
        .map_err(|e| self.fail(e))?;
        self.call(&request)
    }
    /// Raw Step/Fixed/Hist/Close data only. Hello and caller-constructed Open are
    /// intentionally unavailable, so Open construction cannot bypass admission.
    pub fn request(&mut self, request: &Request) -> Result<Response, WideProcessErrorV2> {
        if matches!(request, Request::Hello { .. } | Request::Open { .. }) {
            return Err(self.fail("Hello2 process requires its bounded Open builder"));
        }
        self.call(request)
    }
    /// Abrupt local shutdown. Close is a protocol operation and does not replace
    /// explicit process ownership: a helper could remain alive after Closed.
    pub fn terminate(&mut self) -> Result<(), String> {
        self.poisoned = true;
        let result = stop(&self.child);
        self.client.take();
        result
    }
}
impl<R: Read, W: Write, C: WideExchangeControllerV2> Drop for WideCellProcessV2<R, W, C> {
    fn drop(&mut self) {
        let _ = stop(&self.child);
        self.client.take();
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
