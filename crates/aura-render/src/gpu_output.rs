//! Bounded, reusable GPU buffers for the terminal output stage. ADR-0122.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock, PoisonError,
};
use std::time::Duration;

use wgpu::util::DeviceExt;

/// At most 72 MiB of GPU scratch, independent of the photograph's resolution.
const CHUNK_PIXELS: usize = 2 * 1024 * 1024;
const BYTES_PER_PIXEL: usize = 12;
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);
static DEVICE: OnceLock<Result<Accelerator, String>> = OnceLock::new();

#[derive(Debug)]
struct Scratch {
    input: wgpu::Buffer,
    output: wgpu::Buffer,
    readback: wgpu::Buffer,
    frame: wgpu::Buffer,
    params: wgpu::Buffer,
    bindings: wgpu::BindGroup,
}

/// The adapter identity is part of the rendering/cache identity, never a recipe change.
#[derive(Debug)]
pub(crate) struct Accelerator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    scratch: Mutex<Scratch>,
    disabled: Arc<AtomicBool>,
    pub(crate) identity: String,
    pub(crate) min_pixels: usize,
}

fn initialise() -> Result<Accelerator, String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN | wgpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .map_err(|error| error.to_string())?;
    let info = adapter.get_info();
    if info.device_type == wgpu::DeviceType::Cpu {
        return Err("The available adapter is a software renderer".into());
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("AURA photographic output"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .map_err(|error| error.to_string())?;
    let disabled = Arc::new(AtomicBool::new(false));
    let failed_device = Arc::clone(&disabled);
    device.on_uncaptured_error(Arc::new(move |error| {
        failed_device.store(true, Ordering::Release);
        tracing::warn!(target: "render.gpu", code = "AURA-RENDER-8001", %error, "GPU error; CPU fallback remains available");
    }));
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("AURA output transform"),
        source: wgpu::ShaderSource::Wgsl(crate::shaders::OUTPUT.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("AURA output transform"),
        layout: None,
        module: &shader,
        entry_point: Some("stage_output_transform"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    if let Some(error) = pollster::block_on(device.pop_error_scope()) {
        return Err(format!("GPU output shader: {error}"));
    }
    device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let scratch = scratch_buffers(&device, &pipeline);
    let validation = pollster::block_on(device.pop_error_scope());
    let memory = pollster::block_on(device.pop_error_scope());
    if let Some(error) = validation.or(memory) {
        return Err(format!("GPU scratch allocation: {error}"));
    }
    let identity = format!(
        "wgpu.output.1.{:?}.{:04x}.{:04x}.{}.{}",
        info.backend, info.vendor, info.device, info.driver_info, info.name
    );
    tracing::info!(target: "render.gpu", adapter = %info.name, backend = ?info.backend, "GPU output device ready");
    let mut accelerator = Accelerator {
        device,
        queue,
        pipeline,
        scratch: Mutex::new(scratch),
        disabled,
        identity,
        min_pixels: CHUNK_PIXELS,
    };
    qualify(&mut accelerator)?;
    Ok(accelerator)
}

fn scratch_buffers(device: &wgpu::Device, pipeline: &wgpu::ComputePipeline) -> Scratch {
    let size = (CHUNK_PIXELS * BYTES_PER_PIXEL) as u64;
    let storage = |label, usage| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: false,
        })
    };
    let input = storage(
        "AURA linear pixels",
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    );
    let output = storage(
        "AURA output codes",
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let readback = storage(
        "AURA output readback",
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let frame = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("AURA frame"),
        contents: &[0; 16],
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("AURA output parameters"),
        contents: &[0; 64],
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("AURA output bindings"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: frame.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output.as_entire_binding(),
            },
        ],
    });
    Scratch {
        input,
        output,
        readback,
        frame,
        params,
        bindings,
    }
}

/// Qualify colour first, then select a size where transfers and readback pay for themselves.
fn qualify(accelerator: &mut Accelerator) -> Result<(), String> {
    use crate::{OutputColour, RenderedData};
    use aura_core::clock::{Clock, SystemClock};
    let small: Vec<f32> = (0..257 * 33 * 3)
        .map(|i| ((i * 7919) % 65536) as f32 / 16384.0 - 0.05)
        .collect();
    for space in [
        OutputColour::Srgb,
        OutputColour::AdobeRgb,
        OutputColour::DisplayP3,
    ] {
        for depth in [8, 16] {
            let reference = crate::output::transform(&small, 257, 33, space, depth, (11, 23));
            let actual = accelerator.transform(
                &small,
                257,
                33,
                crate::output::gpu_params(space, depth, (11, 23)),
            )?;
            let expected: Vec<u32> = match reference {
                RenderedData::Eight(values) => values.into_iter().map(u32::from).collect(),
                RenderedData::Sixteen(values) => values.into_iter().map(u32::from).collect(),
            };
            if actual.len() != expected.len()
                || actual
                    .iter()
                    .zip(expected)
                    .any(|(a, b)| a.abs_diff(b) > if depth == 8 { 1 } else { 4 })
            {
                return Err("GPU output did not match the CPU colour reference".into());
            }
        }
    }
    let clock = SystemClock::default();
    for edge in [512_u32, 1024, 2048] {
        let rgb: Vec<f32> = (0..edge as usize * edge as usize * 3)
            .map(|i| (i % 4096) as f32 / 1024.0)
            .collect();
        let params = crate::output::gpu_params(OutputColour::Srgb, 8, (0, 0));
        accelerator.transform(&rgb, edge, edge, params)?;
        let mut cpu_us = 0;
        let mut gpu_us = 0;
        for _ in 0..2 {
            let started = clock.monotonic_us();
            std::hint::black_box(crate::output::transform(
                &rgb,
                edge,
                edge,
                OutputColour::Srgb,
                8,
                (0, 0),
            ));
            cpu_us += clock.monotonic_us().saturating_sub(started);
            let started = clock.monotonic_us();
            let codes = accelerator.transform(&rgb, edge, edge, params)?;
            // Include conversion to the actual CPU-side output representation in the probe.
            std::hint::black_box(codes.into_iter().map(|code| code as u8).collect::<Vec<_>>());
            gpu_us += clock.monotonic_us().saturating_sub(started);
        }
        tracing::info!(target: "render.gpu", edge, cpu_us, gpu_us, "GPU output crossover probe (transfers included)");
        // Require 15% headroom to avoid enabling a path on timer noise alone.
        if gpu_us.saturating_mul(100) < cpu_us.saturating_mul(85) {
            accelerator.min_pixels = edge as usize * edge as usize;
            accelerator.identity =
                format!("{}.min{}", accelerator.identity, accelerator.min_pixels);
            return Ok(());
        }
    }
    Err("GPU output was not faster than the CPU including transfers; keeping the CPU path".into())
}

