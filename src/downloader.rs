use crate::types::{RepoFile, RepoInfo};
use crate::utils::{create_progress_bar, format_size};
use crate::config::Config;
use futures::StreamExt;
use indicatif;
use pyo3::prelude::*;
use reqwest::Client;
use shellexpand;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tokio::runtime::Runtime;
use regex;
use ctrlc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

lazy_static::lazy_static! {
    static ref RUNNING: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));
}

#[pyclass]
pub struct ModelDownloader {
    cache_dir: String,
    client: Client,
    runtime: Runtime,
    config: Config,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
}

#[pymethods]
impl ModelDownloader {
    #[new]
    pub fn new(
        cache_dir: Option<String>,
        include_patterns: Option<Vec<String>>,
        exclude_patterns: Option<Vec<String>>
    ) -> PyResult<Self> {
        // 设置 Ctrl+C 处理
        ctrlc::set_handler(move || {
            println!("\nReceived Ctrl+C, stopping downloads...");
            RUNNING.store(false, Ordering::SeqCst);
        }).expect("Error setting Ctrl+C handler");
        
        let config = Config::load();
        
        // 创建优化的 HTTP 客户端
        let client = Client::builder()
            .pool_max_idle_per_host(config.connections_per_download)
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .tcp_nodelay(true)
            .http2_prior_knowledge()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(60))
            .connection_verbose(true)
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
            runtime: Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e))
            })?,
            config,
            include_patterns: include_patterns.unwrap_or_default(),
            exclude_patterns: exclude_patterns.unwrap_or_default(),
        })
    }

    fn should_download_file(&self, filename: &str) -> bool {
        // 如果没有包含和排除模式，下载所有文件
        if self.include_patterns.is_empty() && self.exclude_patterns.is_empty() {
            return true;
        }

        // 如果有包含模式，文件必须匹配其中之一
        if !self.include_patterns.is_empty() {
            let matches_include = self.include_patterns.iter().any(|pattern| {
                let re = regex::Regex::new(&pattern.replace("*", ".*")).unwrap();
                re.is_match(filename)
            });
            if !matches_include {
                return false;
            }
        }

        // 如果有排除模式，文件不能匹配任何一个
        if !self.exclude_patterns.is_empty() {
            let matches_exclude = self.exclude_patterns.iter().any(|pattern| {
                let re = regex::Regex::new(&pattern.replace("*", ".*")).unwrap();
                re.is_match(filename)
            });
            if matches_exclude {
                return false;
            }
        }

        true
    }

    pub fn download(&self, model_id: &str) -> PyResult<String> {
        // 重置运行状态
        RUNNING.store(true, Ordering::SeqCst);
        
        self.runtime.block_on(async {
            let url = format!("{}/api/models/{}", self.config.endpoint, model_id);
            println!("Fetching repo info from: {}", url);
            
            let response = self.client.get(&url)
                .send()
                .await
                .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(format!("Failed to fetch repo info: {}", e)))?;

            // 如果是重定向，获取新的 URL
            let repo_info = if response.status().is_redirection() {
                if let Some(new_location) = response.headers().get("location") {
                    let new_url = if new_location.to_str().unwrap().starts_with("http") {
                        new_location.to_str().unwrap().to_string()
                    } else {
                        format!("{}{}", self.config.endpoint, new_location.to_str().unwrap())
                    };
                    println!("Following redirect to: {}", new_url);
                    let response = self.client.get(&new_url)
                        .send()
                        .await
                        .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(format!("Failed to fetch repo info: {}", e)))?;
                    
                    if !response.status().is_success() {
                        return Err(pyo3::exceptions::PyConnectionError::new_err(
                            format!("Failed to fetch repo info: HTTP {}", response.status())
                        ));
                    }

                    let mut repo_info: RepoInfo = response.json()
                        .await
                        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Failed to parse repo info: {}", e)))?;

                    // 获取每个文件的大小
                    for file in &mut repo_info.siblings {
                        let file_url = format!(
                            "{}/{}/resolve/main/{}",
                            self.config.endpoint, model_id, file.rfilename
                        );
                        if let Ok(resp) = self.client.head(&file_url).send().await {
                            if let Some(size) = resp.headers().get("content-length") {
                                if let Ok(size) = size.to_str().unwrap_or("0").parse::<u64>() {
                                    file.size = Some(size);
                                }
                            }
                        }
                    }

                    repo_info
                } else {
                    return Err(pyo3::exceptions::PyConnectionError::new_err(
                        format!("Failed to fetch repo info: HTTP {}", response.status())
                    ));
                }
            } else {
                if !response.status().is_success() {
                    return Err(pyo3::exceptions::PyConnectionError::new_err(
                        format!("Failed to fetch repo info: HTTP {}", response.status())
                    ));
                }

                let mut repo_info: RepoInfo = response.json()
                    .await
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Failed to parse repo info: {}", e)))?;

                // 获取每个文件的大小
                for file in &mut repo_info.siblings {
                    let file_url = format!(
                        "{}/{}/resolve/main/{}",
                        self.config.endpoint, model_id, file.rfilename
                    );
                    if let Ok(resp) = self.client.head(&file_url).send().await {
                        if let Some(size) = resp.headers().get("content-length") {
                            if let Ok(size) = size.to_str().unwrap_or("0").parse::<u64>() {
                                file.size = Some(size);
                            }
                        }
                    }
                }

                repo_info
            };

            let base_path = if self.config.use_local_dir {
                PathBuf::from(self.config.get_model_dir(model_id))
            } else {
                PathBuf::from(&self.cache_dir)
            };
            
            // 创建必要的目录
            fs::create_dir_all(&base_path).map_err(|e| {
                pyo3::exceptions::PyOSError::new_err(format!("Failed to create directory: {}", e))
            })?;

            // 过滤并生成下载列表
            let files_to_download: Vec<&RepoFile> = repo_info.siblings.iter()
                .filter(|file| self.should_download_file(&file.rfilename))
                .collect();

            let total_files = files_to_download.len();
            let total_size: u64 = files_to_download.iter()
                .filter_map(|f| f.size)
                .sum();

            println!("Found {} files to download, total size: {}", total_files, format_size(total_size));

            // 创建下载任务
            let mut tasks = Vec::new();
            let multi_progress = indicatif::MultiProgress::new();
            let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(self.config.concurrent_downloads));

            for file in files_to_download {
                if !RUNNING.load(Ordering::SeqCst) {
                    return Ok("Download cancelled".to_string());
                }

                let file_url = format!(
                    "{}/{}/resolve/main/{}",
                    self.config.endpoint, model_id, file.rfilename
                );
                let file_path = base_path.join(&file.rfilename);
                let client = self.client.clone();
                let semaphore = semaphore.clone();
                let config = self.config.clone();

                // 检查文件是否已存在且大小正确
                if let Ok(metadata) = fs::metadata(&file_path) {
                    if let Some(expected_size) = file.size {
                        if metadata.len() == expected_size {
                            continue;
                        }
                    }
                }

                // 创建父目录
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        pyo3::exceptions::PyOSError::new_err(format!("Failed to create directory: {}", e))
                    })?;
                }

                // 创建下载任务
                let progress_bar = multi_progress.add(create_progress_bar(
                    file.size.unwrap_or(0),
                    &file.rfilename
                ));

                let task = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    let response = client.get(&file_url)
                        .send()
                        .await
                        .map_err(|e| format!("Failed to download file: {}", e))?;

                    let total_size = response.content_length().unwrap_or(0);
                    if total_size > 0 {
                        progress_bar.set_length(total_size);
                    }

                    let mut file = fs::File::create(&file_path)
                        .map_err(|e| format!("Failed to create file: {}", e))?;

                    let mut downloaded: u64 = 0;
                    let mut stream = response.bytes_stream();

                    // 使用配置的缓冲区大小
                    let mut buffer = Vec::with_capacity(config.buffer_size);

                    while let Some(chunk) = stream.next().await {
                        if !RUNNING.load(Ordering::SeqCst) {
                            return Err("Download cancelled".to_string());
                        }
                        
                        let chunk = chunk.map_err(|e| format!("Failed to read chunk: {}", e))?;
                        buffer.extend_from_slice(&chunk);
                        
                        // 当缓冲区达到配置的大小时写入文件
                        if buffer.len() >= config.buffer_size {
                            file.write_all(&buffer)
                                .map_err(|e| format!("Failed to write chunk: {}", e))?;
                            downloaded += buffer.len() as u64;
                            progress_bar.set_position(downloaded);
                            buffer.clear();
                        }
                    }

                    // 写入剩余的数据
                    if !buffer.is_empty() {
                        file.write_all(&buffer)
                            .map_err(|e| format!("Failed to write chunk: {}", e))?;
                        downloaded += buffer.len() as u64;
                        progress_bar.set_position(downloaded);
                    }

                    progress_bar.finish_with_message(format!("✓ {}", file_path.file_name().unwrap().to_string_lossy()));
                    Ok::<_, String>(())
                });
                tasks.push(task);
            }

            // 等待所有下载任务完成
            let results = futures::future::join_all(tasks).await;
            for result in results {
                match result {
                    Ok(Ok(_)) => continue,
                    Ok(Err(e)) => {
                        if e == "Download cancelled" {
                            return Ok("Download cancelled".to_string());
                        }
                        return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
                    },
                    Err(e) => return Err(pyo3::exceptions::PyRuntimeError::new_err(format!("Task failed: {}", e))),
                }
            }

            Ok(format!("Downloaded model {} to {}", model_id, base_path.display()))
        })
    }
} 