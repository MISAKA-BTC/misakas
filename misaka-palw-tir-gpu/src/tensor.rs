//! **Device tensors: a buffer, a storage form and a strided layout.**
//!
//! WGSL has no 8- or 16-bit integers, so a tensor's elements sit in one of five *forms*:
//!
//! | form | elements | holds |
//! | --- | --- | --- |
//! | [`Form::S32`] | one signed 32-bit word each | computed `i8`/`i16`/`i32` values, sign-extended — the lane form a commitment uses (spec 04b §10.1) |
//! | [`Form::U32`] | one unsigned 32-bit word each | `idx` |
//! | [`Form::I64`] | one `i64` each | `i64` values, and `i128` nodes the plan stores in `i64` |
//! | [`Form::P8`] | four per word, little-endian | `i8` params as the artifact stores them |
//! | [`Form::P16`] | two per word | `i16` params |
//! | [`Form::I128`] | two `u64` words each, low first | computed `i128` values (the wide norm sums, the fixed-point products before their rounding) and `i128` consts |
//!
//! A form is a storage decision, never a value decision: an element reads back as the same
//! mathematical integer in every form, and a commitment is made of lanes whatever form a value had.
//! The layout is the CPU executor's ([`misaka_palw_tir_exec::layout::Layout`]): views (`Reshape` of a
//! contiguous value, `Transpose`, `Slice`, `Broadcast`, a scalar `Gather`, a history window) are
//! layouts over their input's buffer here too, and every kernel reads its operands through strides.

use std::sync::Arc;

use misaka_palw_tir::DType;
use misaka_palw_tir_exec::elem::{Buf, Slice};
use misaka_palw_tir_exec::layout::Layout;
use misaka_palw_tir_exec::{with_dtype, with_slice};
use wgpu::util::DeviceExt;

use crate::device::GpuDevice;

/// How a tensor's elements are stored in a device buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
    S32,
    U32,
    I64,
    P8,
    P16,
    I128,
}

impl Form {
    /// The form a COMPUTED value (or a const) of `dtype` takes.
    pub fn computed(dtype: DType) -> Option<Form> {
        match dtype {
            DType::I8 | DType::I16 | DType::I32 => Some(Form::S32),
            DType::Idx => Some(Form::U32),
            DType::I64 => Some(Form::I64),
            DType::I128 => Some(Form::I128),
        }
    }

    /// The form a PARAM of `dtype` is uploaded in (packed for the narrow weights).
    pub fn param(dtype: DType) -> Option<Form> {
        match dtype {
            DType::I8 => Some(Form::P8),
            DType::I16 => Some(Form::P16),
            // A param is never i128 (NF-7).
            DType::I128 => None,
            other => Form::computed(other),
        }
    }

    /// Bytes of storage for `n` elements, rounded up to whole 16-byte units (a binding is never
    /// empty and every copy is 4-byte aligned).
    pub fn bytes(self, n: usize) -> u64 {
        let raw = match self {
            Form::S32 | Form::U32 => 4 * n,
            Form::I64 => 8 * n,
            Form::P8 => n,
            Form::P16 => 2 * n,
            Form::I128 => 16 * n,
        };
        (raw.max(1).div_ceil(16) * 16) as u64
    }

    /// The WGSL element type of the buffer array.
    pub fn wgsl_array(self) -> &'static str {
        match self {
            Form::I64 => "i64",
            Form::I128 => "u64",
            _ => "u32",
        }
    }
}

/// A device tensor: storage, its form, the dtype its values belong to, and the layout that names
/// its elements (strides and offset in ELEMENTS, whatever the form).
#[derive(Clone, Debug)]
pub struct DevTensor {
    pub buf: Arc<wgpu::Buffer>,
    pub form: Form,
    pub dtype: DType,
    pub layout: Layout,
}

impl DevTensor {
    pub fn shape(&self) -> &[usize] {
        self.layout.shape()
    }
    pub fn numel(&self) -> usize {
        self.layout.numel()
    }
    /// The same storage under another layout (a view).
    pub fn view(&self, layout: Layout) -> DevTensor {
        DevTensor { buf: Arc::clone(&self.buf), form: self.form, dtype: self.dtype, layout }
    }
}

/// The usages every tensor buffer carries: a kernel operand and output, and a copy source and
/// destination (history rows, commitments, readback).
pub const TENSOR_USAGE: wgpu::BufferUsages =
    wgpu::BufferUsages::STORAGE.union(wgpu::BufferUsages::COPY_SRC).union(wgpu::BufferUsages::COPY_DST);

