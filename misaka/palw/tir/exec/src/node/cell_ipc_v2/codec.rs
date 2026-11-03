//! Explicit Hello2 body codec. V1 DTO layouts/tags are retained independently;
//! only Stepped logits become signed eight-byte lanes. No Buf/model conversion.
//! Logical backing limits require caller accounting for independent live owners.
use std::mem::size_of;

use misaka_palw_tir::{DType, types::MAX_RANK};

use super::budget::{PARSER_SCRATCH_BYTES, WideCellLimitsV2};
pub use crate::cellproc::{CellParamBlobV1, CellValueV1};

pub const CELL_WIRE_VERSION_V2: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellRequestWideV2 {
    Hello { version: u32 },
    Open { program: Vec<u8>, occ: (u32, u32), params: Vec<CellParamBlobV1> },
    Step { token: u32, occ: (u32, u32), carry_in: Vec<Vec<i128>> },
    Fixed { state: u16, layer: Option<u16>, first: u32, n: u32 },
    Hist { state: u16, layer: Option<u16>, h_tile: u32, first_lane: u32, row_lanes: u32 },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellResponseWideV2 {
    Ready { version: u32, name: String, capacity: Option<u64> },
    Opened,
    Stepped { values: Vec<CellValueV1>, carry: Vec<Vec<i128>>, logits: Vec<i64> },
    Lanes(Vec<u8>),
    Closed,
    Refused(String),
}

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "Hello2 size addition overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "Hello2 size multiplication overflow".into())
}
fn count(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "Hello2 native count exceeds u64".into())
}
fn count32(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "Hello2 vector count exceeds u32".into())
}
fn storage<T>(n: usize) -> Result<u64, String> {
    mul(count(n)?, count(size_of::<T>())?)
}
fn ceiling(value: u64, cap: u64) -> Result<(), String> {
    if value > cap { Err("Hello2 field exceeds its limit".into()) } else { Ok(()) }
}
fn dtype(tag: u8) -> Result<DType, String> {
    DType::ALL.into_iter().find(|d| d.tag() == tag).ok_or_else(|| "Hello2 unknown dtype".into())
}
fn product(shape: &[u32]) -> Result<u64, String> {
    if shape.contains(&0) {
        return Ok(0);
    }
    shape.iter().try_fold(1u64, |n, d| mul(n, u64::from(*d)))
}
fn value_geometry(value: &CellValueV1) -> Result<DType, String> {
    count32(value.shape.len())?;
    if value.shape.len() > MAX_RANK || product(&value.shape)? != count(value.lanes.len())? {
        return Err("Hello2 value shape does not match its lane count".into());
    }
    dtype(value.dtype)
}
fn value_lanes(value: &CellValueV1) -> Result<(), String> {
    let ty = value_geometry(value)?;
    if value.lanes.iter().any(|v| !ty.contains(*v)) {
        return Err("Hello2 value lane is outside its dtype".into());
    }
    Ok(())
}
// A slice would discard the owned outer Vec's spare capacity from the quote.
#[allow(clippy::ptr_arg)]
fn carry_backing(carry: &Vec<Vec<i128>>) -> Result<u64, String> {
    carry.iter().try_fold(storage::<Vec<i128>>(carry.capacity())?, |n, row| add(n, storage::<i128>(row.capacity())?))
}

/// Actual pre-owned request backing, including spare capacities and enum header.
pub fn native_backing_request(message: &CellRequestWideV2) -> Result<u64, String> {
    let mut n = count(size_of::<CellRequestWideV2>())?;
    match message {
        CellRequestWideV2::Open { program, params, .. } => {
            n = add(n, count(program.capacity())?)?;
            n = add(n, storage::<CellParamBlobV1>(params.capacity())?)?;
            for param in params {
                n = add(n, count(param.bytes.capacity())?)?;
            }
        }
        CellRequestWideV2::Step { carry_in, .. } => n = add(n, carry_backing(carry_in)?)?,
        _ => {}
    }
    Ok(n)
}

