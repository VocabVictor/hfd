use super::download_task::DownloadTask;
use super::chunk;
use super::progress::DownloadProgress;
use crate::config::Config;
use crate::auth::Auth;
use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use pyo3::prelude::*;
use futures::StreamExt;

pub struct ModelDownloader {
    pub(crate) client: Client,
    pub(crate) config: Config,
    pub(crate) auth: Auth,
    pub(crate) cache_dir: String,
    pub(crate) running: Arc<AtomicBool>,
    pub(crate) repo_info: Option<serde_json::Value>,
}

impl ModelDownloader {
    pub fn new(
        cache_dir: Option<String>,
        include_patterns: Option<Vec<String>>,
        exclude_patterns: Option<Vec<String>>,
        token: Option<String>,
    ) -> PyResult<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        
        // 设置 Ctrl+C 处理
        ctrlc::set_handler(move || {
            running_clone.store(false, Ordering::SeqCst);
        }).expect("Error setting Ctrl+C handler");
        
        let mut config = Config::load();
        
        // 更新配置
        if let Some(patterns) = include_patterns {
            config.include_patterns = patterns;
        }
        if let Some(patterns) = exclude_patterns {
            config.exclude_patterns = patterns;
        }
        
        // 创建优化的 HTTP 客户端
        let client = Client::builder()
            .pool_max_idle_per_host(config.connections_per_download * 2)
            .pool_idle_timeout(std::time::Duration::from_secs(600))
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .tcp_nodelay(true)
            .http2_prior_knowledge()
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(60))
            .connection_verbose(true)
            .use_rustls_tls()
            .http2_keep_alive_interval(std::time::Duration::from_secs(30))
            .http2_keep_alive_timeout(std::time::Duration::from_secs(60))
            .http2_adaptive_window(true)
            .build()
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create HTTP client: {}", e))
            })?;
        
        Ok(Self {
            cache_dir: if let Some(dir) = cache_dir {
                dir
            } else {
                config.get_model_dir("")
            },
            client,
            config,
            auth: Auth { token },
            running,
            repo_info: None,
        })
    }

    pub(crate) fn download_file(&self, task: DownloadTask, model_id: &str) -> PyResult<()> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            
        rt.block_on(async {
            match task {
                DownloadTask::SmallFile { file, path, progress } => {
                    if !self.running.load(Ordering::SeqCst) {
                        progress.cancel_download();
                        return Ok(());
                    }

                    let file_url = self.get_file_url(model_id, &file.rfilename)?;
                    if let Some(size) = file.size {
                        if let Err(e) = self.download_small_file(file_url, path, size, &progress).await {
                            progress.fail_download(&e);
                            return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
                        }
                    }

                    progress.finish_download();
                    Ok(())
                },
                DownloadTask::ChunkedFile { file, path, chunk_size, max_retries, progress } => {
                    if !self.running.load(Ordering::SeqCst) {
                        progress.cancel_download();
                        return Ok(());
                    }

                    let file_url = self.get_file_url(model_id, &file.rfilename)?;
                    if let Some(size) = file.size {
                        if let Err(e) = chunk::download_file_with_chunks(
                            &self.client,
                            file_url,
                            path,
                            size,
                            chunk_size,
                            max_retries,
                            self.auth.token.clone(),
                            progress.progress_bar.clone(),
                            self.running.clone(),
                        ).await {
                            progress.fail_download(&e);
                            return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
                        }
                    }

                    progress.finish_download();
                    Ok(())
                },
                _ => Err(pyo3::exceptions::PyRuntimeError::new_err("Invalid task type for download_file")),
            }
        })
    }

    pub(crate) fn download_folder(&self, task: DownloadTask, model_id: &str) -> PyResult<()> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            
        rt.block_on(async {
            match task {
                DownloadTask::Folder { name, files, base_path, progress } => {
                    for file in files {
                        if !self.running.load(Ordering::SeqCst) {
                            progress.cancel_download();
                            return Ok(());
                        }

                        let file_path = base_path.join(&file.rfilename);
                        if let Some(size) = file.size {
                            let file_url = self.get_file_url(model_id, &file.rfilename)?;

                            // 根据文件大小选择下载方式
                            if size > self.config.chunk_size as u64 {
                                progress.update_message(format!("[{}] Downloading large file: {}", 
                                    name, file.rfilename));
                                
                                if let Err(e) = chunk::download_file_with_chunks(
                                    &self.client,
                                    file_url,
                                    file_path,
                                    size,
                                    self.config.chunk_size,
                                    self.config.max_retries,
                                    self.auth.token.clone(),
                                    progress.progress_bar.clone(),
                                    self.running.clone(),
                                ).await {
                                    progress.fail_download(&e);
                                    return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
                                }
                            } else {
                                progress.update_message(format!("[{}] Downloading small file: {}", 
                                    name, file.rfilename));
                                    
                                if let Err(e) = self.download_small_file(file_url, file_path, size, &progress).await {
                                    progress.fail_download(&e);
                                    return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
                                }
                            }
                        }
                    }

                    progress.finish_download();
                    Ok(())
                },
                _ => Err(pyo3::exceptions::PyRuntimeError::new_err("Invalid task type for download_folder")),
            }
        })
    }

    pub(crate) async fn download_small_file(
        &self,
        url: String,
        file_path: PathBuf,
        size: u64,
        progress: &DownloadProgress,
    ) -> Result<(), String> {
        let mut request = self.client.get(&url);
        if let Some(token) = &self.auth.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("HTTP error: {}", e))?;

        let mut file = std::fs::File::create(&file_path)
            .map_err(|e| format!("Failed to create file: {}", e))?;

        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;

        while let Some(chunk) = stream.next().await {
            if !self.running.load(Ordering::SeqCst) {
                return Ok(());
            }

            let chunk = chunk.map_err(|e| format!("Failed to read response: {}", e))?;
            std::io::Write::write_all(&mut file, &chunk)
                .map_err(|e| format!("Failed to write file: {}", e))?;
            
            downloaded += chunk.len() as u64;
            progress.inc(chunk.len() as u64);
        }

        // 验证下载大小
        if downloaded != size {
            return Err(format!("Download size mismatch: {} != {}", downloaded, size));
        }

        Ok(())
    }

    pub(crate) fn get_file_url(&self, _model_id: &str, filename: &str) -> PyResult<String> {
        let repo_info = self.repo_info.as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Repository info not available"))?;

        let file_url = if let Some(endpoint) = repo_info["dataset_endpoint"].as_str() {
            format!("{}/resolve/main/{}", endpoint, filename)
        } else if let Some(endpoint) = repo_info["model_endpoint"].as_str() {
            format!("{}/resolve/main/{}", endpoint, filename)
        } else {
            return Err(pyo3::exceptions::PyRuntimeError::new_err("No valid endpoint found"));
        };

        Ok(file_url)
    }
} 