use pyo3::prelude::*;

mod auth;
mod config;
mod download;
mod types;
mod cli;

pub use auth::Auth;
pub use config::Config;
pub use download::ModelDownloader;

#[pyfunction]
fn download_model(
    model_id: &str,
    cache_dir: Option<String>,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    token: Option<String>,
    proxy_on: Option<bool>,
) -> PyResult<String> {
    let mut downloader = ModelDownloader::new(
        cache_dir,
        include_patterns,
        exclude_patterns,
        token,
    )?;
    
    if proxy_on.unwrap_or(false) {
        downloader.enable_proxy()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to enable proxy: {}", e)))?;
    }
    
    let model_id = model_id.to_string();
    Python::with_gil(|py| {
        pyo3_asyncio::tokio::run(py, async move {
            downloader.download_model(&model_id).await
        })
    })
}

#[pyfunction]
fn main() -> PyResult<()> {
    if let Some(args) = cli::parse_args() {
        match download_model(
            &args.model_id,
            args.local_dir,
            args.include_patterns,
            args.exclude_patterns,
            args.hf_token,
            Some(args.proxy_on),
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
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
} 