//! **The device: one wgpu adapter that can hold PALW-TIR values exactly, or none.**
//!
//! A device is admitted only if it offers `SHADER_INT64` — native `i64`/`u64` in WGSL (Metal
//! MSL ≥ 2.3 on an Apple3+/Metal3 GPU, `shaderInt64` on Vulkan). Without it an exact accumulator
//! would need a 64-bit emulation in every kernel; with it the hot loops stay 32-bit and only the
//! chunk combines, the narrowing chains and the transcendentals touch 64-bit words. A host whose
//! adapter lacks the feature has no TIR device and runs the CPU executor, which is always correct:
//! the backend is an accelerator, never a requirement.
//!
//! Pipelines are compiled once per distinct WGSL source and cached for the device's life (a kernel
//! is specialised per operand forms and per primitive attributes, see [`crate::wgsl`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Why no TIR device is available (the caller runs the CPU executor instead).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceError {
    /// No adapter at all (no Metal/Vulkan device, or a headless host).
    NoAdapter(String),
    /// The adapter lacks `SHADER_INT64`: it cannot hold an exact 64-bit accumulator natively.
    NoInt64(String),
    /// The adapter refused the device request.
    Request(String),
}

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceError::NoAdapter(m) => write!(f, "no GPU adapter: {m}"),
            DeviceError::NoInt64(m) => write!(f, "the adapter has no SHADER_INT64: {m}"),
            DeviceError::Request(m) => write!(f, "the device request failed: {m}"),
        }
    }
}

impl std::error::Error for DeviceError {}

/// A TIR device: the wgpu device and queue, the adapter's description and limits, and the
/// pipelines compiled so far.
pub struct GpuDevice {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    pub limits: wgpu::Limits,
    pipelines: Mutex<HashMap<String, Arc<wgpu::ComputePipeline>>>,
}

impl GpuDevice {
    /// The high-performance adapter, if it is a TIR device.
    pub fn new() -> Result<Self, DeviceError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| DeviceError::NoAdapter(e.to_string()))?;
        let info = adapter.get_info();
        if !adapter.features().contains(wgpu::Features::SHADER_INT64) {
            return Err(DeviceError::NoInt64(format!("{} ({:?})", info.name, info.backend)));
        }
        // Every limit the adapter has: the kernels size their buffers and dispatches against them.
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("palw-tir-gpu"),
            required_features: wgpu::Features::SHADER_INT64,
            required_limits: limits.clone(),
            experimental_features: Default::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: Default::default(),
        }))
        .map_err(|e| DeviceError::Request(e.to_string()))?;
        Ok(GpuDevice { device, queue, info, limits, pipelines: Mutex::new(HashMap::new()) })
    }

    /// One line naming the device (benchmarks, test logs).
    pub fn describe(&self) -> String {
        format!("{} ({:?}, {:?})", self.info.name, self.info.backend, self.info.device_type)
    }

    /// The compute pipeline of a WGSL source (entry point `main`), compiled on first use.
    ///
    /// A source that fails to compile is a defect of this crate, not of the program being run, so
    /// it panics with the compiler's message: every source this crate generates is compiled by the
    /// conformance suite.
    pub fn pipeline(&self, src: &str) -> Arc<wgpu::ComputePipeline> {
        if let Some(p) = self.pipelines.lock().unwrap_or_else(|p| p.into_inner()).get(src) {
            return Arc::clone(p);
        }
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("palw-tir-gpu kernel"),
            source: wgpu::ShaderSource::Wgsl(src.into()),
        });
        let pipeline = Arc::new(self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("palw-tir-gpu kernel"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        }));
        self.pipelines.lock().unwrap_or_else(|p| p.into_inner()).insert(src.to_string(), Arc::clone(&pipeline));
        pipeline
    }

    /// Pipelines compiled so far.
    pub fn pipeline_count(&self) -> usize {
        self.pipelines.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// Block until every submitted command has finished.
    pub fn wait(&self) {
        self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).expect("the device answers a poll");
    }
}
