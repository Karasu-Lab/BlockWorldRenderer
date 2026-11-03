use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::registry::Key;
use super::resource::{BufferHandle, Sampler, TextureHandle, lock};
use super::{Device, Shared};
use crate::rhi::{BindGroupLayoutEntry, BindingType, PushConstants, ShaderStages};

pub(crate) fn stages(stages: ShaderStages) -> wgpu::ShaderStages {
    let mut flags = wgpu::ShaderStages::empty();
    if stages.contains(ShaderStages::VERTEX) {
        flags |= wgpu::ShaderStages::VERTEX;
    }
    if stages.contains(ShaderStages::FRAGMENT) {
        flags |= wgpu::ShaderStages::FRAGMENT;
    }
    if stages.contains(ShaderStages::COMPUTE) {
        flags |= wgpu::ShaderStages::COMPUTE;
    }
    flags
}

/// The layout of a bind group as the RHI describes it. The wgpu layouts are
/// made per pipeline, from the types its shaders declare.
pub(crate) struct LayoutEntry {
    pub entries: Vec<BindGroupLayoutEntry>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct BindGroupLayout(pub(crate) Key);

impl BindGroupLayout {
    pub fn destroy(self, device: &Device) {
        device.shared.layouts.remove(self.0);
    }
}

pub(crate) struct PipelineLayoutEntry {
    pub groups: Vec<BindGroupLayout>,
    pub push: Option<PushConstants>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct PipelineLayout(pub(crate) Key);

impl PipelineLayout {
    pub fn destroy(self, device: &Device) {
        device.shared.pipeline_layouts.remove(self.0);
    }
}

pub fn create_bind_group_layout(
    device: &Device,
    entries: &[BindGroupLayoutEntry],
) -> BindGroupLayout {
    BindGroupLayout(device.shared.layouts.insert(LayoutEntry {
        entries: entries.to_vec(),
    }))
}

pub fn create_pipeline_layout(
    device: &Device,
    bind_group_layouts: &[BindGroupLayout],
    push_constants: Option<PushConstants>,
) -> PipelineLayout {
    PipelineLayout(device.shared.pipeline_layouts.insert(PipelineLayoutEntry {
        groups: bind_group_layouts.to_vec(),
        push: push_constants,
    }))
}

/// What one binding of a bind group points at. Textures are sampled in the
/// state [`TextureUse::ShaderRead`](crate::rhi::TextureUse::ShaderRead) names.
#[derive(Clone, Copy)]
pub enum BindingResource {
    UniformBuffer { buffer: BufferHandle, size: u64 },
    StorageBuffer { buffer: BufferHandle, size: u64 },
    Texture(TextureHandle),
    Sampler(Sampler),
}

impl BindingResource {
    pub fn uniform(buffer: &super::Buffer, size: u64) -> Self {
        Self::UniformBuffer {
            buffer: buffer.handle(),
            size,
        }
    }

    pub fn storage(buffer: &super::Buffer, size: u64) -> Self {
        Self::StorageBuffer {
            buffer: buffer.handle(),
            size,
        }
    }
}

#[derive(Clone)]
pub(crate) enum Resolved {
    Buffer { buffer: wgpu::Buffer, size: u64 },
    Texture(wgpu::TextureView),
    Sampler(wgpu::Sampler),
}

/// A wgpu bind group layout and the types its entries have.
pub(crate) struct LayoutInfo {
    pub id: u64,
    pub layout: wgpu::BindGroupLayout,
    pub types: Vec<wgpu::BindingType>,
}

pub(crate) struct BindGroupEntry {
    resources: Vec<(u32, Resolved)>,
    /// The wgpu groups made so far, by the id of the layout they follow.
    cache: Mutex<HashMap<u64, wgpu::BindGroup>>,
}

impl BindGroupEntry {
    pub fn get(&self, shared: &Shared, layout: &LayoutInfo) -> Result<wgpu::BindGroup, String> {
        if let Some(group) = lock(&self.cache).get(&layout.id) {
            return Ok(group.clone());
        }
        let entries: Vec<_> = layout
            .types
            .iter()
            .enumerate()
            .map(|(binding, ty)| {
                let resolved = self
                    .resources
                    .iter()
                    .find(|(index, _)| *index as usize == binding)
                    .map(|(_, resolved)| resolved.clone())
                    .unwrap_or_else(|| shared.fallback(ty));
                (binding as u32, resolved)
            })
            .collect();
        let group = shared.scoped("failed to create a bind group", || {
            let wgpu_entries: Vec<_> = entries
                .iter()
                .map(|(binding, resolved)| wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: match resolved {
                        Resolved::Buffer { buffer, size } => {
                            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer,
                                offset: 0,
                                size: NonZeroU64::new(*size),
                            })
                        }
                        Resolved::Texture(view) => wgpu::BindingResource::TextureView(view),
                        Resolved::Sampler(sampler) => wgpu::BindingResource::Sampler(sampler),
                    },
                })
                .collect();
            shared.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout.layout,
                entries: &wgpu_entries,
            })
        })?;
        lock(&self.cache).insert(layout.id, group.clone());
        Ok(group)
    }
}

/// A set of bindings written once, as a WebGPU `GPUBindGroup` can't change.
/// The default is an empty placeholder.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct BindGroup(pub(crate) Key);

impl BindGroup {
    pub fn destroy(self, device: &Device) {
        device.shared.bind_groups.remove(self.0);
    }

