use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use super::registry::Key;
use super::{Device, Shared};
use crate::rhi::{
    AddressMode, BufferUsages, CompareFunction, FilterMode, MemoryLocation, SamplerDesc,
    TextureDesc, TextureFormat, TextureUsages, TextureViewDimension,
};

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Rounds a size up to wgpu's copy alignment.
pub(crate) fn align4(size: u64) -> u64 {
    size.div_ceil(wgpu::COPY_BUFFER_ALIGNMENT) * wgpu::COPY_BUFFER_ALIGNMENT
}

pub(crate) struct BufferEntry {
    pub buffer: wgpu::Buffer,
}

/// What commands and bindings take a [`Buffer`] as: a small copy, valid until
/// the buffer is destroyed.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct BufferHandle(pub(crate) Key);

/// A CPU-readable mirror of a [`MemoryLocation::Readback`] buffer.
struct Readback {
    #[cfg(not(target_arch = "wasm32"))]
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    size: u64,
    shadow: Mutex<Vec<u8>>,
    mapping: AtomicBool,
}

impl Readback {
    /// Starts mapping the buffer; the shadow is refreshed once the GPU work
    /// writing it is done.
    pub(crate) fn pump(self: &Arc<Self>) {
        if self.mapping.swap(true, Ordering::AcqRel) {
            return;
        }
        let this = self.clone();
        self.buffer
            .slice(..self.size)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_ok() {
                    let slice = this.buffer.slice(..this.size);
                    if let Ok(mapped) = slice.get_mapped_range() {
                        lock(&this.shadow).copy_from_slice(&mapped);
                    }
                    this.buffer.unmap();
                }
                this.mapping.store(false, Ordering::Release);
            });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

/// Readback buffers whose shadows follow the GPU, pumped after every submit.
#[derive(Default)]
pub(crate) struct Readbacks(Mutex<Vec<std::sync::Weak<Readback>>>);

impl Readbacks {
    fn register(&self, readback: &Arc<Readback>) {
        lock(&self.0).push(Arc::downgrade(readback));
    }

    pub(crate) fn pump(&self) {
        let mut list = lock(&self.0);
        list.retain(|weak| match weak.upgrade() {
            Some(readback) => {
                readback.pump();
                true
            }
            None => false,
        });
    }
}

/// A buffer and the memory it owns. The default is an empty placeholder, for
/// `std::mem::take` when an owner frees a buffer it holds by value.
#[derive(Default)]
pub struct Buffer {
    key: Key,
    inner: Option<BufferInner>,
}

struct BufferInner {
    raw: wgpu::Buffer,
    queue: wgpu::Queue,
    /// Host buffers keep a CPU copy, so writes can be widened to wgpu's
    /// 4-byte copy alignment.
    shadow: Vec<u8>,
    readback: Option<Arc<Readback>>,
    len: u64,
}

fn buffer_usages(usage: BufferUsages) -> wgpu::BufferUsages {
    let table = [
        (BufferUsages::VERTEX, wgpu::BufferUsages::VERTEX),
        (BufferUsages::INDEX, wgpu::BufferUsages::INDEX),
        (BufferUsages::UNIFORM, wgpu::BufferUsages::UNIFORM),
        (BufferUsages::STORAGE, wgpu::BufferUsages::STORAGE),
        (BufferUsages::INDIRECT, wgpu::BufferUsages::INDIRECT),
        (BufferUsages::COPY_SRC, wgpu::BufferUsages::COPY_SRC),
        (BufferUsages::COPY_DST, wgpu::BufferUsages::COPY_DST),
    ];
    table
        .into_iter()
        .filter(|(bit, _)| usage.contains(*bit))
        .fold(wgpu::BufferUsages::empty(), |flags, (_, flag)| flags | flag)
}

