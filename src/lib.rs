pub use engine;

#[cfg(all(target_arch = "wasm32", any(feature = "replay", feature = "save")))]
mod chunkdata;
#[cfg(target_arch = "wasm32")]
mod entity;
#[cfg(all(target_arch = "wasm32", feature = "replay"))]
mod replay;
#[cfg(all(target_arch = "wasm32", feature = "save"))]
mod save;
#[cfg(target_arch = "wasm32")]
mod web;

/// The page's thread pool, which chunk generation runs on; JavaScript awaits
/// `initThreadPool(n)` before `start`.
#[cfg(target_arch = "wasm32")]
pub use engine::thread::init_thread_pool;
