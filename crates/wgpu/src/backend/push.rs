use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::Shared;
use super::bind::LayoutInfo;
use super::resource::lock;

/// Bytes per ring chunk.
const CHUNK_SIZE: u64 = 4 * 1024 * 1024;

/// Where a pipeline's push constants go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PushPlan {
    None,
    /// wgpu immediate data.
    Immediates {
        size: u32,
    },
    /// A uniform in an extra bind group at `group`, bound at a dynamic offset
    /// into the ring; `bind_size` bytes are visible to the shader.
    Uniform {
        group: u32,
        size: u32,
        bind_size: u64,
    },
}

impl PushPlan {
    pub fn size(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Immediates { size } | Self::Uniform { size, .. } => size,
        }
    }
}

struct Chunk {
    buffer: wgpu::Buffer,
    groups: HashMap<u64, wgpu::BindGroup>,
}

#[derive(Default)]
struct RingState {
    chunks: Vec<Chunk>,
    chunk: usize,
    cursor: u64,
}

/// Uniform memory the emulated push constants are written to, one aligned
/// slot per draw. Writes go through the queue, which applies them in order
/// before the commands that read them, so one ring serves every frame in
/// flight once it is reset at the start of each.
#[derive(Default)]
pub(crate) struct PushRing {
    state: Mutex<RingState>,
    layout: OnceLock<Arc<LayoutInfo>>,
}

impl PushRing {
    pub fn reset(&self) {
        let mut state = lock(&self.state);
        state.chunk = 0;
        state.cursor = 0;
    }

    /// The layout of the push bind group: one uniform buffer at a dynamic
    /// offset.
    pub fn layout(&self, shared: &Shared) -> Result<Arc<LayoutInfo>, String> {
        if let Some(layout) = self.layout.get() {
            return Ok(layout.clone());
        }
        let layout = shared.layout_cache.get(
            shared,
            vec![(
                wgpu::ShaderStages::VERTEX
                    | wgpu::ShaderStages::FRAGMENT
                    | wgpu::ShaderStages::COMPUTE,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
            )],
        )?;
        Ok(self.layout.get_or_init(|| layout).clone())
    }

    /// Writes `data`, padded to `bind_size`, into the next slot; the bind
    /// group to set and the dynamic offset to set it with.
    pub fn push(
        &self,
        shared: &Shared,
        data: &[u8],
        bind_size: u64,
    ) -> Result<(wgpu::BindGroup, u32), String> {
        let layout = self.layout(shared)?;
        let alignment = u64::from(shared.limits.min_uniform_buffer_offset_alignment).max(1);
        let slot = bind_size.div_ceil(alignment) * alignment;
        let mut state = lock(&self.state);
        if state.chunks.is_empty() || state.cursor + slot > CHUNK_SIZE {
            if !state.chunks.is_empty() {
                state.chunk += 1;
            }
            state.cursor = 0;
            while state.chunks.len() <= state.chunk {
                let buffer = shared.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("push_constants"),
                    size: CHUNK_SIZE,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                state.chunks.push(Chunk {
                    buffer,
                    groups: HashMap::new(),
                });
            }
        }
        let offset = state.cursor;
        state.cursor += slot;
        let index = state.chunk;
        let chunk = &mut state.chunks[index];
        let mut padded = vec![0u8; bind_size as usize];
        padded[..data.len()].copy_from_slice(data);
        shared.queue.write_buffer(&chunk.buffer, offset, &padded);
        let group = match chunk.groups.get(&bind_size) {
            Some(group) => group.clone(),
            None => {
                let group = shared.scoped("failed to create the push constant group", || {
                    shared.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("push_constants"),
                        layout: &layout.layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &chunk.buffer,
                                offset: 0,
                                size: std::num::NonZeroU64::new(bind_size),
                            }),
                        }],
                    })
                })?;
                chunk.groups.insert(bind_size, group.clone());
                group
            }
        };
        Ok((group, offset as u32))
    }
}
