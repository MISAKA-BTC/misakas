//! **An out-of-process device for cell steps (RFC-0006): the versioned wire, the client, the server loop.**
//!
//! A device whose dependencies cannot share the node's lock (the GPU backend is its own cargo workspace: wgpu 30 against the consensus
//! stack's `js-sys` pin) runs as a **helper process**. The node ([`TirProcessDeviceV1`]) talks to it over a pipe pair — its stdin and
//! stdout when spawned, or any `Read`/`Write` pair — in length-prefixed frames of the messages below. The helper
//! ([`serve_cell_requests_v1`]) is a loop over any [`TirDeviceV1`], so the GPU helper is `serve(stdin, stdout, &GpuDeviceV1)` and the
//! tests' fake helper is the same loop over a CPU device. Nothing here depends on consensus types.
//!
//! **The conversation** (one session at a time per helper): `Hello{version}` → `Ready{version, name, capacity}`; `Open{program, occ,
//! params}` → `Opened` or `Refused` (the params are the cell's own occurrences' instances, nothing else); then `Step{token, occ,
//! carry_in}` → `Stepped{commit values, carry-out, logits row}`, `Fixed{..}` / `Hist{..}` → `Lanes`; `Close` → `Closed`.
//!
//! **Trust.** The helper is not trusted: with `mirror` on, every answer is checked against the CPU executor's on the same step
//! (values, carry, logits, state lanes) and a difference — like any I/O error — poisons the device and refuses the cell, which the node then
//! runs on the CPU. A refusal is always correct.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::ops::Range;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TirError, TirErrorKind, TirResult};

use crate::cellstep::{CpuCellStepperV1, TirCellStepperV1, TirDeviceV1};
use crate::elem::Buf;
use crate::exec::{NodeValue, StepSink, TirExecutor};
use crate::params::TirParams;
use crate::plan::TirPlan;

/// The wire version both ends must speak.
pub const CELL_WIRE_VERSION_V1: u32 = 1;
/// The largest frame either end accepts (a hostile length is refused, never allocated).
pub const CELL_WIRE_MAX_FRAME_V1: usize = 1 << 30;

// ---------------------------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellParamBlobV1 {
    pub param: u16,
    pub layer: Option<u16>,
    /// The tensor's little-endian bytes (`Tensor::to_le_bytes`); dtype and shape are the program's declaration.
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellRequestV1 {
    Hello { version: u32 },
    Open { program: Vec<u8>, occ: (u32, u32), params: Vec<CellParamBlobV1> },
    Step { token: u32, occ: (u32, u32), carry_in: Vec<Vec<i128>> },
    Fixed { state: u16, layer: Option<u16>, first: u32, n: u32 },
    Hist { state: u16, layer: Option<u16>, h_tile: u32, first_lane: u32, row_lanes: u32 },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellValueV1 {
    pub slot: u32,
    pub block: u8,
    pub layer: Option<u16>,
    pub node: u16,
    pub dtype: u8,
    pub shape: Vec<u32>,
    pub lanes: Vec<i128>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellResponseV1 {
    Ready { version: u32, name: String, capacity: Option<u64> },
    Opened,
    Stepped { values: Vec<CellValueV1>, carry: Vec<Vec<i128>>, logits: Vec<i32> },
    Lanes(Vec<u8>),
    Closed,
    Refused(String),
}

// ---------------------------------------------------------------------------------------------
// Encoding (a hand-rolled, length-prefixed form: no dependency)
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i128(&mut self, v: i128) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.u64(v.len() as u64);
        self.0.extend_from_slice(v);
    }
    fn opt16(&mut self, v: Option<u16>) {
        match v {
            Some(x) => {
                self.u8(1);
                self.u16(x);
            }
            None => self.u8(0),
        }
    }
    fn lanes(&mut self, v: &[i128]) {
        self.u64(v.len() as u64);
        for x in v {
            self.i128(*x);
        }
    }
    fn carries(&mut self, v: &[Vec<i128>]) {
        self.u32(v.len() as u32);
        for c in v {
            self.lanes(c);
        }
    }
}

