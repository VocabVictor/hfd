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
) -> PyResult<String> {
    let mut downloader = ModelDownloader::new(
        cache_dir,
        include_patterns,
        exclude_patterns,
        token,
    )?;
    
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