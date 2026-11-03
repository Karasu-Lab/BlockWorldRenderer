use wasm_bindgen::{JsCast, JsValue};

/// Whether the browser exposes `navigator.gpu`.
pub fn webgpu_available() -> bool {
    web_sys::window()
        .and_then(|window| {
            js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("gpu")).ok()
        })
        .is_some_and(|gpu| !gpu.is_undefined() && !gpu.is_null())
}

/// Whether a canvas can open a WebGL2 context.
pub fn webgl2_available() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.create_element("canvas").ok())
        .and_then(|canvas| canvas.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .and_then(|canvas| canvas.get_context("webgl2").ok().flatten())
        .is_some()
}
