//! Stateful Hello2 transport. Returned data is not a computation verdict. The
//! caller supplies stream deadlines and independent external-owner admission.
use std::{
    fmt,
    io::{self, Read, Write},
    mem::size_of,
    ops::Range,
};

use super::{
    budget::{WideCellLimitsV2, WideCellSessionQuoteV2, WideCellUsageV2, WireBudget, quote_client, quote_server},
    codec::{self, CELL_WIRE_VERSION_V2, CellRequestWideV2 as Request, CellResponseWideV2 as Response, CellValueV1},
    cpu::{CpuWideCellStepperV2, PreparedWideCellV2},
};
use crate::{Elem, NodeValue, StepSink, with_slice};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WideCellIoErrorV2 {
    pub message: String,
    /// Accepted logical charges survive decode/execution/I/O refusal.
    pub usage: WideCellUsageV2,
}
impl fmt::Display for WideCellIoErrorV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for WideCellIoErrorV2 {}
fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "Hello2 arithmetic overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "Hello2 arithmetic overflow".into())
}
fn count(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "Hello2 native index width".into())
}
fn exact<T>(n: usize) -> Result<Vec<T>, String> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| "Hello2 allocation refused")?;
    if v.capacity() != n {
        return Err("Hello2 allocator exceeded exact request".into());
    }
    Ok(v)
}
fn error(message: impl Into<String>, budget: &WireBudget) -> WideCellIoErrorV2 {
    WideCellIoErrorV2 { message: message.into(), usage: budget.usage() }
}