impl GpuDevice {
    /// An uninitialised tensor buffer for `n` elements of `form`.
    pub fn alloc(&self, form: Form, n: usize) -> Arc<wgpu::Buffer> {
        Arc::new(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tir tensor"),
            size: form.bytes(n),
            usage: TENSOR_USAGE,
            mapped_at_creation: false,
        }))
    }

    /// A contiguous tensor of `shape` in `form`, holding the elements of `data` (which has the
    /// dtype and element count of the tensor).
    pub fn upload(&self, data: Slice<'_>, form: Form, shape: &[usize]) -> DevTensor {
        let bytes = pack(data, form);
        let mut padded = bytes;
        padded.resize(form.bytes(data.len()) as usize, 0);
        let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tir tensor"),
            contents: &padded,
            usage: TENSOR_USAGE,
        });
        DevTensor { buf: Arc::new(buf), form, dtype: data.dtype(), layout: Layout::contiguous(shape) }
    }

    /// Read a buffer back: the first `bytes` bytes (a multiple of 4).
    pub fn read_bytes(&self, buf: &wgpu::Buffer, offset: u64, bytes: u64) -> Vec<u8> {
        let size = bytes.max(4).div_ceil(4) * 4;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tir readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(buf, offset, &staging, 0, size);
        self.queue.submit([enc.finish()]);
        staging.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("a readback maps"));
        self.wait();
        let out = staging.slice(..).get_mapped_range().expect("mapped").to_vec();
        staging.unmap();
        out
    }

    /// A tensor's elements as a host buffer of its dtype, row-major over its layout. A strided
    /// layout is gathered on the host (callers that need speed materialize on the device first).
    pub fn download(&self, t: &DevTensor) -> Buf {
        let extent = t.layout.extent();
        let words = unpack(&self.read_bytes(&t.buf, 0, t.form.bytes(extent)), t.form, extent);
        let mut vals: Vec<i128> = Vec::with_capacity(t.numel());
        t.layout.for_each_run(|start, len, stride| {
            vals.extend((0..len).map(|i| words[start + i * stride]));
        });
        Buf::from_i128s(t.dtype, &vals)
    }
}

/// Little-endian bytes of `data` in `form`.
pub fn pack(data: Slice<'_>, form: Form) -> Vec<u8> {
    let mut out = Vec::new();
    match form {
        Form::S32 | Form::U32 => with_slice!(data, v => {
            out.reserve(4 * v.len());
            for x in v.iter() {
                // A lane: the value sign-extended (idx: zero-extended) to 32 bits.
                let w = misaka_palw_tir_exec::elem::Elem::to_i64(*x) as u32;
                out.extend_from_slice(&w.to_le_bytes());
            }
        }),
        Form::I64 => with_slice!(data, v => {
            out.reserve(8 * v.len());
            for x in v.iter() {
                out.extend_from_slice(&misaka_palw_tir_exec::elem::Elem::to_i64(*x).to_le_bytes());
            }
        }),
        Form::P8 => with_slice!(data, v => {
            out.extend(v.iter().map(|x| misaka_palw_tir_exec::elem::Elem::to_i64(*x) as u8));
        }),
        Form::P16 => with_slice!(data, v => {
            for x in v.iter() {
                out.extend_from_slice(&(misaka_palw_tir_exec::elem::Elem::to_i64(*x) as u16).to_le_bytes());
            }
        }),
        Form::I128 => with_slice!(data, v => {
            out.reserve(16 * v.len());
            for x in v.iter() {
                out.extend_from_slice(&misaka_palw_tir_exec::elem::Elem::to_i128(*x).to_le_bytes());
            }
        }),
    }
    out
}

/// The first `n` elements of a buffer's bytes in `form`, as mathematical integers.
pub fn unpack(bytes: &[u8], form: Form, n: usize) -> Vec<i128> {
    match form {
        Form::S32 => bytes.chunks_exact(4).take(n).map(|c| i32::from_le_bytes(c.try_into().unwrap()) as i128).collect(),
        Form::U32 => bytes.chunks_exact(4).take(n).map(|c| u32::from_le_bytes(c.try_into().unwrap()) as i128).collect(),
        Form::I64 => bytes.chunks_exact(8).take(n).map(|c| i64::from_le_bytes(c.try_into().unwrap()) as i128).collect(),
        Form::P8 => bytes.iter().take(n).map(|b| *b as i8 as i128).collect(),
        Form::P16 => bytes.chunks_exact(2).take(n).map(|c| i16::from_le_bytes(c.try_into().unwrap()) as i128).collect(),
        Form::I128 => bytes.chunks_exact(16).take(n).map(|c| i128::from_le_bytes(c.try_into().unwrap())).collect(),
    }
}

/// A host buffer of `dtype` from lanes (sign-extended 32-bit words, or idx words).
pub fn buf_from_lanes(dtype: DType, lanes: &[i128]) -> Buf {
    with_dtype!(dtype, T => {
        let v: Vec<T> = lanes.iter().map(|x| <T as misaka_palw_tir_exec::elem::Elem>::from_i128(*x)).collect();
        <T as misaka_palw_tir_exec::elem::Elem>::into_buf(v)
    })
}
