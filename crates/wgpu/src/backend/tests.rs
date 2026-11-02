use crate::backend::{
    self, BindingResource, Buffer, ColorAttachment, Device, RenderPassDesc, Texture,
};
use crate::rhi::{
    BindGroupLayoutEntry, Blit, BufferCopy, BufferUsages, Extent, LoadOp, MemoryLocation,
    PushConstants, Rect, RenderPipelineDesc, SamplerDesc, ShaderStages, StoreOp, TexelCopy,
    TextureDesc, TextureFormat, TextureRange, TextureUsages,
};

const TARGET: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn open(device: Result<Device, backend::ContextError>) -> Option<Device> {
    match device {
        Ok(device) => Some(device),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error})");
            None
        }
    }
}

fn device() -> Option<Device> {
    open(Device::headless())
}

/// A device whose push constants go through the uniform ring.
fn emulated_device() -> Option<Device> {
    open(Device::headless_emulated_push())
}

const FULLSCREEN: &str = r#"
@vertex
fn main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(corners[index], 0.5, 1.0);
}
"#;

const PUSH_COLOR: &str = r#"
struct Push { color: vec4<f32> }
var<immediate> push: Push;

@fragment
fn main() -> @location(0) vec4<f32> {
    return push.color;
}
"#;

const SAMPLED: &str = r#"
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@fragment
fn main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / 4.0;
    return textureSample(tex, samp, uv) * tint.color;
}
"#;

const FILL: &str = r#"
@group(0) @binding(0) var<storage, read_write> data: array<u32>;

@compute @workgroup_size(4)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    data[id.x] = id.x * 3u + 1u;
}
"#;

fn read_buffer(device: &Device, source: &Buffer, size: u64) -> Vec<u8> {
    let mut readback = Buffer::new(
        device,
        "readback",
        size,
        BufferUsages::COPY_DST,
        MemoryLocation::Readback,
    );
    device.submit(|encoder| {
        encoder.copy_buffer(
            source.handle(),
            readback.handle(),
            &[BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size,
            }],
        );
    });
    let bytes = readback.read(|bytes| bytes.to_vec());
    readback.destroy(device);
    bytes
}

fn read_texture(device: &Device, texture: &Texture, width: u32, height: u32, mip: u32) -> Vec<u8> {
    let mut readback = Buffer::new(
        device,
        "readback",
        u64::from(width * height * 4),
        BufferUsages::COPY_DST,
        MemoryLocation::Readback,
    );
    device.submit(|encoder| {
        encoder.copy_texture_to_buffer(
            texture.handle(),
            readback.handle(),
            &[TexelCopy {
                buffer_offset: 0,
                mip_level: mip,
                layer: 0,
                origin: [0, 0],
                width,
                height,
            }],
        );
    });
    let bytes = readback.read(|bytes| bytes.to_vec());
    readback.destroy(device);
    bytes
}

fn target(device: &Device, size: u32) -> Texture {
    Texture::new(
        device,
        &TextureDesc::new(
            "target",
            size,
            size,
            TextureFormat::Rgba8Unorm,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        ),
    )
}

fn pass_desc(texture: &Texture, size: u32, load: LoadOp<[f32; 4]>) -> RenderPassDesc {
    RenderPassDesc {
        color: Some(ColorAttachment {
            texture: texture.handle(),
            format: TARGET,
            load,
            store: StoreOp::Store,
        }),
        extra_colors: Vec::new(),
        depth: None,
        extent: Extent {
            width: size,
            height: size,
        },
    }
}

#[test]
fn host_writes_reach_the_gpu_at_any_alignment() {
    let Some(device) = device() else { return };
    let mut buffer = Buffer::new(
        &device,
        "host",
        16,
        BufferUsages::COPY_SRC,
        MemoryLocation::Host,
    );
    buffer.write(&[1, 2, 3, 4, 5, 6, 7, 8]);
    buffer.write_at(3, &[9, 9, 9]);
    let bytes = read_buffer(&device, &buffer, 16);
    assert_eq!(&bytes[..8], &[1, 2, 3, 9, 9, 9, 7, 8]);
    buffer.zero();
    assert_eq!(read_buffer(&device, &buffer, 16), vec![0; 16]);
}

