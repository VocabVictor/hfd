use crate::types::FileInfo;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use pyo3::prelude::*;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use std::io::SeekFrom;
use futures::StreamExt;
use std::pin::Pin;
use std::future::Future;
use std::time::Duration;
use tokio::fs;

const DEFAULT_CHUNK_SIZE: usize = 16 * 1024 * 1024; // 16MB
const DEFAULT_MAX_RETRIES: usize = 3;

#[derive(Debug)]
pub enum DownloadTask {
    #[allow(dead_code)]
    SmallFile {
        file: FileInfo,
        path: PathBuf,
        group: String,
        is_dataset: bool,
    },
    #[allow(dead_code)]
    ChunkedFile {
        file: FileInfo,
        path: PathBuf,
        chunk_size: usize,
        max_retries: usize,
        group: String,
        is_dataset: bool,
    },
    #[allow(dead_code)]
    Folder {
        name: String,
        files: Vec<FileInfo>,
        base_path: PathBuf,
        is_dataset: bool,
    },
}

#[allow(dead_code)]
impl DownloadTask {
    pub fn single_file(file: FileInfo, path: PathBuf, group: &str, is_dataset: bool) -> Self {
        Self::SmallFile {
            file,
            path,
            group: group.to_string(),
            is_dataset,
        }
    }

    pub fn large_file(
        file: FileInfo,
        path: PathBuf,
        chunk_size: usize,
        max_retries: usize,
        group: &str,
        is_dataset: bool,
    ) -> Self {
        Self::ChunkedFile {
            file,
            path,
            chunk_size,
            max_retries,
            group: group.to_string(),
            is_dataset,
        }
    }

    pub fn folder(name: String, files: Vec<FileInfo>, base_path: PathBuf, is_dataset: bool) -> Self {
        Self::Folder {
            name,
            files,
            base_path,
            is_dataset,
        }
    }

