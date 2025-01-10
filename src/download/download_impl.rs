use crate::download::downloader::ModelDownloader;
use pyo3::prelude::*;
use std::path::PathBuf;

impl ModelDownloader {
    pub(crate) async fn download_model_impl(&mut self, model_id: &str) -> PyResult<String> {
        // 获取仓库信息
        let repo_info = self.get_repo_info(model_id).await?;
        self.repo_info = Some(repo_info.clone());

        // 创建下载目录
        let base_path = PathBuf::from(&self.cache_dir).join(model_id);
        
        // 下载文件
        self.download_model(model_id, &base_path).await?;

        Ok(base_path.to_string_lossy().to_string())
    }

    pub(crate) async fn download_dataset_impl(&mut self, model_id: &str) -> PyResult<String> {
        // 创建下载目录
        let base_path = PathBuf::from(&self.cache_dir).join(model_id);
        
        // 下载文件
        self.download_dataset(model_id, &base_path).await?;

        Ok(base_path.to_string_lossy().to_string())
    }
} 