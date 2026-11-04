use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use super::profile::RendererProfile;
use super::registry::Key;
use super::resource::{Texture, TextureEntry, TextureHandle, lock};
use super::{CommandEncoder, ContextError, Device, Error, RenderPassDesc, Shared};
use crate::rhi::{self, Acquired, Extent, IndirectDrawFeatures, Presented};

pub const MAX_FRAMES_IN_FLIGHT: usize = 3;

const IMAGE_SLOTS: usize = MAX_FRAMES_IN_FLIGHT;

pub struct Context {
    pub device: Device,
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    surface: Option<Arc<wgpu::Surface<'static>>>,

    pub present_wait: bool,
    pub indirect_draw: IndirectDrawFeatures,

    pub gpu_name: String,
    pub api_version: String,

    surface_extent: Mutex<(u32, u32)>,
}

async fn open_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
    immediates: bool,
) -> Result<(wgpu::Adapter, Arc<Shared>), ContextError> {
    let profile = RendererProfile::detect();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: profile.power_preference(),
            force_fallback_adapter: false,
            compatible_surface: surface,
            ..Default::default()
        })
        .await
        .map_err(|error| ContextError::NoSuitableGpu(error.to_string()))?;

    let features = adapter.features() & profile.optional_features();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("pomme"),
            required_features: features,
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .map_err(|error| ContextError::Gpu(error.to_string()))?;
    device.on_uncaptured_error(Arc::new(|error| {
        super::log_error(&error.to_string());
    }));

    let limits = device.limits();
    let shared = Arc::new(Shared {
        immediates: immediates
            && features.contains(wgpu::Features::IMMEDIATES)
            && limits.max_immediate_size > 0,
        depth_clip_control: features.contains(wgpu::Features::DEPTH_CLIP_CONTROL),
        device,
        queue,
        adapter: adapter.clone(),
        adapter_info: adapter.get_info(),
        features,
        limits,
        buffers: Default::default(),
        textures: Default::default(),
        samplers: Default::default(),
        layouts: Default::default(),
        pipeline_layouts: Default::default(),
        bind_groups: Default::default(),
        render_pipelines: Default::default(),
        compute_pipelines: Default::default(),
        layout_cache: Default::default(),
        fallbacks: Default::default(),
        push: Default::default(),
        helpers: Default::default(),
        readbacks: Default::default(),
        submitted: AtomicU64::new(0),
        completed: Arc::new(AtomicU64::new(0)),
    });
    Ok((adapter, shared))
}

impl Device {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn headless() -> Result<Self, ContextError> {
        pollster::block_on(Self::headless_async(true))
    }

    pub async fn headless_async(immediates: bool) -> Result<Self, ContextError> {
        let instance = Context::instance().await;
        let (_, shared) = open_device(&instance, None, immediates).await?;
        Ok(Device::new(shared))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn headless_emulated_push() -> Result<Self, ContextError> {
        pollster::block_on(Self::headless_async(false))
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn browser_supported() -> bool {
    if RendererProfile::detect().forces_webgl2() {
        return browser_has_webgl2();
    }
    browser_has_webgpu_adapter().await || browser_has_webgl2()
}

#[cfg(target_arch = "wasm32")]
fn browser_has_webgl2() -> bool {
    use wasm_bindgen::JsCast;

    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.create_element("canvas").ok())
        .and_then(|canvas| canvas.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .and_then(|canvas| canvas.get_context("webgl2").ok().flatten())
        .is_some()
}

#[cfg(target_arch = "wasm32")]
async fn browser_has_webgpu_adapter() -> bool {
    use js_sys::{Function, Promise, Reflect};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let adapter = async {
        let navigator = Reflect::get(&js_sys::global(), &"navigator".into()).ok()?;
        let gpu = Reflect::get(&navigator, &"gpu".into()).ok()?;
        let request: Function = Reflect::get(&gpu, &"requestAdapter".into())
            .ok()?
            .dyn_into()
            .ok()?;
        let promise: Promise = request.call0(&gpu).ok()?.dyn_into().ok()?;
        JsFuture::from(promise).await.ok()
    }
    .await;
    adapter.is_some_and(|adapter| !adapter.is_null() && !adapter.is_undefined())
}

impl Context {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new<W: HasWindowHandle + HasDisplayHandle>(window: &W) -> Result<Self, ContextError> {
        pollster::block_on(Self::new_async(window))
    }

    pub async fn new_async<W: HasWindowHandle + HasDisplayHandle>(
        window: &W,
    ) -> Result<Self, ContextError> {
        let instance = Self::instance().await;
        let surface = Self::window_surface(&instance, window)?;
        Self::from_surface(instance, surface).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn from_canvas(canvas: web_sys::HtmlCanvasElement) -> Result<Self, ContextError> {
        let instance = Self::instance().await;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|error| ContextError::Surface(error.to_string()))?;
        Self::from_surface(instance, surface).await
    }

    async fn instance() -> wgpu::Instance {
        #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        #[cfg(target_arch = "wasm32")]
        if RendererProfile::detect().forces_webgl2() || !browser_has_webgpu_adapter().await {
            descriptor.backends = wgpu::Backends::GL;
        }
        wgpu::Instance::new(descriptor)
    }

    fn window_surface<W: HasWindowHandle + HasDisplayHandle>(
        instance: &wgpu::Instance,
        window: &W,
    ) -> Result<wgpu::Surface<'static>, ContextError> {
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(window.display_handle()?.as_raw()),
            raw_window_handle: window.window_handle()?.as_raw(),
        };
        unsafe { instance.create_surface_unsafe(target) }
            .map_err(|error| ContextError::Surface(error.to_string()))
    }

    async fn from_surface(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
    ) -> Result<Self, ContextError> {
        let (adapter, shared) = open_device(&instance, Some(&surface), true).await?;
        let info = &shared.adapter_info;
        let gpu_name = info.name.clone();
        let api_version = format!("wgpu {:?}", info.backend);
        let indirect_draw = IndirectDrawFeatures {
            count: shared
                .features
                .contains(wgpu::Features::MULTI_DRAW_INDIRECT_COUNT),
            multi_draw: cfg!(not(target_arch = "wasm32")),
            gpu_cull: shared
                .features
                .contains(wgpu::Features::INDIRECT_FIRST_INSTANCE)
                && shared
                    .adapter
                    .get_downlevel_capabilities()
                    .flags
                    .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS),
        };
        Ok(Self {
            device: Device::new(shared),
            instance,
            adapter,
            surface: Some(Arc::new(surface)),
            present_wait: false,
            indirect_draw,
            gpu_name,
            api_version,
            surface_extent: Mutex::new((0, 0)),
        })
    }