    pub fn execute<'a>(
        self,
        client: &'a Client,
        token: Option<String>,
        endpoint: &'a str,
        model_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = PyResult<()>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                Self::SmallFile { file, path, group, is_dataset } => {
                    if let Some(size) = file.size {
                        let downloaded_size = Self::get_downloaded_size(&path).await;
                        if downloaded_size >= size {
                            println!("✓ File already downloaded: {}", file.rfilename);
                            return Ok(());
                        }
                        
                        let result = if size > 0 {
                            let pb = Arc::new(ProgressBar::new(size));
                            pb.set_style(ProgressStyle::default_bar()
                                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
                                .unwrap()
                                .progress_chars("#>-"));
                            pb.set_message(format!("Downloading {}", file.rfilename));
                            pb.enable_steady_tick(Duration::from_millis(100));
                            
                            let result = Self::download_small_file(client, &file, &path, token, endpoint, model_id, &group, is_dataset, Some(pb.clone())).await;
                            
                            if result.is_ok() {
                                pb.finish_with_message(format!("✓ Downloaded {}", file.rfilename));
                            } else {
                                pb.abandon_with_message(format!("Failed to download {}", file.rfilename));
                            }
                            result
                        } else {
                            Self::download_small_file(client, &file, &path, token, endpoint, model_id, &group, is_dataset, None).await
                        };
                        result
                    } else {
                        Self::download_small_file(client, &file, &path, token, endpoint, model_id, &group, is_dataset, None).await
                    }
                }
                Self::ChunkedFile { file, path, chunk_size, max_retries, group, is_dataset } => {
                    if let Some(size) = file.size {
                        let downloaded_size = Self::get_downloaded_size(&path).await;
                        if downloaded_size >= size {
                            println!("✓ File already downloaded: {}", file.rfilename);
                            return Ok(());
                        }
                        
                        let result = if size > 0 {
                            let pb = Arc::new(ProgressBar::new(size));
                            pb.set_style(ProgressStyle::default_bar()
                                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
                                .unwrap()
                                .progress_chars("#>-"));
                            pb.set_message(format!("Downloading {}", file.rfilename));
                            pb.enable_steady_tick(Duration::from_millis(100));
                            
                            let result = Self::download_chunked_file(client, &file, &path, chunk_size, max_retries, token, endpoint, model_id, &group, is_dataset, Some(pb.clone())).await;
                            
                            if result.is_ok() {
                                pb.finish_with_message(format!("✓ Downloaded {}", file.rfilename));
                            } else {
                                pb.abandon_with_message(format!("Failed to download {}", file.rfilename));
                            }
                            result
                        } else {
                            Self::download_chunked_file(client, &file, &path, chunk_size, max_retries, token, endpoint, model_id, &group, is_dataset, None).await
                        };
                        result
                    } else {
                        Self::download_chunked_file(client, &file, &path, chunk_size, max_retries, token, endpoint, model_id, &group, is_dataset, None).await
                    }
                }
                Self::Folder { name, files, base_path, is_dataset } => {
                    Self::download_folder(
                        client.clone(),
                        endpoint.to_string(),
                        model_id.to_string(),
                        base_path,
                        name,
                        files,
                        token,
                        is_dataset
                    ).await
                }
            }
        })
    }

    async fn download_small_file(
        client: &Client,
        file: &FileInfo,
        path: &PathBuf,
        token: Option<String>,
        endpoint: &str,
        model_id: &str,
        _group: &str,
        is_dataset: bool,
        shared_pb: Option<Arc<ProgressBar>>,
    ) -> PyResult<()> {
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;
        }

        // 检查文件是否需要下载
        if let Some(size) = file.size {
            let downloaded_size = if path.exists() {
                Self::get_downloaded_size(path).await
            } else {
                0
            };
            if downloaded_size >= size {
                if let Some(ref pb) = shared_pb {
                    pb.inc(size);  // 更新父进度条，但不显示本文件的进度
                }
                return Ok(());
            }
        }

        // let url = if is_dataset {
        //     format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        // } else {
        //     format!("{}/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        // };

        // let mut request = client.get(&url);
        // if let Some(ref token) = token {
        //     request = request.header("Authorization", format!("Bearer {}", token));
        // }

        // // 获取已下载的大小
        // let downloaded_size = Self::get_downloaded_size(path).await;
        // if downloaded_size > 0 {
        //     request = request.header("Range", format!("bytes={}-", downloaded_size));
        // }

        // let response = request.send()
        //     .await
        //     .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to download file: {}", e)))?;

        // let mut output_file = if downloaded_size > 0 {
        //     let mut file = tokio::fs::OpenOptions::new()
        //         .write(true)
        //         .open(path)
        //         .await
        //         .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to open file: {}", e)))?;
            
        //     file.seek(SeekFrom::Start(downloaded_size))
        //         .await
        //         .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to seek: {}", e)))?;
            
        //     file
        // } else {
        //     tokio::fs::File::create(path)
        //         .await
        //         .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create file: {}", e)))?
        // };

        // let mut stream = response.bytes_stream();
        // while let Some(chunk) = stream.next().await {
        //     let chunk = chunk.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to download chunk: {}", e)))?;
        //     output_file.write_all(&chunk)
        //         .await
        //         .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to write chunk: {}", e)))?;
        //     if let Some(ref pb) = shared_pb {
        //         pb.inc(chunk.len() as u64);
        //     }
        // }

        Ok(())
    }

    async fn download_chunked_file(
        client: &Client,
        file: &FileInfo,
        path: &PathBuf,
        chunk_size: usize,
        max_retries: usize,
        token: Option<String>,
        endpoint: &str,
        model_id: &str,
        _group: &str,
        is_dataset: bool,
        shared_pb: Option<Arc<ProgressBar>>,
    ) -> PyResult<()> {
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;
        }

        // 检查文件是否需要下载
        if let Some(size) = file.size {
            let downloaded_size = if path.exists() {
                Self::get_downloaded_size(path).await
            } else {
                0
            };
            if downloaded_size >= size {
                if let Some(ref pb) = shared_pb {
                    pb.inc(size);  // 更新父进度条，但不显示本文件的进度
                }
                return Ok(());
            }
        }

        let url = if is_dataset {
            format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        } else {
            format!("{}/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        };

        let running = Arc::new(AtomicBool::new(true));
        
        // Use chunked download
        if let Err(e) = super::chunk::download_file_with_chunks(
            client,
            url.clone(),
            path.clone(),
            file.size.unwrap_or(0),
            chunk_size,
            max_retries,
            token,
            shared_pb.unwrap_or_else(|| Arc::new(ProgressBar::hidden())),
            running,
        ).await {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
        }

        Ok(())
    }

    pub async fn download_folder(
        client: Client,
        endpoint: String,
        model_id: String,
        base_path: PathBuf,
        name: String,
        files: Vec<FileInfo>,
        token: Option<String>,
        is_dataset: bool,
    ) -> PyResult<()> {
        use crate::INTERRUPT_FLAG;

        let folder_name = name.clone();
        let folder_path = base_path.join(&name);
        tokio::fs::create_dir_all(&folder_path)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;

        let mut need_download_files = Vec::new();
        let mut total_download_size = 0;

        // 首先打印已下载的文件
        for file in &files {
            if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
            }

            let file_path = folder_path.join(&file.rfilename);
            if let Some(size) = file.size {
                let downloaded_size = if file_path.exists() {
                    Self::get_downloaded_size(&file_path).await
                } else {
                    0
                };
                if downloaded_size >= size {
                    println!("✓ File already downloaded: {}/{}", folder_name, file.rfilename);
                } else {
                    total_download_size += size;
                    need_download_files.push(file.clone());
                }
            }
        }

        if need_download_files.is_empty() {
            println!("✓ All files in folder {} are already downloaded", folder_name);
            return Ok(());
        }

        println!("Need to download {} files, total size: {} bytes", need_download_files.len(), total_download_size);

        let pb = Arc::new(ProgressBar::new(total_download_size));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading folder {}", folder_name));
        pb.enable_steady_tick(Duration::from_millis(100));

        let (large_files, small_files): (Vec<_>, Vec<_>) = need_download_files
            .into_iter()
            .partition(|file| file.size.map_or(false, |size| size > DEFAULT_CHUNK_SIZE as u64));

        let mut tasks = Vec::new();
        let max_concurrent_small_files = 3;  // 减少并发数，避免进度显示混乱
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent_small_files));

        for file in small_files {
            if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
                pb.abandon_with_message("Download interrupted by user");
                return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
            }

            let file_path = folder_path.join(&file.rfilename);
            let client = client.clone();
            let token = token.clone();
            let pb = pb.clone();
            let permit = semaphore.clone();
            let endpoint = endpoint.clone();
            let model_id = model_id.clone();

            tasks.push(tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                println!("Starting download of {}", file.rfilename);
                let result = Self::download_small_file(
                    &client,
                    &file,
                    &file_path,
                    token,
                    &endpoint,
                    &model_id,
                    "",
                    is_dataset,
                    Some(pb),
                ).await;
                if result.is_ok() {
                    println!("Completed download of {}", file.rfilename);
                }
                result
            }));
        }

        for file in large_files {
            if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
                pb.abandon_with_message("Download interrupted by user");
                return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
            }

            let file_path = folder_path.join(&file.rfilename);
            let client = client.clone();
            let token = token.clone();
            let pb = pb.clone();
            let endpoint = endpoint.clone();
            let model_id = model_id.clone();

            tasks.push(tokio::spawn(async move {
                println!("Starting download of {}", file.rfilename);
                let result = Self::download_chunked_file(
                    &client,
                    &file,
                    &file_path,
                    DEFAULT_CHUNK_SIZE,
                    DEFAULT_MAX_RETRIES,
                    token,
                    &endpoint,
                    &model_id,
                    "",
                    is_dataset,
                    Some(pb),
                ).await;
                if result.is_ok() {
                    println!("Completed download of {}", file.rfilename);
                }
                result
            }));
        }

        for task in tasks {
            if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
                pb.abandon_with_message("Download interrupted by user");
                return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
            }
            task.await.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Task failed: {}", e)))??;
        }

        pb.finish_with_message(format!("✓ Downloaded folder {}", folder_name));
        Ok(())
    }

    // 检查文件是否需要下载
    async fn should_download(path: &PathBuf, expected_size: Option<u64>) -> bool {
        if let Ok(metadata) = fs::metadata(path).await {
            if let Some(size) = expected_size {
                metadata.len() < size
            } else {
                false
            }
        } else {
            true
        }
    }

    // 获取已下载的文件大小，用于断点续传
    async fn get_downloaded_size(path: &PathBuf) -> u64 {
        if path.exists() {
            if let Ok(metadata) = fs::metadata(path).await {
                metadata.len()
            } else {
                0
            }
        } else {
            0
        }
    }
} 