/// Actual pre-owned response backing, including every native Vec/String capacity.
pub fn native_backing_response(message: &CellResponseWideV2) -> Result<u64, String> {
    let mut n = count(size_of::<CellResponseWideV2>())?;
    match message {
        CellResponseWideV2::Ready { name, .. } | CellResponseWideV2::Refused(name) => n = add(n, count(name.capacity())?)?,
        CellResponseWideV2::Lanes(bytes) => n = add(n, count(bytes.capacity())?)?,
        CellResponseWideV2::Stepped { values, carry, logits } => {
            n = add(n, storage::<CellValueV1>(values.capacity())?)?;
            for value in values {
                n = add(n, storage::<u32>(value.shape.capacity())?)?;
                n = add(n, storage::<i128>(value.lanes.capacity())?)?;
            }
            n = add(n, carry_backing(carry)?)?;
            n = add(n, storage::<i64>(logits.capacity())?)?;
        }
        _ => {}
    }
    Ok(n)
}

fn check_carry(carry: &[Vec<i128>], limits: WideCellLimitsV2, lanes: &mut u64) -> Result<(), String> {
    count32(carry.len())?;
    ceiling(count(carry.len())?, limits.max_carry_vectors)?;
    for row in carry {
        *lanes = add(*lanes, count(row.len())?)?;
        ceiling(*lanes, limits.max_lanes)?;
    }
    Ok(())
}
fn check_request(message: &CellRequestWideV2, limits: WideCellLimitsV2) -> Result<(), String> {
    match message {
        CellRequestWideV2::Open { program, params, .. } => {
            ceiling(count(program.len())?, limits.max_program_bytes)?;
            count32(params.len())?;
            ceiling(count(params.len())?, limits.max_params)?;
            let mut bytes = 0;
            for param in params {
                bytes = add(bytes, count(param.bytes.len())?)?;
                ceiling(bytes, limits.max_param_bytes)?;
            }
        }
        CellRequestWideV2::Step { carry_in, .. } => {
            let mut lanes = 0;
            check_carry(carry_in, limits, &mut lanes)?;
        }
        _ => {}
    }
    Ok(())
}
fn check_response(message: &CellResponseWideV2, limits: WideCellLimitsV2) -> Result<(), String> {
    match message {
        CellResponseWideV2::Ready { name, .. } | CellResponseWideV2::Refused(name) => {
            ceiling(count(name.len())?, limits.max_name_bytes)?;
        }
        CellResponseWideV2::Lanes(bytes) => {
            if bytes.len() % 4 != 0 {
                return Err("Hello2 state payload is not four-byte lanes".into());
            }
            ceiling(count(bytes.len() / 4)?, limits.max_lanes)?;
        }
        CellResponseWideV2::Stepped { values, carry, logits } => {
            count32(values.len())?;
            ceiling(count(values.len())?, limits.max_values)?;
            let mut lanes = 0;
            for value in values {
                ceiling(count(value.shape.len())?, limits.max_rank)?;
                value_geometry(value)?;
                lanes = add(lanes, count(value.lanes.len())?)?;
                ceiling(lanes, limits.max_lanes)?;
            }
            check_carry(carry, limits, &mut lanes)?;
            ceiling(add(lanes, count(logits.len())?)?, limits.max_lanes)?;
            // Aggregate admission precedes the potentially large interval scan.
            for value in values {
                value_lanes(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

trait Wire {
    fn put(&mut self, bytes: &[u8]) -> Result<(), String>;
    fn validate_value(&mut self, value: &CellValueV1) -> Result<(), String> {
        value_lanes(value)
    }
    fn u8(&mut self, v: u8) -> Result<(), String> {
        self.put(&[v])
    }
    fn u16(&mut self, v: u16) -> Result<(), String> {
        self.put(&v.to_le_bytes())
    }
    fn u32(&mut self, v: u32) -> Result<(), String> {
        self.put(&v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) -> Result<(), String> {
        self.put(&v.to_le_bytes())
    }
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.u64(count(bytes.len())?)?;
        self.put(bytes)
    }
    fn opt16(&mut self, v: Option<u16>) -> Result<(), String> {
        match v {
            Some(v) => {
                self.u8(1)?;
                self.u16(v)
            }
            None => self.u8(0),
        }
    }
    fn lanes(&mut self, values: &[i128]) -> Result<(), String> {
        self.u64(count(values.len())?)?;
        for value in values {
            self.put(&value.to_le_bytes())?;
        }
        Ok(())
    }
    fn logits(&mut self, values: &[i64]) -> Result<(), String> {
        self.u64(count(values.len())?)?;
        for value in values {
            self.put(&value.to_le_bytes())?;
        }
        Ok(())
    }
    fn carry(&mut self, carry: &[Vec<i128>]) -> Result<(), String> {
        self.u32(count32(carry.len())?)?;
        for row in carry {
            self.lanes(row)?;
        }
        Ok(())
    }
}
struct Counter(u64);
impl Wire for Counter {
    fn put(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.0 = add(self.0, count(bytes.len())?)?;
        Ok(())
    }
    fn validate_value(&mut self, value: &CellValueV1) -> Result<(), String> {
        value_geometry(value).map(|_| ())
    }
    fn lanes(&mut self, values: &[i128]) -> Result<(), String> {
        self.0 = add(self.0, add(8, mul(count(values.len())?, 16)?)?)?;
        Ok(())
    }
    fn logits(&mut self, values: &[i64]) -> Result<(), String> {
        self.0 = add(self.0, add(8, mul(count(values.len())?, 8)?)?)?;
        Ok(())
    }
}
struct Writer<'a> {
    bytes: &'a mut Vec<u8>,
    limit: usize,
}
impl Wire for Writer<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.bytes.len().checked_add(bytes.len()).is_none_or(|n| n > self.limit) {
            return Err("Hello2 encoded body exceeds its counted size".into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}
fn write_request(message: &CellRequestWideV2, w: &mut impl Wire) -> Result<(), String> {
    match message {
        CellRequestWideV2::Hello { version } => {
            w.u8(0)?;
            w.u32(*version)?;
        }
        CellRequestWideV2::Open { program, occ, params } => {
            w.u8(1)?;
            w.bytes(program)?;
            w.u32(occ.0)?;
            w.u32(occ.1)?;
            w.u32(count32(params.len())?)?;
            for param in params {
                w.u16(param.param)?;
                w.opt16(param.layer)?;
                w.bytes(&param.bytes)?;
            }
        }
        CellRequestWideV2::Step { token, occ, carry_in } => {
            w.u8(2)?;
            w.u32(*token)?;
            w.u32(occ.0)?;
            w.u32(occ.1)?;
            w.carry(carry_in)?;
        }
        CellRequestWideV2::Fixed { state, layer, first, n } => {
            w.u8(3)?;
            w.u16(*state)?;
            w.opt16(*layer)?;
            w.u32(*first)?;
            w.u32(*n)?;
        }
        CellRequestWideV2::Hist { state, layer, h_tile, first_lane, row_lanes } => {
            w.u8(4)?;
            w.u16(*state)?;
            w.opt16(*layer)?;
            w.u32(*h_tile)?;
            w.u32(*first_lane)?;
            w.u32(*row_lanes)?;
        }
        CellRequestWideV2::Close => w.u8(5)?,
    }
    Ok(())
}
fn write_response(message: &CellResponseWideV2, w: &mut impl Wire) -> Result<(), String> {
    match message {
        CellResponseWideV2::Ready { version, name, capacity } => {
            w.u8(0)?;
            w.u32(*version)?;
            w.bytes(name.as_bytes())?;
            match capacity {
                Some(v) => {
                    w.u8(1)?;
                    w.u64(*v)?;
                }
                None => w.u8(0)?,
            }
        }
        CellResponseWideV2::Opened => w.u8(1)?,
        CellResponseWideV2::Stepped { values, carry, logits } => {
            w.u8(2)?;
            w.u32(count32(values.len())?)?;
            for value in values {
                w.validate_value(value)?;
                w.u32(value.slot)?;
                w.u8(value.block)?;
                w.opt16(value.layer)?;
                w.u16(value.node)?;
                w.u8(value.dtype)?;
                w.u32(count32(value.shape.len())?)?;
                for dim in &value.shape {
                    w.u32(*dim)?;
                }
                w.lanes(&value.lanes)?;
            }
            w.carry(carry)?;
            w.logits(logits)?;
        }
        CellResponseWideV2::Lanes(bytes) => {
            if bytes.len() % 4 != 0 {
                return Err("Hello2 state payload is not four-byte lanes".into());
            }
            w.u8(3)?;
            w.bytes(bytes)?;
        }
        CellResponseWideV2::Closed => w.u8(4)?,
        CellResponseWideV2::Refused(why) => {
            w.u8(5)?;
            w.bytes(why.as_bytes())?;
        }
    }
    Ok(())
}

/// Encoded body length, excluding the transport's eight-byte length prefix.
pub fn encoded_length_request(message: &CellRequestWideV2) -> Result<u64, String> {
    let mut counter = Counter(0);
    write_request(message, &mut counter)?;
    Ok(counter.0)
}
/// Encoded body length, excluding the transport's eight-byte length prefix.
/// Checks dtype/shape metadata but does not scan lane values. Encoding and
/// decoding perform interval validation after resource/I/O admission.
pub fn encoded_length_response(message: &CellResponseWideV2) -> Result<u64, String> {
    let mut counter = Counter(0);
    write_response(message, &mut counter)?;
    Ok(counter.0)
}
fn encode_admission(frame: u64, backing: u64, limits: WideCellLimitsV2) -> Result<usize, String> {
    ceiling(frame, limits.max_frame_bytes)?;
    ceiling(backing, limits.max_decoded_bytes)?;
    ceiling(add(add(frame, backing)?, add(PARSER_SCRATCH_BYTES, limits.external_retained_bytes)?)?, limits.max_operation_bytes)?;
    usize::try_from(frame).map_err(|_| "Hello2 body exceeds native allocation width".into())
}
fn exact_vec<T>(count: usize) -> Result<Vec<T>, String> {
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_| "Hello2 allocation refused")?;
    if out.capacity() != count {
        return Err("Hello2 allocator exceeded exact request".into());
    }
    Ok(out)
}
fn encode_request(message: &CellRequestWideV2, limits: WideCellLimitsV2) -> Result<Vec<u8>, String> {
    check_request(message, limits)?;
    let n = encode_admission(encoded_length_request(message)?, native_backing_request(message)?, limits)?;
    let mut bytes = exact_vec(n)?;
    write_request(message, &mut Writer { bytes: &mut bytes, limit: n })?;
    if bytes.len() != n {
        return Err("Hello2 encoded body differs from counted size".into());
    }
    Ok(bytes)
}
fn encode_response(message: &CellResponseWideV2, limits: WideCellLimitsV2) -> Result<Vec<u8>, String> {
    check_response(message, limits)?;
    let n = encode_admission(encoded_length_response(message)?, native_backing_response(message)?, limits)?;
    let mut bytes = exact_vec(n)?;
    write_response(message, &mut Writer { bytes: &mut bytes, limit: n })?;
    if bytes.len() != n {
        return Err("Hello2 encoded body differs from counted size".into());
    }
    Ok(bytes)
}

struct Reader<'a> {
    bytes: &'a [u8],
    storage_left: u64,
    limits: WideCellLimitsV2,
    lanes: u64,
    param_bytes: u64,
}
impl<'a> Reader<'a> {
    fn new(body: &'a [u8], limits: WideCellLimitsV2, root: usize) -> Result<Self, String> {
        let frame = count(body.len())?;
        ceiling(frame, limits.max_frame_bytes)?;
        let left = limits
            .max_operation_bytes
            .checked_sub(frame)
            .and_then(|n| n.checked_sub(limits.external_retained_bytes))
            .and_then(|n| n.checked_sub(PARSER_SCRATCH_BYTES))
            .ok_or("Hello2 decode operation reservation is too small")?;
        let storage_left =
            left.min(limits.max_decoded_bytes).checked_sub(count(root)?).ok_or("Hello2 decoded root exceeds its reservation")?;
        Ok(Self { bytes: body, storage_left, limits, lanes: 0, param_bytes: 0 })
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if n > self.bytes.len() {
            return Err("Hello2 frame ends early".into());
        }
        let (bytes, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(bytes)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("two bytes")))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }
    fn i128(&mut self) -> Result<i128, String> {
        Ok(i128::from_le_bytes(self.take(16)?.try_into().expect("sixteen bytes")))
    }
    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }
    fn opt16(&mut self) -> Result<Option<u16>, String> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u16()?)),
            _ => Err("Hello2 invalid option tag".into()),
        }
    }
    fn length(&self, n: u64, unit: usize, cap: u64) -> Result<usize, String> {
        ceiling(n, cap)?;
        let n = usize::try_from(n).map_err(|_| "Hello2 vector count exceeds native indices")?;
        if unit == 0 || n > self.bytes.len() / unit {
            return Err("Hello2 vector length exceeds the remaining frame".into());
        }
        Ok(n)
    }
    fn len64(&mut self, unit: usize, cap: u64) -> Result<usize, String> {
        let n = self.u64()?;
        self.length(n, unit, cap)
    }
    fn len32(&mut self, unit: usize, cap: u64) -> Result<usize, String> {
        let n = u64::from(self.u32()?);
        self.length(n, unit, cap)
    }
    fn reserve<T>(&mut self, n: usize) -> Result<Vec<T>, String> {
        self.storage_left = self.storage_left.checked_sub(storage::<T>(n)?).ok_or("Hello2 decoded storage exceeds its reservation")?;
        exact_vec(n)
    }
    fn copy_bytes(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let mut bytes = self.reserve::<u8>(n)?;
        bytes.extend_from_slice(self.take(n)?);
        Ok(bytes)
    }
    fn bytes(&mut self, cap: u64) -> Result<Vec<u8>, String> {
        let n = self.len64(1, cap)?;
        self.copy_bytes(n)
    }
    fn string(&mut self) -> Result<String, String> {
        let n = self.len64(1, self.limits.max_name_bytes)?;
        std::str::from_utf8(&self.bytes[..n]).map_err(|_| "Hello2 string is not utf-8")?;
        String::from_utf8(self.copy_bytes(n)?).map_err(|_| "Hello2 string is not utf-8".into())
    }
    fn charge_lanes(&mut self, n: usize) -> Result<(), String> {
        self.lanes = add(self.lanes, count(n)?)?;
        ceiling(self.lanes, self.limits.max_lanes)
    }
    fn lanes(&mut self, ty: Option<DType>, expected: Option<u64>) -> Result<Vec<i128>, String> {
        let n = self.len64(16, self.limits.max_lanes)?;
        let actual = count(n)?;
        if expected.is_some_and(|expected| expected != actual) {
            return Err("Hello2 value shape does not match its lane count".into());
        }
        self.charge_lanes(n)?;
        let mut lanes = self.reserve::<i128>(n)?;
        for _ in 0..n {
            let value = self.i128()?;
            if ty.is_some_and(|ty| !ty.contains(value)) {
                return Err("Hello2 value lane is outside its dtype".into());
            }
            lanes.push(value);
        }
        Ok(lanes)
    }
    fn carry(&mut self) -> Result<Vec<Vec<i128>>, String> {
        let n = self.len32(8, self.limits.max_carry_vectors)?;
        let mut carry = self.reserve::<Vec<i128>>(n)?;
        for _ in 0..n {
            carry.push(self.lanes(None, None)?);
        }
        Ok(carry)
    }
    fn value(&mut self) -> Result<CellValueV1, String> {
        let slot = self.u32()?;
        let block = self.u8()?;
        let layer = self.opt16()?;
        let node = self.u16()?;
        let tag = self.u8()?;
        let ty = dtype(tag)?;
        let rank = self.len32(4, self.limits.max_rank.min(count(MAX_RANK)?))?;
        let mut shape = self.reserve::<u32>(rank)?;
        for _ in 0..rank {
            shape.push(self.u32()?);
        }
        let lanes = self.lanes(Some(ty), Some(product(&shape)?))?;
        Ok(CellValueV1 { slot, block, layer, node, dtype: tag, shape, lanes })
    }
    fn done(&self) -> Result<(), String> {
        if self.bytes.is_empty() { Ok(()) } else { Err("Hello2 trailing bytes".into()) }
    }
}

