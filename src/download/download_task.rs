use crate::types::FileInfo;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use pyo3::prelude::*;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use futures::StreamExt;
use std::pin::Pin;
use std::future::Future;
use std::time::Duration;

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
    pub fn new_small_file(file: FileInfo, path: PathBuf, group: &str, is_dataset: bool) -> Self {
        Self::SmallFile {
            file,
            path,
            group: group.to_string(),
            is_dataset,
        }
    }

    pub fn new_chunked_file(
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

    pub fn new_folder(name: String, files: Vec<FileInfo>, base_path: PathBuf, is_dataset: bool) -> Self {
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
    ) -> Pin<Box<dyn Future<Output = PyResult<()>> + 'a>> {
        Box::pin(async move {
            match self {
                Self::SmallFile { file, path, group, is_dataset } => {
                    Self::download_small_file(client, &file, &path, token, endpoint, model_id, &group, is_dataset).await
                }
                Self::ChunkedFile { file, path, chunk_size, max_retries, group, is_dataset } => {
                    Self::download_chunked_file(client, &file, &path, chunk_size, max_retries, token, endpoint, model_id, &group, is_dataset).await
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
        group: &str,
        is_dataset: bool,
    ) -> PyResult<()> {
        let url = if is_dataset {
            format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        } else {
            format!("{}/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        };

        println!("Downloading {} from {}", file.rfilename, url);

        let mut request = client.get(&url);
        if let Some(ref token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        // 使用已经获取的文件大小
        let pb = ProgressBar::new(file.size.unwrap_or(0));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("{}/{}", group, file.rfilename));
        pb.enable_steady_tick(Duration::from_millis(100));

        let response = request.send()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(
                format!("Failed to download from {}: {}", url, e)
            ))?;

        if !response.status().is_success() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                format!("HTTP error from {}: {}", url, response.status())
            ));
        }

        let mut file_handle = File::create(path)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create file: {}", e)))?;

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to read chunk: {}", e)))?;
            file_handle.write_all(&chunk)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to write chunk: {}", e)))?;
            pb.inc(chunk.len() as u64);
        }

        pb.finish_with_message(format!("✓ Downloaded {}/{}", group, file.rfilename));
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
        group: &str,
        is_dataset: bool,
    ) -> PyResult<()> {
        let url = if is_dataset {
            format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        } else {
            format!("{}/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        };

        println!("Downloading (chunked) {} from {}", file.rfilename, url);

        // 使用已经获取的文件大小
        let total_size = file.size.unwrap_or(0);

        // Create progress bar with better settings
        let pb = Arc::new(ProgressBar::new(total_size));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("{}/{}", group, file.rfilename));
        pb.enable_steady_tick(Duration::from_millis(100));

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
            pb.finish_with_message(format!("✗ Failed to download {}/{}: {}", group, file.rfilename, e));
            return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
        }

        pb.finish_with_message(format!("✓ Downloaded {}/{} (chunked)", group, file.rfilename));
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

        // 创建多进度条管理器
        let mp = MultiProgress::new();
        let total_size: u64 = files.iter().filter_map(|f| f.size).sum();
        let total_pb = mp.add(ProgressBar::new(total_size));
        total_pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) Total Progress")
            .unwrap()
            .progress_chars("#>-"));

        // 在后台线程中渲染进度条
        let mp_clone = mp.clone();
        tokio::task::spawn_blocking(move || {
            mp_clone.clear().unwrap();
        });

        // 下载所有文件
        let mut tasks = Vec::new();
        for file in files {
            let file_path = folder_path.join(file.rfilename.split('/').last().unwrap_or(&file.rfilename));
            if let Some(size) = file.size {
                let pb = mp.add(ProgressBar::new(size));
                pb.set_style(ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"));
                pb.set_message(file.rfilename.clone());

                let task = if size > 100 * 1024 * 1024 { // 100MB
                    DownloadTask::ChunkedFile {
                        file: file.clone(),
                        path: file_path,
                        chunk_size: 16 * 1024 * 1024, // 16MB chunks
                        max_retries: 3,
                        group: name.to_string(),
                        is_dataset,
                    }
                } else {
                    DownloadTask::SmallFile {
                        file: file.clone(),
                        path: file_path,
                        group: name.to_string(),
                        is_dataset,
                    }
                };

                let client = client.clone();
                let token = token.clone();
                let endpoint = endpoint.to_string();
                let model_id = model_id.to_string();
                let pb_clone = pb.clone();
                let total_pb_clone = total_pb.clone();

                tasks.push(tokio::spawn(async move {
                    let result = task.execute(&client, token, &endpoint, &model_id).await;
                    pb_clone.finish_and_clear();
                    if let Ok(()) = result {
                        total_pb_clone.inc(size);
                    }
                    result
                }));
            }
        }

        // 等待所有下载完成
        for task in tasks {
            task.await.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Task failed: {}", e)))??;
        }

        total_pb.finish_with_message("✓ All files downloaded");
        Ok(())
    }
} 