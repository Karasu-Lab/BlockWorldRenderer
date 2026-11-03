use std::sync::{Arc, Mutex};

use super::bind::BindGroup;
use super::helpers::{ClearKey, ClearKind};
use super::pipeline::{ComputePipeline, RenderPipeline, RenderPipelineEntry};
use super::push::PushPlan;
use super::resource::{BufferHandle, TextureEntry, TextureHandle, lock};
use super::{ColorAttachment, Device, RenderPassDesc, Shared, TextureTransition};
use crate::rhi::{
    Blit, BufferCopy, BufferUse, Extent, LoadOp, Rect, ShaderStages, StoreOp, TexelCopy,
    TextureRange,
};

fn color(rgba: [f32; 4]) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(rgba[0]),
        g: f64::from(rgba[1]),
        b: f64::from(rgba[2]),
        a: f64::from(rgba[3]),
    }
}

fn store_op(store: StoreOp) -> wgpu::StoreOp {
    match store {
        StoreOp::Store => wgpu::StoreOp::Store,
        StoreOp::Discard => wgpu::StoreOp::Discard,
    }
}

#[derive(Clone)]
pub struct CommandEncoder {
    inner: Arc<EncoderInner>,
}

struct EncoderInner {
    shared: Arc<Shared>,
    encoder: Mutex<Option<wgpu::CommandEncoder>>,
}

