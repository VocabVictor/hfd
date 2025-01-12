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
    pb: Option<Arc<ProgressBar>>,
) -> PyResult<()> {
    // 检查文件是否已下载
    if let Some(size) = file.size {
        let downloaded_size = get_downloaded_size(path).await;
        if downloaded_size >= size {
            println!("✓ File already downloaded: {}", file.rfilename);
            return Ok(());
        }

        // 创建或使用传入的进度条
        let pb = if let Some(pb) = pb {
            pb
        } else {
            let pb = Arc::new(ProgressBar::new(size));
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
                .unwrap()
                .progress_chars("#>-"));
            pb.set_message(format!("Downloading {}", file.rfilename));
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        };

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;
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
        if downloaded_size > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded_size));
        }

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

        pb.finish_with_message(format!("✓ Downloaded {}", file.rfilename));
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
    pb: Option<Arc<ProgressBar>>,
) -> PyResult<()> {
    // 检查文件是否已下载
    if let Some(size) = file.size {
        let downloaded_size = get_downloaded_size(path).await;
        if downloaded_size >= size {
            println!("✓ File already downloaded: {}", file.rfilename);
            return Ok(());
        }

        // 创建或使用传入的进度条
        let pb = if let Some(pb) = pb {
            pb
        } else {
            let pb = Arc::new(ProgressBar::new(size));
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
                .unwrap()
                .progress_chars("#>-"));
            pb.set_message(format!("Downloading {}", file.rfilename));
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        };

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;
        }

        let url = if is_dataset {
            format!("{}/datasets/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        } else {
            format!("{}/{}/resolve/main/{}", endpoint, model_id, file.rfilename)
        };

        let running = Arc::new(AtomicBool::new(true));
        
        // Use chunked download
        if let Err(e) = super::chunk::download_file_with_chunks(
            client,
            url.clone(),
            path.clone(),
            size,
            chunk_size,
            max_retries,
            token,
            pb,
            running,
        ).await {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(e));
        }
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
    println!("DEBUG: Starting download_folder for {}", folder_name);
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
                total_download_size += size - downloaded_size;  // 只计算需要下载的部分
                need_download_files.push(file.clone());
                println!("DEBUG: Added file to download queue: {}/{} (size: {})", folder_name, file.rfilename, size);
            }
        }
    }

    if need_download_files.is_empty() {
        println!("✓ All files in folder {} are already downloaded", folder_name);
        return Ok(());
    }

    println!("DEBUG: Need to download {} files in folder {}, total size: {} bytes", need_download_files.len(), folder_name, total_download_size);

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

    println!("DEBUG: Split files in folder {}: {} large files, {} small files", 
        folder_name, large_files.len(), small_files.len());

    let mut tasks = Vec::new();
    let max_concurrent_small_files = 3;  // 减少并发数，避免进度显示混乱
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent_small_files));

    // 处理小文件
    println!("DEBUG: Starting to process small files in folder {}", folder_name);
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

        println!("DEBUG: Creating task for small file: {}", file.rfilename);
        tasks.push(tokio::spawn(async move {
            let _permit = permit.acquire().await.unwrap();
            println!("DEBUG: Starting download of small file: {}", file.rfilename);
            let result = download_small_file(
                &client,
                &file,
                &file_path,
                token,
                &endpoint,
                &model_id,
                is_dataset,
                Some(pb),
            ).await;
            if result.is_ok() {
                println!("DEBUG: Completed download of small file: {}", file.rfilename);
            } else {
                println!("DEBUG: Failed to download small file: {}", file.rfilename);
            }
            result
        }));
    }

    // 处理大文件
    println!("DEBUG: Starting to process large files in folder {}", folder_name);
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

        println!("DEBUG: Creating task for large file: {}", file.rfilename);
        tasks.push(tokio::spawn(async move {
            println!("DEBUG: Starting download of large file: {}", file.rfilename);
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
                Some(pb),
            ).await;
            if result.is_ok() {
                println!("DEBUG: Completed download of large file: {}", file.rfilename);
            } else {
                println!("DEBUG: Failed to download large file: {}", file.rfilename);
            }
            result
        }));
    }

    println!("DEBUG: Waiting for all tasks in folder {} to complete", folder_name);
    for (i, task) in tasks.into_iter().enumerate() {
        if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
            pb.abandon_with_message("Download interrupted by user");
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
        }
        println!("DEBUG: Waiting for task {} in folder {}", i + 1, folder_name);
        match task.await {
            Ok(result) => {
                println!("DEBUG: Task {} in folder {} completed with result: {:?}", i + 1, folder_name, result.is_ok());
                result?;
            },
            Err(e) => {
                println!("DEBUG: Task {} in folder {} failed with error: {}", i + 1, folder_name, e);
                return Err(pyo3::exceptions::PyRuntimeError::new_err(format!("Task failed: {}", e)));
            }
        }
    }

    println!("DEBUG: All tasks in folder {} completed", folder_name);
    pb.finish_with_message(format!("✓ Downloaded folder {}", folder_name));
    Ok(())
}

async fn get_downloaded_size(path: &PathBuf) -> u64 {
    if path.exists() {
        match fs::metadata(path).await {
            Ok(metadata) => {
                let size = metadata.len();
                println!("DEBUG: get_downloaded_size for {}: {}", path.display(), size);
                size
            },
            Err(e) => {
                println!("DEBUG: Error getting file size for {}: {}", path.display(), e);
                0
            }
        }
    } else {
        println!("DEBUG: File does not exist: {}", path.display());
        0
    }
} 