// Metadata-only sizing still visits headers/shapes. Admit their bounded work
// before invoking either codec sizing/validation path. No lane value scan is
// hidden in the Counter; the admitted byte cost also covers actual encoding.
fn admit_headers(budget: &mut WireBudget, limits: WideCellLimitsV2) -> Result<(), String> {
    let headers = add(add(limits.max_params, limits.max_carry_vectors)?, mul(limits.max_values, add(limits.max_rank, 1)?)?)?;
    budget.charge_work(mul(add(headers, 128)?, 128)?)
}
fn read_body(reader: &mut dyn Read, budget: &mut WireBudget, quote: &WideCellSessionQuoteV2) -> Result<Option<Vec<u8>>, String> {
    budget.ensure_peak(quote)?;
    let mut prefix = [0u8; 8];
    loop {
        match reader.read(&mut prefix[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    reader.read_exact(&mut prefix[1..]).map_err(|e| e.to_string())?;
    let n = u64::from_le_bytes(prefix);
    budget.admit_read(n)?; // raw body ceiling; accepted bytes include the prefix
    let n = usize::try_from(n).map_err(|_| "Hello2 frame native width")?;
    let mut body = exact(n)?;
    body.resize(n, 0);
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok(Some(body))
}
fn write_request(
    writer: &mut dyn Write,
    message: &Request,
    limits: WideCellLimitsV2,
    budget: &mut WireBudget,
    quote: &WideCellSessionQuoteV2,
) -> Result<(), String> {
    budget.ensure_peak(quote)?;
    admit_headers(budget, limits)?;
    // O(1) outer count checks precede any metadata walk of caller-owned messages.
    match message {
        Request::Open { program, params, .. }
            if count(program.len())? > limits.max_program_bytes || count(params.len())? > limits.max_params =>
        {
            return Err("Hello2 request header ceiling".into());
        }
        Request::Step { carry_in, .. } if count(carry_in.len())? > limits.max_carry_vectors => {
            return Err("Hello2 carry header ceiling".into());
        }
        _ => {}
    }
    let n = codec::encoded_length_request(message)?;
    budget.admit_write(n)?;
    let body = message.encode_bounded(limits)?;
    writer.write_all(&n.to_le_bytes()).and_then(|_| writer.write_all(&body)).and_then(|_| writer.flush()).map_err(|e| e.to_string())
}
fn write_response(
    writer: &mut dyn Write,
    message: &Response,
    limits: WideCellLimitsV2,
    budget: &mut WireBudget,
    quote: &WideCellSessionQuoteV2,
) -> Result<(), String> {
    budget.ensure_peak(quote)?;
    admit_headers(budget, limits)?;
    let n = codec::encoded_length_response(message)?;
    budget.admit_write(n)?;
    let body = message.encode_bounded(limits)?;
    writer.write_all(&n.to_le_bytes()).and_then(|_| writer.write_all(&body)).and_then(|_| writer.flush()).map_err(|e| e.to_string())
}

/// Bounded raw data client, deliberately not a V1 device/kernel backend.
/// No CPU mirror or arithmetic/commitment acceptance is implied by a reply.
pub struct WideCellClientV2<R: Read, W: Write> {
    reader: R,
    writer: W,
    limits: WideCellLimitsV2,
    quote: WideCellSessionQuoteV2,
    budget: WireBudget,
    open: bool,
    poisoned: bool,
    peer_name: String,
    peer_capacity: Option<u64>,
}
impl<R: Read, W: Write> WideCellClientV2<R, W> {
    pub fn connect(reader: R, writer: W, limits: WideCellLimitsV2) -> Result<Self, WideCellIoErrorV2> {
        let budget = WireBudget::new(limits);
        let quote = quote_client(limits).map_err(|e| error(e, &budget))?;
        let mut client = Self {
            reader,
            writer,
            limits,
            quote,
            budget,
            open: false,
            poisoned: false,
            peer_name: String::new(),
            peer_capacity: None,
        };
        let reply = client.transact(&Request::Hello { version: CELL_WIRE_VERSION_V2 })?;
        let backing = codec::native_backing_response(&reply).map_err(|e| client.fail(e))?;
        match reply {
            Response::Ready { version: CELL_WIRE_VERSION_V2, name, capacity } => {
                client.budget.export(backing).map_err(|e| client.fail(e))?;
                client.peer_name = name;
                client.peer_capacity = capacity;
                Ok(client)
            }
            Response::Refused(why) => Err(client.fail(why)),
            _ => Err(client.fail("Hello2 requires Ready version2; no V1 fallback")),
        }
    }
    fn fail(&mut self, message: impl Into<String>) -> WideCellIoErrorV2 {
        self.poisoned = true;
        error(message, &self.budget)
    }
    fn transact(&mut self, request: &Request) -> Result<Response, WideCellIoErrorV2> {
        if self.poisoned {
            return Err(self.fail("Hello2 connection is poisoned"));
        }
        write_request(&mut self.writer, request, self.limits, &mut self.budget, &self.quote).map_err(|e| self.fail(e))?;
        let body = read_body(&mut self.reader, &mut self.budget, &self.quote)
            .map_err(|e| self.fail(e))?
            .ok_or_else(|| self.fail("Hello2 peer closed before reply"))?;
        Response::decode_bounded(&body, self.limits).map_err(|e| self.fail(e))
    }
    pub fn request(&mut self, request: &Request) -> Result<Response, WideCellIoErrorV2> {
        if self.poisoned {
            return Err(self.fail("Hello2 connection is poisoned"));
        }
        let allowed = matches!(
            (self.open, request),
            (false, Request::Open { .. })
                | (true, Request::Step { .. } | Request::Fixed { .. } | Request::Hist { .. } | Request::Close)
        );
        if !allowed {
            return Err(self.fail("Hello2 request violates negotiated session state"));
        }
        if matches!(request, Request::Step { .. }) {
            self.budget.admit_step(0).map_err(|e| self.fail(e))?;
        }
        let reply = self.transact(request)?;
        if let Response::Refused(why) = reply {
            return Err(self.fail(why));
        }
        let valid = matches!(
            (request, &reply),
            (Request::Open { .. }, Response::Opened)
                | (Request::Step { .. }, Response::Stepped { .. })
                | (Request::Fixed { .. } | Request::Hist { .. }, Response::Lanes(_))
                | (Request::Close, Response::Closed)
        );
        if !valid {
            return Err(self.fail("Hello2 response tag violates request state"));
        }
        let expected_query_bytes = match request {
            Request::Fixed { n, .. } => Some(mul(u64::from(*n), 4)),
            Request::Hist { h_tile, row_lanes, .. } => Some(mul(u64::from(*h_tile), u64::from(*row_lanes)).and_then(|n| mul(n, 4))),
            _ => None,
        };
        if let Some(expected) = expected_query_bytes {
            let expected = expected.map_err(|e| self.fail(e))?;
            let Response::Lanes(bytes) = &reply else { unreachable!() };
            if count(bytes.len()).map_err(|e| self.fail(e))? != expected {
                return Err(self.fail("Hello2 query response length differs from request"));
            }
        }
        let backing = codec::native_backing_response(&reply).map_err(|e| self.fail(e))?;
        self.budget.export(backing).map_err(|e| self.fail(e))?;
        match request {
            Request::Open { .. } => self.open = true,
            Request::Close => self.open = false,
            _ => {}
        }
        Ok(reply)
    }
    pub fn poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn usage(&self) -> WideCellUsageV2 {
        self.budget.usage()
    }
    pub fn peer_name(&self) -> &str {
        &self.peer_name
    }
    pub fn peer_capacity(&self) -> Option<u64> {
        self.peer_capacity
    }
}

struct Collector {
    values: Vec<CellValueV1>,
    lanes: u64,
    native: u64,
    limits: WideCellLimitsV2,
}
impl Collector {
    fn new(count: usize, limits: WideCellLimitsV2) -> Result<Self, String> {
        if count as u64 > limits.max_values {
            return Err("Hello2 collector value ceiling".into());
        }
        let native = add(size_of::<Response>() as u64, mul(count as u64, size_of::<CellValueV1>() as u64)?)?;
        if native > limits.max_decoded_bytes {
            return Err("Hello2 collector header ceiling".into());
        }
        Ok(Self { values: exact(count)?, lanes: 0, native, limits })
    }
    fn collect(&mut self, value: &NodeValue<'_>) -> Result<(), String> {
        if !value.commit {
            return Ok(());
        }
        if self.values.len() == self.values.capacity() || value.shape.len() as u64 > self.limits.max_rank {
            return Err("Hello2 collector shape/count ceiling".into());
        }
        let lanes = add(self.lanes, count(value.data.len())?)?;
        let native = add(self.native, add(mul(count(value.shape.len())?, 4)?, mul(count(value.data.len())?, 16)?)?)?;
        if lanes > self.limits.max_lanes || native > self.limits.max_decoded_bytes {
            return Err("Hello2 collector native/lane ceiling".into());
        }
        let mut shape = exact(value.shape.len())?;
        let mut product = 1usize;
        for dim in value.shape {
            product = product.checked_mul(*dim).ok_or("Hello2 collector shape overflow")?;
            shape.push(u32::try_from(*dim).map_err(|_| "Hello2 shape wire width")?);
        }
        if product != value.data.len() {
            return Err("Hello2 native value shape differs".into());
        }
        let mut values = exact(value.data.len())?;
        with_slice!(value.data,data=>{for v in data{let v=v.to_i128();if !value.dtype.contains(v){return Err("Hello2 native value exceeds declared dtype".into());}values.push(v);}});
        self.values.push(CellValueV1 {
            slot: value.slot,
            block: value.block,
            layer: value.layer,
            node: value.node,
            dtype: value.dtype.tag(),
            shape,
            lanes: values,
        });
        self.lanes = lanes;
        self.native = native;
        Ok(())
    }
}
impl StepSink for Collector {
    fn node(&mut self, _: &NodeValue<'_>) {
        unreachable!("Hello2 requires fallible executor delivery");
    }
    fn try_node(&mut self, value: &NodeValue<'_>) -> misaka_palw_tir::TirResult<()> {
        self.collect(value).map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e))
    }
}
fn range(occ: (u32, u32)) -> Result<Range<usize>, String> {
    Ok(usize::try_from(occ.0).map_err(|_| "Hello2 scope width")?..usize::try_from(occ.1).map_err(|_| "Hello2 scope width")?)
}
fn query_work(prepared: &PreparedWideCellV2<'_>, lanes: u64) -> Result<u64, String> {
    let nodes = prepared
        .plan
        .occurrences
        .iter()
        .try_fold(0, |n, (b, _)| add(n, add(1, count(prepared.plan.program.blocks[*b as usize].nodes.len())?)?))?;
    mul(add(add(nodes, count(prepared.plan.instances.len())?)?, add(lanes, 128)?)?, 128)
}
fn process_request<'a>(
    request: Request,
    prepared: &PreparedWideCellV2<'a>,
    limits: WideCellLimitsV2,
    budget: &mut WireBudget,
    hello: &mut bool,
    cell: &mut Option<CpuWideCellStepperV2<'a>>,
) -> Result<Response, String> {
    match request {
        Request::Hello { version } if !*hello && cell.is_none() => {
            if version != CELL_WIRE_VERSION_V2 {
                return Err("Hello2 requires version2; no V1 fallback".into());
            }
            *hello = true;
            Ok(Response::Ready { version: CELL_WIRE_VERSION_V2, name: "cpu-prepared-wide-v2".into(), capacity: None })
        }
        _ if !*hello => Err("Hello2 handshake is required before Open".into()),
        Request::Open { program, occ, params } if cell.is_none() => {
            budget.charge_work(prepared.open_work(limits)?)?;
            let occ = range(occ)?;
            prepared.check_open(&program, occ.clone(), &params, limits)?;
            *cell = Some(prepared.open_cell(occ, limits)?);
            Ok(Response::Opened)
        }
        Request::Close if cell.is_some() => {
            *cell = None;
            Ok(Response::Closed)
        }
        Request::Step { token, occ, carry_in } => {
            let cell = cell.as_mut().ok_or("Hello2 no cell is open")?;
            let occ = range(occ)?;
            budget.admit_step(cell.kernel_work_for_next(occ.clone())?)?;
            let n = cell.committed_count_for_next(occ.clone())?;
            let mut sink = Collector::new(n, limits)?;
            let carry = cell.step_data(token, occ, &carry_in, &mut sink)?;
            let logits = cell.logits_lanes()?;
            if sink.values.len() != n {
                return Err("Hello2 CPU omitted committed data".into());
            }
            Ok(Response::Stepped { values: sink.values, carry, logits })
        }
        Request::Fixed { state, layer, first, n } => {
            budget.charge_work(query_work(prepared, u64::from(n))?)?;
            Ok(Response::Lanes(cell.as_ref().ok_or("Hello2 no cell is open")?.fixed_lanes(
                state,
                layer,
                first as usize,
                n as usize,
            )?))
        }
        Request::Hist { state, layer, h_tile, first_lane, row_lanes } => {
            budget.charge_work(query_work(prepared, mul(u64::from(h_tile), u64::from(row_lanes))?)?)?;
            Ok(Response::Lanes(cell.as_ref().ok_or("Hello2 no cell is open")?.hist_tile_lanes(
                state,
                layer,
                h_tile as usize,
                first_lane as usize,
                row_lanes as usize,
            )?))
        }
        _ => Err("Hello2 message violates session state".into()),
    }
}

