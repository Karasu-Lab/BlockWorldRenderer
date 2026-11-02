mod error;
mod frame;
mod scene;
mod size;

#[cfg(target_arch = "wasm32")]
mod gpu;
#[cfg(target_arch = "wasm32")]
mod runtime;

pub use error::CanvasError;
pub use frame::FrameClock;
pub use scene::{CHUNK_SIZE, ChunkLayout, ChunkScene};
pub use size::CanvasSize;

#[cfg(target_arch = "wasm32")]
pub use gpu::CanvasGpu;
#[cfg(target_arch = "wasm32")]
pub use runtime::{Canvas, CanvasOptions, RendererFactory};