struct R<'a>(&'a [u8]);

impl R<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        if self.0.len() < n {
            return Err("a frame ends early".to_string());
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("2")))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }
    fn i128(&mut self) -> Result<i128, String> {
        Ok(i128::from_le_bytes(self.take(16)?.try_into().expect("16")))
    }
    fn len(&mut self, unit: usize) -> Result<usize, String> {
        let n = usize::try_from(self.u64()?).map_err(|_| "a length past usize".to_string())?;
        if n.checked_mul(unit).is_none_or(|b| b > self.0.len()) {
            return Err("a length past the frame".to_string());
        }
        Ok(n)
    }
    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let n = self.len(1)?;
        Ok(self.take(n)?.to_vec())
    }
    fn opt16(&mut self) -> Result<Option<u16>, String> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u16()?)),
            _ => Err("a bad option tag".to_string()),
        }
    }
    fn lanes(&mut self) -> Result<Vec<i128>, String> {
        let n = self.len(16)?;
        (0..n).map(|_| self.i128()).collect()
    }
    fn carries(&mut self) -> Result<Vec<Vec<i128>>, String> {
        let n = self.u32()? as usize;
        if n > 1 << 16 {
            return Err("too many carries".to_string());
        }
        (0..n).map(|_| self.lanes()).collect()
    }
    fn done(&self) -> Result<(), String> {
        if self.0.is_empty() { Ok(()) } else { Err("trailing bytes in a frame".to_string()) }
    }
}

impl CellRequestV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match self {
            Self::Hello { version } => {
                w.u8(0);
                w.u32(*version);
            }
            Self::Open { program, occ, params } => {
                w.u8(1);
                w.bytes(program);
                w.u32(occ.0);
                w.u32(occ.1);
                w.u32(params.len() as u32);
                for p in params {
                    w.u16(p.param);
                    w.opt16(p.layer);
                    w.bytes(&p.bytes);
                }
            }
            Self::Step { token, occ, carry_in } => {
                w.u8(2);
                w.u32(*token);
                w.u32(occ.0);
                w.u32(occ.1);
                w.carries(carry_in);
            }
            Self::Fixed { state, layer, first, n } => {
                w.u8(3);
                w.u16(*state);
                w.opt16(*layer);
                w.u32(*first);
                w.u32(*n);
            }
            Self::Hist { state, layer, h_tile, first_lane, row_lanes } => {
                w.u8(4);
                w.u16(*state);
                w.opt16(*layer);
                w.u32(*h_tile);
                w.u32(*first_lane);
                w.u32(*row_lanes);
            }
            Self::Close => w.u8(5),
        }
        w.0
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut r = R(bytes);
        let out = match r.u8()? {
            0 => Self::Hello { version: r.u32()? },
            1 => {
                let program = r.bytes()?;
                let occ = (r.u32()?, r.u32()?);
                let n = r.u32()? as usize;
                if n > 1 << 20 {
                    return Err("too many params".to_string());
                }
                let mut params = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    params.push(CellParamBlobV1 { param: r.u16()?, layer: r.opt16()?, bytes: r.bytes()? });
                }
                Self::Open { program, occ, params }
            }
            2 => Self::Step { token: r.u32()?, occ: (r.u32()?, r.u32()?), carry_in: r.carries()? },
            3 => Self::Fixed { state: r.u16()?, layer: r.opt16()?, first: r.u32()?, n: r.u32()? },
            4 => Self::Hist { state: r.u16()?, layer: r.opt16()?, h_tile: r.u32()?, first_lane: r.u32()?, row_lanes: r.u32()? },
            5 => Self::Close,
            t => return Err(format!("request tag {t}")),
        };
        r.done()?;
        Ok(out)
    }
}

