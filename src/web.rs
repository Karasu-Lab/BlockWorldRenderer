use engine::canvas::{Canvas, CanvasOptions};
use engine::settings::{ClientSettings, Settings};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::backend::{ClearSource, WgpuFactory, world_generator};

const BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// A world drawing into a canvas.
#[wasm_bindgen]
pub struct WorldHandle {
    canvas: Canvas,
}

#[wasm_bindgen]
impl WorldHandle {
    pub fn stop(&mut self) {
        self.canvas.stop();
    }

    pub fn set_ui_enabled(&self, enabled: bool) {
        self.canvas.set_ui_enabled(enabled);
    }

    pub fn debug_lines(&self) -> Vec<String> {
        self.canvas.debug_lines()
    }
}

/// Draws the fixed world from the fixed camera, with the UI off, until the
/// returned handle is stopped.
#[wasm_bindgen]
pub async fn start(canvas: HtmlCanvasElement) -> Result<WorldHandle, JsError> {
    let settings = Settings {
        client: Some(ClientSettings::default()),
        ..Settings::default()
    };
    let canvas = Canvas::start(
        canvas,
        CanvasOptions::new(settings),
        world_generator(),
        WgpuFactory::new(ClearSource(BACKGROUND)),
    )
    .await
    .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(WorldHandle { canvas })
}
