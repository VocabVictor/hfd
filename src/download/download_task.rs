use crate::types::FileInfo;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use pyo3::prelude::*;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use futures::StreamExt;
use std::pin::Pin;
use std::future::Future;

#[derive(Debug)]
pub enum DownloadTask {
    #[allow(dead_code)]
    SmallFile {
        file: FileInfo,
        path: PathBuf,
        group: String,
    },
    #[allow(dead_code)]
    ChunkedFile {
        file: FileInfo,
        path: PathBuf,
        chunk_size: usize,
        max_retries: usize,
        group: String,
    },
    #[allow(dead_code)]
    Folder {
        name: String,
        files: Vec<FileInfo>,
        base_path: PathBuf,
    },
}

#[allow(dead_code)]
impl DownloadTask {
    pub fn new_small_file(file: FileInfo, path: PathBuf, group: &str) -> Self {
        Self::SmallFile {
            file,
            path,
            group: group.to_string(),
        }
    }

    pub fn new_chunked_file(
        file: FileInfo,
        path: PathBuf,
        chunk_size: usize,
        max_retries: usize,
        group: &str,
    ) -> Self {
        Self::ChunkedFile {
            file,
            path,
            chunk_size,
            max_retries,
            group: group.to_string(),
        }
    }

    pub fn new_folder(name: String, files: Vec<FileInfo>, base_path: PathBuf) -> Self {
        Self::Folder {
            name,
            files,
            base_path,
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
                Self::SmallFile { file, path, group } => {
                    Self::download_small_file(client, &file, &path, token, endpoint, model_id, &group).await
                }
                Self::ChunkedFile { file, path, chunk_size, max_retries, group } => {
                    Self::download_chunked_file(client, &file, &path, chunk_size, max_retries, token, endpoint, model_id, &group).await
                }
                Self::Folder { name, files, base_path } => {
                    Self::download_folder(client, &name, &files, &base_path, token, endpoint, model_id).await
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
    ) -> PyResult<()> {
        let url = format!("{}/api/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename);
        let mut request = client.get(&url);
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        // Try dataset URL first
        let mut response = request.send().await;
        let mut final_url = url;
        
        // If dataset URL fails, try model URL
        if response.is_err() || !response.as_ref().unwrap().status().is_success() {
            let model_url = format!("{}/api/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename);
            let mut model_request = client.get(&model_url);
            if let Some(token) = &token {
                model_request = model_request.header("Authorization", format!("Bearer {}", token));
            }
            response = model_request.send().await;
            final_url = model_url;
        }

        // Create progress bar
        let pb = ProgressBar::new(file.size.unwrap_or(0));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("{}/{}", group, file.rfilename));

        let response = match response {
            Ok(resp) => resp,
            Err(e) => return Err(pyo3::exceptions::PyRuntimeError::new_err(
                format!("Failed to download from {}: {}", final_url, e)
            )),
        };

        if !response.status().is_success() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                format!("HTTP error from {}: {}", final_url, response.status())
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

        pb.finish_with_message(format!("Downloaded {}/{}", group, file.rfilename));
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
    ) -> PyResult<()> {
        // Try dataset URL first
        let mut url = format!("{}/api/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename);
        let mut request = client.get(&url);
        if let Some(token) = &token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await;
        
        // If dataset URL fails, try model URL
        if response.is_err() || !response.as_ref().unwrap().status().is_success() {
            url = format!("{}/api/models/{}/resolve/main/{}", endpoint, model_id, file.rfilename);
        }

        let total_size = file.size.unwrap_or(0);

        // Create progress bar
        let pb = Arc::new(ProgressBar::new(total_size));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("{}/{}", group, file.rfilename));

        let running = Arc::new(AtomicBool::new(true));
        
        // Use chunked download
        if let Err(e) = super::chunk::download_file_with_chunks(
            client,
            url,
            path.clone(),
            total_size,
            chunk_size,
            max_retries,
            token,
            pb.clone(),
            running,
        ).await {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
        }

        pb.finish_with_message(format!("Downloaded {}/{}", group, file.rfilename));
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
    ) -> PyResult<()> {
        // 创建文件夹
        let folder_path = base_path.join(name);
        tokio::fs::create_dir_all(&folder_path)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;

        // 下载所有文件
        for file in files {
            let file_path = folder_path.join(file.rfilename.split('/').last().unwrap_or(&file.rfilename));
            if let Some(size) = file.size {
                let task = if size > 50 * 1024 * 1024 { // 50MB
                    DownloadTask::ChunkedFile {
                        file: file.clone(),
                        path: file_path,
                        chunk_size: 16 * 1024 * 1024, // 16MB
                        max_retries: 3,
                        group: name.to_string(),
                    }
                } else {
                    DownloadTask::SmallFile {
                        file: file.clone(),
                        path: file_path,
                        group: name.to_string(),
                    }
                };
                task.execute(client, token.clone(), endpoint, model_id).await?;
            }
        }

        Ok(())
    }
} 