    /// Points the group at new resources. A bind group can't change, so this
    /// makes a new one and frees the old.
    pub fn replace(
        &mut self,
        device: &Device,
        layout: BindGroupLayout,
        bindings: &[(u32, BindingResource)],
    ) {
        std::mem::replace(self, device.create_bind_group(layout, bindings)).destroy(device);
    }
}

impl Device {
    /// A bind group of `layout` holding `bindings`.
    pub fn create_bind_group(
        &self,
        layout: BindGroupLayout,
        bindings: &[(u32, BindingResource)],
    ) -> BindGroup {
        let shared = &self.shared;
        assert!(
            shared.layouts.get(layout.0).is_some(),
            "bind group layout was destroyed"
        );
        let resources = bindings
            .iter()
            .map(|&(binding, resource)| {
                let resolved = match resource {
                    BindingResource::UniformBuffer { buffer, size }
                    | BindingResource::StorageBuffer { buffer, size } => {
                        let entry = shared
                            .buffers
                            .get(buffer.0)
                            .expect("bind group buffer was destroyed");
                        Resolved::Buffer {
                            buffer: entry.buffer.clone(),
                            size,
                        }
                    }
                    BindingResource::Texture(texture) => Resolved::Texture(
                        shared
                            .textures
                            .get(texture.0)
                            .expect("bind group texture was destroyed")
                            .view
                            .clone(),
                    ),
                    BindingResource::Sampler(sampler) => Resolved::Sampler(
                        shared
                            .samplers
                            .get(sampler.0)
                            .expect("bind group sampler was destroyed")
                            .as_ref()
                            .clone(),
                    ),
                };
                (binding, resolved)
            })
            .collect();
        BindGroup(shared.bind_groups.insert(BindGroupEntry {
            resources,
            cache: Mutex::new(HashMap::new()),
        }))
    }
}

/// The layouts made so far, by definition: pipelines whose shaders declare
/// the same bindings share one, and so share bind groups.
pub(crate) type LayoutTypes = Vec<(wgpu::ShaderStages, wgpu::BindingType)>;

#[derive(Default)]
pub(crate) struct LayoutCache {
    layouts: Mutex<HashMap<LayoutTypes, Arc<LayoutInfo>>>,
    next_id: AtomicU64,
}

impl LayoutCache {
    pub fn get(&self, shared: &Shared, entries: LayoutTypes) -> Result<Arc<LayoutInfo>, String> {
        let mut layouts = lock(&self.layouts);
        if let Some(found) = layouts.get(&entries) {
            return Ok(found.clone());
        }
        let wgpu_entries: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(binding, (visibility, ty))| wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: *visibility,
                ty: *ty,
                count: None,
            })
            .collect();
        let layout = shared.scoped("failed to create a bind group layout", || {
            shared
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &wgpu_entries,
                })
        })?;
        let info = Arc::new(LayoutInfo {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            layout,
            types: entries.iter().map(|(_, ty)| *ty).collect(),
        });
        layouts.insert(entries, info.clone());
        Ok(info)
    }
}

/// The type an RHI binding has when no shader of the pipeline declares it.
pub(crate) fn default_binding(entry: &BindGroupLayoutEntry) -> wgpu::BindingType {
    match entry.ty {
        BindingType::UniformBuffer => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        BindingType::StorageBuffer => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage {
                read_only: !entry.visibility.contains(ShaderStages::COMPUTE),
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        BindingType::Texture => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        BindingType::Sampler => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
    }
}

/// Stand-ins for bindings a shader declares but a bind group leaves unwritten,
/// which Vulkan lets stay empty while unused.
#[derive(Default)]
pub(crate) struct Fallbacks {
    resources: Mutex<HashMap<String, Resolved>>,
}

impl Shared {
    pub(crate) fn fallback(&self, ty: &wgpu::BindingType) -> Resolved {
        let key = format!("{ty:?}");
        if let Some(found) = lock(&self.fallbacks.resources).get(&key) {
            return found.clone();
        }
        let resolved = match *ty {
            wgpu::BindingType::Buffer { .. } => {
                let size = 64 * 1024;
                Resolved::Buffer {
                    buffer: self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("fallback_buffer"),
                        size,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::STORAGE,
                        mapped_at_creation: false,
                    }),
                    size,
                }
            }
            wgpu::BindingType::Texture {
                sample_type,
                view_dimension,
                ..
            } => {
                let format = match sample_type {
                    wgpu::TextureSampleType::Depth => wgpu::TextureFormat::Depth32Float,
                    wgpu::TextureSampleType::Sint => wgpu::TextureFormat::Rgba8Sint,
                    wgpu::TextureSampleType::Uint => wgpu::TextureFormat::Rgba8Uint,
                    wgpu::TextureSampleType::Float { .. } => wgpu::TextureFormat::Rgba8Unorm,
                };
                let layers = match view_dimension {
                    wgpu::TextureViewDimension::Cube => 6,
                    wgpu::TextureViewDimension::CubeArray => 6,
                    _ => 1,
                };
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("fallback_texture"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: layers,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                Resolved::Texture(texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(view_dimension),
                    ..Default::default()
                }))
            }
            wgpu::BindingType::Sampler(kind) => Resolved::Sampler(
                self.device.create_sampler(&wgpu::SamplerDescriptor {
                    compare: matches!(kind, wgpu::SamplerBindingType::Comparison)
                        .then_some(wgpu::CompareFunction::Always),
                    ..Default::default()
                }),
            ),
            _ => unreachable!("the RHI binds buffers, textures and samplers only"),
        };
        lock(&self.fallbacks.resources).insert(key, resolved.clone());
        resolved
    }
}