    pub fn set_surface_extent(&self, width: u32, height: u32) {
        *lock(&self.surface_extent) = (width, height);
    }

    pub fn destroy_surface(&mut self) {
        self.surface = None;
    }

    pub fn recreate_surface<W: HasWindowHandle + HasDisplayHandle>(
        &mut self,
        window: &W,
    ) -> Result<(), ContextError> {
        self.surface = Some(Arc::new(Self::window_surface(&self.instance, window)?));
        Ok(())
    }

    fn surface(&self) -> Result<Arc<wgpu::Surface<'static>>, ContextError> {
        self.surface
            .clone()
            .ok_or_else(|| ContextError::Surface("the surface was released".into()))
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { self.device.destroy() };
    }
}

fn choose_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ]
    .into_iter()
    .find(|want| formats.contains(want))
    .unwrap_or(formats[0])
}

pub struct Swapchain {
    surface: Option<Arc<wgpu::Surface<'static>>>,
    pub format: wgpu::TextureFormat,
    pub extent: Extent,
    pub present_mode: wgpu::PresentMode,
    pub depth: Texture,
    images: Vec<Key>,
    current: Mutex<Option<wgpu::SurfaceTexture>>,
    next: AtomicUsize,
    suboptimal: Mutex<bool>,
}

impl Swapchain {
    pub fn new(ctx: &Context, width: u32, height: u32, vsync: bool) -> Result<Self, ContextError> {
        let surface = ctx.surface()?;
        let (format, present_mode, extent) = Self::configure(ctx, &surface, width, height, vsync)?;
        let depth = Texture::try_new(&ctx.device, &rhi::TextureDesc::swapchain_depth(extent))
            .map_err(ContextError::Texture)?;
        let shared = &ctx.device.shared;
        let images = (0..IMAGE_SLOTS)
            .map(|_| {
                let placeholder = shared.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("swapchain_slot"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                shared.textures.insert(TextureEntry::new(
                    placeholder,
                    format,
                    rhi::TextureViewDimension::D2,
                ))
            })
            .collect();
        Ok(Self {
            surface: Some(surface),
            format,
            extent,
            present_mode,
            depth,
            images,
            current: Mutex::new(None),
            next: AtomicUsize::new(0),
            suboptimal: Mutex::new(false),
        })
    }

    fn configure(
        ctx: &Context,
        surface: &wgpu::Surface<'static>,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> Result<(wgpu::TextureFormat, wgpu::PresentMode, Extent), ContextError> {
        let capabilities = surface.get_capabilities(&ctx.adapter);
        if capabilities.formats.is_empty() {
            return Err(ContextError::Surface(
                "the surface offers no formats".into(),
            ));
        }
        let format = choose_format(&capabilities.formats);
        let present_mode = if vsync {
            wgpu::PresentMode::Fifo
        } else {
            [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate]
                .into_iter()
                .find(|mode| capabilities.present_modes.contains(mode))
                .unwrap_or(wgpu::PresentMode::Fifo)
        };
        let extent = Extent {
            width: width.max(1),
            height: height.max(1),
        };
        let mut config = surface
            .get_default_config(&ctx.adapter, extent.width, extent.height)
            .ok_or_else(|| ContextError::Surface("the adapter can't present here".into()))?;
        config.format = format;
        config.present_mode = present_mode;
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | (capabilities.usages & wgpu::TextureUsages::COPY_SRC);
        config.desired_maximum_frame_latency = MAX_FRAMES_IN_FLIGHT as u32 - 1;
        surface.configure(&ctx.device.shared.device, &config);
        *lock(&ctx.surface_extent) = (extent.width, extent.height);
        Ok((format, present_mode, extent))
    }

    pub fn rebuild(
        &mut self,
        ctx: &Context,
        frames: &mut Frames,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> Result<(), ContextError> {
        lock(&self.current).take();
        let surface = ctx.surface()?;
        let (format, present_mode, extent) = Self::configure(ctx, &surface, width, height, vsync)?;
        self.surface = Some(surface);
        let depth = Texture::try_new(&ctx.device, &rhi::TextureDesc::swapchain_depth(extent))
            .map_err(ContextError::Texture)?;
        std::mem::replace(&mut self.depth, depth).destroy(&ctx.device);
        self.format = format;
        self.present_mode = present_mode;
        self.extent = extent;
        frames.resize(&ctx.device, self.images.len())?;
        Ok(())
    }

    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    pub fn targets(&self) -> super::PipelineTargets {
        super::PipelineTargets::new(self.format, true)
    }

    pub fn image(&self, index: u32) -> TextureHandle {
        TextureHandle(self.images[index as usize])
    }

    pub fn pass(&self, index: u32, color: rhi::LoadOp<[f32; 4]>) -> RenderPassDesc {
        RenderPassDesc::swapchain(
            self.image(index),
            self.format,
            color,
            self.depth.handle(),
            self.extent,
        )
    }

    pub fn is_fifo(&self) -> bool {
        self.present_mode == wgpu::PresentMode::Fifo
    }

    pub fn surface_resized(&self, ctx: &Context) -> bool {
        let (width, height) = *lock(&ctx.surface_extent);
        width > 0 && (width, height) != (self.extent.width, self.extent.height)
    }

    pub fn is_bgra(&self) -> bool {
        matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        )
    }

    pub fn ready_to_present(&self, _encoder: &CommandEncoder, _index: u32) {}

    pub fn release_surface(&mut self, _ctx: &Context) {
        lock(&self.current).take();
        self.surface = None;
    }

    pub fn destroy(&mut self, ctx: &Context) {
        ctx.device.wait_idle();
        self.release_surface(ctx);
        for key in self.images.drain(..) {
            ctx.device.shared.textures.remove(key);
        }
        std::mem::take(&mut self.depth).destroy(&ctx.device);
    }
}

pub struct Frames {
    index: usize,
}

impl Frames {
    pub fn new(_device: &Device, _image_count: usize) -> Result<Self, Error> {
        Ok(Self { index: 0 })
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn wait(&self, device: &Device) -> Result<(), Error> {
        device
            .shared
            .wait_for_frames(MAX_FRAMES_IN_FLIGHT as u64 - 1, None);
        Ok(())
    }

    pub fn wait_all(&self, device: &Device) {
        device.shared.wait_for_frames(0, None);
    }

    pub fn acquire(&self, device: &Device, swapchain: &Swapchain) -> Result<Acquired, Error> {
        let shared = &device.shared;
        let Some(surface) = swapchain.surface.as_ref() else {
            return Err(Error::Surface("the surface was released".into()));
        };
        let (texture, suboptimal) = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost => return Ok(Acquired::OutOfDate),
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(Error::Surface("the surface failed validation".into()));
            }
        };
        let index = swapchain.next.fetch_add(1, Ordering::Relaxed) % swapchain.images.len();
        let key = swapchain.images[index];
        shared.textures.replace(
            key,
            TextureEntry::new(
                texture.texture.clone(),
                swapchain.format,
                rhi::TextureViewDimension::D2,
            ),
        );
        *lock(&swapchain.current) = Some(texture);
        *lock(&swapchain.suboptimal) = suboptimal;
        Ok(Acquired::Image {
            index: index as u32,
            suboptimal,
        })
    }

