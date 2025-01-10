use crate::types::FileInfo;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use pyo3::prelude::*;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub enum DownloadTask {
    SmallFile {
        file: FileInfo,
        path: PathBuf,
        group: String,
    },
    ChunkedFile {
        file: FileInfo,
        path: PathBuf,
        chunk_size: usize,
        max_retries: usize,
        group: String,
    },
    Folder {
        name: String,
        files: Vec<FileInfo>,
        base_path: PathBuf,
    },
}

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

    pub async fn execute(
        &self,
        client: &Client,
        token: Option<String>,
        endpoint: &str,
        model_id: &str,
    ) -> PyResult<()> {
        match self {
            Self::SmallFile { file, path, group } => {
                self.download_small_file(client, file, path, token, endpoint, model_id, group).await
            }
            Self::ChunkedFile { file, path, chunk_size, max_retries, group } => {
                self.download_chunked_file(client, file, path, *chunk_size, *max_retries, token, endpoint, model_id, group).await
            }
            Self::Folder { name, files, base_path } => {
                self.download_folder(client, name, files, base_path, token, endpoint, model_id).await
            }
        }
    }

    async fn download_small_file(
        &self,
        client: &Client,
        file: &FileInfo,
        path: &PathBuf,
        token: Option<String>,
        endpoint: &str,
        model_id: &str,
        group: &str,
    ) -> PyResult<()> {
        let url = format!("{}/api/models/{}/resolve/{}", endpoint, model_id, file.rfilename);
        let mut request = client.get(&url);
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        // 创建进度条
        let pb = ProgressBar::new(file.size.unwrap_or(0));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("{}/{}", group, file.rfilename));

        let response = request.send()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to download: {}", e)))?;

        if !response.status().is_success() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                format!("HTTP error: {}", response.status())
            ));
        }

        let mut file_handle = File::create(path)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create file: {}", e)))?;

        let mut stream = response.bytes_stream();
        while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
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
        &self,
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
        let url = format!("{}/api/models/{}/resolve/{}", endpoint, model_id, file.rfilename);
        let total_size = file.size.unwrap_or(0);

        // 创建进度条
        let pb = Arc::new(ProgressBar::new(total_size));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("{}/{}", group, file.rfilename));

        let running = Arc::new(AtomicBool::new(true));
        
        // 使用分块下载
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
        &self,
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
                if size > 50 * 1024 * 1024 { // 50MB
                    let task = Self::ChunkedFile {
                        file: file.clone(),
                        path: file_path,
                        chunk_size: 16 * 1024 * 1024, // 16MB
                        max_retries: 3,
                        group: name.to_string(),
                    };
                    task.execute(client, token.clone(), endpoint, model_id).await?;
                } else {
                    let task = Self::SmallFile {
                        file: file.clone(),
                        path: file_path,
                        group: name.to_string(),
                    };
                    task.execute(client, token.clone(), endpoint, model_id).await?;
                }
            }
        }

        Ok(())
    }
} 