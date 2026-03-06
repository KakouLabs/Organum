#[cfg(feature = "gpu-warp")]
use std::sync::{Mutex, OnceLock};

pub fn synthesize(
    f0: &Vec<f64>,
    sp: &mut Vec<Vec<f64>>,
    ap: &mut Vec<Vec<f64>>,
    sample_rate: u32,
    frame_period: f64,
) -> Vec<f64> {
    rsworld::synthesis(f0, sp, ap, frame_period, sample_rate as i32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarpBackend {
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug)]
pub struct WarpDispatchConfig {
    pub gpu_warp_enabled: bool,
    pub gpu_warp_min_frames: usize,
}

impl WarpDispatchConfig {
    #[inline]
    pub fn choose_backend(&self, render_length: usize) -> WarpBackend {
        if self.gpu_warp_enabled && render_length >= self.gpu_warp_min_frames {
            WarpBackend::Gpu
        } else {
            WarpBackend::Cpu
        }
    }
}

pub struct WarpLut {
    pub idx_floor: Vec<usize>,
    pub frac: Vec<f64>,
    pub clamped: Vec<bool>,
}

impl WarpLut {
    pub fn new(len: usize, fs: f64, factor: f64) -> Self {
        let df = fs / ((len - 1) as f64 * 2.0);
        let last = (len - 1) as f64;

        let mut idx_floor = Vec::with_capacity(len);
        let mut frac = Vec::with_capacity(len);
        let mut clamped = Vec::with_capacity(len);

        for i in 0..len {
            let f_dst = i as f64 * df;
            let m_dst = 2595.0 * (1.0 + f_dst / 700.0).log10();
            let m_src = m_dst * factor;
            let f_src = 700.0 * (10.0f64.powf(m_src / 2595.0) - 1.0);
            let src_idx = f_src / df;

            if src_idx >= last {
                idx_floor.push(len - 1);
                frac.push(0.0);
                clamped.push(true);
            } else {
                let fl = src_idx as usize;
                idx_floor.push(fl);
                frac.push(src_idx - fl as f64);
                clamped.push(false);
            }
        }

        Self {
            idx_floor,
            frac,
            clamped,
        }
    }

    #[inline]
    pub fn apply(&self, in_out: &mut Vec<f64>) {
        let original = in_out.clone();
        let last_val = *original.last().unwrap_or(&0.0);

        for (i, v) in in_out.iter_mut().enumerate() {
            if self.clamped[i] {
                *v = last_val;
            } else {
                let fl = self.idx_floor[i];
                let t = self.frac[i];
                *v = original[fl] * (1.0 - t) + original[fl + 1] * t;
            }
        }
    }
}

pub fn warp_spectrum(sp: &mut Vec<f64>, fs: f64, factor: f64) {
    if (factor - 1.0).abs() < 0.001 {
        return;
    }
    let lut = WarpLut::new(sp.len(), fs, factor);
    lut.apply(sp);
}

#[inline]
pub fn apply_warp_with_backend(sp: &mut Vec<f64>, lut: &WarpLut, backend: WarpBackend) {
    match backend {
        WarpBackend::Cpu => lut.apply(sp),
        WarpBackend::Gpu => lut.apply(sp),
    }
}

#[inline]
pub fn try_apply_warp_batch_with_backend(
    frames: &mut [Vec<f64>],
    lut: &WarpLut,
    backend: WarpBackend,
) -> Result<(), String> {
    match backend {
        WarpBackend::Cpu => {
            for frame in frames.iter_mut() {
                lut.apply(frame);
            }
            Ok(())
        }
        WarpBackend::Gpu => try_apply_warp_gpu_batch(frames, lut),
    }
}

pub fn try_apply_warp_gpu_batch(frames: &mut [Vec<f64>], lut: &WarpLut) -> Result<(), String> {
    #[cfg(feature = "gpu-warp")]
    {
        return pollster::block_on(run_wgpu_warp_batch(frames, lut));
    }
    #[cfg(not(feature = "gpu-warp"))]
    {
        let _ = frames;
        let _ = lut;
        Err("gpu-warp feature is disabled at build time".to_string())
    }
}

#[cfg(feature = "gpu-warp")]
fn allocate_gpu_buffers(
    device: &wgpu::Device,
    ctx: &GpuWarpContext,
    bins: usize,
    frame_count: usize,
) -> GpuWarpBufferCache {
    let alloc_frames = frame_count.next_power_of_two().max(2048);
    let alloc_total = alloc_frames * bins;

    let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_input"),
        size: (alloc_total * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_output"),
        size: (alloc_total * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let idx_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_idx"),
        size: (bins * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let frac_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_frac"),
        size: (bins * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let clamped_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_clamped"),
        size: (bins * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_params"),
        size: std::mem::size_of::<WarpParams>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("warp_bg"),
        layout: &ctx.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: idx_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: frac_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: clamped_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: params_buffer.as_entire_binding(),
            },
        ],
    });

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("warp_readback"),
        size: (alloc_total * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    GpuWarpBufferCache {
        bins,
        frames: alloc_frames,
        input_buffer,
        output_buffer,
        idx_buffer,
        frac_buffer,
        clamped_buffer,
        params_buffer,
        readback,
        bind_group,
        host_input_data: Vec::with_capacity(alloc_total),
        host_idx_floor: vec![0; bins],
        host_frac: vec![0.0; bins],
        host_clamped: vec![0; bins],
        lut_uploaded: false,
    }
}

#[cfg(feature = "gpu-warp")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WarpParams {
    bins: u32,
    frames: u32,
    _pad0: u32,
    _pad1: u32,
}

#[cfg(feature = "gpu-warp")]
struct GpuWarpContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

#[cfg(feature = "gpu-warp")]
static GPU_WARP_CONTEXT: OnceLock<Result<GpuWarpContext, String>> = OnceLock::new();

#[cfg(feature = "gpu-warp")]
const WARP_SHADER: &str = r#"
struct WarpParams {
    bins: u32,
    frames: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<storage, read> input_data: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_data: array<f32>;
@group(0) @binding(2) var<storage, read> idx_floor: array<u32>;
@group(0) @binding(3) var<storage, read> frac: array<f32>;
@group(0) @binding(4) var<storage, read> clamped: array<u32>;
@group(0) @binding(5) var<uniform> params: WarpParams;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let bin = gid.x;
    let frame = gid.y;
    if (bin >= params.bins || frame >= params.frames) {
        return;
    }

    let base = frame * params.bins;
    let i = base + bin;

    if (clamped[bin] != 0u) {
        output_data[i] = input_data[base + params.bins - 1u];
    } else {
        let fl = idx_floor[bin];
        let t = frac[bin];
        output_data[i] = input_data[base + fl] * (1.0 - t) + input_data[base + fl + 1u] * t;
    }
}
"#;

#[cfg(feature = "gpu-warp")]
fn get_or_init_gpu_context() -> Result<&'static GpuWarpContext, String> {
    let result = GPU_WARP_CONTEXT.get_or_init(|| pollster::block_on(init_gpu_context()));
    match result {
        Ok(ctx) => Ok(ctx),
        Err(e) => Err(e.clone()),
    }
}

#[cfg(feature = "gpu-warp")]
async fn init_gpu_context() -> Result<GpuWarpContext, String> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .ok_or_else(|| "no compatible GPU adapter found".to_string())?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .map_err(|e| format!("failed to request device: {e}"))?;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("warp_shader"),
        source: wgpu::ShaderSource::Wgsl(WARP_SHADER.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("warp_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("warp_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("warp_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: "main",
        compilation_options: wgpu::PipelineCompilationOptions::default(),
    });

    Ok(GpuWarpContext {
        device,
        queue,
        bind_group_layout,
        pipeline,
    })
}

#[cfg(feature = "gpu-warp")]
struct GpuWarpBufferCache {
    bins: usize,
    frames: usize,
    input_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    idx_buffer: wgpu::Buffer,
    frac_buffer: wgpu::Buffer,
    clamped_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    readback: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    host_input_data: Vec<f32>,
    host_idx_floor: Vec<u32>,
    host_frac: Vec<f32>,
    host_clamped: Vec<u32>,
    lut_uploaded: bool,
}

#[cfg(feature = "gpu-warp")]
static GPU_WARP_CACHE: Mutex<Option<GpuWarpBufferCache>> = Mutex::new(None);

#[cfg(feature = "gpu-warp")]
fn return_cache(bufs: GpuWarpBufferCache) {
    match GPU_WARP_CACHE.lock() {
        Ok(mut cache_guard) => {
            *cache_guard = Some(bufs);
        }
        Err(e) => {
            tracing::warn!("Failed to return GPU warp cache due to poisoned mutex: {}", e);
        }
    }
}

#[cfg(feature = "gpu-warp")]
async fn run_wgpu_warp_batch(frames: &mut [Vec<f64>], lut: &WarpLut) -> Result<(), String> {
    if frames.is_empty() {
        return Ok(());
    }

    let bins = lut.idx_floor.len();
    if bins == 0 {
        return Ok(());
    }

    if frames.iter().any(|f| f.len() != bins) {
        return Err("inconsistent spectrum frame length for gpu warp".to_string());
    }

    let frame_count = frames.len();
    let total = frame_count
        .checked_mul(bins)
        .ok_or_else(|| "gpu warp size overflow".to_string())?;

    let ctx = get_or_init_gpu_context()?;
    let device = &ctx.device;
    let queue = &ctx.queue;

    let mut bufs = {
        let mut cache_guard = GPU_WARP_CACHE.lock().map_err(|e| format!("cache mutex poisoned: {}", e))?;
        if let Some(b) = cache_guard.take() {
            if b.bins == bins && b.frames >= frame_count {
                b
            } else {
                allocate_gpu_buffers(device, ctx, bins, frame_count)
            }
        } else {
            allocate_gpu_buffers(device, ctx, bins, frame_count)
        }
    };

    bufs.host_input_data.clear();
    for frame in frames.iter() {
        bufs.host_input_data.extend(frame.iter().map(|&v| v as f32));
    }

    let mut lut_changed = !bufs.lut_uploaded;
    for i in 0..bins {
        let idx_u32 = lut.idx_floor[i] as u32;
        let frac_f32 = lut.frac[i] as f32;
        let clamped_u32 = u32::from(lut.clamped[i]);
        
        if bufs.host_idx_floor[i] != idx_u32 || bufs.host_frac[i] != frac_f32 || bufs.host_clamped[i] != clamped_u32 {
            bufs.host_idx_floor[i] = idx_u32;
            bufs.host_frac[i] = frac_f32;
            bufs.host_clamped[i] = clamped_u32;
            lut_changed = true;
        }
    }

    let input_bytes = bytemuck::cast_slice(&bufs.host_input_data);
    queue.write_buffer(&bufs.input_buffer, 0, input_bytes);

    if lut_changed {
        queue.write_buffer(&bufs.idx_buffer, 0, bytemuck::cast_slice(&bufs.host_idx_floor));
        queue.write_buffer(&bufs.frac_buffer, 0, bytemuck::cast_slice(&bufs.host_frac));
        queue.write_buffer(&bufs.clamped_buffer, 0, bytemuck::cast_slice(&bufs.host_clamped));
        bufs.lut_uploaded = true;
    }

    let params = WarpParams {
        bins: bins as u32,
        frames: frame_count as u32,
        _pad0: 0,
        _pad1: 0,
    };
    queue.write_buffer(&bufs.params_buffer, 0, bytemuck::bytes_of(&params));

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("warp_encoder"),
    });

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("warp_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&ctx.pipeline);
        pass.set_bind_group(0, &bufs.bind_group, &[]);
        let workgroup_size = 256u32;
        let groups_x = (bins as u32).div_ceil(workgroup_size);
        let groups_y = frame_count as u32;
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }

    encoder.copy_buffer_to_buffer(
        &bufs.output_buffer,
        0,
        &bufs.readback,
        0,
        input_bytes.len() as u64,
    );
    queue.submit(Some(encoder.finish()));

    let slice = bufs.readback.slice(0..input_bytes.len() as u64);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res: Result<(), wgpu::BufferAsyncError>| {
        let _ = tx.send(res);
    });
    device.poll(wgpu::Maintain::Wait);

    let map_result = match rx.recv() {
        Ok(res) => res,
        Err(e) => {
            return_cache(bufs);
            return Err(format!("gpu map channel failed: {e}"));
        }
    };
    
    if let Err(e) = map_result {
        return_cache(bufs);
        return Err(format!("failed to map readback buffer: {e:?}"));
    }

    {
        let mapped = slice.get_mapped_range();
        let output: &[f32] = bytemuck::cast_slice(&mapped);
        if output.len() != total {
            drop(mapped); // End the borrow of `slice` / `bufs`
            bufs.readback.unmap();
            return_cache(bufs);
            return Err("gpu output size mismatch".to_string());
        }

        // Avoid repeated index checks by unwrapping chunks and fast-copying
        frames
            .iter_mut()
            .zip(output.chunks_exact(bins))
            .for_each(|(frame, chunk)| {
                // Avoid bounds checks: length guaranteed to match exactly `bins` length.
                for i in 0..bins {
                    frame[i] = chunk[i] as f64;
                }
            });
    }

    bufs.readback.unmap();
    return_cache(bufs);
    Ok(())
}