impl Buffer {
    pub fn new(
        device: &Device,
        label: &str,
        size: u64,
        usage: BufferUsages,
        location: MemoryLocation,
    ) -> Self {
        Self::try_new(device, label, size, usage, location)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_new(
        device: &Device,
        label: &str,
        size: u64,
        usage: BufferUsages,
        location: MemoryLocation,
    ) -> Result<Self, String> {
        let shared = &device.shared;
        let allocated = align4(size).max(wgpu::COPY_BUFFER_ALIGNMENT);
        let mut flags = buffer_usages(usage);
        match location {
            MemoryLocation::Host => flags |= wgpu::BufferUsages::COPY_DST,
            MemoryLocation::Device => {}
            MemoryLocation::Readback => {
                let allowed = BufferUsages::COPY_DST | BufferUsages::COPY_SRC;
                let extra = buffer_usages(usage) - (buffer_usages(allowed) & buffer_usages(usage));
                if !extra.is_empty() {
                    return Err(format!(
                        "readback buffer {label} may only be a copy source or destination"
                    ));
                }
                flags = wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST;
            }
        }
        if size > shared.limits.max_buffer_size {
            return Err(format!(
                "buffer {label} of {size} bytes exceeds the device limit of {}",
                shared.limits.max_buffer_size
            ));
        }
        let raw = shared.scoped(&format!("failed to create buffer {label}"), || {
            shared.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: allocated,
                usage: flags,
                mapped_at_creation: false,
            })
        })?;

        let key = shared.buffers.insert(BufferEntry {
            buffer: raw.clone(),
        });
        let readback = matches!(location, MemoryLocation::Readback).then(|| {
            let readback = Arc::new(Readback {
                #[cfg(not(target_arch = "wasm32"))]
                device: shared.device.clone(),
                buffer: raw.clone(),
                size: allocated,
                shadow: Mutex::new(vec![0; allocated as usize]),
                mapping: AtomicBool::new(false),
            });
            shared.readbacks.register(&readback);
            readback
        });
        Ok(Self {
            key,
            inner: Some(BufferInner {
                raw,
                queue: shared.queue.clone(),
                shadow: if matches!(location, MemoryLocation::Host) {
                    vec![0; allocated as usize]
                } else {
                    Vec::new()
                },
                readback,
                len: size,
            }),
        })
    }

    /// A host buffer holding `data`.
    pub fn with_data(device: &Device, label: &str, data: &[u8], usage: BufferUsages) -> Self {
        Self::try_with_data(device, label, data, usage).unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_with_data(
        device: &Device,
        label: &str,
        data: &[u8],
        usage: BufferUsages,
    ) -> Result<Self, String> {
        let mut buffer = Self::try_new(
            device,
            label,
            data.len() as u64,
            usage,
            MemoryLocation::Host,
        )?;
        buffer.write(data);
        Ok(buffer)
    }

    pub fn handle(&self) -> BufferHandle {
        BufferHandle(self.key)
    }

    /// Reads a [`MemoryLocation::Readback`] buffer; the caller has already
    /// waited on the frame that wrote it. On the web, where nothing can block,
    /// this is the contents as of the latest finished mapping.
    pub fn read<R>(&mut self, read: impl FnOnce(&[u8]) -> R) -> R {
        let inner = self.inner.as_ref().expect("buffer was destroyed");
        let readback = inner
            .readback
            .as_ref()
            .expect("buffer is not a readback buffer");
        #[cfg(not(target_arch = "wasm32"))]
        {
            readback.pump();
            while readback.mapping.load(Ordering::Acquire) {
                readback.wait();
            }
        }
        let shadow = lock(&readback.shadow);
        read(&shadow[..inner.len as usize])
    }

    /// Writes `data` at the start of a [`MemoryLocation::Host`] buffer.
    pub fn write(&mut self, data: &[u8]) {
        self.write_at(0, data);
    }

    /// Writes `data` at byte `offset` of a [`MemoryLocation::Host`] buffer.
    pub fn write_at(&mut self, offset: usize, data: &[u8]) {
        let inner = self.inner.as_mut().expect("buffer was destroyed");
        assert!(!inner.shadow.is_empty(), "buffer is not host-written");
        let end = offset + data.len();
        inner.shadow[offset..end].copy_from_slice(data);
        let start = offset & !3;
        let end = (end + 3) & !3;
        inner
            .queue
            .write_buffer(&inner.raw, start as u64, &inner.shadow[start..end]);
    }

    /// Zeroes a whole host-written buffer.
    pub fn zero(&mut self) {
        let inner = self.inner.as_mut().expect("buffer was destroyed");
        if let Some(readback) = &inner.readback {
            lock(&readback.shadow).fill(0);
            return;
        }
        inner.shadow.fill(0);
        inner.queue.write_buffer(&inner.raw, 0, &inner.shadow);
    }

    pub fn destroy(self, device: &Device) {
        if self.inner.is_some() {
            device.shared.buffers.remove(self.key);
        }
    }
}

