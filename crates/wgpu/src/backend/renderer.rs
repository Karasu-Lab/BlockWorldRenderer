use ::shader::{ShaderError, ShaderProgram};
use camera::Camera;
use debugger::{RendererInfo, RendererInfoSource};
use renderer::{RenderScene, RenderTarget, Renderer, RendererError};
use resourcepack::ResourceProvider;

use super::shader::{self, PushMode};
use super::{CommandEncoder, Context, ContextError, Device, Frames, Swapchain};
use crate::rhi::{Acquired, LoadOp};

/// What a frame is recorded against.
pub struct FrameTarget<'a> {
    pub device: &'a Device,
    pub encoder: &'a CommandEncoder,
    pub swapchain: &'a Swapchain,
    /// The swapchain image the frame draws into.
    pub image: u32,
    /// The frame slot, indexing per-frame resources.
    pub frame: usize,
}

/// Draws the world into a frame. The client's renderer is one: it keeps its
/// pipelines and buffers and records its passes with the RHI.
pub trait SceneDrawer {
    fn resize(&mut self, device: &Device, swapchain: &Swapchain) -> Result<(), RendererError> {
        let _ = (device, swapchain);
        Ok(())
    }

    fn load_resources(
        &mut self,
        device: &Device,
        provider: &dyn ResourceProvider,
    ) -> Result<(), RendererError> {
        let _ = (device, provider);
        Ok(())
    }

    /// Takes a shader program whose sources have already been validated.
    fn load_shader(
        &mut self,
        device: &Device,
        program: ShaderProgram,
    ) -> Result<(), RendererError> {
        let _ = (device, program);
        Ok(())
    }

    fn draw(
        &mut self,
        target: &FrameTarget<'_>,
        camera: &Camera,
        scene: &dyn RenderScene,
    ) -> Result<(), RendererError>;
}

/// A drawer that only clears the frame.
pub struct ClearDrawer {
    pub color: [f32; 4],
}

impl SceneDrawer for ClearDrawer {
    fn draw(
        &mut self,
        target: &FrameTarget<'_>,
        _camera: &Camera,
        _scene: &dyn RenderScene,
    ) -> Result<(), RendererError> {
        target
            .encoder
            .begin_render_pass(
                target.device,
                &target
                    .swapchain
                    .pass(target.image, LoadOp::Clear(self.color)),
            )
            .end();
        Ok(())
    }
}

/// The renderer over wgpu: it owns the GPU context, the swapchain and the
/// frame pacing, and hands each frame to its [`SceneDrawer`].
pub struct WgpuRenderer {
    ctx: Context,
    swapchain: Swapchain,
    frames: Frames,
    drawer: Box<dyn SceneDrawer>,
    vsync: bool,
    pending: Option<RenderTarget>,
}

fn device_error(error: impl std::fmt::Display) -> RendererError {
    RendererError::Device(error.to_string())
}

impl WgpuRenderer {
    pub fn new(
        ctx: Context,
        target: RenderTarget,
        vsync: bool,
        drawer: Box<dyn SceneDrawer>,
    ) -> Result<Self, ContextError> {
        let swapchain = Swapchain::new(&ctx, target.width, target.height, vsync)?;
        let frames = Frames::new(&ctx.device, swapchain.image_count())
            .map_err(|error| ContextError::Gpu(error.to_string()))?;
        Ok(Self {
            ctx,
            swapchain,
            frames,
            drawer,
            vsync,
            pending: None,
        })
    }

    pub fn context(&self) -> &Context {
        &self.ctx
    }

    pub fn device(&self) -> &Device {
        &self.ctx.device
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        self.ctx.device.adapter_info()
    }

    fn rebuild(&mut self, target: RenderTarget) -> Result<(), RendererError> {
        self.ctx.device.wait_idle();
        self.swapchain
            .rebuild(
                &self.ctx,
                &mut self.frames,
                target.width,
                target.height,
                self.vsync,
            )
            .map_err(device_error)?;
        self.drawer.resize(&self.ctx.device, &self.swapchain)
    }
}

impl Renderer for WgpuRenderer {
    fn resize(&mut self, target: RenderTarget) {
        self.pending = Some(target);
    }

    fn load_resources(&mut self, provider: &dyn ResourceProvider) -> Result<(), RendererError> {
        self.drawer.load_resources(&self.ctx.device, provider)
    }

    fn load_shader(&mut self, program: ShaderProgram) -> Result<(), RendererError> {
        for source in &program.sources {
            shader::process(&source.code, PushMode::Immediates)
                .map_err(|error| RendererError::Shader(ShaderError::Compile(error)))?;
        }
        self.drawer.load_shader(&self.ctx.device, program)
    }

    fn render_frame(
        &mut self,
        camera: &Camera,
        scene: &dyn RenderScene,
    ) -> Result<(), RendererError> {
        if let Some(target) = self.pending.take() {
            self.rebuild(target)?;
        }
        self.frames.wait(&self.ctx.device).map_err(device_error)?;
        let image = match self
            .frames
            .acquire(&self.ctx.device, &self.swapchain)
            .map_err(device_error)?
        {
            Acquired::Image { index, .. } => index,
            Acquired::OutOfDate => {
                let extent = self.swapchain.extent;
                return self.rebuild(RenderTarget {
                    width: extent.width,
                    height: extent.height,
                });
            }
        };
        let encoder = self.frames.begin(&self.ctx.device).map_err(device_error)?;
        self.drawer.draw(
            &FrameTarget {
                device: &self.ctx.device,
                encoder: &encoder,
                swapchain: &self.swapchain,
                image,
                frame: self.frames.index(),
            },
            camera,
            scene,
        )?;
        self.swapchain.ready_to_present(&encoder, image);
        self.frames
            .submit_and_present(encoder, &self.ctx, &self.swapchain, image)
            .map_err(device_error)?;
        Ok(())
    }
}

impl RendererInfoSource for WgpuRenderer {
    fn renderer_info(&self) -> RendererInfo {
        let info = self.adapter_info();
        RendererInfo {
            backend: format!("wgpu {:?}", info.backend),
            adapter: info.name.clone(),
            driver: if info.driver_info.is_empty() {
                info.driver.clone()
            } else {
                format!("{} {}", info.driver, info.driver_info)
            },
        }
    }
}

impl Drop for WgpuRenderer {
    fn drop(&mut self) {
        self.ctx.device.wait_idle();
        self.frames.destroy(&self.ctx.device);
        self.swapchain.destroy(&self.ctx);
    }
}