/// Detection happens on the image worker, once. The explicit kill switch avoids probing.
pub(crate) fn available() -> Option<&'static Accelerator> {
    if std::env::var_os("AURA_DISABLE_GPU").is_some_and(|value| value == "1") {
        return None;
    }
    match DEVICE.get_or_init(|| {
        let result = initialise();
        if let Err(reason) = &result {
            tracing::warn!(target: "render.gpu", code = "AURA-RENDER-8001", %reason, "GPU output unavailable; using the CPU");
        }
        result
    }) {
        Ok(accelerator) if !accelerator.disabled.load(Ordering::Acquire) => Some(accelerator),
        _ => None,
    }
}

pub(crate) fn unavailable_reason() -> Option<String> {
    if std::env::var_os("AURA_DISABLE_GPU").is_some_and(|value| value == "1") {
        return Some("GPU output explicitly disabled by AURA_DISABLE_GPU".into());
    }
    let _ = available();
    match DEVICE.get() {
        Some(Err(reason)) => Some(reason.clone()),
        Some(Ok(device)) if device.disabled.load(Ordering::Acquire) => {
            Some("GPU failed and was disabled for this session".into())
        }
        _ => None,
    }
}

impl Accelerator {
    /// Returns complete integer codes or a refusal; never changes the input pixels.
    pub(crate) fn transform(
        &self,
        rgb: &[f32],
        width: u32,
        height: u32,
        params: [u32; 16],
    ) -> Result<Vec<u32>, String> {
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .ok_or("Output dimensions overflow")?;
        if pixels == 0
            || width as usize > CHUNK_PIXELS
            || rgb.len() != pixels.checked_mul(3).ok_or("Output size overflow")?
            || rgb.iter().any(|value| !value.is_finite())
        {
            return Err(
                "GPU output requires finite, complete RGB pixels and a supported row width".into(),
            );
        }
        let result = self.process(rgb, width, height, params);
        if let Err(reason) = &result {
            self.disabled.store(true, Ordering::Release);
            tracing::warn!(target: "render.gpu", code = "AURA-RENDER-8001", %reason, "Disabling GPU output for this session; restarting from untouched CPU input");
        }
        result
    }

    fn process(
        &self,
        rgb: &[f32],
        width: u32,
        height: u32,
        mut params: [u32; 16],
    ) -> Result<Vec<u32>, String> {
        let scratch = self.scratch.lock().unwrap_or_else(PoisonError::into_inner);
        let rows = (CHUNK_PIXELS / width as usize).max(1);
        let samples_per_row = width as usize * 3;
        let mut codes = Vec::with_capacity(rgb.len());
        let origin_y = params.get(15).copied().ok_or("Missing output origin")?;
        for y in (0..height as usize).step_by(rows) {
            let batch_rows = rows.min(height as usize - y);
            let first = y * samples_per_row;
            let samples = batch_rows * samples_per_row;
            let input = rgb
                .get(first..first + samples)
                .ok_or("Incomplete GPU stripe")?;
            let bytes = (samples * 4) as u64;
            if let Some(slot) = params.get_mut(15) {
                *slot = origin_y.saturating_add(y as u32);
            }
            self.queue
                .write_buffer(&scratch.input, 0, bytemuck::cast_slice(input));
            self.queue.write_buffer(
                &scratch.frame,
                0,
                bytemuck::cast_slice(&[width, batch_rows as u32, 0, 0]),
            );
            self.queue
                .write_buffer(&scratch.params, 0, bytemuck::cast_slice(&params));
            self.device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut commands =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("AURA GPU output"),
                    });
            {
                let mut pass = commands.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("AURA output"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &scratch.bindings, &[]);
                pass.dispatch_workgroups((width * batch_rows as u32).div_ceil(64), 1, 1);
            }
            commands.copy_buffer_to_buffer(&scratch.output, 0, &scratch.readback, 0, bytes);
            let submission = self.queue.submit([commands.finish()]);
            let slice = scratch.readback.slice(..bytes);
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
            self.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(WAIT_TIMEOUT),
                })
                .map_err(|error| format!("GPU completion: {error}"))?;
            receive
                .recv_timeout(WAIT_TIMEOUT)
                .map_err(|error| format!("GPU readback: {error}"))?
                .map_err(|error| format!("GPU mapping: {error}"))?;
            {
                let mapped = slice.get_mapped_range();
                codes.extend_from_slice(
                    bytemuck::try_cast_slice(&mapped).map_err(|error| error.to_string())?,
                );
            }
            scratch.readback.unmap();
            if let Some(error) = pollster::block_on(self.device.pop_error_scope()) {
                return Err(error.to_string());
            }
        }
        Ok(codes)
    }
}