/// First subset: one independently prepared resident plan/parameter source.
/// No artifact loader, wire program decoder, GPU, native claim or registry path.
/// The optional actual guard owned by an opened CPU cell covers workspace only;
/// transport/copy caps and caller external reservations remain separate.
pub fn serve_wide_v2_for_plan(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    prepared: &PreparedWideCellV2<'_>,
    limits: WideCellLimitsV2,
) -> Result<WideCellUsageV2, WideCellIoErrorV2> {
    let mut budget = WireBudget::new(limits);
    let quote = quote_server(prepared.verified.space(), prepared.plan, prepared.ctx, limits).map_err(|e| error(e, &budget))?;
    let mut hello = false;
    let mut cell = None;
    loop {
        let attempt = (|| -> Result<Option<Response>, String> {
            let Some(body) = read_body(reader, &mut budget, &quote)? else {
                if hello && cell.is_none() {
                    return Ok(None);
                }
                return Err("Hello2 EOF before handshake or while cell open".into());
            };
            let request = Request::decode_bounded(&body, limits)?;
            process_request(request, prepared, limits, &mut budget, &mut hello, &mut cell).map(Some)
        })();
        match attempt {
            Ok(None) => return Ok(budget.usage()),
            Ok(Some(reply)) => write_response(writer, &reply, limits, &mut budget, &quote).map_err(|e| error(e, &budget))?,
            Err(message) => {
                let reply = Response::Refused(message);
                let _ = write_response(writer, &reply, limits, &mut budget, &quote);
                let Response::Refused(message) = reply else { unreachable!() };
                return Err(error(message, &budget)); // terminal; no reset/retry/fallback
            }
        }
    }
}
