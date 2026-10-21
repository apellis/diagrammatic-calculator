use pyo3::prelude::*;

/// The core version, so notebooks can show what they run.
#[pyfunction]
fn version() -> &'static str {
    diagcalc_core::VERSION
}

/// Runs an operation: `op` names it (see `spec/ops/`), `request` is its JSON request, and the
/// JSON response comes back as a string.
#[pyfunction]
fn call(op: &str, request: &str) -> String {
    let v = match serde_json::from_str(request) {
        Ok(input) => diagcalc_core::ops::call(op, &input),
        Err(e) => {
            serde_json::json!({ "status": "error", "kind": "syntax", "message": e.to_string() })
        }
    };
    v.to_string()
}

#[pymodule]
fn diagcalc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(call, m)?)?;
    Ok(())
}
