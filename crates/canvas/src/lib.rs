mod error;
mod frame;
mod scene;
mod size;

#[cfg(target_arch = "wasm32")]
mod runtime;
#[cfg(target_arch = "wasm32")]
mod webgpu;

pub use error::CanvasError;
pub use frame::FrameClock;
pub use scene::{CHUNK_SIZE, ChunkLayout, ChunkScene, StreamingScene};
pub use size::CanvasSize;

#[cfg(target_arch = "wasm32")]
pub use runtime::{Canvas, CanvasOptions, CanvasRenderer, RendererFactory};
#[cfg(target_arch = "wasm32")]
pub use webgpu::webgpu_available;