pub(crate) struct TextureEntry {
    pub texture: wgpu::Texture,
    /// The view shaders sample: every level and layer, in the texture's
    /// own dimension.
    pub view: wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    attachment: OnceLock<wgpu::TextureView>,
    subviews: Mutex<HashMap<(u32, u32), wgpu::TextureView>>,
}

impl TextureEntry {
    pub fn new(
        texture: wgpu::Texture,
        format: wgpu::TextureFormat,
        dimension: TextureViewDimension,
    ) -> Self {
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            dimension: Some(match dimension {
                TextureViewDimension::D2 => wgpu::TextureViewDimension::D2,
                TextureViewDimension::D2Array => wgpu::TextureViewDimension::D2Array,
                TextureViewDimension::Cube => wgpu::TextureViewDimension::Cube,
            }),
            ..Default::default()
        });
        Self {
            texture,
            view,
            format,
            attachment: OnceLock::new(),
            subviews: Mutex::new(HashMap::new()),
        }
    }

    /// The first level and layer as a 2D view, which render passes take.
    pub fn attachment_view(&self) -> wgpu::TextureView {
        self.attachment.get_or_init(|| self.subview(0, 0)).clone()
    }

    /// One level of one layer as a 2D view.
    pub fn subview(&self, mip: u32, layer: u32) -> wgpu::TextureView {
        if let Some(view) = lock(&self.subviews).get(&(mip, layer)) {
            return view.clone();
        }
        let view = self.texture.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: mip,
            mip_level_count: Some(1),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        });
        lock(&self.subviews).insert((mip, layer), view.clone());
        view
    }
}

/// A texture, its view of every level and layer, and the memory it owns.
/// The default is an empty placeholder, like [`Buffer`]'s.
#[derive(Default)]
pub struct Texture {
    key: Key,
}

/// What barriers, copies, bindings and attachments take a texture as: valid
/// until their owner destroys it.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct TextureHandle(pub(crate) Key);