impl CommandEncoder {
    pub(crate) fn new(shared: &Arc<Shared>) -> Self {
        Self {
            inner: Arc::new(EncoderInner {
                shared: shared.clone(),
                encoder: Mutex::new(Some(
                    shared
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None }),
                )),
            }),
        }
    }

    pub(crate) fn finish(&self) -> wgpu::CommandBuffer {
        lock(&self.inner.encoder)
            .take()
            .expect("the encoder was already finished")
            .finish()
    }

    fn with<R>(&self, record: impl FnOnce(&mut wgpu::CommandEncoder, &Shared) -> R) -> R {
        let mut encoder = lock(&self.inner.encoder);
        record(
            encoder.as_mut().expect("the encoder was already finished"),
            &self.inner.shared,
        )
    }

    fn texture(&self, handle: TextureHandle) -> Arc<TextureEntry> {
        self.inner
            .shared
            .textures
            .get(handle.0)
            .expect("texture was destroyed")
    }

    fn buffer(&self, handle: BufferHandle) -> wgpu::Buffer {
        self.inner
            .shared
            .buffers
            .get(handle.0)
            .expect("buffer was destroyed")
            .buffer
            .clone()
    }

    pub fn transition_textures(&self, _transitions: &[TextureTransition]) {}

    pub fn transition_buffers(&self, _from: BufferUse, _to: BufferUse) {}

    pub fn copy_buffer(&self, src: BufferHandle, dst: BufferHandle, copies: &[BufferCopy]) {
        let mapping = self
            .inner
            .shared
            .buffers
            .get(dst.0)
            .and_then(|entry| entry.readback.clone())
            .is_some_and(|readback| readback.is_mapping());
        if mapping {
            return;
        }
        let (src, dst) = (self.buffer(src), self.buffer(dst));
        self.with(|encoder, _| {
            for copy in copies {
                encoder.copy_buffer_to_buffer(
                    &src,
                    copy.src_offset,
                    &dst,
                    copy.dst_offset,
                    Some(copy.size),
                );
            }
        });
    }

    pub fn copy_buffer_to_texture(
        &self,
        buffer: BufferHandle,
        image: TextureHandle,
        copies: &[TexelCopy],
    ) {
        let buffer = self.buffer(buffer);
        let image = self.texture(image);
        self.with(|encoder, _| {
            for_each_row_block(&image, copies, |copy, row_offset, rows, layout_rows| {
                encoder.copy_buffer_to_texture(
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: layout_rows,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &image.texture,
                        mip_level: copy.mip_level,
                        origin: wgpu::Origin3d {
                            x: copy.origin[0],
                            y: copy.origin[1] + row_offset,
                            z: copy.layer,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: copy.width,
                        height: rows,
                        depth_or_array_layers: 1,
                    },
                );
            });
        });
    }

    pub fn copy_texture_to_buffer(
        &self,
        image: TextureHandle,
        buffer: BufferHandle,
        copies: &[TexelCopy],
    ) {
        let buffer = self.buffer(buffer);
        let image = self.texture(image);
        self.with(|encoder, _| {
            for_each_row_block(&image, copies, |copy, row_offset, rows, layout_rows| {
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &image.texture,
                        mip_level: copy.mip_level,
                        origin: wgpu::Origin3d {
                            x: copy.origin[0],
                            y: copy.origin[1] + row_offset,
                            z: copy.layer,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: layout_rows,
                    },
                    wgpu::Extent3d {
                        width: copy.width,
                        height: rows,
                        depth_or_array_layers: 1,
                    },
                );
            });
        });
    }

    pub fn blit(&self, src: TextureHandle, dst: TextureHandle, blit: &Blit) {
        let (src, dst) = (self.texture(src), self.texture(dst));
        self.with(|encoder, shared| {
            let helper = shared.helpers.blit(shared, dst.format);
            for layer in 0..blit.layers {
                let source_view = src.subview(blit.src_level, layer);
                let target_view = dst.subview(blit.dst_level, layer);
                let group = shared.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rhi_blit"),
                    layout: &helper.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&source_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(
                                shared.helpers.sampler(shared),
                            ),
                        },
                    ],
                });
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("rhi_blit"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&helper.pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.set_viewport(
                    0.0,
                    0.0,
                    blit.dst_size[0] as f32,
                    blit.dst_size[1] as f32,
                    0.0,
                    1.0,
                );
                pass.draw(0..3, 0..1);
            }
        });
    }

    pub fn clear_texture(&self, image: TextureHandle, range: TextureRange, rgba: [f32; 4]) {
        let image = self.texture(image);
        self.with(|encoder, _| {
            for mip in range.first_mip..range.first_mip + range.mips {
                for layer in range.first_layer..range.first_layer + range.layers {
                    let view = image.subview(mip, layer);
                    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("rhi_clear_texture"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(color(rgba)),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    }));
                }
            }
        });
    }

    pub fn clear_depth(&self, image: TextureHandle, depth: f32) {
        let image = self.texture(image);
        let view = image.attachment_view();
        self.with(|encoder, _| {
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rhi_clear_depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(depth),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
        });
    }

    pub fn copy_depth(&self, src: TextureHandle, dst: TextureHandle, extent: Extent) {
        let (src, dst) = (self.texture(src), self.texture(dst));
        self.with(|encoder, _| {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &src.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::DepthOnly,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &dst.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::DepthOnly,
                },
                wgpu::Extent3d {
                    width: extent.width,
                    height: extent.height,
                    depth_or_array_layers: 1,
                },
            );
        });
    }

    pub fn upload_texture(
        &self,
        staging: BufferHandle,
        image: TextureHandle,
        _range: TextureRange,
        copies: &[TexelCopy],
    ) {
        self.copy_buffer_to_texture(staging, image, copies);
    }

    pub fn begin_render_pass(&self, _device: &Device, desc: &RenderPassDesc) -> RenderPassEncoder {
        let shared = self.inner.shared.clone();
        let colors: Vec<Option<&ColorAttachment>> = std::iter::once(desc.color.as_ref())
            .chain(desc.extra_colors.iter().map(Some))
            .collect();
        let views: Vec<Option<wgpu::TextureView>> = colors
            .iter()
            .map(|attachment| {
                attachment.map(|attachment| {
                    shared
                        .textures
                        .get(attachment.texture.0)
                        .expect("pass target was destroyed")
                        .attachment_view()
                })
            })
            .collect();
        let formats: Vec<Option<wgpu::TextureFormat>> = colors
            .iter()
            .map(|attachment| attachment.map(|attachment| attachment.format))
            .collect();
        let color_attachments: Vec<_> = colors
            .iter()
            .zip(&views)
            .map(|(attachment, view)| {
                let (attachment, view) = (attachment.as_ref()?, view.as_ref()?);
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: match attachment.load {
                            LoadOp::Clear(rgba) => wgpu::LoadOp::Clear(color(rgba)),
                            LoadOp::Load => wgpu::LoadOp::Load,
                        },
                        store: store_op(attachment.store),
                    },
                })
            })
            .collect();
        let depth_view = desc.depth.map(|depth| {
            shared
                .textures
                .get(depth.texture.0)
                .expect("pass depth target was destroyed")
                .attachment_view()
        });
        let depth_stencil_attachment = desc.depth.zip(depth_view.as_ref()).map(|(depth, view)| {
            wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: match depth.load {
                        LoadOp::Clear(value) => wgpu::LoadOp::Clear(value),
                        LoadOp::Load => wgpu::LoadOp::Load,
                    },
                    store: store_op(depth.store),
                }),
                stencil_ops: None,
            }
        });
        let pass = self.with(|encoder, _| {
            encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &color_attachments,
                    depth_stencil_attachment,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime()
        });
        let full = Rect::covering(desc.extent);
        RenderPassEncoder {
            shared,
            inner: Mutex::new(PassInner {
                pass: Some(pass),
                state: PassState {
                    pipeline: None,
                    groups: Vec::new(),
                    vertex: Vec::new(),
                    index: None,
                    viewport: full,
                    scissor: full,
                    push: Vec::new(),
                    push_dirty: false,
                },
                formats,
                depth: desc.depth.is_some(),
                full,
            }),
        }
    }

    pub fn dispatch(&self, pipeline: &ComputePipeline, group: BindGroup, groups: [u32; 3]) {
        let shared = &self.inner.shared;
        let pipeline = shared
            .compute_pipelines
            .get(pipeline.0)
            .expect("pipeline was destroyed");
        let group_entry = shared
            .bind_groups
            .get(group.0)
            .expect("bind group was destroyed");
        let layout = pipeline.info.layouts.first();
        let bind_group = layout.map(|layout| group_entry.get(shared, layout));
        self.with(|encoder, _| {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline.pipeline);
            match bind_group {
                Some(Ok(bind_group)) => pass.set_bind_group(0, &bind_group, &[]),
                Some(Err(error)) => {
                    shared.log_error(&error);
                    return;
                }
                None => {}
            }
            pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
        });
    }
}

