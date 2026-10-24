//! Optional GPU acceleration (wgpu compute shaders on Vulkan, DX12 or Metal).
//!
//! Only the network math moves to the GPU: during GA/ES evaluation all matches of
//! a generation run in lockstep on the CPU, and at every decision step one dispatch
//! computes the next action of every network-driven tank. Callers fall back to the
//! CPU path when no hardware adapter is found or the user turns the GPU off.

use crate::nn::Arch;
use crate::nn::fixed::{Layout, N_IN, N_OUT};
use std::sync::OnceLock;
use wgpu::util::DeviceExt;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_layout: wgpu::BindGroupLayout,
    mlp: wgpu::ComputePipeline,
    elman: wgpu::ComputePipeline,
    gru: wgpu::ComputePipeline,
    /// Adapter name and backend, for display.
    pub name: String,
}

static GPU: OnceLock<Option<Gpu>> = OnceLock::new();

/// The shared GPU, initialised on first use; `None` when no hardware adapter exists.
pub fn get() -> Option<&'static Gpu> {
    GPU.get_or_init(|| Gpu::new().ok()).as_ref()
}

/// Whether the GPU kernels cover this architecture.
pub fn supports(arch: Arch) -> bool {
    !matches!(arch, Arch::Neat | Arch::GruClock)
}

/// Human-readable adapter status.
pub fn status() -> String {
    match get() {
        Some(g) => g.name.clone(),
        None => "no GPU found, CPU only".into(),
    }
}

/// Mirrors `Meta` in nets.wgsl (uniform layout: 16-byte rows).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Meta {
    stride: u32,
    hidden: u32,
    n_layers: u32,
    _pad: u32,
    dims: [u32; 8],
    w_off: [u32; 4],
    b_off: [u32; 4],
    u_off: [u32; 4],
}

impl Gpu {
    fn new() -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let info = adapter.get_info();
        // A software rasterizer would only be slower than the CPU path.
        if info.device_type == wgpu::DeviceType::Cpu {
            return Err(format!("{} is a software adapter", info.name));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("claude_tanks"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nets"),
            source: wgpu::ShaderSource::Wgsl(include_str!("nets.wgsl").into()),
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nets"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, false),
                storage(3, false),
                storage(4, true),
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
            label: Some("nets"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let (mlp, elman, gru) = (pipeline("mlp"), pipeline("elman"), pipeline("gru"));
        Ok(Gpu {
            name: format!("{} ({:?})", info.name, info.backend),
            device,
            queue,
            bind_layout,
            mlp,
            elman,
            gru,
        })
    }

    /// Upload parameter sets (one per candidate or opponent), transposing every
    /// weight matrix so that neighbouring GPU threads read neighbouring addresses.
    pub fn upload(&self, arch: Arch, sets: &[&[f32]]) -> Params<'_> {
        let (layout, n) = Layout::new(arch);
        let mut meta = Meta {
            stride: n as u32,
            hidden: layout.hidden() as u32,
            n_layers: 0,
            _pad: 0,
            dims: [0; 8],
            w_off: [0; 4],
            b_off: [0; 4],
            u_off: [0; 4],
        };
        // Matrices to transpose: (offset, rows, cols) of each row-major block.
        let mut mats = Vec::new();
        let h = layout.hidden();
        let pipeline = match &layout {
            Layout::Mlp(layers) => {
                meta.n_layers = layers.len() as u32;
                meta.dims[0] = layers[0].0 as u32;
                for (l, &(i, o, w, b)) in layers.iter().enumerate() {
                    meta.dims[l + 1] = o as u32;
                    meta.w_off[l] = w as u32;
                    meta.b_off[l] = b as u32;
                    mats.push((w, o, i));
                }
                &self.mlp
            }
            Layout::Elman {
                wx, wh, b, wo, bo, ..
            } => {
                meta.w_off = [*wx as u32, *wh as u32, *wo as u32, 0];
                meta.b_off = [*b as u32, *bo as u32, 0, 0];
                mats.extend([(*wx, h, N_IN), (*wh, h, h), (*wo, N_OUT, h)]);
                &self.elman
            }
            Layout::Gru {
                wx, wh, b, wo, bo, ..
            } => {
                meta.w_off = [wx[0] as u32, wx[1] as u32, wx[2] as u32, *wo as u32];
                meta.b_off = [b[0] as u32, b[1] as u32, b[2] as u32, *bo as u32];
                meta.u_off = [wh[0] as u32, wh[1] as u32, wh[2] as u32, 0];
                for g in 0..3 {
                    mats.push((wx[g], h, N_IN));
                    mats.push((wh[g], h, h));
                }
                mats.push((*wo, N_OUT, h));
                &self.gru
            }
            Layout::GruClock { .. } => unreachable!("no GPU kernel for {arch:?}"),
        };
        let mut data = Vec::with_capacity(n * sets.len());
        for s in sets {
            let mut p = s.to_vec();
            for &(off, rows, cols) in &mats {
                for r in 0..rows {
                    for c in 0..cols {
                        p[off + c * rows + r] = s[off + r * cols + c];
                    }
                }
            }
            data.extend_from_slice(&p);
        }
        use wgpu::BufferUsages as U;
        let init = |label, contents: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        Params {
            gpu: self,
            pipeline,
            hidden: h,
            params: init("params", bytemuck::cast_slice(&data), U::STORAGE),
            meta: init("meta", bytemuck::bytes_of(&meta), U::UNIFORM),
        }
    }

    /// Upload and allocate in one go (one batch using every set).
    pub fn batch(&self, arch: Arch, sets: &[&[f32]], row_param: &[u32]) -> Batch<'_> {
        let p = self.upload(arch, sets);
        p.batch(row_param).with_params(p)
    }
}

