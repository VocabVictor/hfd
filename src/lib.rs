use pyo3::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use lazy_static::lazy_static;
use tokio::runtime::Runtime;

lazy_static! {
    pub static ref INTERRUPT_FLAG: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

mod auth;
mod config;
mod download;
mod types;
pub mod cli;

pub fn setup_interrupt_handler() {
    let flag = INTERRUPT_FLAG.clone();
    ctrlc::set_handler(move || {
        flag.store(true, Ordering::SeqCst);
        println!("\nReceived Ctrl+C, interrupting downloads...");
    }).expect("Error setting Ctrl+C handler");
}

#[pyfunction]
pub fn download(
    model_id: String,
    local_dir: Option<String>,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    hf_token: Option<String>,
) -> PyResult<String> {
    let rt = Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e)))?;

    let mut config = crate::config::Config::default();
    if let Some(dir) = local_dir {
        config.local_dir_base = dir;
    }
    
    rt.block_on(crate::cli::download_file(model_id, config))
}

#[pyfunction]
fn main() -> PyResult<()> {
    INTERRUPT_FLAG.store(false, Ordering::SeqCst);
    setup_interrupt_handler();
    
    cli::run_cli()
}

#[pymodule]
fn hfd(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
} 