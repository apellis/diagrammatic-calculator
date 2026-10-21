use wasm_bindgen::prelude::*;

/// The core version, so the web app can show what it runs.
#[wasm_bindgen]
pub fn version() -> String {
    diagcalc_core::VERSION.to_string()
}

/// Runs an operation: `op` names it (see `spec/ops/`), `request` is its JSON request, and the
/// JSON response comes back as a string.
#[wasm_bindgen]
pub fn call(op: &str, request: &str) -> String {
    let v = match serde_json::from_str(request) {
        Ok(input) => diagcalc_core::ops::call(op, &input),
        Err(e) => {
            serde_json::json!({ "status": "error", "kind": "syntax", "message": e.to_string() })
        }
    };
    v.to_string()
}