impl Texture {
    pub fn new(device: &Device, desc: &TextureDesc<'_>) -> Self {
        Self::try_new(device, desc).unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_new(device: &Device, desc: &TextureDesc<'_>) -> Result<Self, String> {
        let shared = &device.shared;
        let format = shared.texture_format(desc.format)?;
        let usage = shared.texture_usages(desc.usage, format);
        let texture = shared.scoped(&format!("failed to create image {}", desc.label), || {
            shared.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(desc.label),
                size: wgpu::Extent3d {
                    width: desc.width,
                    height: desc.height,
                    depth_or_array_layers: desc.layers,
                },
                mip_level_count: desc.mip_levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        })?;
        let entry = TextureEntry::new(texture, format, desc.dimension);
        Ok(Self {
            key: shared.textures.insert(entry),
        })
    }

    pub fn handle(&self) -> TextureHandle {
        TextureHandle(self.key)
    }

    /// Frees the texture.
    pub fn destroy(self, device: &Device) {
        device.shared.textures.remove(self.key);
    }
}

pub fn texture_format(format: TextureFormat) -> wgpu::TextureFormat {
    match format {
        TextureFormat::R8Unorm => wgpu::TextureFormat::R8Unorm,
        TextureFormat::Rg8Unorm => wgpu::TextureFormat::Rg8Unorm,
        TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        TextureFormat::Rgba16Unorm => wgpu::TextureFormat::Rgba16Unorm,
        TextureFormat::Rgb10a2Unorm => wgpu::TextureFormat::Rgb10a2Unorm,
        TextureFormat::R16Float => wgpu::TextureFormat::R16Float,
        TextureFormat::Rg16Float => wgpu::TextureFormat::Rg16Float,
        TextureFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        TextureFormat::R32Float => wgpu::TextureFormat::R32Float,
        TextureFormat::Rg32Float => wgpu::TextureFormat::Rg32Float,
        TextureFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        TextureFormat::Rg11b10Float => wgpu::TextureFormat::Rg11b10Ufloat,
        TextureFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
    }
}

impl Shared {
    /// The wgpu format for `format`, or why this device can't have textures
    /// of it.
    pub(crate) fn texture_format(
        &self,
        format: TextureFormat,
    ) -> Result<wgpu::TextureFormat, String> {
        if format == TextureFormat::Rgba16Unorm
            && !self
                .features
                .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
        {
            return Err(
                "16-bit normalised textures need the texture-format-16-bit-norm feature".into(),
            );
        }
        Ok(texture_format(format))
    }

    /// The usages of `usage` plus the others the RHI's untracked uses need
    /// (blits, clears and copies move between them), within what the format
    /// allows.
    pub(crate) fn texture_usages(
        &self,
        usage: TextureUsages,
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureUsages {
        let wanted = [
            (TextureUsages::COPY_SRC, wgpu::TextureUsages::COPY_SRC),
            (TextureUsages::COPY_DST, wgpu::TextureUsages::COPY_DST),
            (
                TextureUsages::TEXTURE_BINDING,
                wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            (
                TextureUsages::RENDER_ATTACHMENT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
        ]
        .into_iter()
        .filter(|(bit, _)| usage.contains(*bit))
        .fold(wgpu::TextureUsages::empty(), |flags, (_, flag)| {
            flags | flag
        });
        let allowed = self
            .adapter
            .get_texture_format_features(format)
            .allowed_usages;
        let implied = wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT;
        (wanted | implied) & allowed
    }
}

/// A sampler. Copy: the device keeps the object.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct Sampler(pub(crate) Key);

impl Sampler {
    pub fn destroy(self, device: &Device) {
        device.shared.samplers.remove(self.0);
    }
}

pub fn create_sampler(device: &Device, desc: &SamplerDesc) -> Sampler {
    try_create_sampler(device, desc).unwrap_or_else(|error| panic!("{error}"))
}

pub fn try_create_sampler(device: &Device, desc: &SamplerDesc) -> Result<Sampler, String> {
    let filter = |mode| match mode {
        FilterMode::Nearest => wgpu::FilterMode::Nearest,
        FilterMode::Linear => wgpu::FilterMode::Linear,
    };
    let address = match desc.address_mode {
        AddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        AddressMode::Repeat => wgpu::AddressMode::Repeat,
    };
    let shared = &device.shared;
    let sampler = shared.scoped("failed to create sampler", || {
        shared.device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: address,
            address_mode_v: address,
            address_mode_w: address,
            mag_filter: filter(desc.filter),
            min_filter: filter(desc.filter),
            mipmap_filter: match desc.mipmap_filter {
                FilterMode::Nearest => wgpu::MipmapFilterMode::Nearest,
                FilterMode::Linear => wgpu::MipmapFilterMode::Linear,
            },
            lod_min_clamp: 0.0,
            lod_max_clamp: desc.lod_max_clamp,
            compare: desc.compare.map(|compare| match compare {
                CompareFunction::Less => wgpu::CompareFunction::Less,
                CompareFunction::LessEqual => wgpu::CompareFunction::LessEqual,
            }),
            anisotropy_clamp: 1,
            border_color: None,
        })
    })?;
    Ok(Sampler(shared.samplers.insert(sampler)))
}
