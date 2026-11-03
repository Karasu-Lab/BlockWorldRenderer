// wgpu's handles are not `Send` on the web, where there is one thread.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

pub mod rhi;

#[cfg(feature = "backend")]
pub mod backend;
