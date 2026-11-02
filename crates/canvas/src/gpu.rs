use wasm_bindgen::JsValue;
use web_sys::HtmlCanvasElement;

use crate::error::CanvasError;
use crate::size::CanvasSize;

/// The wgpu objects made for one canvas. A renderer takes ownership and draws
/// into `surface`.
pub struct CanvasGpu {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

impl CanvasGpu {
    pub async fn request(
        canvas: &HtmlCanvasElement,
        size: CanvasSize,
    ) -> Result<Self, CanvasError> {
        if !browser_has_webgpu() {
            return Err(CanvasError::Unsupported);
        }
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|error| CanvasError::Setup(error.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| CanvasError::Setup(error.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("canvas_device"),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|error| CanvasError::Setup(error.to_string()))?;
        let mut config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| CanvasError::Setup("the adapter cannot present to the canvas".into()))?;
        config.format =
            preferred_format(&surface.get_capabilities(&adapter).formats).unwrap_or(config.format);
        surface.configure(&device, &config);
        Ok(Self {
            instance,
            surface,
            adapter,
            device,
            queue,
            config,
        })
    }

    pub fn max_texture_dimension(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }
}

/// Frames are written as plain 8-bit UNORM, as the Vulkan swapchain is.
fn preferred_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ]
    .into_iter()
    .find(|format| formats.contains(format))
}

fn browser_has_webgpu() -> bool {
    web_sys::window()
        .and_then(|window| {
            js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("gpu")).ok()
        })
        .is_some_and(|gpu| !gpu.is_undefined() && !gpu.is_null())
}
