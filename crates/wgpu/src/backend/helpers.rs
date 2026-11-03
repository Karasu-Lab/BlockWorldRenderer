use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::Shared;
use super::resource::lock;

const SHADER: &str = r#"
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

override clear_r: f32 = 0.0;
override clear_g: f32 = 0.0;
override clear_b: f32 = 0.0;
override clear_a: f32 = 0.0;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> Vertex {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let corner = corners[index];
    var out: Vertex;
    out.position = vec4<f32>(corner, 0.0, 1.0);
    out.uv = vec2<f32>((corner.x + 1.0) * 0.5, (1.0 - corner.y) * 0.5);
    return out;
}

@fragment
fn blit_main(in: Vertex) -> @location(0) vec4<f32> {
    return textureSampleLevel(source, source_sampler, in.uv, 0.0);
}

@fragment
fn clear_color_main(in: Vertex) -> @location(0) vec4<f32> {
    return vec4<f32>(clear_r, clear_g, clear_b, clear_a);
}

struct DepthOut {
    @builtin(frag_depth) depth: f32,
}

@fragment
fn clear_depth_main(in: Vertex) -> DepthOut {
    var out: DepthOut;
    out.depth = 1.0;
    return out;
}
"#;

/// What a mid-pass clear draws over.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct ClearKey {
    pub colors: Vec<Option<wgpu::TextureFormat>>,
    pub depth: bool,
    pub kind: ClearKind,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum ClearKind {
    Depth,
    /// Colour 0, by blend constant on formats that blend.
    Color,
    /// Colour 0, baked into the pipeline for formats that don't blend.
    ColorBaked([u32; 4]),
}

pub(crate) struct Blit {
    pub pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
}

/// Pipelines the encoder draws with where the RHI has a command wgpu lacks:
/// scaling copies and clears inside a pass.
#[derive(Default)]
pub(crate) struct Helpers {
    module: OnceLock<wgpu::ShaderModule>,
    sampler: OnceLock<wgpu::Sampler>,
    blits: Mutex<HashMap<wgpu::TextureFormat, Arc<Blit>>>,
    clears: Mutex<HashMap<ClearKey, wgpu::RenderPipeline>>,
}

impl Helpers {
    fn module(&self, shared: &Shared) -> &wgpu::ShaderModule {
        self.module.get_or_init(|| {
            shared
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("rhi_helpers"),
                    source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
                })
        })
    }

    pub fn sampler(&self, shared: &Shared) -> &wgpu::Sampler {
        self.sampler.get_or_init(|| {
            shared.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("rhi_blit"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        })
    }

    /// The pipeline blitting into `format` targets.
    pub fn blit(&self, shared: &Shared, format: wgpu::TextureFormat) -> Arc<Blit> {
        if let Some(found) = lock(&self.blits).get(&format) {
            return found.clone();
        }
        let layout = shared
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rhi_blit"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let pipeline_layout =
            shared
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("rhi_blit"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
        let module = self.module(shared);
        let pipeline = shared
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("rhi_blit"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vertex_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("blit_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        let blit = Arc::new(Blit { pipeline, layout });
        lock(&self.blits).insert(format, blit.clone());
        blit
    }

    /// The pipeline clearing inside a pass with these targets.
    pub fn clear(&self, shared: &Shared, key: &ClearKey) -> wgpu::RenderPipeline {
        if let Some(found) = lock(&self.clears).get(key) {
            return found.clone();
        }
        let pipeline_layout =
            shared
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("rhi_clear"),
                    bind_group_layouts: &[],
                    immediate_size: 0,
                });
        let module = self.module(shared);
        let colors = matches!(key.kind, ClearKind::Color | ClearKind::ColorBaked(_));
        let targets: Vec<_> = key
            .colors
            .iter()
            .enumerate()
            .map(|(slot, format)| {
                format.map(|format| wgpu::ColorTargetState {
                    format,
                    blend: (slot == 0 && matches!(key.kind, ClearKind::Color)).then_some(
                        wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::Constant,
                                dst_factor: wgpu::BlendFactor::Zero,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::Constant,
                                dst_factor: wgpu::BlendFactor::Zero,
                                operation: wgpu::BlendOperation::Add,
                            },
                        },
                    ),
                    write_mask: if colors && slot == 0 {
                        wgpu::ColorWrites::ALL
                    } else {
                        wgpu::ColorWrites::empty()
                    },
                })
            })
            .collect();
        let baked = match key.kind {
            ClearKind::ColorBaked(bits) => bits.map(|bits| f64::from(f32::from_bits(bits))),
            // The blend constant scales a source of one.
            ClearKind::Color => [1.0; 4],
            ClearKind::Depth => [0.0; 4],
        };
        let constants = [
            ("clear_r", baked[0]),
            ("clear_g", baked[1]),
            ("clear_b", baked[2]),
            ("clear_a", baked[3]),
        ];
        let pipeline = shared
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("rhi_clear"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vertex_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: key.depth.then(|| wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(matches!(key.kind, ClearKind::Depth)),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(if colors {
                        "clear_color_main"
                    } else {
                        "clear_depth_main"
                    }),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            });
        lock(&self.clears).insert(key.clone(), pipeline.clone());
        pipeline
    }
}