impl CellResponseV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match self {
            Self::Ready { version, name, capacity } => {
                w.u8(0);
                w.u32(*version);
                w.bytes(name.as_bytes());
                match capacity {
                    Some(c) => {
                        w.u8(1);
                        w.u64(*c);
                    }
                    None => w.u8(0),
                }
            }
            Self::Opened => w.u8(1),
            Self::Stepped { values, carry, logits } => {
                w.u8(2);
                w.u32(values.len() as u32);
                for v in values {
                    w.u32(v.slot);
                    w.u8(v.block);
                    w.opt16(v.layer);
                    w.u16(v.node);
                    w.u8(v.dtype);
                    w.u32(v.shape.len() as u32);
                    for d in &v.shape {
                        w.u32(*d);
                    }
                    w.lanes(&v.lanes);
                }
                w.carries(carry);
                w.u64(logits.len() as u64);
                for x in logits {
                    w.u32(*x as u32);
                }
            }
            Self::Lanes(b) => {
                w.u8(3);
                w.bytes(b);
            }
            Self::Closed => w.u8(4),
            Self::Refused(why) => {
                w.u8(5);
                w.bytes(why.as_bytes());
            }
        }
        w.0
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut r = R(bytes);
        let out = match r.u8()? {
            0 => {
                let version = r.u32()?;
                let name = String::from_utf8(r.bytes()?).map_err(|_| "a name that is not utf-8".to_string())?;
                let capacity = match r.u8()? {
                    0 => None,
                    1 => Some(r.u64()?),
                    _ => return Err("a bad capacity tag".to_string()),
                };
                Self::Ready { version, name, capacity }
            }
            1 => Self::Opened,
            2 => {
                let n = r.u32()? as usize;
                if n > 1 << 24 {
                    return Err("too many values".to_string());
                }
                let mut values = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let (slot, block, layer, node, dtype) = (r.u32()?, r.u8()?, r.opt16()?, r.u16()?, r.u8()?);
                    let dims = r.u32()? as usize;
                    if dims > 16 {
                        return Err("a rank past 16".to_string());
                    }
                    let shape = (0..dims).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?;
                    values.push(CellValueV1 { slot, block, layer, node, dtype, shape, lanes: r.lanes()? });
                }
                let carry = r.carries()?;
                let n = r.len(4)?;
                let logits = (0..n).map(|_| r.u32().map(|x| x as i32)).collect::<Result<Vec<_>, _>>()?;
                Self::Stepped { values, carry, logits }
            }
            3 => Self::Lanes(r.bytes()?),
            4 => Self::Closed,
            5 => Self::Refused(String::from_utf8(r.bytes()?).map_err(|_| "a refusal that is not utf-8".to_string())?),
            t => return Err(format!("response tag {t}")),
        };
        r.done()?;
        Ok(out)
    }
}

fn write_frame(w: &mut dyn Write, payload: &[u8]) -> std::io::Result<()> {
    w.write_all(&(payload.len() as u64).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

fn read_frame(r: &mut dyn Read) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 8];
    r.read_exact(&mut len)?;
    let n = u64::from_le_bytes(len);
    if n > CELL_WIRE_MAX_FRAME_V1 as u64 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "a frame past the ceiling"));
    }
    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------------------------
// The server loop (the helper's whole job)
// ---------------------------------------------------------------------------------------------

struct Collect(Vec<CellValueV1>);

impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.push(CellValueV1 {
                slot: v.slot,
                block: v.block,
                layer: v.layer,
                node: v.node,
                dtype: v.dtype.tag(),
                shape: v.shape.iter().map(|d| *d as u32).collect(),
                lanes: v.data.to_i128s(),
            });
        }
    }
}

