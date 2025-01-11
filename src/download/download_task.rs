use crate::types::FileInfo;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use pyo3::prelude::*;
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
        group: &str,
        is_dataset: bool,
        shared_pb: Option<Arc<ProgressBar>>,
    ) -> PyResult<()> {
        let url = if is_dataset {
            format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        } else {
            format!("{}/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        };

        let mut request = client.get(&url);
        if let Some(ref token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        // 创建进度条（如果没有共享进度条）
        let total_size = file.size.unwrap_or(0);
        let pb = if let Some(pb) = shared_pb {
            pb
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

        let mut output_file = tokio::fs::File::create(path)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create file: {}", e)))?;

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
        group: &str,
        is_dataset: bool,
        shared_pb: Option<Arc<ProgressBar>>,
    ) -> PyResult<()> {
        let url = if is_dataset {
            format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        } else {
            format!("{}/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        };

        // 使用已经获取的文件大小
        let total_size = file.size.unwrap_or(0);

        // 使用共享进度条或创建新的进度条
        let pb = if let Some(pb) = shared_pb {
            pb
        } else {
            let pb = Arc::new(ProgressBar::new(total_size));
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
                .unwrap()
                .progress_chars("#>-"));
            pb.set_message(format!("{}/{}", group, file.rfilename));
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
                pb.finish_with_message(format!("✗ Failed to download {}/{}: {}", group, file.rfilename, e));
            }
            return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
        }

        if shared_pb.is_none() {
            pb.finish_with_message(format!("✓ Downloaded {}/{} (chunked)", group, file.rfilename));
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

        // 计算文件夹总大小
        let total_size: u64 = files.iter().filter_map(|f| f.size).sum();

        // 为整个文件夹创建一个进度条
        let pb = Arc::new(ProgressBar::new(total_size));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading folder: {}", name));
        pb.enable_steady_tick(Duration::from_millis(100));

        // 下载所有文件
        let mut tasks = Vec::new();
        let client = Arc::new(client.clone());
        let name = name.to_string();

        for file in files {
            let file_path = folder_path.join(file.rfilename.split('/').last().unwrap_or(&file.rfilename));
            let file = file.clone();
            let token = token.clone();
            let endpoint = endpoint.to_string();
            let model_id = model_id.to_string();
            let name = name.clone();
            let client = client.clone();
            let pb = pb.clone();

            // 根据文件大小选择下载方式
            if let Some(size) = file.size {
                if size > 10 * 1024 * 1024 {  // 大于10MB的文件使用分块下载
                    tasks.push(tokio::spawn(async move {
                        Self::download_chunked_file(
                            &client,
                            &file,
                            &file_path,
                            1024 * 1024,  // 1MB chunks
                            3,
                            token,
                            &endpoint,
                            &model_id,
                            &name,
                            is_dataset,
                            Some(pb),
                        ).await
                    }));
                } else {
                    tasks.push(tokio::spawn(async move {
                        Self::download_small_file(
                            &client,
                            &file,
                            &file_path,
                            token,
                            &endpoint,
                            &model_id,
                            &name,
                            is_dataset,
                            Some(pb),
                        ).await
                    }));
                }
            }
        }

        // 等待所有下载完成
        for task in tasks {
            task.await.map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to join download task: {}", e))
            })??;
        }

        pb.finish_with_message(format!("✓ Folder {} downloaded successfully", name));
        Ok(())
    }
} 