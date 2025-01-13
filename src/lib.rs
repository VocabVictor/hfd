use pyo3::prelude::*;
use tokio::sync::broadcast;

mod config;
mod download;
mod types;
mod cli;

pub struct ShutdownHandle {
    tx: broadcast::Sender<()>,
}

impl ShutdownHandle {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(());
    }
}

fn setup_interrupt_handler(handle: ShutdownHandle) {
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, interrupting downloads...");
        handle.shutdown();
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
    let handle = ShutdownHandle::new();
    setup_interrupt_handler(handle.clone());

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e)))?;
    
    rt.block_on(cli::download_file(model_id, local_dir, include_patterns, exclude_patterns, hf_token, handle))
}

#[pyfunction]
fn main() -> PyResult<()> {
    let handle = ShutdownHandle::new();
    setup_interrupt_handler(handle.clone());
    
    cli::run_cli()
}

#[pymodule]
fn hfd(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_file, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
} 