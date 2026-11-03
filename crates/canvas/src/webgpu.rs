use wasm_bindgen::JsValue;

/// Whether the browser exposes `navigator.gpu`.
pub fn webgpu_available() -> bool {
    web_sys::window()
        .and_then(|window| {
            js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("gpu")).ok()
        })
        .is_some_and(|gpu| !gpu.is_undefined() && !gpu.is_null())
}
