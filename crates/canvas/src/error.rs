use renderer::RendererError;
use thiserror::Error;
use worldgen::WorldgenError;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanvasError {
    #[error("the browser does not provide WebGPU")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let codes = [
            CanvasError::Unsupported.code(),
            CanvasError::Setup(String::new()).code(),
            CanvasError::from(RendererError::Device(String::new())).code(),
            CanvasError::from(WorldgenError::Generation(String::new())).code(),
        ];
        assert_eq!(codes, [1, 2, 3, 4]);
    }
}
