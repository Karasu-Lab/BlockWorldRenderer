use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;

use camera::Camera;
use debugger::{
    DebugInfoProvider, DebugScreen, DebuggerError, RendererInfo, RendererInfoProvider,
    RendererInfoSource,
};
use renderer::{RenderScene, RenderTarget, Renderer, RendererError};
use settings::Settings;
use ui::{NullUiRenderer, UiLayer, UiRenderer, UiSwitch};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlCanvasElement, KeyboardEvent, Window};
use worldgen::ChunkGenerator;

use crate::error::CanvasError;
use crate::frame::FrameClock;
use crate::scene::{ChunkLayout, ChunkScene, StreamingScene};
use crate::size::CanvasSize;
use crate::webgpu::{webgl2_available, webgpu_available};

/// A renderer the canvas can drive.
pub trait CanvasRenderer: Renderer + RendererInfoSource + 'static {
    /// The widest texture the GPU takes, which bounds the canvas' size.
    fn max_texture_dimension(&self) -> u32;
}

/// Builds the renderer that draws into a canvas' surface.
pub trait RendererFactory {
    type Renderer: CanvasRenderer;

    /// Opens the GPU for `canvas`, whose backing store is `size` big.
    fn create(
        self,
        canvas: HtmlCanvasElement,
        size: CanvasSize,
    ) -> impl Future<Output = Result<Self::Renderer, RendererError>>;
}

pub struct CanvasOptions {
    pub settings: Settings,
    pub layout: ChunkLayout,
    /// Draws the UI while it is switched on; with the UI off it is never called.
    pub ui: Box<dyn UiRenderer>,
}

impl CanvasOptions {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            layout: ChunkLayout::OVERWORLD,
            ui: Box::new(NullUiRenderer),
        }
    }
}

type FrameCallback = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

struct FixedRendererInfo(RendererInfo);

impl RendererInfoSource for FixedRendererInfo {
    fn renderer_info(&self) -> RendererInfo {
        self.0.clone()
    }
}

struct Lines(Vec<String>);

impl DebugInfoProvider for Lines {
    fn lines(&self) -> Result<Vec<String>, DebuggerError> {
        Ok(self.0.clone())
    }
}

struct Fps(Rc<Cell<f32>>);

impl DebugInfoProvider for Fps {
    fn lines(&self) -> Result<Vec<String>, DebuggerError> {
        Ok(vec![format!("{:.0} fps", self.0.get())])
    }
}

struct State {
    window: Window,
    canvas: HtmlCanvasElement,
    renderer: Box<dyn Renderer>,
    scene: Box<dyn StreamingScene>,
    camera: Camera,
    size: CanvasSize,
    max_dimension: u32,
    clock: FrameClock,
    fps: Rc<Cell<f32>>,
    ui: UiLayer,
    debug: DebugScreen,
    running: bool,
    last_error: Option<CanvasError>,
}

impl State {
    fn current_size(&self) -> CanvasSize {
        CanvasSize::from_css(
            f64::from(self.canvas.client_width()),
            f64::from(self.canvas.client_height()),
            self.window.device_pixel_ratio(),
            self.max_dimension,
        )
    }

    fn apply_size(&mut self, size: CanvasSize) {
        self.canvas.set_width(size.width);
        self.canvas.set_height(size.height);
        self.renderer.resize(RenderTarget {
            width: size.width,
            height: size.height,
        });
        self.size = size;
    }

    fn frame(&mut self, now_ms: f64) -> Result<(), CanvasError> {
        let delta = self.clock.tick(now_ms);
        if delta > 0.0 {
            self.fps.set(1.0 / delta);
        }
        let size = self.current_size();
        if size != self.size {
            self.apply_size(size);
        }
        self.scene.poll()?;
        let scene: &dyn RenderScene = self.scene.as_ref();
        self.renderer.render_frame(&self.camera, scene)?;
        self.ui
            .render()
            .map_err(|error| CanvasError::Setup(error.to_string()))?;
        Ok(())
    }

    fn toggle_debug_screen(&mut self) {
        if self.ui.switch_mut().enabled() {
            self.debug.switch_mut().toggle();
        }
    }
}

/// A running canvas: the fixed camera over the fixed-seed world, drawn every
/// animation frame.
pub struct Canvas {
    state: Rc<RefCell<State>>,
    frame_callback: FrameCallback,
    key_callback: Option<Closure<dyn FnMut(KeyboardEvent)>>,
}

