use wasm_bindgen::prelude::*;

/// The core version, so the web app can show what it runs.
#[wasm_bindgen]
pub fn version() -> String {
    diagcalc_core::VERSION.to_string()
}