/// Network weights of one evaluation round, shared by every batch of rows.
pub struct Params<'g> {
    gpu: &'g Gpu,
    pipeline: &'g wgpu::ComputePipeline,
    hidden: usize,
    params: wgpu::Buffer,
    meta: wgpu::Buffer,
}

impl<'g> Params<'g> {
    /// Per-row buffers for a group of rows; row `r` runs set `row_param[r]`.
    /// New buffers are zero-filled, so every recurrent state starts at 0.
    pub fn batch(&self, row_param: &[u32]) -> Batch<'g> {
        let d = &self.gpu.device;
        let rows = row_param.len();
        let empty = |label, size: usize, usage| {
            d.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (size.max(1) * 4) as u64,
                usage,
                mapped_at_creation: false,
            })
        };
        use wgpu::BufferUsages as U;
        let rowp = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("row_param"),
            contents: bytemuck::cast_slice(row_param),
            usage: U::STORAGE,
        });
        let obs = empty("obs", rows * N_IN, U::STORAGE | U::COPY_DST);
        let hidden = empty("hidden", rows * self.hidden, U::STORAGE | U::COPY_SRC);
        let out = empty("out", rows * N_OUT, U::STORAGE | U::COPY_SRC);
        let readback = empty("readback", rows * N_OUT, U::MAP_READ | U::COPY_DST);
        fn entry(binding: u32, buf: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry {
                binding,
                resource: buf.as_entire_binding(),
            }
        }
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nets"),
            layout: &self.gpu.bind_layout,
            entries: &[
                entry(0, &self.params),
                entry(1, &obs),
                entry(2, &hidden),
                entry(3, &out),
                entry(4, &rowp),
                entry(5, &self.meta),
            ],
        });
        Batch {
            gpu: self.gpu,
            pipeline: self.pipeline,
            bind,
            obs,
            out,
            readback,
            hidden,
            hidden_size: self.hidden,
            rows: rows as u32,
            _keep: vec![rowp],
            _params: None,
        }
    }
}

/// Networks of one evaluation round living on the GPU.
pub struct Batch<'g> {
    gpu: &'g Gpu,
    pipeline: &'g wgpu::ComputePipeline,
    bind: wgpu::BindGroup,
    obs: wgpu::Buffer,
    out: wgpu::Buffer,
    readback: wgpu::Buffer,
    hidden: wgpu::Buffer,
    hidden_size: usize,
    rows: u32,
    _keep: Vec<wgpu::Buffer>,
    _params: Option<Box<Params<'g>>>,
}

impl<'g> Batch<'g> {
    fn with_params(mut self, p: Params<'g>) -> Self {
        self._params = Some(Box::new(p));
        self
    }

    /// One decision step: `obs` holds `rows x N_IN` inputs, `out` receives `rows x N_OUT`
    /// tanh outputs. Recurrent state advances on the GPU.
    pub fn step(&self, obs: &[f32], out: &mut [f32]) -> Result<(), String> {
        let i = self.submit(obs);
        self.wait(i, out)
    }

    /// Start a step without blocking, so the CPU can work while the GPU computes.
    pub fn submit(&self, obs: &[f32]) -> wgpu::SubmissionIndex {
        let g = self.gpu;
        g.queue
            .write_buffer(&self.obs, 0, bytemuck::cast_slice(obs));
        let mut enc = g.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.dispatch_workgroups(self.rows, 1, 1);
        }
        enc.copy_buffer_to_buffer(&self.out, 0, &self.readback, 0, None);
        let i = g.queue.submit(Some(enc.finish()));
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |_| {});
        i
    }

    /// Finish the step started by `submit`.
    pub fn wait(&self, i: wgpu::SubmissionIndex, out: &mut [f32]) -> Result<(), String> {
        self.gpu
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(i),
                timeout: None,
            })
            .map_err(|e| e.to_string())?;
        {
            let view = self
                .readback
                .slice(..)
                .get_mapped_range()
                .map_err(|e| e.to_string())?;
            out.copy_from_slice(&bytemuck::cast_slice(&view)[..out.len()]);
        }
        self.readback.unmap();
        Ok(())
    }

    /// Recurrent state of every row (`rows x hidden`), e.g. to continue on the CPU.
    pub fn hidden(&self) -> Result<Vec<f32>, String> {
        let g = self.gpu;
        let size = (self.rows as usize * self.hidden_size * 4) as u64;
        if size == 0 {
            return Ok(Vec::new());
        }
        let staging = g.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hidden readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = g.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(&self.hidden, 0, &staging, 0, size);
        let i = g.queue.submit(Some(enc.finish()));
        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        g.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(i),
                timeout: None,
            })
            .map_err(|e| e.to_string())?;
        let view = staging
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        Ok(bytemuck::cast_slice(&view).to_vec())
    }
}
