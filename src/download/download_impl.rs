use super::downloader::ModelDownloader;
use crate::utils::{create_progress_bar, print_status, clear_progress};
use pyo3::prelude::*;
use std::fs::{self, OpenOptions};
use std::io::{Write, Read};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use futures::StreamExt;
use flate2::read::GzDecoder;
use tokio::sync::Semaphore;
use tokio::select;
use serde_json::Value;

impl ModelDownloader {
    pub(crate) async fn download_model(&self, model_id: &str) -> PyResult<String> {
        let repo_info = self.get_repo_info(model_id).await?;

        let requires_auth = match &repo_info.gated {
            Value::Bool(gated) => *gated,
            Value::String(gated_str) => gated_str == "manual",
            _ => false,
        };

        if requires_auth && self.auth.token.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "该仓库需要认证。请提供 Hugging Face token。可以从 https://huggingface.co/settings/tokens 获取。"
            ));
        }

        let base_path = if self.config.use_local_dir {
            PathBuf::from(self.config.get_model_dir(model_id))
        } else {
            PathBuf::from(&self.cache_dir)
        };

        fs::create_dir_all(&base_path).map_err(|e| {
            pyo3::exceptions::PyOSError::new_err(format!("Failed to create directory: {}", e))
        })?;

        let (mut files_to_download, total_size) = self.prepare_download_list(&repo_info, model_id, &base_path).await?;

        if files_to_download.is_empty() {
            return Ok("所有文件已下载完成".to_string());
        }

        if let Err(e) = print_status(&format!("Found {} files to download, total size: {:.2} MB", 
            files_to_download.len(), 
            total_size as f64 / 1024.0 / 1024.0
        )) {
            println!("Warning: Failed to print status: {}", e);
        }

        // 按大小降序排序，先下载大文件
        files_to_download.sort_by(|a, b| b.size.unwrap_or(0).cmp(&a.size.unwrap_or(0)));

        let running = self.running.clone();
        let client = self.client.clone();
        let auth_token = self.auth.token.clone();
        let endpoint = self.config.endpoint.clone();
        let model_id = model_id.to_string();
        let base_path = base_path.clone();

        // 创建信号量来限制并发下载数
        let semaphore = Arc::new(Semaphore::new(self.config.concurrent_downloads)); 

        // 收集所有需要下载的文件信息
        let download_info: Vec<_> = files_to_download.into_iter()
            .map(|file| (file.rfilename.clone(), file.size))
            .collect();

        // 在后台线程中运行下载任务
        let download_handle = tokio::spawn(async move {
            let mut download_tasks = Vec::new();
            let _total_files = download_info.len();

            for (_idx, (filename, file_size)) in download_info.into_iter().enumerate() {
                if !running.load(Ordering::SeqCst) {
                    return Ok("下载已取消".to_string());
                }

                let file_path = base_path.join(&filename);
                
                // 如果文件路径包含目录，确保创建所有必要的父目录
                if let Some(parent) = file_path.parent() {
                    if !parent.exists() {
                        if let Err(e) = fs::create_dir_all(parent) {
                            return Err(format!("创建目录失败 {}: {}", parent.display(), e));
                        }
                    }
                }

                // 检查文件是否已下载
                if let Ok(metadata) = fs::metadata(&file_path) {
                    if let Some(expected_size) = file_size {
                        if metadata.len() == expected_size {
                            if let Err(e) = print_status(&format!("跳过已下载的文件: {}", filename)) {
                                println!("Warning: Failed to print status: {}", e);
                            }
                            continue;  // 跳过已下载完成的文件
                        }
                    }
                }

                let file_url = format!("{}/{}/resolve/main/{}", endpoint, model_id, filename);

                // 获取文件总大小
                let total_size = if let Some(size) = file_size {
                    size
                } else {
                    let mut request = client.get(&file_url);
                    if let Some(token) = &auth_token {
                        request = request.header("Authorization", format!("Bearer {}", token));
                    }
                    let resp = request
                        .send()
                        .await
                        .map_err(|e| format!("获取文件大小失败: {}", e))?;
                    resp.content_length().unwrap_or(0)
                };

                let pb = create_progress_bar(
                    total_size,
                    &filename,
                    if let Ok(metadata) = fs::metadata(&file_path) {
                        metadata.len()
                    } else {
                        0
                    }
                );

                const MAX_RETRIES: usize = 5;
                const SMALL_FILE_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB

                // 克隆需要的变量
                let client = client.clone();
                let file_url = file_url.clone();
                let file_path = file_path.clone();
                let auth_token = auth_token.clone();
                let running = running.clone();
                let semaphore = semaphore.clone();
                let pb = pb.clone();
                let filename = filename.clone();

                let download_task = tokio::spawn(async move {
                    // 获取信号量许可
                    let _permit = semaphore.acquire().await.map_err(|e| format!("获取信号量失败: {}", e))?;

                    select! {
                        download_result = async {
                            if total_size <= SMALL_FILE_THRESHOLD {
                                Self::download_small_file(
                                    &client,
                                    &file_url,
                                    &file_path,
                                    auth_token.clone(),
                                    pb.clone(),
                                ).await
                            } else {
                                Self::download_file_with_chunks(
                                    &client,
                                    file_url.clone(),
                                    file_path.clone(),
                                    total_size,
                                    0,
                                    MAX_RETRIES,
                                    auth_token.clone(),
                                    pb.clone(),
                                    running.clone(),
                                ).await
                            }
                        } => {
                            match download_result {
                                Ok(_) => {
                                    pb.finish_with_message(format!("{} ✓", filename));
                                    Ok(())
                                },
                                Err(e) => Err(e),
                            }
                        }
                        _ = tokio::signal::ctrl_c() => {
                            pb.abandon_with_message(format!("{} 已取消", filename));
                            running.store(false, Ordering::SeqCst);
                            Ok(())
                        }
                    }
                });

                download_tasks.push(download_task);
            }

            // 等待所有下载任务完成
            for task in download_tasks {
                if let Err(e) = task.await {
                    return Err(format!("下载任务失败: {}", e));
                }
            }

            if let Err(e) = clear_progress() {
                println!("Warning: Failed to clear progress: {}", e);
            }
            Ok(format!("Downloaded model {} to {}", model_id, base_path.display()))
        });

        // 等待下载完成并转换错误类型
        match download_handle.await {
            Ok(Ok(msg)) => Ok(msg),
            Ok(Err(e)) => Err(pyo3::exceptions::PyRuntimeError::new_err(e)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("下载任务失败: {}", e))),
        }
    }

    pub(crate) async fn download_small_file(
        client: &reqwest::Client,
        url: &str,
        file_path: &PathBuf,
        auth_token: Option<String>,
        pb: indicatif::ProgressBar,
    ) -> Result<(), String> {
        let mut request = client.get(url);
        if let Some(token) = auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        request = request
            .header("Accept-Encoding", "identity")
            .header("Connection", "keep-alive")
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .timeout(std::time::Duration::from_secs(300));

        let response = request
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?
            .error_for_status()
            .map_err(|e| format!("HTTP错误: {}", e))?;

        let total_size = response.content_length().unwrap_or(0);
        pb.set_length(total_size);

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)
            .map_err(|e| format!("创建文件失败: {}", e))?;

        let mut stream = response.bytes_stream();
        let mut buffer = Vec::with_capacity(8 * 1024 * 1024); // 8MB buffer
        let mut last_progress = std::time::Instant::now();
        let mut _downloaded = 0u64; // 改为 _downloaded 表示有意不使用

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("读取响应失败: {}", e))?;
            buffer.extend_from_slice(&chunk);
            _downloaded += chunk.len() as u64;
            
            let now = std::time::Instant::now();
            if now.duration_since(last_progress).as_secs() > 60 {
                return Err("下载速度过慢".to_string());
            }
            
            if buffer.len() >= 8 * 1024 * 1024 { // 8MB
                file.write_all(&buffer)
                    .map_err(|e| format!("写入文件失败: {}", e))?;
                pb.set_position(_downloaded);
                buffer.clear();
                last_progress = now;
            }
        }

        if !buffer.is_empty() {
            file.write_all(&buffer)
                .map_err(|e| format!("写入文件失败: {}", e))?;
            pb.set_position(_downloaded);
        }

        file.sync_all()
            .map_err(|e| format!("同步文件失败: {}", e))?;

        // 验证文件大小
        if let Ok(metadata) = fs::metadata(file_path) {
            if metadata.len() != total_size && total_size != 0 {
                return Err(format!("文件大小不匹配: {} != {}", metadata.len(), total_size));
            }
        }

        Ok(())
    }

    fn is_gzip_file(data: &[u8]) -> bool {
        data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
    }

    fn decompress_gzip_file(file_path: &PathBuf) -> PyResult<()> {
        let mut file = fs::File::open(file_path).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to open file: {}", e))
        })?;
        
        // 读取文件头部来检查是否为gzip
        let mut header = [0u8; 2];
        if let Err(_) = file.read_exact(&mut header) {
            return Ok(());  // 如果读取失败，假设不是gzip文件
        }
        
        if !Self::is_gzip_file(&header) {
            return Ok(());  // 不是gzip文件，直接返回
        }
        
        // 重新打开文件并读取所有内容
        let mut file = fs::File::open(file_path).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to reopen file: {}", e))
        })?;
        let mut compressed = Vec::new();
        file.read_to_end(&mut compressed).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to read file: {}", e))
        })?;
        
        // 解压缩
        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to decompress: {}", e))
        })?;
        
        // 写回原文件
        let mut file = fs::File::create(file_path).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to create file: {}", e))
        })?;
        file.write_all(&decompressed).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to write file: {}", e))
        })?;
        
        Ok(())
    }
} 