/// **Serve cell steps over `reader`/`writer` on `device`** until the peer closes the pipe. One session at a time.
pub fn serve_cell_requests_v1(reader: &mut dyn Read, writer: &mut dyn Write, device: &dyn TirDeviceV1) -> std::io::Result<()> {
    loop {
        let frame = match read_frame(reader) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let reply = match CellRequestV1::decode(&frame) {
            Err(why) => CellResponseV1::Refused(why),
            Ok(CellRequestV1::Hello { version }) if version == CELL_WIRE_VERSION_V1 => {
                CellResponseV1::Ready { version, name: device.name(), capacity: device.capacity_bytes() }
            }
            Ok(CellRequestV1::Hello { version }) => CellResponseV1::Refused(format!("wire version {version}, this helper speaks {CELL_WIRE_VERSION_V1}")),
            Ok(CellRequestV1::Open { program, occ, params }) => {
                match open_session(&program, occ, &params) {
                    Err(why) => CellResponseV1::Refused(why),
                    Ok((plan, tp)) => {
                        let range = occ.0 as usize..occ.1 as usize;
                        match device.cell_stepper(&plan, &tp, range) {
                            Err(why) => CellResponseV1::Refused(why),
                            Ok(mut stepper) => {
                                write_frame(writer, &CellResponseV1::Opened.encode())?;
                                session_loop(reader, writer, stepper.as_mut())?;
                                continue;
                            }
                        }
                    }
                }
            }
            Ok(_) => CellResponseV1::Refused("no session is open".to_string()),
        };
        write_frame(writer, &reply.encode())?;
    }
}

fn open_session(program: &[u8], occ: (u32, u32), params: &[CellParamBlobV1]) -> Result<(TirPlan, TirParams<'static>), String> {
    let program = TirProgramV1::decode_canonical(program).map_err(|e| format!("the program: {e}"))?;
    let plan = TirPlan::compile(&program).map_err(|e| format!("the plan: {e}"))?;
    if occ.0 >= occ.1 || occ.1 as usize > plan.occurrences.len() {
        return Err(format!("a cell of occurrences {occ:?} of {}", plan.occurrences.len()));
    }
    let mut map = MapParams::default();
    for p in params {
        let d = program.params.get(p.param as usize).ok_or("a param the program does not declare")?;
        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
        let t = Tensor::from_le_bytes(d.dtype, &shape, &p.bytes).map_err(|e| format!("param {}: {e}", d.name))?;
        map.tensors.insert((p.param, p.layer), t);
    }
    let tp = TirParams::from_map_lenient(&plan, &map).map_err(|e| format!("the params: {e}"))?;
    Ok((plan, tp))
}

fn session_loop(reader: &mut dyn Read, writer: &mut dyn Write, stepper: &mut dyn TirCellStepperV1) -> std::io::Result<()> {
    loop {
        let frame = read_frame(reader)?;
        let reply = match CellRequestV1::decode(&frame) {
            Err(why) => CellResponseV1::Refused(why),
            Ok(CellRequestV1::Close) => {
                write_frame(writer, &CellResponseV1::Closed.encode())?;
                return Ok(());
            }
            Ok(CellRequestV1::Step { token, occ, carry_in }) => {
                let mut sink = Collect(Vec::new());
                match stepper.step_cell(token, occ.0 as usize..occ.1 as usize, &carry_in, &mut sink) {
                    Ok(carry) => CellResponseV1::Stepped { values: sink.0, carry, logits: stepper.logits_lanes() },
                    Err(e) => CellResponseV1::Refused(format!("{:?}: {}", e.kind, e.msg)),
                }
            }
            Ok(CellRequestV1::Fixed { state, layer, first, n }) => {
                let mut out = Vec::new();
                match stepper.fixed_lanes(state, layer, first as usize, n as usize, &mut out) {
                    Ok(()) => CellResponseV1::Lanes(out),
                    Err(why) => CellResponseV1::Refused(why),
                }
            }
            Ok(CellRequestV1::Hist { state, layer, h_tile, first_lane, row_lanes }) => {
                let mut out = Vec::new();
                match stepper.hist_tile_lanes(state, layer, h_tile as usize, first_lane as usize, row_lanes as usize, &mut out) {
                    Ok(()) => CellResponseV1::Lanes(out),
                    Err(why) => CellResponseV1::Refused(why),
                }
            }
            Ok(_) => CellResponseV1::Refused("a session is open".to_string()),
        };
        write_frame(writer, &reply.encode())?;
    }
}