impl CellRequestWideV2 {
    pub fn encode_bounded(&self, limits: WideCellLimitsV2) -> Result<Vec<u8>, String> {
        encode_request(self, limits)
    }
    pub fn decode_bounded(body: &[u8], limits: WideCellLimitsV2) -> Result<Self, String> {
        let mut r = Reader::new(body, limits, size_of::<Self>())?;
        let message = match r.u8()? {
            0 => Self::Hello { version: r.u32()? },
            1 => {
                let program = r.bytes(limits.max_program_bytes)?;
                let occ = (r.u32()?, r.u32()?);
                let n = r.len32(11, limits.max_params)?;
                let mut params = r.reserve::<CellParamBlobV1>(n)?;
                for _ in 0..n {
                    let param = r.u16()?;
                    let layer = r.opt16()?;
                    let length = r.len64(1, limits.max_param_bytes)?;
                    r.param_bytes = add(r.param_bytes, count(length)?)?;
                    ceiling(r.param_bytes, limits.max_param_bytes)?;
                    params.push(CellParamBlobV1 { param, layer, bytes: r.copy_bytes(length)? });
                }
                Self::Open { program, occ, params }
            }
            2 => Self::Step { token: r.u32()?, occ: (r.u32()?, r.u32()?), carry_in: r.carry()? },
            3 => Self::Fixed { state: r.u16()?, layer: r.opt16()?, first: r.u32()?, n: r.u32()? },
            4 => Self::Hist { state: r.u16()?, layer: r.opt16()?, h_tile: r.u32()?, first_lane: r.u32()?, row_lanes: r.u32()? },
            5 => Self::Close,
            _ => return Err("Hello2 unknown request tag".into()),
        };
        r.done()?;
        Ok(message)
    }
}
impl CellResponseWideV2 {
    pub fn encode_bounded(&self, limits: WideCellLimitsV2) -> Result<Vec<u8>, String> {
        encode_response(self, limits)
    }
    pub fn decode_bounded(body: &[u8], limits: WideCellLimitsV2) -> Result<Self, String> {
        let mut r = Reader::new(body, limits, size_of::<Self>())?;
        let message = match r.u8()? {
            0 => {
                let version = r.u32()?;
                let name = r.string()?;
                let capacity = match r.u8()? {
                    0 => None,
                    1 => Some(r.u64()?),
                    _ => return Err("Hello2 invalid capacity tag".into()),
                };
                Self::Ready { version, name, capacity }
            }
            1 => Self::Opened,
            2 => {
                let n = r.len32(21, limits.max_values)?;
                let mut values = r.reserve::<CellValueV1>(n)?;
                for _ in 0..n {
                    values.push(r.value()?);
                }
                let carry = r.carry()?;
                let n = r.len64(8, limits.max_lanes)?;
                r.charge_lanes(n)?;
                let mut logits = r.reserve::<i64>(n)?;
                for _ in 0..n {
                    logits.push(r.i64()?);
                }
                Self::Stepped { values, carry, logits }
            }
            3 => {
                let n = r.len64(1, u64::MAX)?;
                if n % 4 != 0 {
                    return Err("Hello2 state payload is not four-byte lanes".into());
                }
                r.charge_lanes(n / 4)?;
                Self::Lanes(r.copy_bytes(n)?)
            }
            4 => Self::Closed,
            5 => Self::Refused(r.string()?),
            _ => return Err("Hello2 unknown response tag".into()),
        };
        r.done()?;
        Ok(message)
    }
}
