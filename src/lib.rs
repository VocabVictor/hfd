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
#[pyo3(signature = (model_id, cache_dir=None, include_patterns=None, exclude_patterns=None, token=None))]
pub fn download_file(
    model_id: String,
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
        // 获取仓库信息
        let repo_info = download::repo::get_repo_info(
            &downloader.client,
            &downloader.config,
            &model_id,
            &downloader.auth,
        ).await?;

        // 根据仓库信息判断是否为数据集
        let is_dataset = repo_info.is_dataset();
        println!("Detected repository type: {}", if is_dataset { "dataset" } else { "model" });

        // 创建下载目录
        let base_path = std::path::PathBuf::from(&downloader.cache_dir).join(&model_id);
        
        // 下载文件
        downloader.download_files(&model_id, &base_path, is_dataset).await?;

        Ok(base_path.to_string_lossy().to_string())
    })
}

#[pyfunction]
pub fn main() -> PyResult<()> {
    if let Some(args) = cli::parse_args() {
        match download_file(
            args.model_id.to_string(),
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