use crate::types::FileInfo;
use std::path::PathBuf;
use reqwest::Client;
use pyo3::prelude::*;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use std::io::SeekFrom;
use std::time::Duration;
use tokio::fs;
use crate::download::chunk::download_chunked_file;
use crate::download::DownloadManager;
use crate::INTERRUPT_FLAG;

pub async fn download_small_file(
    client: &Client,
    file: &FileInfo,
    path: &PathBuf,
    token: Option<String>,
    endpoint: &str,
    model_id: &str,
    is_dataset: bool,
    download_manager: &DownloadManager,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<(), String> {
    // 检查文件是否已经下载
    if let Some(size) = file.size {
        if let Ok(metadata) = tokio::fs::metadata(path).await {
            if metadata.len() >= size {
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

    // 创建进度条
    let _pb = download_manager.create_file_progress(file.rfilename.clone(), total_size).await;

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

    let download_task = async {
        // 对于小文件，直接下载整个内容
        let bytes = response.bytes()
            .await
            .map_err(|e| format!("Failed to download file: {}", e))?;

        // 写入文件
        output_file.write_all(&bytes)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;

        // 更新进度
        let bytes_len = bytes.len() as u64;
        if bytes_len > 0 {
            download_manager.update_progress(&file.rfilename, bytes_len).await;
        }

        Ok::<_, String>(())
    };

    tokio::select! {
        result = download_task => {
            result?;
            // 完成下载
            download_manager.finish_file(&file.rfilename).await;
            Ok(())
        }
        _ = shutdown.recv() => {
            download_manager.handle_interrupt(&file.rfilename).await;
            Err("Download interrupted by user".to_string())
        }
    }
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
    shutdown: crate::ShutdownHandle,
) -> PyResult<()> {
    let folder_name = name.clone();
    let folder_path = base_path;
    tokio::fs::create_dir_all(&folder_path)
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;

    let mut need_download_files = Vec::new();
    let mut total_download_size = 0;
    let mut downloaded_size = 0;

    // 检查需要下载的文件
    let mut downloaded_files = 0;
    for file in &files {
        let file_path = folder_path.join(&file.rfilename);
        if let Some(size) = file.size {
            let file_downloaded_size = get_downloaded_size(&file_path).await;
            downloaded_size += file_downloaded_size;
            if file_downloaded_size < size {
                total_download_size += size - file_downloaded_size;
                need_download_files.push(file.clone());
            } else {
                downloaded_files += 1;
            }
        }
    }

    // 如果所有文件都已下载完成，直接返回
    if need_download_files.is_empty() {
        return Ok(());
    }

    println!("Found {} already downloaded files, downloading remaining {} files, total size: {} bytes",
            downloaded_files, need_download_files.len(), total_download_size);

    // 检查是否所有文件都在同一个子文件夹中
    let is_subfolder_download = if let Some(first_file) = need_download_files.first() {
        // 检查文件路径中是否包含斜杠（表示在子文件夹中）
        first_file.rfilename.contains('/')
    } else {
        false
    };

    // 创建下载管理器
    let download_manager = if is_subfolder_download {
        // 获取子文件夹名称
        let folder_display_name = if let Some(first_file) = need_download_files.first() {
            first_file.rfilename.split('/').next().unwrap_or(&folder_name).to_string()
        } else {
            folder_name.clone()
        };
        DownloadManager::new_folder(total_download_size + downloaded_size, folder_display_name, crate::config::Config::default())
    } else {
        DownloadManager::new_folder(total_download_size + downloaded_size, folder_name.clone(), crate::config::Config::default())
    };

    // 设置已下载的大小
    let pb = download_manager.create_file_progress("".to_string(), total_download_size + downloaded_size).await;
    pb.inc(downloaded_size);

    let download_task = async {
        let mut tasks = Vec::new();

        for file in need_download_files {
            let file_path = folder_path.join(&file.rfilename);
            let client = client.clone();
            let token = token.clone();
            let endpoint = endpoint.clone();
            let model_id = model_id.clone();
            let download_manager = download_manager.clone();
            let mut shutdown_rx = shutdown.subscribe();

            let task = tokio::spawn(async move {
                if file.size.unwrap_or(0) > download_manager.get_config().parallel_download_threshold {
                    download_chunked_file(
                        &client,
                        &file,
                        &file_path,
                        download_manager.get_config().chunk_size,
                        download_manager.get_config().max_retries,
                        token,
                        &endpoint,
                        &model_id,
                        is_dataset,
                        &download_manager,
                        shutdown_rx,
                    ).await
                } else {
                    download_small_file(
                        &client,
                        &file,
                        &file_path,
                        token,
                        &endpoint,
                        &model_id,
                        is_dataset,
                        &download_manager,
                        shutdown_rx,
                    ).await
                }
            });

            tasks.push(task);
        }

        for task in tasks {
            task.await.map_err(|e| format!("Task failed: {}", e))??;
        }

        Ok::<_, String>(())
    };

    tokio::select! {
        result = download_task => {
            match result {
                Ok(_) => {
                    download_manager.finish_folder().await;
                    Ok(())
                },
                Err(e) => {
                    download_manager.handle_folder_interrupt().await;
                    Err(pyo3::exceptions::PyRuntimeError::new_err(e))
                }
            }
        }
        _ = shutdown.subscribe().recv() => {
            download_manager.handle_folder_interrupt().await;
            Err(pyo3::exceptions::PyRuntimeError::new_err("Download interrupted by user"))
        }
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