fn for_each_row_block(
    image: &TextureEntry,
    copies: &[TexelCopy],
    mut each: impl FnMut(&TexelCopy, u32, u32, wgpu::TexelCopyBufferLayout),
) {
    let texel = image.format.block_copy_size(None).unwrap_or(4);
    for copy in copies {
        let row_bytes = copy.width * texel;
        if copy.height <= 1 {
            each(
                copy,
                0,
                copy.height.max(1),
                wgpu::TexelCopyBufferLayout {
                    offset: copy.buffer_offset,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
            );
        } else if row_bytes.is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) {
            each(
                copy,
                0,
                copy.height,
                wgpu::TexelCopyBufferLayout {
                    offset: copy.buffer_offset,
                    bytes_per_row: Some(row_bytes),
                    rows_per_image: Some(copy.height),
                },
            );
        } else {
            for row in 0..copy.height {
                each(
                    copy,
                    row,
                    1,
                    wgpu::TexelCopyBufferLayout {
                        offset: copy.buffer_offset + u64::from(row) * u64::from(row_bytes),
                        bytes_per_row: None,
                        rows_per_image: None,
                    },
                );
            }
        }
    }
}

struct PassState {
    pipeline: Option<Arc<RenderPipelineEntry>>,
    groups: Vec<Option<(BindGroup, Option<u64>)>>,
    vertex: Vec<Option<(BufferHandle, u64)>>,
    index: Option<BufferHandle>,
    viewport: Rect,
    scissor: Rect,
    push: Vec<u8>,
    push_dirty: bool,
}

