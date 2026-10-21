use pyo3::prelude::*;

/// The core version, so notebooks can show what they run.
#[pyfunction]
fn version() -> &'static str {
    diagcalc_core::VERSION
}

#[pymodule]
fn diagcalc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
