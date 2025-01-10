mod types;
mod utils;
mod download;
mod cli;
mod config;

use pyo3::prelude::*;
use crate::download::ModelDownloader;
use tokio::runtime::Runtime;

#[pyfunction]
pub fn download_model(
    model_id: &str,
    cache_dir: Option<String>,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    token: Option<String>,
) -> PyResult<String> {
    let rt = Runtime::new().unwrap();
    let mut downloader = ModelDownloader::new(
        cache_dir,
        include_patterns,
        exclude_patterns,
        token,
    )?;
    rt.block_on(downloader.download_model(model_id))
}

#[pyfunction]
fn main() -> PyResult<()> {
    if let Some(args) = cli::parse_args() {
        match download_model(
            &args.model_id,
            args.local_dir,
            None,  // include_patterns
            None,  // exclude_patterns
            args.hf_token,
        ) {
            Ok(result) => println!("Download completed: {}", result),
            Err(e) => println!("Error during download: {:?}", e),
        }
    }
    Ok(())
}

#[pymodule]
fn hfd(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_model, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
} 