#[test]
fn unaligned_texture_rows_round_trip() {
    let Some(device) = device() else { return };
    let (width, height) = (3u32, 5u32);
    let texels: Vec<u8> = (0..width * height * 4).map(|i| i as u8).collect();
    let staging = Buffer::with_data(&device, "staging", &texels, BufferUsages::COPY_SRC);
    let texture = Texture::new(
        &device,
        &TextureDesc::new(
            "upload",
            width,
            height,
            TextureFormat::Rgba8Unorm,
            TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING,
        ),
    );
    device.submit(|encoder| {
        encoder.upload_texture(
            staging.handle(),
            texture.handle(),
            TextureRange::BASE,
            &[TexelCopy {
                buffer_offset: 0,
                mip_level: 0,
                layer: 0,
                origin: [0, 0],
                width,
                height,
            }],
        );
    });
    assert_eq!(read_texture(&device, &texture, width, height, 0), texels);
}

#[test]
fn push_constants_reach_the_fragment_shader() {
    let Some(device) = device() else { return };
    check_push_constants(&device);
}

#[test]
fn emulated_push_constants_reach_the_fragment_shader() {
    let Some(device) = emulated_device() else {
        return;
    };
    check_push_constants(&device);
}

fn check_push_constants(device: &Device) {
    let layout = backend::create_pipeline_layout(
        device,
        &[],
        Some(PushConstants {
            visibility: ShaderStages::FRAGMENT,
            size: 16,
        }),
    );
    let pipeline = backend::create_render_pipeline(
        device,
        backend::PipelineTargets::new(TARGET, false),
        layout,
        &RenderPipelineDesc::new("push", FULLSCREEN.as_bytes(), PUSH_COLOR.as_bytes(), &[]),
    );
    let texture = target(device, 4);
    device.submit(|encoder| {
        let mut pass =
            encoder.begin_render_pass(device, &pass_desc(&texture, 4, LoadOp::Clear([0.0; 4])));
        pass.set_pipeline(&pipeline);
        pass.set_push_constants(
            ShaderStages::FRAGMENT,
            0,
            bytemuck_bytes(&[1.0f32, 0.0, 0.0, 1.0]),
        );
        pass.draw(3, 1, 0, 0);
        pass.set_push_constants(
            ShaderStages::FRAGMENT,
            0,
            bytemuck_bytes(&[0.0f32, 0.0, 1.0, 1.0]),
        );
        pass.set_scissor_rect(Rect {
            x: 2,
            y: 0,
            width: 2,
            height: 4,
        });
        pass.draw(3, 1, 0, 0);
        pass.end();
    });
    let pixels = read_texture(device, &texture, 4, 4, 0);
    assert_eq!(&pixels[..4], &[255, 0, 0, 255], "left half is red");
    assert_eq!(&pixels[12..16], &[0, 0, 255, 255], "right half is blue");
}

fn bytemuck_bytes(values: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding or invalid bit patterns.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), size_of_val(values)) }
}

#[test]
fn bind_groups_follow_the_shader_declared_types() {
    let Some(device) = device() else { return };
    let group_layout = backend::create_bind_group_layout(
        &device,
        &[
            BindGroupLayoutEntry::uniform(ShaderStages::FRAGMENT),
            BindGroupLayoutEntry::texture(ShaderStages::FRAGMENT),
            BindGroupLayoutEntry::sampler(ShaderStages::FRAGMENT),
        ],
    );
    let layout = backend::create_pipeline_layout(&device, &[group_layout], None);
    let pipeline = backend::create_render_pipeline(
        &device,
        backend::PipelineTargets::new(TARGET, false),
        layout,
        &RenderPipelineDesc::new("sampled", FULLSCREEN.as_bytes(), SAMPLED.as_bytes(), &[]),
    );

    let tint = Buffer::with_data(
        &device,
        "tint",
        bytemuck_bytes(&[0.5, 1.0, 1.0, 1.0]),
        BufferUsages::UNIFORM,
    );
    let source = Texture::new(
        &device,
        &TextureDesc::new(
            "source",
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
        ),
    );
    let staging = Buffer::with_data(&device, "px", &[200, 100, 50, 255], BufferUsages::COPY_SRC);
    device.submit(|encoder| {
        encoder.upload_texture(
            staging.handle(),
            source.handle(),
            TextureRange::BASE,
            &[TexelCopy {
                buffer_offset: 0,
                mip_level: 0,
                layer: 0,
                origin: [0, 0],
                width: 1,
                height: 1,
            }],
        );
    });
    let sampler = backend::create_sampler(&device, &SamplerDesc::NEAREST);
    let group = device.create_bind_group(
        group_layout,
        &[
            (0, BindingResource::uniform(&tint, 16)),
            (1, BindingResource::Texture(source.handle())),
            (2, BindingResource::Sampler(sampler)),
        ],
    );

    let texture = target(&device, 4);
    device.submit(|encoder| {
        let mut pass =
            encoder.begin_render_pass(&device, &pass_desc(&texture, 4, LoadOp::Clear([0.0; 4])));
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, group);
        pass.draw(3, 1, 0, 0);
        pass.end();
    });
    let pixels = read_texture(&device, &texture, 4, 4, 0);
    assert_eq!(pixels[0], 100, "red is halved by the tint");
    assert_eq!(&pixels[1..4], &[100, 50, 255]);
}

