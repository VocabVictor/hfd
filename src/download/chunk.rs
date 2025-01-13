use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use std::io::SeekFrom;
use futures::StreamExt;
use std::time::Duration;
use crate::INTERRUPT_FLAG;
use crate::types::FileInfo;
use super::DownloadManager;

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
    download_manager: &DownloadManager,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<(), String> {
    let size = file.size.ok_or("File size is required for chunked download")?;

    // 检查文件是否已经下载
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        if metadata.len() >= size {
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

    // 计算需要下载的块
    let mut chunks: Vec<u64> = (0..((size + chunk_size as u64 - 1) / chunk_size as u64)).collect();
    chunks.reverse(); // 从后往前下载，这样可以更好地处理断点续传

    // 创建或打开文件
    let file_handle = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;
    let file_handle = Arc::new(tokio::sync::Mutex::new(file_handle));

    // 创建共享的下载速度计数器
    let bytes_downloaded = Arc::new(AtomicU64::new(0));
    let last_update = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));

    // 创建信号量来限制并发连接数
    let semaphore = Arc::new(tokio::sync::Semaphore::new(download_manager.get_config().connections_per_download));

    let download_task = async {
        let mut tasks = Vec::new();

        while !chunks.is_empty() {
            // 获取一个信号量许可
            let permit = match semaphore.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    // 等待一个任务完成后继续
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };

            let chunk_index = chunks.pop().unwrap();
            let start = chunk_index * chunk_size as u64;
            let end = std::cmp::min(start + chunk_size as u64, size);
            
            let client = client.clone();
            let url = url.clone();
            let token = token.clone();
            let file_handle = file_handle.clone();
            let bytes_downloaded = bytes_downloaded.clone();
            let last_update = last_update.clone();
            let filename = file.rfilename.clone();
            let download_manager = download_manager.clone();
            let mut shutdown_rx = shutdown.resubscribe();

            let task = tokio::spawn(async move {
                let _permit = permit;
                
                let mut retries = 0;
                while retries < max_retries {
                    let mut request = client.get(&url)
                        .header("Range", format!("bytes={}-{}", start, end - 1))
                        .timeout(std::time::Duration::from_secs(30));

                    if let Some(ref token) = token {
                        request = request.header("Authorization", format!("Bearer {}", token));
                    }

                    match tokio::time::timeout(
                        Duration::from_secs(30),
                        request.send()
                    ).await {
                        Ok(Ok(response)) => {
                            if response.status().is_success() {
                                let mut stream = response.bytes_stream();
                                let mut current_pos = start;
                                
                                let chunk_download = async {
                                    while let Ok(Some(chunk_result)) = tokio::time::timeout(
                                        Duration::from_secs(30),
                                        stream.next()
                                    ).await {
                                        let chunk = chunk_result.map_err(|e| format!("Failed to download chunk: {}", e))?;
                                        let chunk_size = chunk.len() as u64;

                                        // 写入文件
                                        let mut file = file_handle.lock().await;
                                        file.seek(SeekFrom::Start(current_pos))
                                            .await
                                            .map_err(|e| format!("Failed to seek: {}", e))?;
                                        file.write_all(&chunk)
                                            .await
                                            .map_err(|e| format!("Failed to write: {}", e))?;

                                        // 更新进度
                                        current_pos += chunk_size;
                                        bytes_downloaded.fetch_add(chunk_size, Ordering::SeqCst);

                                        // 定期更新进度条
                                        let mut last = last_update.lock().unwrap();
                                        let now = std::time::Instant::now();
                                        if now.duration_since(*last).as_millis() > 100 {
                                            download_manager.update_progress(&filename, bytes_downloaded.load(Ordering::SeqCst)).await;
                                            *last = now;
                                        }
                                    }
                                    Ok::<_, String>(())
                                };

                                tokio::select! {
                                    result = chunk_download => {
                                        result?;
                                        return Ok(());
                                    }
                                    _ = shutdown_rx.recv() => {
                                        return Err("Download interrupted by user".to_string());
                                    }
                                }
                            }
                            Err(format!("Failed to download chunk: {}", response.status()))
                        }
                        Ok(Err(e)) => {
                            retries += 1;
                            if retries >= max_retries {
                                return Err(format!("Failed to download chunk after {} retries: {}", max_retries, e));
                            }
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                        Err(_) => {
                            retries += 1;
                            if retries >= max_retries {
                                return Err(format!("Download timed out after {} retries", max_retries));
                            }
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    }
                }
                
                Err("Maximum retries exceeded".to_string())
            });

            tasks.push(task);
        }

        // 等待所有任务完成
        for task in tasks {
            task.await.map_err(|e| format!("Task failed: {}", e))??;
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
            Err("Download interrupted by user".to_string())
        }
    }
} 