impl Canvas {
    pub async fn start<F, G>(
        canvas: HtmlCanvasElement,
        options: CanvasOptions,
        generator: G,
        factory: F,
    ) -> Result<Self, CanvasError>
    where
        F: RendererFactory,
        G: ChunkGenerator + Send + Sync + 'static,
    {
        if !webgpu_available() && !webgl2_available() {
            return Err(CanvasError::Unsupported);
        }
        let window =
            web_sys::window().ok_or_else(|| CanvasError::Setup("there is no window".into()))?;
        let CanvasOptions {
            settings,
            layout,
            ui,
        } = options;
        let client = settings.client.unwrap_or_default();

        let initial = CanvasSize::from_css(
            f64::from(canvas.client_width()),
            f64::from(canvas.client_height()),
            window.device_pixel_ratio(),
            u32::MAX,
        );
        canvas.set_width(initial.width);
        canvas.set_height(initial.height);

        let mut renderer = factory.create(canvas.clone(), initial).await?;
        let max_dimension = renderer.max_texture_dimension();
        let size = CanvasSize::from_css(
            f64::from(canvas.client_width()),
            f64::from(canvas.client_height()),
            window.device_pixel_ratio(),
            max_dimension,
        );
        let renderer_info = renderer.renderer_info();
        renderer.resize(RenderTarget {
            width: size.width,
            height: size.height,
        });
        canvas.set_width(size.width);
        canvas.set_height(size.height);

        let camera = settings.camera.to_camera();
        let mut scene = ChunkScene::new(generator, settings.world.seed, layout);
        scene.request_around(&camera, client.render_distance)?;

        let fps = Rc::new(Cell::new(0.0));
        let mut debug = DebugScreen::new();
        debug.add_provider(Box::new(RendererInfoProvider(FixedRendererInfo(
            renderer_info,
        ))));
        debug.add_provider(Box::new(Lines(vec![
            format!("Seed: {}", settings.world.seed.0),
            format!(
                "XYZ: {:.1} / {:.1} / {:.1}",
                camera.position[0], camera.position[1], camera.position[2]
            ),
        ])));
        debug.add_provider(Box::new(Fps(fps.clone())));

        let state = Rc::new(RefCell::new(State {
            window: window.clone(),
            canvas,
            renderer: Box::new(renderer),
            scene: Box::new(scene),
            camera,
            size,
            max_dimension,
            clock: FrameClock::default(),
            fps,
            ui: UiLayer::new(UiSwitch::new(client.ui_enabled), ui),
            debug,
            running: true,
            last_error: None,
        }));

        let key_callback = {
            let state = state.clone();
            let callback = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
                if event.key() == "F3" {
                    event.prevent_default();
                    state.borrow_mut().toggle_debug_screen();
                }
            });
            window
                .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref())
                .map_err(|_| CanvasError::Setup("failed to listen for keys".into()))?;
            callback
        };

        let frame_callback: FrameCallback = Rc::new(RefCell::new(None));
        {
            let state = state.clone();
            let next = frame_callback.clone();
            let window = window.clone();
            *frame_callback.borrow_mut() = Some(Closure::new(move |now_ms: f64| {
                {
                    let mut state = state.borrow_mut();
                    if !state.running {
                        return;
                    }
                    if let Err(error) = state.frame(now_ms) {
                        web_sys::console::error_1(&error.to_string().into());
                        state.last_error = Some(error);
                        state.running = false;
                        return;
                    }
                }
                if let Some(callback) = next.borrow().as_ref() {
                    let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
                }
            }));
        }
        if let Some(callback) = frame_callback.borrow().as_ref() {
            window
                .request_animation_frame(callback.as_ref().unchecked_ref())
                .map_err(|_| CanvasError::Setup("failed to start the frame loop".into()))?;
        }

        Ok(Self {
            state,
            frame_callback,
            key_callback: Some(key_callback),
        })
    }

    pub fn set_ui_enabled(&self, enabled: bool) {
        let mut state = self.state.borrow_mut();
        state.ui.switch_mut().set(enabled);
        if !enabled {
            state.debug.switch_mut().set(false);
        }
    }

    /// The debug screen's lines; empty unless the UI is on and F3 was pressed.
    pub fn debug_lines(&self) -> Vec<String> {
        let mut state = self.state.borrow_mut();
        if !state.ui.switch_mut().enabled() {
            return Vec::new();
        }
        state.debug.lines().unwrap_or_default()
    }

    pub fn last_error(&self) -> Option<CanvasError> {
        self.state.borrow().last_error.clone()
    }

    pub fn stop(&mut self) {
        let mut state = self.state.borrow_mut();
        state.running = false;
        if let Some(callback) = self.key_callback.take() {
            let _ = state
                .window
                .remove_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref());
        }
        self.frame_callback.borrow_mut().take();
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        self.stop();
    }
}
