use super::downloader::ModelDownloader;
use pyo3::prelude::*;
use std::path::PathBuf;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::atomic::Ordering;

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

        // 创建总进度条
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));

        // 保存仓库信息以供后续使用
        self.repo_info = Some(repo_info);

        // 下载所有文件
        for file in files_to_download {
            if !self.running.load(Ordering::SeqCst) {
                pb.finish_with_message("Download cancelled".to_string());
                return Ok("Download cancelled".to_string());
            }

            let file_path = base_path.join(&file.rfilename);
            if let Some(size) = file.size {
                let file_url = self.get_file_url(model_id, &file.rfilename)?;
                pb.set_message(format!("Downloading {}", file.rfilename));

                // 使用分块下载
                if let Err(e) = Self::download_file_with_chunks(
                    &self.client,
                    file_url,
                    file_path.clone(),
                    size,
                    self.config.chunk_size,
                    self.config.max_retries,
                    self.auth.token.clone(),
                    pb.clone(),
                    self.running.clone(),
                ).await {
                    let error_msg = format!("Failed to download {}: {}", file.rfilename, e);
                    pb.finish_with_message(error_msg.clone());
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(error_msg));
                }
            }
        }

        pb.finish_with_message("Download completed".to_string());
        Ok(format!("Downloaded model {} to {}", model_id, base_path.display()))
    }
} 