// ---------------------------------------------------------------------------------------------
// A CPU device (the fake helper's, and a mirror's reference)
// ---------------------------------------------------------------------------------------------

/// The CPU executor as a [`TirDeviceV1`] — what a fake helper serves in tests.
pub struct CpuDeviceV1;

impl TirDeviceV1 for CpuDeviceV1 {
    fn name(&self) -> String {
        "cpu-device".to_string()
    }
    fn capacity_bytes(&self) -> Option<u64> {
        None
    }
    fn cell_stepper<'a>(
        &'a self,
        plan: &'a TirPlan,
        params: &'a TirParams<'a>,
        occ: Range<usize>,
    ) -> Result<Box<dyn TirCellStepperV1 + 'a>, String> {
        let mut exec = TirExecutor::new_cell(plan, params, occ).map_err(|e| e.to_string())?;
        exec.set_hist_tail(HIST_TAIL_ROWS);
        Ok(Box::new(CpuCellStepperV1(exec)))
    }
}

/// Rows of history tail a CPU stepper keeps for tile reads (at least every class's `h_tile`).
const HIST_TAIL_ROWS: usize = 256;

// ---------------------------------------------------------------------------------------------
// The client: a process (or any pipe pair) as a TirDeviceV1
// ---------------------------------------------------------------------------------------------

struct Io {
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    /// The spawned helper, killed when the device drops.
    child: Option<std::process::Child>,
}

