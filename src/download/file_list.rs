use super::downloader::ModelDownloader;
use crate::types::{FileInfo, RepoInfo};
use pyo3::prelude::*;
use std::path::PathBuf;

impl ModelDownloader {
    pub(crate) async fn prepare_download_list(
        &self,
        repo_info: &RepoInfo,
        model_id: &str,
        base_path: &PathBuf,
    ) -> PyResult<(Vec<FileInfo>, u64)> {
        // 创建基础目录
        std::fs::create_dir_all(base_path)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;

        // 过滤并准备文件列表
        let mut files_to_download = Vec::new();
        let mut total_size = 0u64;

        for file in &repo_info.files {
            if self.should_download(file) {
                if let Some(size) = file.size {
                    total_size += size;
                }
                files_to_download.push(file.clone());
            }
        }

        println!("Total files to download: {}", files_to_download.len());
        println!("Total size: {:.2} MB", total_size as f64 / 1024.0 / 1024.0);

        Ok((files_to_download, total_size))
    }
} 