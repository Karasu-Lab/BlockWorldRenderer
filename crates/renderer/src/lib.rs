use bwr_block::BlockGetter;
use bwr_camera::Camera;
use bwr_resourcepack::{ResourcePackError, ResourceProvider};
use bwr_shader::{ShaderError, ShaderProgram};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderTarget {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RendererError {
    #[error("graphics device error: {0}")]
    Device(String),
    #[error(transparent)]
    Shader(#[from] ShaderError),
    #[error(transparent)]
    Resource(#[from] ResourcePackError),
}

impl RendererError {
    pub fn code(&self) -> u16 {
        match self {
            RendererError::Device(_) => 1,
            RendererError::Shader(_) => 2,
            RendererError::Resource(_) => 3,
        }
    }
}

pub trait RenderScene: BlockGetter {
    fn visible_chunks(&self, camera: &Camera) -> Vec<(i32, i32)>;
}

pub trait Renderer {
    fn resize(&mut self, target: RenderTarget);

    fn load_resources(&mut self, provider: &dyn ResourceProvider) -> Result<(), RendererError>;

    fn load_shader(&mut self, program: ShaderProgram) -> Result<(), RendererError>;

    fn render_frame(
        &mut self,
        camera: &Camera,
        scene: &dyn RenderScene,
    ) -> Result<(), RendererError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let codes = [
            RendererError::Device(String::new()).code(),
            RendererError::from(ShaderError::Compile(String::new())).code(),
            RendererError::from(ResourcePackError::Invalid(String::new())).code(),
        ];
        assert_eq!(codes, [1, 2, 3]);
    }
}
