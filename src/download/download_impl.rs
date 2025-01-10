use crate::download::downloader::ModelDownloader;
use crate::download::repo;
use pyo3::prelude::*;
use std::path::PathBuf;

impl ModelDownloader {
    pub(crate) async fn download_model_impl(&mut self, model_id: &str) -> PyResult<String> {
        // 获取仓库信息
        let repo_info = repo::get_repo_info(
            &self.client,
            &self.config,
            model_id,
            &self.auth,
        ).await?;
        self.repo_info = Some(serde_json::to_value(&repo_info)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?);

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