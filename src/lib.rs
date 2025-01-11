use pyo3::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref INTERRUPT_FLAG: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

mod auth;
mod config;
mod download;
mod types;
mod cli;

fn setup_interrupt_handler() {
    let flag = INTERRUPT_FLAG.clone();
    ctrlc::set_handler(move || {
        flag.store(true, Ordering::SeqCst);
        println!("\nReceived Ctrl+C, interrupting downloads...");
    }).expect("Error setting Ctrl+C handler");
}

#[pyfunction]
fn download_file(
    model_id: String,
    local_dir: Option<String>,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    hf_token: Option<String>,
) -> PyResult<String> {
    INTERRUPT_FLAG.store(false, Ordering::SeqCst);
    setup_interrupt_handler();

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e)))?;
    
    rt.block_on(cli::download_file(model_id, local_dir, include_patterns, exclude_patterns, hf_token))
}

#[pyfunction]
fn main() -> PyResult<()> {
    INTERRUPT_FLAG.store(false, Ordering::SeqCst);
    setup_interrupt_handler();
    
    cli::run_cli()
}

#[pymodule]
fn hfd(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_file, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
} 