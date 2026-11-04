use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError};

use engine::cache::CacheNotifier;
use engine::wgpu::backend::RendererProfile;
use engine::settings::{CameraSettings, ClientSettings, Settings};
use js_sys::JsString;
use pomme_client::viewer::assets::{self, Endpoints, HttpTransfer, Progress, Stage};
use pomme_client::viewer::steel::{PROTOCOL, SteelWorld, VERSION, install_thread_spawner};
use pomme_client::viewer::web::{self, ViewerHandle};
use pomme_client::viewer::{ViewerCamera, ViewerConfig, WorldSource};
use tracing_subscriber::prelude::*;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::notify::{self, DownloadNotifier, NotifyTransfer};
#[cfg(feature = "replay")]
use crate::replay::ReplaySession;
#[cfg(feature = "save")]
use crate::save::SaveData;

static LOADING: Mutex<Progress> = Mutex::new(Progress {
    stage: Stage::Info,
    done: 0,
    total: 0,
    status: String::new(),
    file_done: 0,
    file_total: 0,
});

fn loading() -> MutexGuard<'static, Progress> {
    LOADING.lock().unwrap_or_else(PoisonError::into_inner)
}

#[wasm_bindgen]
pub fn load_progress() -> f32 {
    loading().fraction()
}

/// The progress of the one file currently downloading, or a negative
/// value while no file download is in flight.
#[wasm_bindgen]
pub fn load_file_progress() -> f32 {
    let progress = loading();
    if progress.file_total == 0 {
        -1.0
    } else {
        progress.file_done as f32 / progress.file_total as f32
    }
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

/// The version of this package.
#[wasm_bindgen]
pub fn package_version() -> String {
    format!("v{}+mc{}", env!("CARGO_PKG_VERSION"), VERSION)
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

    /// Moves the camera to a position and look direction, in degrees.
    pub fn set_camera(&self, x: f64, y: f64, z: f64, yaw_deg: f32, pitch_deg: f32) {
        self.viewer.set_camera(ViewerCamera {
            position: [x, y, z],
            yaw_deg,
            pitch_deg,
        });
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

/// The assets every viewer start needs, prepared once per start.
struct Prepared {
    client: ClientSettings,
    dirs: assets::ViewerAssets,
}

async fn prepare(
    version_manifest: Option<String>,
    resources_base: Option<String>,
    notifier: Option<DownloadNotifier>,
    cache: Option<CacheNotifier>,
) -> Result<Prepared, JsError> {
    init_logging();
    if let Some(cache) = cache {
        engine::cache::listen(cache);
    }
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
    let report = |progress: Progress| {
        notify::notify(&progress);
        *loading() = progress;
    };
    let version = pomme_client::viewer::steel::VERSION;
    let location = client.resource_pack_location.clone();
    let dirs = match notifier {
        Some(notifier) => {
            notify::listen(notifier.clone().unchecked_into::<DownloadNotifier>());
            let transfer = NotifyTransfer::new(notifier, assets::ASSETS_NAMESPACE);
            assets::prepare_with(&transfer, version, endpoints, location, &report).await
        }
        None => {
            assets::prepare_with(
                &HttpTransfer::default(),
                version,
                endpoints,
                location,
                &report,
            )
            .await
        }
    }
    .map_err(failure)?;
    Ok(Prepared { client, dirs })
}

/// Opens the generated world the registry data (and the seed view)
/// comes from, on its own thread as the world builder requires.
async fn open_generated_world(seed: i64) -> Result<SteelWorld, JsError> {
    enable_steel_workers(
        MAIN_JS.with(Clone::clone),
        wasm_bindgen::module(),
        wasm_bindgen::memory(),
    );
    install_thread_spawner(spawn_steel_worker);
    let (sender, receiver) = futures_channel::oneshot::channel();
    engine::thread::spawn("world-open", move || {
        let _ = sender.send(SteelWorld::open(seed, None));
    })
    .map_err(failure)?;
    receiver.await.map_err(failure)?.map_err(failure)
}

/// Starts the viewer on `source` once the render assets are warm.
async fn start_viewer(
    canvas: HtmlCanvasElement,
    prepared: Prepared,
    camera: ViewerCamera,
    source: Arc<dyn WorldSource>,
) -> Result<WorldHandle, JsError> {
    let (preloaded_sender, preloaded) = futures_channel::oneshot::channel();
    {
        let dirs = prepared.dirs.dirs.clone();
        let asset_index = prepared.dirs.asset_index.clone();
        engine::thread::spawn("render-assets", move || {
            pomme_client::viewer::preload_render_assets(&dirs, &asset_index);
            let _ = preloaded_sender.send(());
        })
        .map_err(failure)?;
    }
    preloaded.await.map_err(failure)?;

    let client = prepared.client;
    let config = ViewerConfig {
        camera,
        render_distance: RendererProfile::detect().render_distance(client.render_distance),
        dirs: prepared.dirs.dirs,
        asset_index: prepared.dirs.asset_index,
        vsync: client.vsync,
        ui_enabled: client.ui_enabled,
        protocol: PROTOCOL,
        resource_pack_location: prepared.dirs.resource_pack_location,
    };
    let viewer = web::start(canvas, config, source).map_err(failure)?;
    Ok(WorldHandle { viewer })
}

/// The camera a start uses: the caller's `[x, y, z, yaw degrees, pitch
/// degrees]` when given, else the settings default.
fn viewer_camera(given: Option<Vec<f64>>) -> ViewerCamera {
    if let Some([x, y, z, yaw, pitch]) = given.as_deref() {
        return ViewerCamera {
            position: [*x, *y, *z],
            yaw_deg: *yaw as f32,
            pitch_deg: *pitch as f32,
        };
    }
    let camera = CameraSettings::default();
    ViewerCamera {
        position: camera.position.map(f64::from),
        yaw_deg: camera.yaw.to_degrees(),
        pitch_deg: -camera.pitch.to_degrees(),
    }
}

fn default_seed() -> i64 {
    Settings::default().world.seed.0
}

#[wasm_bindgen]
pub async fn start(
    canvas: HtmlCanvasElement,
    version_manifest: Option<String>,
    resources_base: Option<String>,
    seed: Option<i64>,
    camera: Option<Vec<f64>>,
    notifier: Option<DownloadNotifier>,
    cache: Option<CacheNotifier>,
) -> Result<WorldHandle, JsError> {
    let prepared = prepare(version_manifest, resources_base, notifier, cache).await?;
    let world = open_generated_world(seed.unwrap_or_else(default_seed)).await?;
    start_viewer(canvas, prepared, viewer_camera(camera), Arc::new(world)).await
}

/// An opened replay file, played through [`start_replay`].
#[cfg(feature = "replay")]
#[wasm_bindgen]
pub struct ReplayHandle {
    session: ReplaySession,
}

#[cfg(feature = "replay")]
#[wasm_bindgen]
pub fn open_replay(bytes: &[u8]) -> Result<ReplayHandle, JsError> {
    Ok(ReplayHandle {
        session: ReplaySession::open(bytes).map_err(failure)?,
    })
}

#[cfg(feature = "replay")]
#[wasm_bindgen]
impl ReplayHandle {
    pub fn duration_ms(&self) -> u32 {
        self.session.metadata().duration_ms
    }

    pub fn position_ms(&self) -> u32 {
        self.session.position_ms()
    }

    pub fn mc_version(&self) -> String {
        self.session.metadata().mc_version.clone()
    }

    pub fn protocol(&self) -> Option<i32> {
        self.session.metadata().protocol
    }

    /// Moves the playhead; packets in between are applied to the world
    /// the viewer reads, backward moves replay from the start.
    pub fn apply_until(&mut self, target_ms: u32) {
        self.session.apply_until(target_ms);
    }

    /// The entities the recording has shown so far, as JSON.
    pub fn entities_json(&self) -> String {
        crate::entity::entities_json(&self.session.entities())
    }
}

#[cfg(feature = "replay")]
#[wasm_bindgen]
pub async fn start_replay(
    canvas: HtmlCanvasElement,
    version_manifest: Option<String>,
    resources_base: Option<String>,
    seed: Option<i64>,
    camera: Option<Vec<f64>>,
    replay: &ReplayHandle,
    notifier: Option<DownloadNotifier>,
    cache: Option<CacheNotifier>,
) -> Result<WorldHandle, JsError> {
    let prepared = prepare(version_manifest, resources_base, notifier, cache).await?;
    let base = open_generated_world(seed.unwrap_or_else(default_seed)).await?;
    let world = crate::replay::ReplayWorld::new(base, replay.session.state());
    start_viewer(canvas, prepared, viewer_camera(camera), Arc::new(world)).await
}

/// An opened world save, shown through [`start_save`].
#[cfg(feature = "save")]
#[wasm_bindgen]
pub struct SaveHandle {
    data: SaveData,
}

#[cfg(feature = "save")]
#[wasm_bindgen]
pub fn open_save(bytes: &[u8]) -> Result<SaveHandle, JsError> {
    Ok(SaveHandle {
        data: SaveData::open(bytes).map_err(failure)?,
    })
}

#[cfg(feature = "save")]
#[wasm_bindgen]
impl SaveHandle {
    pub fn seed(&self) -> i64 {
        self.data.seed
    }

    /// The save's players, as JSON.
    pub fn players_json(&self) -> String {
        crate::entity::entities_json(&self.data.players)
    }
}

#[cfg(feature = "save")]
#[wasm_bindgen]
pub async fn start_save(
    canvas: HtmlCanvasElement,
    version_manifest: Option<String>,
    resources_base: Option<String>,
    camera: Option<Vec<f64>>,
    save: SaveHandle,
    notifier: Option<DownloadNotifier>,
    cache: Option<CacheNotifier>,
) -> Result<WorldHandle, JsError> {
    let prepared = prepare(version_manifest, resources_base, notifier, cache).await?;
    let base = open_generated_world(save.data.seed).await?;
    let world = save.data.world(base);
    start_viewer(canvas, prepared, viewer_camera(camera), Arc::new(world)).await
}
