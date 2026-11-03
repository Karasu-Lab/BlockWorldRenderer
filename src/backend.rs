//! Where the canvas runtime meets the concrete renderer and world generator.

use std::io;

use engine::camera::Camera;
use engine::canvas::{CanvasRenderer, CanvasSize, RendererFactory};
use engine::debugger::{RendererInfo, RendererInfoSource};
use engine::renderer::{RenderScene, RenderTarget, Renderer, RendererError};
use engine::resourcepack::ResourceProvider;
use engine::shader::ShaderProgram;
use engine::wgpu::backend::{ClearDrawer, Context, SceneDrawer, WgpuRenderer};
use engine::worldgen::{ChunkGenerator, SteelChunkGenerator, install_thread_spawner};
use web_sys::HtmlCanvasElement;

/// Makes the [`SceneDrawer`] a renderer draws its frames with.
///
/// The client's viewer is one: it keeps the world's pipelines and buffers and
/// records its passes through the wgpu backend.
pub trait DrawerSource {
    fn drawer(self) -> Box<dyn SceneDrawer>;
}

/// Frames that only clear the canvas, for a page that has no viewer yet.
pub struct ClearSource(pub [f32; 4]);

impl DrawerSource for ClearSource {
    fn drawer(self) -> Box<dyn SceneDrawer> {
        Box::new(ClearDrawer { color: self.0 })
    }
}

/// The wgpu renderer as the canvas runtime drives it.
pub struct WebRenderer(WgpuRenderer);

impl Renderer for WebRenderer {
    fn resize(&mut self, target: RenderTarget) {
        self.0.resize(target);
    }

    fn load_resources(&mut self, provider: &dyn ResourceProvider) -> Result<(), RendererError> {
        self.0.load_resources(provider)
    }

    fn load_shader(&mut self, program: ShaderProgram) -> Result<(), RendererError> {
        self.0.load_shader(program)
    }

    fn render_frame(
        &mut self,
        camera: &Camera,
        scene: &dyn RenderScene,
    ) -> Result<(), RendererError> {
        self.0.render_frame(camera, scene)
    }
}

impl RendererInfoSource for WebRenderer {
    fn renderer_info(&self) -> RendererInfo {
        self.0.renderer_info()
    }
}

impl CanvasRenderer for WebRenderer {
    fn max_texture_dimension(&self) -> u32 {
        self.0.device().max_texture_size
    }
}

pub struct WgpuFactory<S> {
    drawers: S,
}

impl<S> WgpuFactory<S> {
    pub fn new(drawers: S) -> Self {
        Self { drawers }
    }
}

impl<S: DrawerSource> RendererFactory for WgpuFactory<S> {
    type Renderer = WebRenderer;

    async fn create(
        self,
        canvas: HtmlCanvasElement,
        size: CanvasSize,
    ) -> Result<WebRenderer, RendererError> {
        let device_error =
            |error: engine::wgpu::backend::ContextError| RendererError::Device(error.to_string());
        let context = Context::from_canvas(canvas).await.map_err(device_error)?;
        let renderer = WgpuRenderer::new(
            context,
            RenderTarget {
                width: size.width,
                height: size.height,
            },
            true,
            self.drawers.drawer(),
        )
        .map_err(device_error)?;
        Ok(WebRenderer(renderer))
    }
}

/// Starts a Steel worker on the page's thread pool, as SteelMC asks of a host
/// on wasm32.
fn spawn_steel_worker(name: String, task: Box<dyn FnOnce() + Send>) -> io::Result<()> {
    engine::thread::spawn(name, task)
        .map(|_| ())
        .map_err(io::Error::other)
}

/// The chunks come from SteelMC's generator, run for the settings' fixed seed.
pub fn world_generator() -> impl ChunkGenerator + Send + Sync + 'static {
    install_thread_spawner(spawn_steel_worker);
    SteelChunkGenerator::new()
}