#[test]
fn clears_inside_a_pass_keep_the_pass_state() {
    let Some(device) = device() else { return };
    check_clears(&device);
}

#[test]
fn clears_inside_a_pass_keep_emulated_push_constants() {
    let Some(device) = emulated_device() else {
        return;
    };
    check_clears(&device);
}

fn check_clears(device: &Device) {
    let layout = backend::create_pipeline_layout(
        device,
        &[],
        Some(PushConstants {
            visibility: ShaderStages::FRAGMENT,
            size: 16,
        }),
    );
    let pipeline = backend::create_render_pipeline(
        device,
        backend::PipelineTargets::new(TARGET, false),
        layout,
        &RenderPipelineDesc::new("push", FULLSCREEN.as_bytes(), PUSH_COLOR.as_bytes(), &[]),
    );
    let texture = target(device, 4);
    device.submit(|encoder| {
        let mut pass =
            encoder.begin_render_pass(device, &pass_desc(&texture, 4, LoadOp::Clear([0.0; 4])));
        pass.set_pipeline(&pipeline);
        pass.set_push_constants(
            ShaderStages::FRAGMENT,
            0,
            bytemuck_bytes(&[1.0, 0.0, 0.0, 1.0]),
        );
        pass.draw(3, 1, 0, 0);
        pass.clear_color(
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 4,
            },
            [0.0, 1.0, 0.0, 1.0],
        );
        pass.set_scissor_rect(Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 1,
        });
        pass.draw(3, 1, 0, 0);
        pass.end();
    });
    let pixels = read_texture(device, &texture, 4, 4, 0);
    let pixel = |x: usize, y: usize| &pixels[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
    assert_eq!(pixel(0, 2), &[0, 255, 0, 255], "cleared region");
    assert_eq!(pixel(3, 2), &[255, 0, 0, 255], "untouched region");
    assert_eq!(
        pixel(0, 0),
        &[255, 0, 0, 255],
        "drawn again with the same push constants"
    );
}

#[test]
fn blit_scales_a_level_into_the_next() {
    let Some(device) = device() else { return };
    let texture = Texture::new(
        &device,
        &TextureDesc {
            mip_levels: 2,
            ..TextureDesc::new(
                "mips",
                2,
                2,
                TextureFormat::Rgba8Unorm,
                TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING,
            )
        },
    );
    let texels = [
        255u8, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
    ];
    let staging = Buffer::with_data(&device, "px", &texels, BufferUsages::COPY_SRC);
    device.submit(|encoder| {
        encoder.upload_texture(
            staging.handle(),
            texture.handle(),
            TextureRange::BASE,
            &[TexelCopy {
                buffer_offset: 0,
                mip_level: 0,
                layer: 0,
                origin: [0, 0],
                width: 2,
                height: 2,
            }],
        );
        encoder.blit(
            texture.handle(),
            texture.handle(),
            &Blit {
                src_level: 0,
                src_size: [2, 2],
                dst_level: 1,
                dst_size: [1, 1],
                layers: 1,
            },
        );
    });
    assert_eq!(
        read_texture(&device, &texture, 1, 1, 1),
        vec![255, 0, 0, 255]
    );
}

