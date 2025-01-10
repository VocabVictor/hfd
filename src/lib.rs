use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

mod auth;
mod config;
mod download;
mod types;
mod cli;

pub use auth::Auth;
pub use config::Config;
pub use download::downloader::ModelDownloader;

#[pyfunction]
#[pyo3(signature = (model_id, is_dataset=false, cache_dir=None, include_patterns=None, exclude_patterns=None, token=None))]
pub fn download_file(
    model_id: String,
    is_dataset: bool,
    cache_dir: Option<String>,
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
        if is_dataset {
            downloader.download_dataset_impl(&model_id).await
        } else {
            downloader.download_model_impl(&model_id).await
        }
    })
}

#[pyfunction]
pub fn main() -> PyResult<()> {
    if let Some(args) = cli::parse_args() {
        match download_file(
            args.model_id.to_string(),
            false,  // 默认下载模型
            args.local_dir,
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
fn hfd(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_file, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
} 