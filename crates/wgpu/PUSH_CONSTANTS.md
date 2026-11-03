# Shader contract of the wgpu backend

The Vulkan backend takes SPIR-V; the wgpu backend takes **WGSL as UTF-8 bytes**
(`rhi::Shader`), one module per stage, with the entry point named `main`.
Whoever turns the client's GLSL into shaders (the build script) hands the
backend these modules; the rules below are what the backend relies on.

## Push constants

A push constant block is declared in the module as naga writes SPIR-V
`PushConstant` storage: `var<immediate> push: Block;`. Nothing else is needed;
the backend decides how it reaches the GPU when a pipeline is created.

- **Device has immediate data** (`Features::IMMEDIATES`, and the block fits
  `max_immediate_size`): the module is kept as is, and
  `RenderPassEncoder::set_push_constants` calls `set_immediates`.
- **Otherwise** (browsers without immediate data): the backend rewrites the
  global to `var<uniform>` at `@group(N) @binding(0)`, where **`N` is the
  number of bind group layouts in the pipeline layout**. Data is written, per
  draw, to a slot of a ring of uniform memory and the group is bound with a
  dynamic offset:
  - slots are aligned to `min_uniform_buffer_offset_alignment` (256 bytes on
    most devices) and span the block size rounded up to 16 bytes;
  - the ring is 4 MiB chunks, grown on demand, and reset by `Frames::begin`;
  - the bytes are written by the queue, so they are ordered with the commands
    that read them and one ring serves every frame in flight.

  `create_*_pipeline` returns `Err` when `N + 1` exceeds the device's
  `max_bind_groups` (WebGPU's default is 4), i.e. a layout of four bind groups
  with push constants only works where immediate data exists.

`set_push_constants` may update a sub-range (`offset`, `data`); the backend
keeps the whole block and applies it before the next draw.

## Clip space

Vulkan's clip space has +Y down; WebGPU's has +Y up. The client's matrices are
written for Vulkan, so every vertex shader must negate `position.y` of its
`@builtin(position)` output (naga's SPIR-V front end does this with
`adjust_coordinate_space`, minus the depth remap, which both APIs share as
0..1). With that, framebuffer coordinates and triangle winding come out as
they do under Vulkan, and `FrontFace`/`CullMode` are passed through unchanged.

## Bindings

The backend reads the module to learn the type of each binding — buffer
(uniform, read-only or writable storage), texture (sample type, dimension,
depth) and sampler (filtering or comparison) — and builds the wgpu bind group
layouts from it, because the RHI's `BindGroupLayoutEntry` only says "texture"
or "sampler". Bindings the module does not declare get a default type and
stand-in resources, which Vulkan lets stay unwritten.

## Features the backend asks for

Required: `INDIRECT_FIRST_INSTANCE` (the chunk pass indexes section meta by
`firstInstance`). Used when present: `IMMEDIATES`, `DEPTH_CLIP_CONTROL`
(`RenderPipelineDesc::depth_clamp`), `FLOAT32_BLENDABLE`, `FLOAT32_FILTERABLE`,
`TEXTURE_FORMAT_16BIT_NORM` (needed for `TextureFormat::Rgba16Unorm`, which
shader packs use), `MULTI_DRAW_INDIRECT_COUNT` (`IndirectDrawFeatures::count`).
