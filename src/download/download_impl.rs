use super::downloader::ModelDownloader;
use crate::download::file_list;
use crate::download::repo;
use pyo3::prelude::*;
use std::path::PathBuf;

impl ModelDownloader {
    pub(crate) async fn download_model(&mut self, model_id: &str) -> PyResult<String> {
        // 获取仓库信息
        let repo_info = repo::get_repo_info(
            &self.client,
            &self.config.endpoint,
            model_id,
            &self.auth,
        ).await?;

        // 准备下载目录
        let base_path = PathBuf::from(&self.cache_dir).join(model_id.split('/').last().unwrap_or(model_id));
        std::fs::create_dir_all(&base_path)?;

        // 准备下载列表
        let (files_to_download, total_size) = self.prepare_download_list(&repo_info, model_id, &base_path).await?;

        // 开始下载
        println!("Found {} files to download, total size: {:.2} MB", 
            files_to_download.len(), total_size as f64 / 1024.0 / 1024.0);

        // 保存仓库信息以供后续使用
        self.repo_info = Some(repo_info);

        Ok(format!("Downloaded model {} to {}", model_id, base_path.display()))
    }
} 