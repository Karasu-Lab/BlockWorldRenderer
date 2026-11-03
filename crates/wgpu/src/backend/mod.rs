mod bind;
mod context;
mod encoder;
mod helpers;
mod pipeline;
mod push;
mod registry;
mod renderer;
mod resource;
mod shader;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

pub use bind::{
    BindGroup, BindGroupLayout, BindingResource, PipelineLayout, create_bind_group_layout,
    create_pipeline_layout,
};
#[cfg(target_arch = "wasm32")]
pub use context::browser_supported;
pub use context::{Context, Frames, MAX_FRAMES_IN_FLIGHT, Swapchain};
pub use encoder::{CommandEncoder, RenderPassEncoder};
pub use pipeline::{
    ComputePipeline, PipelineTargets, RenderPipeline, create_compute_pipeline,
    create_render_pipeline, try_create_compute_pipeline, try_create_render_pipeline,
};
pub use renderer::{ClearDrawer, FrameTarget, SceneDrawer, WgpuRenderer};
pub use resource::{
    Buffer, BufferHandle, Sampler, Texture, TextureHandle, create_sampler, texture_format,
    try_create_sampler,
};
pub use shader::PushMode;

use crate::rhi;

pub type ColorAttachment = rhi::ColorAttachment<TextureHandle, wgpu::TextureFormat>;
pub type DepthAttachment = rhi::DepthAttachment<TextureHandle>;
pub type RenderPassDesc = rhi::RenderPassDesc<TextureHandle, wgpu::TextureFormat>;
pub type TextureTransition = rhi::TextureTransition<TextureHandle>;

#[derive(Error, Debug)]
pub enum ContextError {
    #[error("wgpu error: {0}")]
    Gpu(String),

    #[error("no suitable GPU found: {0}")]
    NoSuitableGpu(String),

    #[error("{0}")]
    Texture(String),

    #[error("surface error: {0}")]
    Surface(String),

    #[error("surface error: {0}")]
    HandleError(#[from] raw_window_handle::HandleError),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Gpu(String),

    #[error("surface error: {0}")]
    Surface(String),
}

impl From<Error> for ContextError {
    fn from(error: Error) -> Self {
        Self::Gpu(error.to_string())
    }
}

impl From<ContextError> for Error {
    fn from(error: ContextError) -> Self {
        Self::Gpu(error.to_string())
    }
}

pub(crate) fn log_error(message: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("[wgpu] {message}");
    #[cfg(target_arch = "wasm32")]
    web_sys::console::error_1(&format!("[wgpu] {message}").into());
}

pub(crate) struct Shared {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::Adapter,
    pub adapter_info: wgpu::AdapterInfo,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
    pub immediates: bool,
    pub depth_clip_control: bool,

    pub buffers: registry::Slab<resource::BufferEntry>,
    pub textures: registry::Slab<resource::TextureEntry>,
    pub samplers: registry::Slab<wgpu::Sampler>,
    pub layouts: registry::Slab<bind::LayoutEntry>,
    pub pipeline_layouts: registry::Slab<bind::PipelineLayoutEntry>,
    pub bind_groups: registry::Slab<bind::BindGroupEntry>,
    pub render_pipelines: registry::Slab<pipeline::RenderPipelineEntry>,
    pub compute_pipelines: registry::Slab<pipeline::ComputePipelineEntry>,

    pub layout_cache: bind::LayoutCache,
    pub fallbacks: bind::Fallbacks,
    pub push: push::PushRing,
    pub helpers: helpers::Helpers,
    pub readbacks: resource::Readbacks,

    pub submitted: AtomicU64,
    pub completed: Arc<AtomicU64>,
}

impl Shared {
    pub(crate) fn scoped<T>(&self, what: &str, create: impl FnOnce() -> T) -> Result<T, String> {
        let guard = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let value = create();
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(error) = pollster::block_on(guard.pop()) {
                return Err(format!("{what}: {error}"));
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let what = what.to_owned();
            let error = guard.pop();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(error) = error.await {
                    log_error(&format!("{what}: {error}"));
                }
            });
        }
        Ok(value)
    }

    pub(crate) fn log_error(&self, message: &str) {
        log_error(message);
    }

    pub(crate) fn blendable(&self, format: wgpu::TextureFormat) -> bool {
        self.features.contains(wgpu::Features::FLOAT32_BLENDABLE)
            || !matches!(
                format,
                wgpu::TextureFormat::R32Float
                    | wgpu::TextureFormat::Rg32Float
                    | wgpu::TextureFormat::Rgba32Float
            )
    }

    pub(crate) fn submit(&self, commands: wgpu::CommandBuffer) {
        self.queue.submit([commands]);
        self.readbacks.pump();
    }

    pub(crate) fn track_frame(&self) {
        self.submitted.fetch_add(1, Ordering::AcqRel);
        let completed = self.completed.clone();
        self.queue.on_submitted_work_done(move || {
            completed.fetch_add(1, Ordering::AcqRel);
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn wait_for_frames(&self, limit: u64, timeout: Option<std::time::Duration>) {
        while self.submitted.load(Ordering::Acquire) - self.completed.load(Ordering::Acquire)
            > limit
        {
            let result = self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout,
            });
            if timeout.is_some() || result.is_err() {
                break;
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn wait_for_frames(&self, _limit: u64, _timeout: Option<std::time::Duration>) {}
}

pub struct Device {
    pub(crate) shared: Arc<Shared>,
    pub device_local_bytes: u64,
    pub discrete: bool,
    pub max_texture_array_layers: u32,
    pub max_texture_size: u32,
}

impl Device {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        let limits = &shared.limits;
        let ceiling: u64 = match (cfg!(target_arch = "wasm32"), shared.adapter_info.backend) {
            (true, wgpu::Backend::Gl) => 512 << 20,
            (true, _) => 1 << 30,
            (false, _) => 8 << 30,
        };
        Self {
            device_local_bytes: limits.max_buffer_size.saturating_mul(4).min(ceiling),
            discrete: shared.adapter_info.device_type == wgpu::DeviceType::DiscreteGpu,
            max_texture_array_layers: limits.max_texture_array_layers,
            max_texture_size: limits.max_texture_dimension_2d,
            shared,
        }
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.shared.adapter_info
    }

    pub fn wait_idle(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self.shared.device.poll(wgpu::PollType::wait_indefinitely());
    }

    pub fn forget_targets(&self, _textures: &[TextureHandle]) {}

    pub fn submit(&self, record: impl FnOnce(&CommandEncoder)) {
        self.try_submit(record)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    pub fn try_submit(&self, record: impl FnOnce(&CommandEncoder)) -> Result<(), String> {
        let shared = &self.shared;
        let encoder = CommandEncoder::new(shared);
        record(&encoder);
        shared.scoped("failed to run a command buffer", || {
            shared.submit(encoder.finish());
        })?;
        self.wait_idle();
        Ok(())
    }

    pub unsafe fn destroy(&mut self) {
        self.wait_idle();
        let shared = &self.shared;
        shared.buffers.clear();
        shared.textures.clear();
        shared.samplers.clear();
        shared.layouts.clear();
        shared.pipeline_layouts.clear();
        shared.bind_groups.clear();
        shared.render_pipelines.clear();
        shared.compute_pipelines.clear();
    }
}
