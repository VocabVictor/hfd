use super::downloader::ModelDownloader;
use crate::download::repo;
use crate::download::DownloadTask;
use pyo3::prelude::*;
use std::path::PathBuf;
use std::collections::HashMap;

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

        // 分离根目录文件和子目录文件
        let mut root_files = Vec::new();
        let mut folder_groups: HashMap<String, Vec<_>> = HashMap::new();

        for file in files_to_download {
            let path = PathBuf::from(&file.rfilename);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    // 子目录文件
                    let folder = parent.to_string_lossy().into_owned();
                    folder_groups.entry(folder).or_default().push(file);
                } else {
                    // 根目录文件
                    root_files.push(file);
                }
            } else {
                // 根目录文件
                root_files.push(file);
            }
        }

        // 保存仓库信息以供后续使用
        self.repo_info = Some(serde_json::to_value(&repo_info)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?);

        // 下载根目录文件
        for file in root_files {
            let file_path = base_path.join(&file.rfilename);
            let task = if let Some(size) = file.size {
                if size > self.config.chunk_size as u64 {
                    DownloadTask::new_chunked_file(
                        file.clone(),
                        file_path,
                        self.config.chunk_size,
                        self.config.max_retries,
                        "root",
                    )
                } else {
                    DownloadTask::new_small_file(
                        file.clone(),
                        file_path,
                        "root",
                    )
                }
            } else {
                DownloadTask::new_small_file(
                    file,
                    file_path,
                    "root",
                )
            };

            // 执行下载任务
            if let Err(e) = self.download_file(task, model_id) {
                return Err(e);
            }
        }

        // 下载子目录文件
        for (folder, files) in folder_groups {
            // 创建文件夹下载任务
            let task = DownloadTask::new_folder(folder.clone(), files, base_path.clone());

            // 执行下载任务
            if let Err(e) = self.download_folder(task, model_id) {
                return Err(e);
            }
        }

        Ok(format!("Downloaded model {} to {}", model_id, base_path.display()))
    }
} 