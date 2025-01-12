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
use std::time::Duration;
use tokio::fs;
use crate::download::chunk::download_file_with_chunks;
use crate::config::Config;

const DEFAULT_CHUNK_SIZE: usize = 16 * 1024 * 1024; // 16MB
const DEFAULT_MAX_RETRIES: usize = 3;

pub async fn download_small_file(
    client: &Client,
    file: &FileInfo,
    path: &PathBuf,
    token: Option<String>,
    endpoint: &str,
    model_id: &str,
    is_dataset: bool,
    parent_pb: Option<Arc<ProgressBar>>,
) -> Result<(), String> {
    let size = file.size.unwrap_or(0);

    // 创建文件自己的进度条
    let file_pb = Arc::new(ProgressBar::new(size));
    file_pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
        .unwrap()
        .progress_chars("#>-"));
    file_pb.set_message(format!("Downloading {}", file.rfilename));
    file_pb.enable_steady_tick(Duration::from_millis(100));

    // 检查文件是否已经下载
    if let Some(size) = file.size {
        if let Ok(metadata) = tokio::fs::metadata(path).await {
            if metadata.len() >= size {
                file_pb.finish_with_message(format!("✓ File already exists: {}", file.rfilename));
                if let Some(pb) = parent_pb {
                    pb.inc(size);
                }
                return Ok(());
            }
        }
    }

    // 确保父目录存在
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let url = if is_dataset {
        format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
    } else {
        format!("{}/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
    };

    let mut request = client.get(&url);
    if let Some(ref token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    // 获取已下载的大小
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        if metadata.len() > 0 {
            request = request.header("Range", format!("bytes={}-", metadata.len()));
        }
    }

    let response = request.send()
        .await
        .map_err(|e| format!("Failed to download file: {}", e))?;

    let mut output_file = if let Ok(metadata) = tokio::fs::metadata(path).await {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .await
            .map_err(|e| format!("Failed to open file: {}", e))?;
        
        file.seek(SeekFrom::Start(metadata.len()))
            .await
            .map_err(|e| format!("Failed to seek: {}", e))?;
        
        file
    } else {
        tokio::fs::File::create(path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?
    };

    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Failed to download chunk: {}", e))?;
        output_file.write_all(&chunk).await.map_err(|e| format!("Failed to write chunk: {}", e))?;
        if let Some(pb) = &parent_pb {
            pb.inc(chunk.len() as u64);
        }
    }

    Ok(())
}

