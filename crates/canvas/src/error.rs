use renderer::RendererError;
use thiserror::Error;
use worldgen::WorldgenError;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanvasError {
    #[error("the browser provides neither WebGPU nor WebGL2")]
    Unsupported,
    #[error("failed to set up the canvas: {0}")]
    Setup(String),
    #[error(transparent)]
    Renderer(#[from] RendererError),
    #[error(transparent)]
    World(#[from] WorldgenError),
}

impl CanvasError {
    pub fn code(&self) -> u16 {
        match self {
            CanvasError::Unsupported => 1,
            CanvasError::Setup(_) => 2,
            CanvasError::Renderer(_) => 3,
            CanvasError::World(_) => 4,
        }
    }
}
