use std::borrow::Cow;
use std::sync::Arc;

use super::Device;
use super::Shared;
use super::bind::{LayoutInfo, PipelineLayout, default_binding, stages};
use super::push::PushPlan;
use super::registry::Key;
use super::shader::{self, ProcessedShader, PushMode};
use crate::rhi::{
    self, BlendFactor, BlendState, CompareFunction, CullMode, FrontFace, RenderPipelineDesc,
    Topology, VertexFormat, VertexStepMode,
};

pub type PipelineTargets = rhi::PipelineTargets<wgpu::TextureFormat>;

/// What binding a pipeline's resources takes: the wgpu layout of each of the
/// RHI's bind groups, and where its push constants go.
pub(crate) struct PipelineInfo {
    pub layouts: Vec<Arc<LayoutInfo>>,
    pub push: PushPlan,
}

pub(crate) struct RenderPipelineEntry {
    pub pipeline: wgpu::RenderPipeline,
    pub info: PipelineInfo,
}

pub(crate) struct ComputePipelineEntry {
    pub pipeline: wgpu::ComputePipeline,
    pub info: PipelineInfo,
}

/// A graphics pipeline and the layout its bind groups and push constants
/// follow. The default is an empty placeholder.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct RenderPipeline(pub(crate) Key);

impl RenderPipeline {
    pub fn destroy(self, device: &Device) {
        device.shared.render_pipelines.remove(self.0);
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct ComputePipeline(pub(crate) Key);

impl ComputePipeline {
    pub fn destroy(self, device: &Device) {
        device.shared.compute_pipelines.remove(self.0);
    }
}

fn shader_text(bytes: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(bytes).map_err(|_| {
        "the wgpu backend takes WGSL shaders as UTF-8; got something else (SPIR-V?)".to_owned()
    })
}

impl Shared {
    fn push_mode(
        &self,
        push: Option<rhi::PushConstants>,
        groups: usize,
    ) -> Result<Option<PushMode>, String> {
        let Some(push) = push else {
            return Ok(None);
        };
        if self.immediates && push.size <= self.limits.max_immediate_size {
            return Ok(Some(PushMode::Immediates));
        }
        if groups + 1 > self.limits.max_bind_groups as usize {
            return Err(format!(
                "push constants need a bind group of their own, but the layout already uses {groups} of the device's {}",
                self.limits.max_bind_groups
            ));
        }
        Ok(Some(PushMode::Uniform {
            group: groups as u32,
        }))
    }

    /// The module for `shader`, with the layouts of `layout` fitted to what
    /// the shaders declare.
    fn build_layout(
        &self,
        layout: PipelineLayout,
        processed: &[&ProcessedShader],
        mode: Option<PushMode>,
    ) -> Result<(wgpu::PipelineLayout, PipelineInfo), String> {
        let entry = self
            .pipeline_layouts
            .get(layout.0)
            .ok_or("pipeline layout was destroyed")?;
        let mut layouts = Vec::new();
        for (group, rhi_layout) in entry.groups.iter().enumerate() {
            let rhi_layout = self
                .layouts
                .get(rhi_layout.0)
                .ok_or("bind group layout was destroyed")?;
            let types = rhi_layout
                .entries
                .iter()
                .enumerate()
                .map(|(binding, rhi_entry)| {
                    let declared = processed.iter().find_map(|shader| {
                        shader
                            .bindings
                            .iter()
                            .find(|b| b.group == group as u32 && b.binding == binding as u32)
                    });
                    (
                        stages(rhi_entry.visibility),
                        declared.map_or_else(|| default_binding(rhi_entry), |b| b.ty),
                    )
                })
                .collect();
            layouts.push(self.layout_cache.get(self, types)?);
        }
        let (push, push_layout) = match mode {
            None => (PushPlan::None, None),
            Some(PushMode::Immediates) => {
                let size = entry.push.map_or(0, |push| push.size);
                (PushPlan::Immediates { size }, None)
            }
            Some(PushMode::Uniform { group }) => {
                let size = entry.push.map_or(0, |push| push.size);
                let declared = processed.iter().filter_map(|shader| shader.push_size).max();
                let bind_size = u64::from(declared.unwrap_or(size).max(size)).div_ceil(16) * 16;
                (
                    PushPlan::Uniform {
                        group,
                        size,
                        bind_size: bind_size.max(16),
                    },
                    Some(self.push.layout(self)?),
                )
            }
        };
        let mut refs: Vec<Option<&wgpu::BindGroupLayout>> =
            layouts.iter().map(|info| Some(&info.layout)).collect();
        if let Some(push_layout) = &push_layout {
            refs.push(Some(&push_layout.layout));
        }
        let pipeline_layout = self.scoped("failed to create a pipeline layout", || {
            self.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &refs,
                    immediate_size: match push {
                        PushPlan::Immediates { size } => size,
                        _ => 0,
                    },
                })
        })?;
        Ok((pipeline_layout, PipelineInfo { layouts, push }))
    }

