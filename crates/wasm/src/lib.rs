use wasm_bindgen::prelude::*;

/// The core version, so the web app can show what it runs.
#[wasm_bindgen]
pub fn version() -> String {
    diagcalc_core::VERSION.to_string()
}

/// The largest response returned, in bytes of JSON: a larger one is replaced by an error, so a
/// shared document cannot make the page hold or copy hundreds of megabytes.
pub const MAX_RESPONSE: usize = 32 << 20;

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
    let text = v.to_string();
    if text.len() > MAX_RESPONSE {
        return serde_json::json!({
            "status": "error",
            "kind": "limit",
            "message": format!("the response has {} bytes, more than the limit of {MAX_RESPONSE}; ask for less", text.len()),
        })
        .to_string();
    }
    text
}