struct PassInner {
    pass: Option<wgpu::RenderPass<'static>>,
    state: PassState,
    formats: Vec<Option<wgpu::TextureFormat>>,
    depth: bool,
    full: Rect,
}

pub struct RenderPassEncoder {
    shared: Arc<Shared>,
    inner: Mutex<PassInner>,
}

impl PassInner {
    fn pass(&mut self) -> &mut wgpu::RenderPass<'static> {
        self.pass.as_mut().expect("the pass has ended")
    }

    fn apply_group(&mut self, shared: &Shared, index: usize) {
        let Some(pipeline) = self.state.pipeline.clone() else {
            return;
        };
        let Some(Some((group, _))) = self.state.groups.get(index).copied() else {
            return;
        };
        let Some(layout) = pipeline.info.layouts.get(index) else {
            return;
        };
        let Some(entry) = shared.bind_groups.get(group.0) else {
            shared.log_error("a bind group was destroyed while bound");
            return;
        };
        match entry.get(shared, layout) {
            Ok(bind_group) => {
                self.pass().set_bind_group(index as u32, &bind_group, &[]);
                self.state.groups[index] = Some((group, Some(layout.id)));
            }
            Err(error) => shared.log_error(&error),
        }
    }

    fn apply_pipeline(&mut self, shared: &Shared, pipeline: &Arc<RenderPipelineEntry>) {
        self.pass().set_pipeline(&pipeline.pipeline);
        let wanted = pipeline.info.push.size() as usize;
        self.state.push.resize(wanted, 0);
        self.state.push_dirty = matches!(pipeline.info.push, PushPlan::Uniform { .. });
        self.state.pipeline = Some(pipeline.clone());
        for index in 0..self.state.groups.len() {
            let stale = match self.state.groups[index] {
                Some((_, applied)) => {
                    applied != pipeline.info.layouts.get(index).map(|layout| layout.id)
                }
                None => false,
            };
            if stale {
                self.apply_group(shared, index);
            }
        }
    }

    fn flush_push(&mut self, shared: &Shared) {
        if !self.state.push_dirty {
            return;
        }
        let Some(pipeline) = self.state.pipeline.clone() else {
            return;
        };
        let PushPlan::Uniform {
            group, bind_size, ..
        } = pipeline.info.push
        else {
            return;
        };
        match shared.push.push(shared, &self.state.push, bind_size) {
            Ok((bind_group, offset)) => {
                self.pass().set_bind_group(group, &bind_group, &[offset]);
                self.state.push_dirty = false;
            }
            Err(error) => shared.log_error(&error),
        }
    }

    fn set_rect_state(&mut self) {
        let (viewport, scissor) = (self.state.viewport, self.state.scissor);
        let pass = self.pass();
        pass.set_viewport(
            viewport.x as f32,
            viewport.y as f32,
            viewport.width as f32,
            viewport.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
    }

    fn restore(&mut self, shared: &Shared) {
        if let Some(pipeline) = self.state.pipeline.clone() {
            self.pass().set_pipeline(&pipeline.pipeline);
            for index in 0..self.state.groups.len() {
                self.apply_group(shared, index);
            }
            self.state.push_dirty = matches!(pipeline.info.push, PushPlan::Uniform { .. });
            if matches!(pipeline.info.push, PushPlan::Immediates { .. }) {
                let data = self.state.push.clone();
                self.pass().set_immediates(0, &data);
            }
        }
        for slot in 0..self.state.vertex.len() {
            if let Some((buffer, offset)) = self.state.vertex[slot]
                && let Some(entry) = shared.buffers.get(buffer.0)
            {
                self.pass()
                    .set_vertex_buffer(slot as u32, entry.buffer.slice(offset..));
            }
        }
        if let Some(buffer) = self.state.index
            && let Some(entry) = shared.buffers.get(buffer.0)
        {
            self.pass()
                .set_index_buffer(entry.buffer.slice(..), wgpu::IndexFormat::Uint32);
        }
        self.set_rect_state();
    }
}