    fn shader_module(&self, label: &str, source: String) -> Result<wgpu::ShaderModule, String> {
        self.scoped(
            &format!("failed to create the {label} shader module"),
            || {
                self.device
                    .create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some(label),
                        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
                    })
            },
        )
    }
}

pub fn create_compute_pipeline(
    device: &Device,
    layout: PipelineLayout,
    label: &str,
    shader: &[u8],
) -> ComputePipeline {
    try_create_compute_pipeline(device, layout, label, shader)
        .unwrap_or_else(|error| panic!("failed to create {label} pipeline: {error}"))
}

pub fn try_create_compute_pipeline(
    device: &Device,
    layout: PipelineLayout,
    label: &str,
    shader: &[u8],
) -> Result<ComputePipeline, String> {
    let shared = &device.shared;
    let entry = shared
        .pipeline_layouts
        .get(layout.0)
        .ok_or("pipeline layout was destroyed")?;
    let mode = shared.push_mode(entry.push, entry.groups.len())?;
    let processed = shader::process(shader_text(shader)?, mode.unwrap_or(PushMode::Immediates))?;
    let (pipeline_layout, info) = shared.build_layout(layout, &[&processed], mode)?;
    let module = shared.shader_module(label, processed.source)?;
    let pipeline = shared.scoped(&format!("failed to create {label} pipeline"), || {
        shared
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            })
    })?;
    Ok(ComputePipeline(
        shared
            .compute_pipelines
            .insert(ComputePipelineEntry { pipeline, info }),
    ))
}

pub fn create_render_pipeline(
    device: &Device,
    targets: PipelineTargets,
    layout: PipelineLayout,
    desc: &RenderPipelineDesc,
) -> RenderPipeline {
    try_create_render_pipeline(device, targets, layout, desc)
        .unwrap_or_else(|e| panic!("failed to create {} pipeline: {e}", desc.label))
}

