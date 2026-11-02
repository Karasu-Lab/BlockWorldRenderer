//! Where the canvas runtime meets the concrete renderer and world generator.

use core::canvas::{CanvasGpu, RendererFactory};
use core::renderer::RendererError;
use core::wgpu::WgpuRenderer;
use core::worldgen::{ChunkGenerator, SteelChunkGenerator};

pub struct WgpuFactory;

impl RendererFactory for WgpuFactory {
    type Renderer = WgpuRenderer;

    async fn create(self, gpu: CanvasGpu) -> Result<WgpuRenderer, RendererError> {
        WgpuRenderer::new(
            gpu.instance,
            gpu.adapter,
            gpu.device,
            gpu.queue,
            gpu.surface,
            gpu.config,
        )
    }
}

/// The chunks come from SteelMC's generator, run for the settings' fixed seed.
pub fn world_generator() -> impl ChunkGenerator + 'static {
    SteelChunkGenerator::new()
}