impl RenderPassEncoder {
    fn with<R>(&self, record: impl FnOnce(&mut PassInner, &Shared) -> R) -> R {
        let mut inner = lock(&self.inner);
        record(&mut inner, &self.shared)
    }

    pub fn end(self) {
        lock(&self.inner).pass.take();
    }

    pub fn set_pipeline(&mut self, pipeline: &RenderPipeline) {
        let entry = self
            .shared
            .render_pipelines
            .get(pipeline.0)
            .expect("pipeline was destroyed");
        self.with(|inner, shared| inner.apply_pipeline(shared, &entry));
    }

    pub fn set_bind_groups(&self, first: u32, groups: &[BindGroup]) {
        self.with(|inner, shared| {
            let end = first as usize + groups.len();
            if inner.state.groups.len() < end {
                inner.state.groups.resize(end, None);
            }
            for (offset, group) in groups.iter().enumerate() {
                let index = first as usize + offset;
                inner.state.groups[index] = Some((*group, None));
                inner.apply_group(shared, index);
            }
        });
    }

    pub fn set_bind_group(&self, index: u32, group: BindGroup) {
        self.set_bind_groups(index, &[group]);
    }

    pub fn set_push_constants(&self, _visibility: ShaderStages, offset: u32, data: &[u8]) {
        self.with(|inner, _| {
            let Some(pipeline) = inner.state.pipeline.clone() else {
                debug_assert!(false, "no pipeline set");
                return;
            };
            match pipeline.info.push {
                PushPlan::None => debug_assert!(false, "the pipeline has no push constants"),
                PushPlan::Immediates { .. } | PushPlan::Uniform { .. } => {
                    let end = offset as usize + data.len();
                    if inner.state.push.len() < end {
                        inner.state.push.resize(end, 0);
                    }
                    inner.state.push[offset as usize..end].copy_from_slice(data);
                    if matches!(pipeline.info.push, PushPlan::Immediates { .. }) {
                        inner.pass().set_immediates(offset, data);
                    } else {
                        inner.state.push_dirty = true;
                    }
                }
            }
        });
    }

    pub fn set_vertex_buffer(&self, slot: u32, buffer: BufferHandle) {
        self.set_vertex_buffer_at(slot, buffer, 0);
    }

    pub fn set_vertex_buffer_at(&self, slot: u32, buffer: BufferHandle, offset: u64) {
        let Some(entry) = self.shared.buffers.get(buffer.0) else {
            self.shared
                .log_error("a vertex buffer was destroyed while bound");
            return;
        };
        self.with(|inner, _| {
            if inner.state.vertex.len() <= slot as usize {
                inner.state.vertex.resize(slot as usize + 1, None);
            }
            inner.state.vertex[slot as usize] = Some((buffer, offset));
            inner
                .pass()
                .set_vertex_buffer(slot, entry.buffer.slice(offset..));
        });
    }

    pub fn set_index_buffer(&self, buffer: BufferHandle) {
        let Some(entry) = self.shared.buffers.get(buffer.0) else {
            self.shared
                .log_error("an index buffer was destroyed while bound");
            return;
        };
        self.with(|inner, _| {
            inner.state.index = Some(buffer);
            inner
                .pass()
                .set_index_buffer(entry.buffer.slice(..), wgpu::IndexFormat::Uint32);
        });
    }

    pub fn set_viewport(&self, rect: Rect) {
        self.with(|inner, _| {
            inner.state.viewport = rect;
            inner.set_rect_state();
        });
    }