impl Drop for Io {
    fn drop(&mut self) {
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// **A helper process as a device.** `mirror`: check every answer against the CPU executor's.
pub struct TirProcessDeviceV1 {
    io: Mutex<Io>,
    name: String,
    capacity: Option<u64>,
    mirror: bool,
    poisoned: AtomicBool,
}

impl TirProcessDeviceV1 {
    /// Connect over a pipe pair and say hello.
    pub fn connect(reader: Box<dyn Read + Send>, writer: Box<dyn Write + Send>, mirror: bool) -> Result<Self, String> {
        Self::connect_with(Io { reader, writer, child: None }, mirror)
    }

    /// Spawn `program args…` with its stdin and stdout as the pipe pair.
    pub fn spawn(program: &std::path::Path, args: &[String], mirror: bool) -> Result<Self, String> {
        let mut child = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| format!("cannot start the device helper {}: {e}", program.display()))?;
        let writer = child.stdin.take().ok_or("the helper has no stdin")?;
        let reader = child.stdout.take().ok_or("the helper has no stdout")?;
        Self::connect_with(Io { reader: Box::new(reader), writer: Box::new(writer), child: Some(child) }, mirror)
    }

    /// (Tests) say hello with a version the helper does not speak.
    pub fn connect_version_for_test(reader: Box<dyn Read + Send>, writer: Box<dyn Write + Send>, version: u32) -> Result<(), String> {
        let mut io = Io { reader, writer, child: None };
        match Self::call(&mut io, &CellRequestV1::Hello { version })? {
            CellResponseV1::Refused(why) => Err(why),
            other => Ok(drop(other)),
        }
    }

    fn connect_with(mut io: Io, mirror: bool) -> Result<Self, String> {
        let reply = Self::call(&mut io, &CellRequestV1::Hello { version: CELL_WIRE_VERSION_V1 })?;
        let CellResponseV1::Ready { version, name, capacity } = reply else { return Err(format!("the helper answered hello with {reply:?}")) };
        if version != CELL_WIRE_VERSION_V1 {
            return Err(format!("the helper speaks wire version {version}"));
        }
        Ok(Self { io: Mutex::new(io), name, capacity, mirror, poisoned: AtomicBool::new(false) })
    }

    fn call(io: &mut Io, req: &CellRequestV1) -> Result<CellResponseV1, String> {
        write_frame(&mut io.writer, &req.encode()).map_err(|e| format!("the helper's pipe: {e}"))?;
        let frame = read_frame(&mut io.reader).map_err(|e| format!("the helper's pipe: {e}"))?;
        CellResponseV1::decode(&frame)
    }

    /// Whether an error or a mirror difference has taken this device out of service.
    pub fn poisoned(&self) -> bool {
        self.poisoned.load(Ordering::SeqCst)
    }
}

impl TirDeviceV1 for TirProcessDeviceV1 {
    fn name(&self) -> String {
        format!("helper:{}{}", self.name, if self.mirror { " (mirrored)" } else { "" })
    }
    fn capacity_bytes(&self) -> Option<u64> {
        self.capacity
    }
    fn cell_stepper<'a>(
        &'a self,
        plan: &'a TirPlan,
        params: &'a TirParams<'a>,
        occ: Range<usize>,
    ) -> Result<Box<dyn TirCellStepperV1 + 'a>, String> {
        if self.poisoned() {
            return Err("the device helper is out of service (an error or a mirror difference)".to_string());
        }
        // The params of the cell's own occurrences, and nothing else.
        let mut wanted: BTreeMap<(u16, Option<u16>), ()> = BTreeMap::new();
        for (o, &(block, layer)) in plan.occurrences.iter().enumerate() {
            if !occ.contains(&o) {
                continue;
            }
            for n in &plan.program.blocks[block as usize].nodes {
                for r in &n.inputs {
                    if let Ref::Param(j) = r {
                        let l = if plan.program.params[*j as usize].per_layer { layer } else { None };
                        wanted.insert((*j, l), ());
                    }
                }
            }
        }
        let mut blobs = Vec::with_capacity(wanted.len());
        for (j, l) in wanted.keys() {
            let d = &plan.program.params[*j as usize];
            let slice = params.get(*j, *l).ok_or_else(|| format!("param {} is not bound for the cell", d.name))?;
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            let t = Tensor { dtype: d.dtype, shape, data: slice.to_i128s() };
            blobs.push(CellParamBlobV1 { param: *j, layer: *l, bytes: t.to_le_bytes() });
        }
        let cpu = if self.mirror {
            let mut e = TirExecutor::new_cell(plan, params, occ.clone()).map_err(|e| e.to_string())?;
            e.set_hist_tail(HIST_TAIL_ROWS);
            Some(CpuCellStepperV1(e))
        } else {
            None
        };
        let mut guard = self.io.lock().map_err(|_| "the helper's lock is poisoned".to_string())?;
        let open = CellRequestV1::Open { program: plan.program.encode(), occ: (occ.start as u32, occ.end as u32), params: blobs };
        match Self::call(&mut guard, &open) {
            Ok(CellResponseV1::Opened) => {}
            Ok(CellResponseV1::Refused(why)) => return Err(format!("the helper refuses the cell: {why}")),
            Ok(other) => {
                self.poisoned.store(true, Ordering::SeqCst);
                return Err(format!("the helper answered open with {other:?}"));
            }
            Err(why) => {
                self.poisoned.store(true, Ordering::SeqCst);
                return Err(why);
            }
        }
        Ok(Box::new(ProcessStepper { device: self, pipe: std::cell::RefCell::new(Some(guard)), cpu, logits: Vec::new() }))
    }
}

struct ProcessStepper<'a> {
    device: &'a TirProcessDeviceV1,
    pipe: std::cell::RefCell<Option<std::sync::MutexGuard<'a, Io>>>,
    cpu: Option<CpuCellStepperV1<'a>>,
    logits: Vec<i32>,
}

