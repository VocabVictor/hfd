use pyo3::prelude::*;

mod auth;
mod config;
mod download;
mod types;

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
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: hfd <model_id> [--cache-dir <dir>] [--include <pattern>...] [--exclude <pattern>...] [--token <token>]");
        return Ok(());
    }

    let model_id = &args[1];
    let mut cache_dir = None;
    let mut include_patterns = None;
    let mut exclude_patterns = None;
    let mut token = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--cache-dir" => {
                if i + 1 < args.len() {
                    cache_dir = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    println!("Error: --cache-dir requires a value");
                    return Ok(());
                }
            }
            "--include" => {
                let mut patterns = Vec::new();
                i += 1;
                while i < args.len() && !args[i].starts_with("--") {
                    patterns.push(args[i].clone());
                    i += 1;
                }
                if !patterns.is_empty() {
                    include_patterns = Some(patterns);
                }
            }
            "--exclude" => {
                let mut patterns = Vec::new();
                i += 1;
                while i < args.len() && !args[i].starts_with("--") {
                    patterns.push(args[i].clone());
                    i += 1;
                }
                if !patterns.is_empty() {
                    exclude_patterns = Some(patterns);
                }
            }
            "--token" => {
                if i + 1 < args.len() {
                    token = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    println!("Error: --token requires a value");
                    return Ok(());
                }
            }
            _ => {
                println!("Unknown argument: {}", args[i]);
                return Ok(());
            }
        }
    }

    match download_model(model_id, cache_dir, include_patterns, exclude_patterns, token) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }

    Ok(())
}

#[pymodule]
fn hfd(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_model, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
} 