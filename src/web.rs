use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError};

use engine::settings::{ClientSettings, Settings};
use js_sys::JsString;
use pomme_client::viewer::assets::{self, Endpoints, Progress};
use pomme_client::viewer::steel::{PROTOCOL, SteelWorld, VERSION, install_thread_spawner};
use pomme_client::viewer::web::{self, ViewerHandle};
use pomme_client::viewer::{ViewerCamera, ViewerConfig};
use tracing_subscriber::prelude::*;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

static LOADING: Mutex<Progress> = Mutex::new(Progress {
    done: 0,
    total: 0,
    status: String::new(),
});

fn loading() -> MutexGuard<'static, Progress> {
    LOADING.lock().unwrap_or_else(PoisonError::into_inner)
}

#[wasm_bindgen]
pub fn load_progress() -> f32 {
    loading().fraction()
}

#[wasm_bindgen]
pub fn load_status() -> String {
    loading().status.clone()
}

static RENDERER_AVAILABLE: AtomicBool = AtomicBool::new(false);

#[wasm_bindgen]
pub fn renderer_available() -> bool {
    RENDERER_AVAILABLE.load(Ordering::Relaxed)
}

#[wasm_bindgen]
pub struct WorldHandle {
    viewer: ViewerHandle,
}

#[wasm_bindgen]
impl WorldHandle {
    pub fn stop(&self) {
        self.viewer.stop();
    }

    pub fn set_ui_enabled(&self, enabled: bool) {
        self.viewer.set_ui_enabled(enabled);
    }

    pub fn debug_lines(&self) -> Vec<String> {
        self.viewer.debug_lines()
    }
}

#[wasm_bindgen(module = "/src/steel_worker.js")]
extern "C" {
    #[wasm_bindgen(js_name = enableSteelWorkers)]
    fn enable_steel_workers(main_js: JsString, module: JsValue, memory: JsValue);

    #[wasm_bindgen(js_name = requestSteelWorker)]
    fn request_steel_worker(task: usize, name: &str);
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(thread_local_v2, js_namespace = ["import", "meta"], js_name = url)]
    static MAIN_JS: JsString;
}

type SteelTask = Box<dyn FnOnce() + Send>;

#[wasm_bindgen]
pub fn steel_worker_entry(task: usize) {
    let task = unsafe { Box::from_raw(task as *mut SteelTask) };
    task();
}

fn spawn_steel_worker(name: String, task: SteelTask) -> io::Result<()> {
    request_steel_worker(Box::into_raw(Box::new(task)) as usize, &name);
    Ok(())
}

fn failure(error: impl ToString) -> JsError {
    JsError::new(&error.to_string())
}

fn init_logging() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let console = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .without_time()
            .with_writer(tracing_web::MakeConsoleWriter);
        tracing_subscriber::registry()
            .with(console.with_filter(tracing_subscriber::filter::LevelFilter::INFO))
            .init();
        std::panic::set_hook(Box::new(|info| tracing::error!("{info}")));
    });
}

#[wasm_bindgen]
pub async fn start(
    canvas: HtmlCanvasElement,
    version_manifest: Option<String>,
    resources_base: Option<String>,
) -> Result<WorldHandle, JsError> {
    init_logging();
    let settings = Settings {
        client: Some(ClientSettings::default()),
        ..Settings::default()
    };
    let client = settings.client.clone().unwrap_or_default();

    if !engine::wgpu::backend::browser_supported().await {
        return Err(failure("neither WebGPU nor WebGL2 is available"));
    }
    RENDERER_AVAILABLE.store(true, Ordering::Relaxed);

    let mut endpoints = Endpoints::default();
    if let Some(url) = version_manifest {
        endpoints.version_manifest = url;
    }
    if let Some(url) = resources_base {
        endpoints.resources_base = url;
    }
    let assets = assets::prepare(
        VERSION,
        endpoints,
        client.resource_pack_location.clone(),
        &|progress| *loading() = progress,
    )
    .await
    .map_err(failure)?;

    enable_steel_workers(
        MAIN_JS.with(Clone::clone),
        wasm_bindgen::module(),
        wasm_bindgen::memory(),
    );
    install_thread_spawner(spawn_steel_worker);
    let seed = settings.world.seed.0;
    let (sender, receiver) = futures_channel::oneshot::channel();
    engine::thread::spawn("world-open", move || {
        let _ = sender.send(SteelWorld::open(seed, None));
    })
    .map_err(failure)?;
    let (preloaded_sender, preloaded) = futures_channel::oneshot::channel();
    {
        let dirs = assets.dirs.clone();
        let asset_index = assets.asset_index.clone();
        engine::thread::spawn("render-assets", move || {
            pomme_client::viewer::preload_render_assets(&dirs, &asset_index);
            let _ = preloaded_sender.send(());
        })
        .map_err(failure)?;
    }
    let world = receiver.await.map_err(failure)?.map_err(failure)?;
    preloaded.await.map_err(failure)?;

    let camera = settings.camera;
    let config = ViewerConfig {
        camera: ViewerCamera {
            position: camera.position.map(f64::from),
            yaw_deg: camera.yaw.to_degrees(),
            pitch_deg: -camera.pitch.to_degrees(),
        },
        render_distance: client.render_distance,
        dirs: assets.dirs,
        asset_index: assets.asset_index,
        vsync: client.vsync,
        ui_enabled: client.ui_enabled,
        protocol: PROTOCOL,
        resource_pack_location: assets.resource_pack_location,
    };
    let viewer = web::start(canvas, config, Arc::new(world)).map_err(failure)?;
    Ok(WorldHandle { viewer })
}
