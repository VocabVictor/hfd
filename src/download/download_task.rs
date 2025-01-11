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
                    Self::download_small_file(client, &file, &path, token, endpoint, model_id, &group, is_dataset, None).await
                }
                Self::ChunkedFile { file, path, chunk_size, max_retries, group, is_dataset } => {
                    Self::download_chunked_file(client, &file, &path, chunk_size, max_retries, token, endpoint, model_id, &group, is_dataset, None).await
                }
                Self::Folder { name, files, base_path, is_dataset } => {
                    Self::download_folder(client, &name, &files, &base_path, token, endpoint, model_id, is_dataset).await
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
        // 检查文件是否需要下载
        if !Self::should_download(path, file.size).await {
            return Ok(());
        }

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;
        }

        let url = if is_dataset {
            format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        } else {
            format!("{}/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        };

        let mut request = client.get(&url);
        if let Some(ref token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        // 获取已下载的大小
        let downloaded_size = Self::get_downloaded_size(path).await;
        if downloaded_size > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded_size));
        }

        // 创建进度条（如果没有共享进度条）
        let total_size = file.size.unwrap_or(0);
        let shared_pb = shared_pb.as_ref();
        let pb = if let Some(pb) = shared_pb {
            pb.clone()
        } else {
            let pb = Arc::new(ProgressBar::new(total_size));
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
                .unwrap()
                .progress_chars("#>-"));
            pb.set_message(format!("{}", &file.rfilename));
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        };

        let response = request.send()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to download file: {}", e)))?;

        let mut output_file = if downloaded_size > 0 {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to open file: {}", e)))?;
            
            file.seek(SeekFrom::Start(downloaded_size))
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to seek: {}", e)))?;
            
            file
        } else {
            tokio::fs::File::create(path)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create file: {}", e)))?
        };

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to download chunk: {}", e)))?;
            output_file.write_all(&chunk)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to write chunk: {}", e)))?;
            pb.inc(chunk.len() as u64);
        }

        // 只有在不是共享进度条的情况下才显示完成消息
        if shared_pb.is_none() {
            pb.finish_with_message(format!("✓ Downloaded {}", &file.rfilename));
        }
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
        // 检查文件是否需要下载
        if !Self::should_download(path, file.size).await {
            return Ok(());
        }

        let url = if is_dataset {
            format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        } else {
            format!("{}/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        };

        // 使用已经获取的文件大小
        let total_size = file.size.unwrap_or(0);

        // 使用共享进度条或创建新的进度条
        let shared_pb = shared_pb.as_ref();
        let pb = if let Some(pb) = shared_pb {
            pb.clone()
        } else {
            let pb = Arc::new(ProgressBar::new(total_size));
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
                .unwrap()
                .progress_chars("#>-"));
            pb.set_message(format!("{}", &file.rfilename));
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        };

        let running = Arc::new(AtomicBool::new(true));
        
        // Use chunked download
        if let Err(e) = super::chunk::download_file_with_chunks(
            client,
            url.clone(),
            path.clone(),
            total_size,
            chunk_size,
            max_retries,
            token,
            pb.clone(),
            running,
        ).await {
            if shared_pb.is_none() {
                pb.finish_with_message(format!("✗ Failed to download {}: {}", file.rfilename, e));
            }
            return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
        }

        if shared_pb.is_none() {
            pb.finish_with_message(format!("✓ Downloaded {} (chunked)", file.rfilename));
        }
        Ok(())
    }

    pub async fn download_folder(
        client: &Client,
        name: &str,
        files: &[FileInfo],
        base_path: &PathBuf,
        token: Option<String>,
        endpoint: &str,
        model_id: &str,
        is_dataset: bool,
    ) -> PyResult<()> {
        let folder_path = base_path.join(name);
        tokio::fs::create_dir_all(&folder_path)
            .await
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Failed to create directory: {}", e)))?;

        // 先检查哪些文件需要下载
        let mut need_download_files = Vec::new();
        let mut total_download_size = 0u64;

        for file in files {
            let file_path = folder_path.join(&file.rfilename);
            if Self::should_download(&file_path, file.size).await {
                if let Some(size) = file.size {
                    total_download_size += size;
                }
                need_download_files.push(file);
            }
        }

        // 如果没有文件需要下载，直接返回
        if need_download_files.is_empty() {
            return Ok(());
        }

        // 创建进度条
        let pb = Arc::new(ProgressBar::new(total_download_size));
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"));

        // 将文件分为大文件和小文件两组
        let (large_files, small_files): (Vec<_>, Vec<_>) = need_download_files
            .iter()
            .partition(|file| file.size.map_or(false, |size| size > DEFAULT_CHUNK_SIZE as u64));

        // 创建并发任务
        let mut tasks = Vec::new();

        // 处理小文件 - 使用高并发
        let max_concurrent_small_files = 32; // 可以根据系统性能调整
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent_small_files));

        for file in small_files {
            let file_path = folder_path.join(&file.rfilename);
            let client = client.clone();
            let token = token.clone();
            let endpoint = endpoint.to_string();
            let model_id = model_id.to_string();
            let name = name.to_string();
            let pb = pb.clone();
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let task = tokio::spawn(async move {
                let _permit = permit;
                Self::download_small_file(
                    &client,
                    file,
                    &file_path,
                    token,
                    &endpoint,
                    &model_id,
                    &name,
                    is_dataset,
                    Some(pb),
                ).await
            });
            tasks.push(task);
        }

        // 处理大文件 - 使用较低并发
        for file in large_files {
            let file_path = folder_path.join(&file.rfilename);
            let client = client.clone();
            let token = token.clone();
            let endpoint = endpoint.to_string();
            let model_id = model_id.to_string();
            let name = name.to_string();
            let pb = pb.clone();

            let task = tokio::spawn(async move {
                Self::download_chunked_file(
                    &client,
                    file,
                    &file_path,
                    DEFAULT_CHUNK_SIZE,
                    DEFAULT_MAX_RETRIES,
                    token,
                    &endpoint,
                    &model_id,
                    &name,
                    is_dataset,
                    Some(pb),
                ).await
            });
            tasks.push(task);
        }

        // 等待所有任务完成
        for task in tasks {
            task.await.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Task failed: {}", e)))??;
        }

        if !need_download_files.is_empty() {
            pb.finish_with_message(format!("✓ Folder {} downloaded successfully", name));
        }
        Ok(())
    }

    // 检查文件是否需要下载
    async fn should_download(path: &PathBuf, expected_size: Option<u64>) -> bool {
        if let Ok(metadata) = fs::metadata(path).await {
            if let Some(size) = expected_size {
                // 如果文件大小不匹配，需要重新下载
                metadata.len() != size
            } else {
                // 如果没有期望的大小信息，但文件存在，则跳过
                false
            }
        } else {
            // 文件不存在，需要下载
            true
        }
    }

    // 获取已下载的文件大小，用于断点续传
    async fn get_downloaded_size(path: &PathBuf) -> u64 {
        if let Ok(metadata) = fs::metadata(path).await {
            metadata.len()
        } else {
            0
        }
    }
} 