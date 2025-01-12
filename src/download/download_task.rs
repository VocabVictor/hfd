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
    use crate::INTERRUPT_FLAG;

    // 检查文件是否已经下载
    if let Some(size) = file.size {
        if let Ok(metadata) = tokio::fs::metadata(path).await {
            if metadata.len() >= size {
                if let Some(ref pb) = parent_pb {
                    pb.set_position(pb.position() + size);
                }
                println!("File {} is already downloaded.", file.rfilename);
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
    let mut downloaded_size = 0;
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        if metadata.len() > 0 {
            downloaded_size = metadata.len();
            request = request.header("Range", format!("bytes={}-", downloaded_size));
        }
    }

    let response = request.send()
        .await
        .map_err(|e| format!("Failed to download file: {}", e))?;

    // 获取文件总大小
    let total_size = if let Some(size) = file.size {
        size
    } else if let Some(content_length) = response.content_length() {
        content_length + downloaded_size
    } else {
        return Err("Could not determine file size".to_string());
    };

    // 如果没有父进度条，创建一个新的进度条
    let pb = if let Some(ref pb) = parent_pb {
        pb.clone()
    } else {
        let pb = Arc::new(ProgressBar::new(total_size));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading {}", file.rfilename));
        pb.enable_steady_tick(Duration::from_millis(100));
        if downloaded_size > 0 {
            pb.set_position(downloaded_size);
        }
        pb
    };

    let mut output_file = if downloaded_size > 0 {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .await
            .map_err(|e| format!("Failed to open file: {}", e))?;
        
        file.seek(SeekFrom::Start(downloaded_size))
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
        if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Download interrupted by user".to_string());
        }

        let chunk = chunk_result.map_err(|e| format!("Failed to download chunk: {}", e))?;
        output_file.write_all(&chunk).await.map_err(|e| format!("Failed to write chunk: {}", e))?;
        pb.set_position(pb.position() + chunk.len() as u64);
    }

    if parent_pb.is_none() {
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
    parent_pb: Option<Arc<ProgressBar>>,
    config: &Config,
) -> Result<(), String> {
    use crate::INTERRUPT_FLAG;

    let size = file.size.ok_or("File size is required for chunked download")?;

    // 检查文件是否已经下载
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        if metadata.len() >= size {
            if let Some(ref pb) = parent_pb {
                pb.set_position(pb.position() + size);
            }
            println!("File {} is already downloaded.", file.rfilename);
            return Ok(());
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

    let running = Arc::new(AtomicBool::new(true));

    // 创建一个监听中断的任务
    let running_clone = running.clone();
    tokio::spawn(async move {
        while !INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        running_clone.store(false, std::sync::atomic::Ordering::SeqCst);
    });

    // 如果没有父进度条，创建一个新的进度条
    let pb = if let Some(ref pb) = parent_pb {
        pb.clone()
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

    let result = download_file_with_chunks(
        client,
        url,
        path.clone(),
        size,
        chunk_size,
        max_retries,
        token,
        pb.clone(),
        running.clone(),
        config,
    ).await;

    if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Download interrupted by user".to_string());
    }

    if parent_pb.is_none() {
        if result.is_ok() {
            pb.finish_with_message(format!("✓ Downloaded {}", file.rfilename));
        } else {
            pb.abandon_with_message(format!("Failed to download {}", file.rfilename));
        }
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
    use tokio::select;

    let folder_name = name.clone();
    let folder_path = base_path;
    tokio::fs::create_dir_all(&folder_path)
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;

    let mut need_download_files = Vec::new();
    let mut total_download_size = 0;

    // 检查需要下载的文件
    let mut total_files = files.len();
    let mut downloaded_files = 0;
    for file in &files {
        if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"));
        }

        let file_path = folder_path.join(&file.rfilename);
        if let Some(size) = file.size {
            let file_downloaded_size = get_downloaded_size(&file_path).await;
            if file_downloaded_size < size {
                total_download_size += size - file_downloaded_size;
                need_download_files.push(file.clone());
            } else {
                downloaded_files += 1;
                println!("File {} is already downloaded.", file.rfilename);
            }
        }
    }

    // 如果所有文件都已下载完成，直接返回
    if need_download_files.is_empty() {
        println!("All {} files in folder {} are already downloaded.", total_files, folder_name);
        return Ok(());
    }

    // 如果有需要下载的文件，才创建进度条和开始下载任务
    if total_download_size > 0 {
        println!("Found {} already downloaded files, downloading remaining {} files, total size: {} bytes", 
                downloaded_files, need_download_files.len(), total_download_size);

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

        // 创建一个中断检测任务
        let interrupt_task = tokio::spawn(async move {
            while !INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });

        // 处理小文件 - 并发下载
        for file in small_files {
            let file_path = folder_path.join(&file.rfilename);
            let client = client.clone();
            let token = token.clone();
            let permit = semaphore.clone();
            let endpoint = endpoint.clone();
            let model_id = model_id.clone();
            let total_pb = total_pb.clone();

            tasks.push(tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
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
                result.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
            }));
        }

        // 处理大文件 - 每个文件内部使用分块并发下载
        for file in large_files {
            let file_path = folder_path.join(&file.rfilename);
            let client = client.clone();
            let token = token.clone();
            let permit = semaphore.clone();
            let endpoint = endpoint.clone();
            let model_id = model_id.clone();
            let total_pb = total_pb.clone();

            tasks.push(tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
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
                result.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
            }));
        }

        // 使用 select! 等待任务完成或中断
        let download_task = async {
            for task in tasks {
                match task.await {
                    Ok(result) => result?,
                    Err(e) => return Err(pyo3::exceptions::PyRuntimeError::new_err(format!("Task failed: {}", e))),
                }
            }
            Ok(())
        };

        select! {
            result = download_task => {
                match result {
                    Ok(_) => {
                        total_pb.finish_with_message(format!("✓ Downloaded folder {}", folder_name));
                        Ok(())
                    },
                    Err(e) => {
                        total_pb.abandon_with_message("Download failed");
                        Err(e)
                    }
                }
            },
            _ = interrupt_task => {
                total_pb.abandon_with_message("Download interrupted by user");
                Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"))
            }
        }
    } else {
        Ok(())
    }
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