/// [`create_render_pipeline`] for shaders that may not be valid, such as a
/// shader pack's.
pub fn try_create_render_pipeline(
    device: &Device,
    targets: PipelineTargets,
    layout: PipelineLayout,
    desc: &RenderPipelineDesc,
) -> Result<RenderPipeline, String> {
    let shared = &device.shared;
    let entry = shared
        .pipeline_layouts
        .get(layout.0)
        .ok_or("pipeline layout was destroyed")?;
    let mode = shared.push_mode(entry.push, entry.groups.len())?;
    let push_mode = mode.unwrap_or(PushMode::Immediates);
    let vertex = shader::process(shader_text(desc.vertex_shader)?, push_mode)?;
    let fragment = shader::process(shader_text(desc.fragment_shader)?, push_mode)?;
    let (pipeline_layout, info) = shared.build_layout(layout, &[&vertex, &fragment], mode)?;
    let vertex_module = shared.shader_module(&format!("{}_vertex", desc.label), vertex.source)?;
    let fragment_module =
        shared.shader_module(&format!("{}_fragment", desc.label), fragment.source)?;

    let attributes: Vec<Vec<wgpu::VertexAttribute>> = desc
        .vertex_buffers
        .iter()
        .map(|buffer| {
            buffer
                .attributes
                .iter()
                .map(|attribute| wgpu::VertexAttribute {
                    format: vertex_format(attribute.format),
                    offset: u64::from(attribute.offset),
                    shader_location: attribute.location,
                })
                .collect()
        })
        .collect();
    let buffers: Vec<_> = desc
        .vertex_buffers
        .iter()
        .zip(&attributes)
        .map(|(buffer, attributes)| {
            Some(wgpu::VertexBufferLayout {
                array_stride: u64::from(buffer.stride),
                step_mode: match buffer.step_mode {
                    VertexStepMode::Vertex => wgpu::VertexStepMode::Vertex,
                    VertexStepMode::Instance => wgpu::VertexStepMode::Instance,
                },
                attributes,
            })
        })
        .collect();

    let color_targets: Vec<_> = targets
        .color
        .into_iter()
        .chain(targets.extra_colors.iter().flatten().copied())
        .map(|format| {
            let blendable = shared.features.contains(wgpu::Features::FLOAT32_BLENDABLE)
                || !matches!(
                    format,
                    wgpu::TextureFormat::R32Float
                        | wgpu::TextureFormat::Rg32Float
                        | wgpu::TextureFormat::Rgba32Float
                );
            Some(wgpu::ColorTargetState {
                format,
                blend: desc.blend.filter(|_| blendable).map(blend_state),
                write_mask: wgpu::ColorWrites::ALL,
            })
        })
        .collect();

    let depth_stencil = targets.depth.then(|| {
        let state = desc.depth;
        wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(state.is_some_and(|depth| depth.write)),
            depth_compare: Some(match state.map(|depth| depth.compare) {
                Some(CompareFunction::Less) => wgpu::CompareFunction::Less,
                Some(CompareFunction::LessEqual) => wgpu::CompareFunction::LessEqual,
                None => wgpu::CompareFunction::Always,
            }),
            stencil: wgpu::StencilState::default(),
            bias: state.and_then(|depth| depth.bias).map_or_else(
                wgpu::DepthBiasState::default,
                |bias| wgpu::DepthBiasState {
                    constant: bias.constant,
                    slope_scale: bias.slope,
                    clamp: 0.0,
                },
            ),
        }
    });
    if desc.depth.is_some() && !targets.depth {
        return Err("a depth test needs a depth target".into());
    }

    let pipeline = shared.scoped(&format!("failed to create {} pipeline", desc.label), || {
        shared
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(desc.label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vertex_module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: match desc.topology {
                        Topology::TriangleList => wgpu::PrimitiveTopology::TriangleList,
                        Topology::LineList => wgpu::PrimitiveTopology::LineList,
                    },
                    strip_index_format: None,
                    front_face: match desc.front_face {
                        FrontFace::Ccw => wgpu::FrontFace::Ccw,
                        FrontFace::Cw => wgpu::FrontFace::Cw,
                    },
                    cull_mode: match desc.cull_mode {
                        CullMode::None => None,
                        CullMode::Front => Some(wgpu::Face::Front),
                        CullMode::Back => Some(wgpu::Face::Back),
                    },
                    unclipped_depth: desc.depth_clamp && shared.depth_clip_control,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &fragment_module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &color_targets,
                }),
                multiview_mask: None,
                cache: None,
            })
    })?;
    Ok(RenderPipeline(
        shared
            .render_pipelines
            .insert(RenderPipelineEntry { pipeline, info }),
    ))
}

fn vertex_format(format: VertexFormat) -> wgpu::VertexFormat {
    match format {
        VertexFormat::Float32 => wgpu::VertexFormat::Float32,
        VertexFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
        VertexFormat::Float32x3 => wgpu::VertexFormat::Float32x3,
        VertexFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
        VertexFormat::Sint32x3 => wgpu::VertexFormat::Sint32x3,
        VertexFormat::Uint32 => wgpu::VertexFormat::Uint32,
        VertexFormat::Sint16x2 => wgpu::VertexFormat::Sint16x2,
        VertexFormat::Uint16 => wgpu::VertexFormat::Uint16,
        VertexFormat::Unorm16 => wgpu::VertexFormat::Unorm16,
        VertexFormat::Unorm16x2 => wgpu::VertexFormat::Unorm16x2,
        VertexFormat::Uint8x2 => wgpu::VertexFormat::Uint8x2,
        VertexFormat::Unorm8x4 => wgpu::VertexFormat::Unorm8x4,
        VertexFormat::Snorm8x4 => wgpu::VertexFormat::Snorm8x4,
    }
}

fn blend_factor(factor: BlendFactor) -> wgpu::BlendFactor {
    match factor {
        BlendFactor::Zero => wgpu::BlendFactor::Zero,
        BlendFactor::One => wgpu::BlendFactor::One,
        BlendFactor::SrcColor => wgpu::BlendFactor::Src,
        BlendFactor::OneMinusSrcColor => wgpu::BlendFactor::OneMinusSrc,
        BlendFactor::DstColor => wgpu::BlendFactor::Dst,
        BlendFactor::OneMinusDstColor => wgpu::BlendFactor::OneMinusDst,
        BlendFactor::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
        BlendFactor::OneMinusSrcAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
    }
}

fn blend_state(blend: BlendState) -> wgpu::BlendState {
    let component = |component: rhi::BlendComponent| wgpu::BlendComponent {
        src_factor: blend_factor(component.src),
        dst_factor: blend_factor(component.dst),
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState {
        color: component(blend.color),
        alpha: component(blend.alpha),
    }
}
