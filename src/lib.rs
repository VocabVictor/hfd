use pyo3::prelude::*;

mod auth;
mod config;
mod download;
mod types;
mod cli;

#[pyfunction]
fn download_file(
    model_id: String,
    local_dir: Option<String>,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    hf_token: Option<String>,
) -> PyResult<String> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e)))?;
    
    rt.block_on(cli::download_file(model_id, local_dir, include_patterns, exclude_patterns, hf_token))
}

#[pyfunction]
fn main() -> PyResult<()> {
    cli::run_cli()
}

#[pymodule]
fn hfd(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_file, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
} 