    pub fn begin(&self, device: &Device) -> Result<CommandEncoder, Error> {
        device.shared.push.reset();
        Ok(CommandEncoder::new(&device.shared))
    }

    pub fn submit_and_present(
        &mut self,
        encoder: CommandEncoder,
        ctx: &Context,
        swapchain: &Swapchain,
        _image: u32,
    ) -> Result<Presented, Error> {
        let shared = &ctx.device.shared;
        shared.submit(encoder.finish());
        shared.track_frame();
        let texture = lock(&swapchain.current).take();
        let suboptimal = std::mem::take(&mut *lock(&swapchain.suboptimal));
        if let Some(texture) = texture {
            shared.queue.present(texture);
        }
        self.index = (self.index + 1) % MAX_FRAMES_IN_FLIGHT;
        Ok(if suboptimal {
            Presented::Suboptimal
        } else {
            Presented::Done
        })
    }

    pub fn pace(&self, ctx: &Context, _swapchain: &Swapchain, timeout_ns: u64) -> bool {
        ctx.device
            .shared
            .wait_for_frames(1, Some(std::time::Duration::from_nanos(timeout_ns)));
        true
    }

    pub fn resize(&mut self, _device: &Device, _image_count: usize) -> Result<(), Error> {
        Ok(())
    }

    pub fn destroy(&mut self, _device: &Device) {}
}
