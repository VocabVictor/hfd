use crate::types::FileInfo;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use pyo3::prelude::*;
use tokio::io::AsyncWriteExt;
use futures::StreamExt;
use std::pin::Pin;
use std::future::Future;
use std::time::Duration;
use tokio::fs;
use std::io::SeekFrom;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

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

    async fn download_folder(
        client: &Client,
        name: &str,
        files: &[FileInfo],
        base_path: &PathBuf,
        token: Option<String>,
        endpoint: &str,
        model_id: &str,
        is_dataset: bool,
    ) -> PyResult<()> {
        // 创建文件夹
        let folder_path = base_path.join(name);
        tokio::fs::create_dir_all(&folder_path)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;

        // 计算需要下载的文件总大小
        let mut total_size = 0;
        let mut need_download_files = Vec::new();

        for file in files {
            let file_path = folder_path.join(&file.rfilename);
            if Self::should_download(&file_path, file.size).await {
                if let Some(size) = file.size {
                    total_size += size;
                }
                need_download_files.push(file);
            }
        }

        // 如果没有需要下载的文件，直接返回
        if need_download_files.is_empty() {
            return Ok(());
        }

        // 创建共享进度条
        let pb = Arc::new(ProgressBar::new(total_size));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading folder: {}", name));
        pb.enable_steady_tick(Duration::from_millis(100));

        // 下载需要的文件
        for file in need_download_files {
            let file_path = folder_path.join(&file.rfilename);
            if let Some(size) = file.size {
                if size > chunk_size as u64 {
                    Self::download_chunked_file(
                        client,
                        file,
                        &file_path,
                        chunk_size,
                        max_retries,
                        token.clone(),
                        endpoint,
                        model_id,
                        name,
                        is_dataset,
                        Some(pb.clone()),
                    ).await?;
                } else {
                    Self::download_small_file(
                        client,
                        file,
                        &file_path,
                        token.clone(),
                        endpoint,
                        model_id,
                        name,
                        is_dataset,
                        Some(pb.clone()),
                    ).await?;
                }
            } else {
                Self::download_small_file(
                    client,
                    file,
                    &file_path,
                    token.clone(),
                    endpoint,
                    model_id,
                    name,
                    is_dataset,
                    Some(pb.clone()),
                ).await?;
            }
        }

        pb.finish_with_message(format!("✓ Folder {} downloaded successfully", name));
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

    pub async fn execute(
        self,
        client: &Client,
        token: Option<String>,
        endpoint: &str,
        model_id: &str,
    ) -> Result<(), String> {
        match self {
            Self::SmallFile { file, path, group, .. } => {
                // 检查文件是否需要下载
                if !Self::should_download(&path, file.size).await {
                    return Ok(());
                }

                // 创建父目录
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| format!("Failed to create directory: {}", e))?;
                }

                let mut request = client.get(&format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename));
                if let Some(token) = token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }

                let response = request
                    .send()
                    .await
                    .map_err(|e| format!("Failed to download file: {}", e))?;

                let mut file = tokio::fs::File::create(&path)
                    .await
                    .map_err(|e| format!("Failed to create file: {}", e))?;

                let content = response
                    .bytes()
                    .await
                    .map_err(|e| format!("Failed to read response: {}", e))?;

                file.write_all(&content)
                    .await
                    .map_err(|e| format!("Failed to write file: {}", e))?;

                Ok(())
            }
            Self::ChunkedFile { file, path, chunk_size, max_retries, group, .. } => {
                // 检查文件是否需要下载
                if !Self::should_download(&path, file.size).await {
                    return Ok(());
                }

                // 创建父目录
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| format!("Failed to create directory: {}", e))?;
                }

                // 获取已下载的大小
                let downloaded_size = Self::get_downloaded_size(&path).await;
                
                let mut request = client.get(&format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename))
                    .header("Range", format!("bytes={}-", downloaded_size));

                if let Some(token) = token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }

                let response = request
                    .send()
                    .await
                    .map_err(|e| format!("Failed to download file: {}", e))?;

                let mut file = if downloaded_size > 0 {
                    let mut file = tokio::fs::OpenOptions::new()
                        .write(true)
                        .open(&path)
                        .await
                        .map_err(|e| format!("Failed to open file: {}", e))?;
                    
                    file.seek(SeekFrom::Start(downloaded_size))
                        .await
                        .map_err(|e| format!("Failed to seek: {}", e))?;
                    
                    file
                } else {
                    tokio::fs::File::create(&path)
                        .await
                        .map_err(|e| format!("Failed to create file: {}", e))?
                };

                let mut stream = response.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| format!("Failed to read chunk: {}", e))?;
                    file.write_all(&chunk)
                        .await
                        .map_err(|e| format!("Failed to write chunk: {}", e))?;
                }

                Ok(())
            }
            Self::Folder { name, files, base_path, is_dataset } => {
                // 创建文件夹
                let folder_path = base_path.join(&name);
                fs::create_dir_all(&folder_path)
                    .await
                    .map_err(|e| format!("Failed to create directory: {}", e))?;

                let mut total_size = 0;
                let mut need_download_files = Vec::new();

                // 预处理：计算需要下载的文件
                for file in files {
                    let file_path = folder_path.join(&file.rfilename);
                    if Self::should_download(&file_path, file.size).await {
                        if let Some(size) = file.size {
                            total_size += size;
                        }
                        need_download_files.push((file, file_path));
                    }
                }

                // 如果没有需要下载的文件，直接返回
                if need_download_files.is_empty() {
                    return Ok(());
                }

                // 创建进度条
                let pb = ProgressBar::new(total_size);

                // 下载需要的文件
                for (file, file_path) in need_download_files {
                    let task = if let Some(size) = file.size {
                        if size > chunk_size as u64 {
                            Self::large_file(file, file_path, chunk_size, max_retries, &name, is_dataset)
                        } else {
                            Self::single_file(file, file_path, &name, is_dataset)
                        }
                    } else {
                        Self::single_file(file, file_path, &name, is_dataset)
                    };

                    task.execute(client, token.clone(), endpoint, model_id).await?;
                    if let Some(size) = file.size {
                        pb.inc(size);
                    }
                }

                pb.finish_with_message("✓ Folder downloaded successfully");
                Ok(())
            }
        }
    }
} 