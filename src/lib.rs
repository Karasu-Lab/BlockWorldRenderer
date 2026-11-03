pub use engine;

#[cfg(target_arch = "wasm32")]
mod backend;
#[cfg(target_arch = "wasm32")]
mod web;

/// The page's thread pool, which chunk generation runs on; JavaScript awaits
/// `initThreadPool(n)` before `start`.
#[cfg(target_arch = "wasm32")]
pub use engine::thread::init_thread_pool;