pub async fn download_chunked_file(
    client: &Client,
    file: &FileInfo,
    path: &PathBuf,
    chunk_size: usize,
    max_retries: usize,
    token: Option<String>,
    endpoint: &str,
    model_id: &str,
    is_dataset: bool,
    parent_pb: Option<Arc<ProgressBar>>,
    config: &Config,
) -> Result<(), String> {
    let size = file.size.ok_or("File size is required for chunked download")?;

    // 创建文件自己的进度条
    let file_pb = Arc::new(ProgressBar::new(size));
    file_pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
        .unwrap()
        .progress_chars("#>-"));
    file_pb.set_message(format!("Downloading {}", file.rfilename));
    file_pb.enable_steady_tick(Duration::from_millis(100));

    // 确保父目录存在
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let url = if is_dataset {
        format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
    } else {
        format!("{}/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
    };

    let running = Arc::new(AtomicBool::new(true));
    let result = download_file_with_chunks(
        client,
        url,
        path.clone(),
        size,
        chunk_size,
        max_retries,
        token,
        file_pb.clone(),
        running.clone(),
        config,
    ).await;

    if result.is_ok() {
        file_pb.finish_with_message(format!("✓ Downloaded {}", file.rfilename));
        if let Some(pb) = parent_pb {
            pb.inc(size);
        }
    } else {
        file_pb.abandon_with_message(format!("Failed to download {}", file.rfilename));
    }

    result
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
            let downloaded_size = get_downloaded_size(&file_path).await;
            if downloaded_size >= size {
                println!("✓ File already downloaded: {}/{}", folder_name, file.rfilename);
            } else {
                total_download_size += size - downloaded_size;
                need_download_files.push(file.clone());
            }
        }
    }

    if need_download_files.is_empty() {
        println!("✓ All files in folder {} are already downloaded", folder_name);
        return Ok(());
    }

    println!("Need to download {} files, total size: {} bytes", need_download_files.len(), total_download_size);

    // 创建文件夹的总进度条
    let total_pb = Arc::new(ProgressBar::new(total_download_size));
    total_pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
        .unwrap()
        .progress_chars("#>-"));
    total_pb.set_message(format!("Downloading folder {}", folder_name));
    total_pb.enable_steady_tick(Duration::from_millis(100));

    // 将文件分为大文件和小文件
    let (large_files, small_files): (Vec<_>, Vec<_>) = need_download_files
        .into_iter()
        .partition(|file| file.size.map_or(false, |size| size > DEFAULT_CHUNK_SIZE as u64));

    let mut tasks = Vec::new();
    let max_concurrent_files = 3;  // 文件夹内的并发下载数
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent_files));

    // 处理小文件 - 并发下载
    for file in small_files {
        if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
            total_pb.abandon_with_message("Download interrupted by user");
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
        }

        let file_path = folder_path.join(&file.rfilename);
        let client = client.clone();
        let token = token.clone();
        let permit = semaphore.clone();
        let endpoint = endpoint.clone();
        let model_id = model_id.clone();
        let total_pb = total_pb.clone();

        tasks.push(tokio::spawn(async move {
            let _permit = permit.acquire().await.unwrap();
            println!("Starting download of {}", file.rfilename);
            let result = download_small_file(
                &client,
                &file,
                &file_path,
                token,
                &endpoint,
                &model_id,
                is_dataset,
                Some(total_pb),
            ).await;
            if result.is_ok() {
                println!("Completed download of {}", file.rfilename);
            }
            result.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
        }));
    }

    // 处理大文件 - 每个文件内部使用分块并发下载
    for file in large_files {
        if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
            total_pb.abandon_with_message("Download interrupted by user");
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
        }

        let file_path = folder_path.join(&file.rfilename);
        let client = client.clone();
        let token = token.clone();
        let permit = semaphore.clone();
        let endpoint = endpoint.clone();
        let model_id = model_id.clone();
        let total_pb = total_pb.clone();

        tasks.push(tokio::spawn(async move {
            let _permit = permit.acquire().await.unwrap();
            println!("Starting download of {}", file.rfilename);
            let result = download_chunked_file(
                &client,
                &file,
                &file_path,
                DEFAULT_CHUNK_SIZE,
                DEFAULT_MAX_RETRIES,
                token,
                &endpoint,
                &model_id,
                is_dataset,
                Some(total_pb),
                &Config::default(),
            ).await;
            if result.is_ok() {
                println!("Completed download of {}", file.rfilename);
            }
            result.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
        }));
    }

    // 等待所有任务完成
    for task in tasks {
        if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
            total_pb.abandon_with_message("Download interrupted by user");
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
        }
        task.await.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Task failed: {}", e)))?;
    }

    total_pb.finish_with_message(format!("✓ Downloaded folder {}", folder_name));
    Ok(())
}

async fn get_downloaded_size(path: &PathBuf) -> u64 {
    if path.exists() {
        match fs::metadata(path).await {
            Ok(metadata) => metadata.len(),
            Err(_) => 0
        }
    } else {
        0
    }
}

// 单文件下载入口
pub async fn download_single_file(
    client: &Client,
    file: &FileInfo,
    path: &PathBuf,
    token: Option<String>,
    endpoint: &str,
    model_id: &str,
    is_dataset: bool,
) -> Result<(), String> {
    // 如果是大文件，使用分块并发下载
    if file.size.map_or(false, |size| size > DEFAULT_CHUNK_SIZE as u64) {
        download_chunked_file(
            client,
            file,
            path,
            DEFAULT_CHUNK_SIZE,
            DEFAULT_MAX_RETRIES,
            token,
            endpoint,
            model_id,
            is_dataset,
            None,
            &Config::default(),
        ).await
    } else {
        // 小文件直接下载
        download_small_file(
            client,
            file,
            path,
            token,
            endpoint,
            model_id,
            is_dataset,
            None,
        ).await
    }
} 