    pub fn set_scissor_rect(&self, rect: Rect) {
        self.with(|inner, _| {
            inner.state.scissor = rect;
            inner.set_rect_state();
        });
    }

    pub fn draw(
        &self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        self.with(|inner, shared| {
            inner.flush_push(shared);
            inner.pass().draw(
                first_vertex..first_vertex + vertex_count,
                first_instance..first_instance + instance_count,
            );
        });
    }

    pub fn draw_indexed(
        &self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    ) {
        self.with(|inner, shared| {
            inner.flush_push(shared);
            inner.pass().draw_indexed(
                first_index..first_index + index_count,
                base_vertex,
                first_instance..first_instance + instance_count,
            );
        });
    }

    pub fn draw_indexed_indirect(
        &self,
        buffer: BufferHandle,
        offset: u64,
        draw_count: u32,
        stride: u32,
    ) {
        let Some(entry) = self.shared.buffers.get(buffer.0) else {
            self.shared
                .log_error("an indirect buffer was destroyed while bound");
            return;
        };
        self.with(|inner, shared| {
            inner.flush_push(shared);
            let packed = stride == INDIRECT_ARGS_SIZE;
            if draw_count > 1 && packed {
                inner
                    .pass()
                    .multi_draw_indexed_indirect(&entry.buffer, offset, draw_count);
            } else {
                for draw in 0..draw_count {
                    inner.pass().draw_indexed_indirect(
                        &entry.buffer,
                        offset + u64::from(draw) * u64::from(stride),
                    );
                }
            }
        });
    }

    pub fn draw_indexed_indirect_count(
        &self,
        buffer: BufferHandle,
        count_buffer: BufferHandle,
        max_draws: u32,
        stride: u32,
    ) {
        debug_assert_eq!(
            stride, INDIRECT_ARGS_SIZE,
            "wgpu reads packed draw arguments"
        );
        let (Some(entry), Some(count)) = (
            self.shared.buffers.get(buffer.0),
            self.shared.buffers.get(count_buffer.0),
        ) else {
            self.shared
                .log_error("an indirect buffer was destroyed while bound");
            return;
        };
        self.with(|inner, shared| {
            inner.flush_push(shared);
            inner.pass().multi_draw_indexed_indirect_count(
                &entry.buffer,
                0,
                &count.buffer,
                0,
                max_draws,
            );
        });
    }

    pub fn clear_depth(&self, rect: Rect) {
        self.clear(ClearKind::Depth, rect, None);
    }

    pub fn clear_color(&self, rect: Rect, rgba: [f32; 4]) {
        self.clear(ClearKind::Color, rect, Some(rgba));
    }

    fn clear(&self, kind: ClearKind, rect: Rect, rgba: Option<[f32; 4]>) {
        self.with(|inner, shared| {
            if matches!(kind, ClearKind::Depth) && !inner.depth {
                return;
            }
            let colors = inner.formats.clone();
            let kind = match (kind, rgba) {
                (ClearKind::Color, Some(rgba)) => {
                    let blendable = colors
                        .first()
                        .copied()
                        .flatten()
                        .is_none_or(|format| shared.blendable(format));
                    if blendable {
                        ClearKind::Color
                    } else {
                        ClearKind::ColorBaked(rgba.map(f32::to_bits))
                    }
                }
                (kind, _) => kind,
            };
            let key = ClearKey {
                colors,
                depth: inner.depth,
                kind: kind.clone(),
            };
            let pipeline = shared.helpers.clear(shared, &key);
            let full = inner.full;
            let pass = inner.pass();
            pass.set_pipeline(&pipeline);
            if let (ClearKind::Color, Some(rgba)) = (&kind, rgba) {
                pass.set_blend_constant(color(rgba));
            }
            pass.set_viewport(0.0, 0.0, full.width as f32, full.height as f32, 0.0, 1.0);
            pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
            pass.draw(0..3, 0..1);
            inner.restore(shared);
        });
    }
}

const INDIRECT_ARGS_SIZE: u32 = 20;
