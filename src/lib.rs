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
fn download(model_id: &str, local_dir: Option<String>, token: Option<String>) -> PyResult<String> {
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
    let downloader = ModelDownloader::new(config, auth)?;

    // 运行下载
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(downloader.download_model(model_id))
}

#[pymodule]
fn hfd(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download, m)?)?;
    Ok(())
} 