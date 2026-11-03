use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use engine::settings::{ClientSettings, Settings};
use pomme_client::viewer::assets::{self, Endpoints, Progress};
use pomme_client::viewer::steel::{PROTOCOL, SteelWorld, VERSION, install_thread_spawner};
use pomme_client::viewer::web::{self, ViewerHandle};
use pomme_client::viewer::{ViewerCamera, ViewerConfig};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// How far the assets' download has got, for the page to show while `start`
/// waits.
static LOADING: Mutex<Progress> = Mutex::new(Progress {
    done: 0,
    total: 0,
    status: String::new(),
});

fn loading() -> MutexGuard<'static, Progress> {
    LOADING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The fraction of the assets downloaded, 0 to 1.
#[wasm_bindgen]
pub fn load_progress() -> f32 {
    loading().fraction()
}

/// What the download is doing.
#[wasm_bindgen]
pub fn load_status() -> String {
    loading().status.clone()
}

/// A world drawing into a canvas.
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

/// Starts a Steel worker on the page's thread pool, as SteelMC asks of a host
/// on wasm32.
fn spawn_steel_worker(name: String, task: Box<dyn FnOnce() + Send>) -> io::Result<()> {
    engine::thread::spawn(name, task)
        .map(|_| ())
        .map_err(io::Error::other)
}

fn failure(error: impl ToString) -> JsError {
    JsError::new(&error.to_string())
}

/// Draws the fixed world from the fixed camera, with the UI off, until the
/// returned handle is stopped. The assets come from Mojang's servers unless a
/// mirror is given for the version manifest and the asset objects.
#[wasm_bindgen]
pub async fn start(
    canvas: HtmlCanvasElement,
    version_manifest: Option<String>,
    resources_base: Option<String>,
) -> Result<WorldHandle, JsError> {
    let settings = Settings {
        client: Some(ClientSettings::default()),
        ..Settings::default()
    };
    let client = settings.client.clone().unwrap_or_default();

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

    install_thread_spawner(spawn_steel_worker);
    let world = SteelWorld::open(settings.world.seed.0, None).map_err(failure)?;

    // The settings' pitch is positive looking up; the client's is positive
    // looking down.
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
