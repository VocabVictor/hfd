use pyo3::prelude::*;

mod auth;
mod config;
mod download;
mod types;
mod cli;

pub use auth::Auth;
pub use config::Config;
pub use download::downloader::ModelDownloader;

pub fn download_model(
    cache_dir: Option<String>,
    model_id: String,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    token: Option<String>,
) -> PyResult<String> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let mut downloader = ModelDownloader::new(
        cache_dir,
        include_patterns,
        exclude_patterns,
        token,
    )?;

    rt.block_on(async move {
        downloader.download_model_impl(&model_id).await
    })
}

pub fn download_dataset(
    cache_dir: Option<String>,
    model_id: String,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    token: Option<String>,
) -> PyResult<String> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let mut downloader = ModelDownloader::new(
        cache_dir,
        include_patterns,
        exclude_patterns,
        token,
    )?;

    rt.block_on(async move {
        downloader.download_dataset_impl(&model_id).await
    })
}

#[pyfunction]
pub fn main() -> PyResult<()> {
    if let Some(args) = cli::parse_args() {
        match download_model(
            args.local_dir,
            args.model_id.to_string(),
            args.include_patterns,
            args.exclude_patterns,
            args.hf_token,
        ) {
            Ok(result) => println!("{}", result),
            Err(e) => println!("Error: {}", e),
        }
    }
    Ok(())
}

#[pymodule]
fn hfd(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_model, m)?)?;
    m.add_function(wrap_pyfunction!(download_dataset, m)?)?;
    Ok(())
} 