impl ProcessStepper<'_> {
    fn call(&self, req: &CellRequestV1) -> Result<CellResponseV1, String> {
        let mut pipe = self.pipe.borrow_mut();
        let guard = pipe.as_mut().ok_or("the session is closed")?;
        TirProcessDeviceV1::call(guard, req).inspect_err(|_| self.device.poisoned.store(true, Ordering::SeqCst))
    }
    fn differ(&self, what: &str) -> String {
        self.device.poisoned.store(true, Ordering::SeqCst);
        format!("the device's {what} differs from the CPU executor's: the helper is out of service")
    }
    /// A read of state lanes, from the helper — and from the CPU too when mirrored, the two byte strings equal.
    fn lanes(
        &self,
        req: CellRequestV1,
        out: &mut Vec<u8>,
        cpu_read: impl FnOnce(&CpuCellStepperV1<'_>, &mut Vec<u8>) -> Result<(), String>,
    ) -> Result<(), String> {
        let CellResponseV1::Lanes(bytes) = (match self.call(&req)? {
            CellResponseV1::Refused(why) => return Err(why),
            other => other,
        }) else {
            return Err("the helper's lanes answer".to_string());
        };
        if let Some(cpu) = self.cpu.as_ref() {
            let mut mine = Vec::new();
            cpu_read(cpu, &mut mine)?;
            if mine != bytes {
                return Err(self.differ("state lanes"));
            }
        }
        out.extend_from_slice(&bytes);
        Ok(())
    }
}

impl Drop for ProcessStepper<'_> {
    fn drop(&mut self) {
        let _ = self.call(&CellRequestV1::Close);
    }
}

impl TirCellStepperV1 for ProcessStepper<'_> {
    fn step_cell(&mut self, token: u32, occ: Range<usize>, carry_in: &[Vec<i128>], sink: &mut dyn StepSink) -> TirResult<Vec<Vec<i128>>> {
        let fail = |m: String| TirError::new(TirErrorKind::Operand, m);
        let req = CellRequestV1::Step { token, occ: (occ.start as u32, occ.end as u32), carry_in: carry_in.to_vec() };
        let reply = self.call(&req).map_err(fail)?;
        let (values, carry, logits) = match reply {
            CellResponseV1::Stepped { values, carry, logits } => (values, carry, logits),
            CellResponseV1::Refused(why) => return Err(fail(format!("the helper refuses the step: {why}"))),
            other => return Err(fail(format!("the helper's step answer: {other:?}"))),
        };
        // The mirror: the CPU runs the same step; any difference takes the device out of service.
        if let Some(cpu) = self.cpu.as_mut() {
            let mut mine = Collect(Vec::new());
            let cpu_carry = cpu.step_cell(token, occ, carry_in, &mut mine)?;
            if mine.0 != values {
                return Err(fail(self.differ("committed values")));
            }
            if cpu_carry != carry {
                return Err(fail(self.differ("carry-out")));
            }
            if cpu.logits_lanes() != logits {
                return Err(fail(self.differ("logits row")));
            }
        }
        self.logits = logits;
        for v in &values {
            let dtype = *DType::ALL.get(v.dtype as usize).ok_or_else(|| fail("a dtype tag the helper invented".to_string()))?;
            let shape: Vec<usize> = v.shape.iter().map(|d| *d as usize).collect();
            let buf = Buf::from_i128s(dtype, &v.lanes);
            sink.node(&NodeValue { slot: v.slot, block: v.block, layer: v.layer, node: v.node, commit: true, dtype, shape: &shape, data: buf.slice() });
        }
        Ok(carry)
    }

    fn logits_lanes(&self) -> Vec<i32> {
        self.logits.clone()
    }

    fn fixed_lanes(&self, state: u16, layer: Option<u16>, first: usize, n: usize, out: &mut Vec<u8>) -> Result<(), String> {
        self.lanes(CellRequestV1::Fixed { state, layer, first: first as u32, n: n as u32 }, out, |cpu, tmp| {
            cpu.fixed_lanes(state, layer, first, n, tmp)
        })
    }

    fn hist_tile_lanes(
        &self,
        state: u16,
        layer: Option<u16>,
        h_tile: usize,
        first_lane: usize,
        row_lanes: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), String> {
        self.lanes(
            CellRequestV1::Hist { state, layer, h_tile: h_tile as u32, first_lane: first_lane as u32, row_lanes: row_lanes as u32 },
            out,
            |cpu, tmp| cpu.hist_tile_lanes(state, layer, h_tile, first_lane, row_lanes, tmp),
        )
    }
}
