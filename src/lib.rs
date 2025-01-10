mod types;
mod utils;
mod download;
mod cli;
mod config;

use pyo3::prelude::*;
use crate::types::AuthInfo;
use crate::config::Config;
use crate::download::ModelDownloader;

#[pyfunction]
fn download_model(model_id: &str, local_dir: Option<String>, token: Option<String>) -> PyResult<String> {
    println!("Starting download for model: {}", model_id);
    
    // 创建配置
    let mut config = Config::default();
    if let Some(dir) = local_dir {
        config.use_local_dir = true;
        config.local_dir_base = dir;
    }

    // 创建认证信息
    let auth = AuthInfo {
        token: token.or_else(|| std::env::var("HF_TOKEN").ok()),
    };

    // 创建下载器
    let downloader = ModelDownloader::new(
        Some(config.get_model_dir(model_id)),
        None,  // include_patterns
        None,  // exclude_patterns
        auth.token,
    )?;

    // 运行下载
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(downloader.download_model(model_id))
}

#[pyfunction]
fn main() -> PyResult<()> {
    if let Some(args) = cli::parse_args() {
        match download_model(&args.model_id, args.local_dir, args.hf_token) {
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