#[test]
fn compute_writes_storage_buffers() {
    let Some(device) = device() else { return };
    let group_layout = backend::create_bind_group_layout(
        &device,
        &[BindGroupLayoutEntry::storage(ShaderStages::COMPUTE)],
    );
    let layout = backend::create_pipeline_layout(&device, &[group_layout], None);
    let pipeline = backend::create_compute_pipeline(&device, layout, "fill", FILL.as_bytes());
    let storage = Buffer::new(
        &device,
        "storage",
        16,
        BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        MemoryLocation::Device,
    );
    let group =
        device.create_bind_group(group_layout, &[(0, BindingResource::storage(&storage, 16))]);
    device.submit(|encoder| encoder.dispatch(&pipeline, group, [1, 1, 1]));
    let bytes = read_buffer(&device, &storage, 16);
    let words: Vec<u32> = bytes
        .chunks(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    assert_eq!(words, vec![1, 4, 7, 10]);
}

#[test]
fn depth_clears_and_copies_run() {
    let Some(device) = device() else { return };
    let depth = |label| {
        Texture::new(
            &device,
            &TextureDesc::new(
                label,
                4,
                4,
                TextureFormat::Depth32Float,
                TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::COPY_SRC
                    | TextureUsages::COPY_DST
                    | TextureUsages::TEXTURE_BINDING,
            ),
        )
    };
    let (source, copy) = (depth("source"), depth("copy"));
    device.submit(|encoder| {
        encoder.clear_depth(source.handle(), 0.25);
        encoder.copy_depth(
            source.handle(),
            copy.handle(),
            Extent {
                width: 4,
                height: 4,
            },
        );
    });
}

const VERTEX_INPUT: &str = r#"
struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) shade: f32,
}

@vertex
fn main(@location(0) position: vec2<f32>, @location(1) shade: f32) -> Out {
    var out: Out;
    out.position = vec4<f32>(position, 0.5, 1.0);
    out.shade = shade;
    return out;
}
"#;

const SHADE: &str = r#"
@fragment
fn main(@location(0) shade: f32) -> @location(0) vec4<f32> {
    return vec4<f32>(shade, shade, shade, 1.0);
}
"#;

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn indexed_indirect_draws_with_depth() {
    use crate::rhi::{
        DepthAttachment, DepthState, VertexAttribute, VertexBufferLayout, VertexFormat,
        VertexStepMode,
    };
    let Some(device) = device() else { return };
    let attributes = [
        VertexAttribute {
            location: 0,
            format: VertexFormat::Float32x2,
            offset: 0,
        },
        VertexAttribute {
            location: 1,
            format: VertexFormat::Float32,
            offset: 8,
        },
    ];
    let vertex_buffers = [VertexBufferLayout {
        stride: 12,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    }];
    let layout = backend::create_pipeline_layout(&device, &[], None);
    let pipeline = backend::create_render_pipeline(
        &device,
        backend::PipelineTargets::new(TARGET, true),
        layout,
        &RenderPipelineDesc {
            depth: Some(DepthState::WRITE),
            ..RenderPipelineDesc::new(
                "vertex",
                VERTEX_INPUT.as_bytes(),
                SHADE.as_bytes(),
                &vertex_buffers,
            )
        },
    );
    // A full-screen triangle of shade 0.5.
    let vertices = Buffer::with_data(
        &device,
        "vertices",
        &floats(&[-1.0, -1.0, 0.5, 3.0, -1.0, 0.5, -1.0, 3.0, 0.5]),
        BufferUsages::VERTEX,
    );
    let indices = Buffer::with_data(
        &device,
        "indices",
        &[0u32, 1, 2]
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect::<Vec<_>>(),
        BufferUsages::INDEX,
    );
    // index_count, instance_count, first_index, base_vertex, first_instance
    let args: Vec<u8> = [3u32, 1, 0, 0, 0, 3, 1, 0, 0, 0]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    let indirect = Buffer::with_data(&device, "indirect", &args, BufferUsages::INDIRECT);
    let depth = Texture::new(
        &device,
        &TextureDesc::swapchain_depth(Extent {
            width: 4,
            height: 4,
        }),
    );
    let texture = target(&device, 4);
    device.submit(|encoder| {
        let mut desc = pass_desc(&texture, 4, LoadOp::Clear([0.0; 4]));
        desc.depth = Some(DepthAttachment::cleared(depth.handle()));
        let mut pass = encoder.begin_render_pass(&device, &desc);
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertices.handle());
        pass.set_index_buffer(indices.handle());
        pass.draw_indexed_indirect(indirect.handle(), 0, 2, 20);
        pass.end();
    });
    let pixels = read_texture(&device, &texture, 4, 4, 0);
    assert!(
        matches!(pixels[0], 127 | 128),
        "half shade, got {}",
        pixels[0]
    );
    assert_eq!(pixels[3], 255);
}
