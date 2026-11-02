//! The wgpu backend of the RHI: the same surface the Vulkan backend has,
//! over wgpu (native Vulkan, Metal and DX12, and WebGPU in the browser).
//!
//! Handles are copies keyed into the [`Device`]'s registries, valid until
//! their owner destroys the object, as Vulkan's raw handles are.

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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

pub use bind::{
    BindGroup, BindGroupLayout, BindingResource, PipelineLayout, create_bind_group_layout,
    create_pipeline_layout,
};
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

/// A failure while drawing a frame.
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

/// Reports an error that has nowhere to be returned to.
pub(crate) fn log_error(message: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("[wgpu] {message}");
    #[cfg(target_arch = "wasm32")]
    web_sys::console::error_1(&format!("[wgpu] {message}").into());
}

/// What every resource and command is made with: the wgpu device and queue,
/// and the registries the RHI's copyable handles key into.
pub(crate) struct Shared {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::Adapter,
    pub adapter_info: wgpu::AdapterInfo,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
    /// The device has immediate data, so push constants need no emulation.
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

    /// Frames submitted and frames the GPU has finished.
    pub submitted: AtomicU64,
    pub completed: Arc<AtomicU64>,
}

impl Shared {
    /// Runs `create` with its validation errors caught: on native they come
    /// back as the `Err`; in the browser, where nothing can block, they are
    /// logged when they arrive.
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

    /// Whether a format of this kind can be blended into.
    pub(crate) fn blendable(&self, format: wgpu::TextureFormat) -> bool {
        self.features.contains(wgpu::Features::FLOAT32_BLENDABLE)
            || !matches!(
                format,
                wgpu::TextureFormat::R32Float
                    | wgpu::TextureFormat::Rg32Float
                    | wgpu::TextureFormat::Rgba32Float
            )
    }

    /// Sends a command buffer to the queue, then starts the readbacks that
    /// follow it.
    pub(crate) fn submit(&self, commands: wgpu::CommandBuffer) {
        self.queue.submit([commands]);
        self.readbacks.pump();
    }

    /// Counts a frame as in flight until the GPU finishes it.
    pub(crate) fn track_frame(&self) {
        self.submitted.fetch_add(1, Ordering::AcqRel);
        let completed = self.completed.clone();
        self.queue.on_submitted_work_done(move || {
            completed.fetch_add(1, Ordering::AcqRel);
        });
    }

    /// Blocks until at most `limit` frames are in flight. The browser can't
    /// block; its frame callbacks pace it.
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

/// The GPU and what every resource is made with. WebGPU's `GPUDevice` with
/// its queue.
pub struct Device {
    pub(crate) shared: Arc<Shared>,
    /// An estimate of the GPU's own memory, in bytes: WebGPU doesn't report
    /// it, so it follows the largest buffer the device allows.
    pub device_local_bytes: u64,
    /// A GPU with memory of its own, which uploads reach through a copy.
    pub discrete: bool,
    pub max_texture_array_layers: u32,
    /// The widest 2D texture, in texels.
    pub max_texture_size: u32,
}

impl Device {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        let limits = &shared.limits;
        Self {
            device_local_bytes: limits.max_buffer_size.saturating_mul(4).min(8 << 30),
            discrete: shared.adapter_info.device_type == wgpu::DeviceType::DiscreteGpu,
            max_texture_array_layers: limits.max_texture_array_layers,
            max_texture_size: limits.max_texture_dimension_2d,
            shared,
        }
    }

    /// The adapter this device runs on.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.shared.adapter_info
    }

    pub fn wait_idle(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self.shared.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// Texture handles are keys the registry drops with their owners, so there
    /// are no framebuffers to forget; kept for the Vulkan backend's callers.
    pub fn forget_targets(&self, _textures: &[TextureHandle]) {}

    /// Records commands and queues them after all earlier work. Once it
    /// returns, the resources they use may be rewritten or destroyed through
    /// the RHI; their results reach the CPU only through a readback buffer.
    /// Native waits for the GPU here; the browser's queue order gives the same.
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

    /// Frees what the device made, once the GPU is idle.
    ///
    /// # Safety
    /// Nothing may use the device afterwards, and every resource made from
    /// it must already be destroyed.
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
