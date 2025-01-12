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
) -> Result<(), String> {
    let size = file.size.ok_or("File size is required for chunked download")?;

    // 检查文件是否已经下载
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        if metadata.len() >= size {
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

    // 创建进度条
    let pb = download_manager.create_file_progress(file.rfilename.clone(), size).await;

    // 获取已下载的大小
    let downloaded_size = if let Ok(metadata) = tokio::fs::metadata(&path).await {
        metadata.len()
    } else {
        0
    };

    // 如果文件已经完全下载，直接返回
    if downloaded_size >= size {
        download_manager.finish_file(&file.rfilename).await;
        return Ok(());
    }

    // 计算剩余需要下载的块
    let start_chunk = downloaded_size / chunk_size as u64;
    let num_chunks = (size + chunk_size as u64 - 1) / chunk_size as u64;
    let mut chunks: Vec<_> = (start_chunk..num_chunks).collect();

    // 如果没有需要下载的块，直接返回
    if chunks.is_empty() {
        download_manager.finish_file(&file.rfilename).await;
        return Ok(());
    }

    println!("Starting download with {} chunks", chunks.len());

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

    // 创建任务队列
    let mut tasks = Vec::new();

    // 创建信号量来限制并发连接数
    let semaphore = Arc::new(tokio::sync::Semaphore::new(download_manager.get_config().connections_per_download));

    while !chunks.is_empty() && !INTERRUPT_FLAG.load(Ordering::SeqCst) {
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

        let task = tokio::spawn(async move {
            let _permit = permit;
            
            let mut retries = 0;
            while retries < max_retries && !INTERRUPT_FLAG.load(Ordering::SeqCst) {
                let mut request = client.get(&url)
                    .header("Range", format!("bytes={}-{}", start, end - 1))
                    .timeout(std::time::Duration::from_secs(30));

                if let Some(ref token) = token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }

                println!("Downloading chunk {} (bytes {}-{})", chunk_index, start, end - 1);
                
                match tokio::time::timeout(std::time::Duration::from_secs(35), request.send()).await {
                    Ok(response_result) => {
                        match response_result {
                            Ok(response) => {
                                println!("Chunk {} received response with status: {}", chunk_index, response.status());
                                if !response.status().is_success() {
                                    let status = response.status();
                                    let error_text = response.text().await.unwrap_or_default();
                                    println!("Chunk {} failed with status {} and error: {}", chunk_index, status, error_text);
                                    retries += 1;
                                    if retries >= max_retries {
                                        return Err(format!("Server error {} after {} retries: {}", status, max_retries, error_text));
                                    }
                                    tokio::time::sleep(Duration::from_secs(1)).await;
                                    continue;
                                }

                                let mut stream = response.bytes_stream();
                                let mut current_pos = start;

                                println!("Starting to read chunk {} stream", chunk_index);
                                
                                while let Ok(Some(chunk_result)) = tokio::time::timeout(
                                    Duration::from_secs(30),
                                    stream.next()
                                ).await {
                                    if INTERRUPT_FLAG.load(Ordering::SeqCst) {
                                        return Err("Download interrupted by user".to_string());
                                    }

                                    match chunk_result {
                                        Ok(chunk) => {
                                            let chunk_len = chunk.len() as u64;
                                            let mut file = file_handle.lock().await;
                                            
                                            file.seek(SeekFrom::Start(current_pos))
                                                .await
                                                .map_err(|e| format!("Failed to seek: {}", e))?;
                                            
                                            file.write_all(&chunk)
                                                .await
                                                .map_err(|e| format!("Failed to write chunk: {}", e))?;
                                            
                                            current_pos += chunk_len;

                                            // 更新进度
                                            download_manager.update_progress(&filename, chunk_len).await;

                                            // 更新下载速度统计
                                            bytes_downloaded.fetch_add(chunk_len, Ordering::Relaxed);
                                            let now = std::time::Instant::now();
                                            let mut last = last_update.lock().unwrap();
                                            if now.duration_since(*last) >= Duration::from_millis(100) {
                                                let bytes = bytes_downloaded.swap(0, Ordering::Relaxed);
                                                let elapsed = now.duration_since(*last).as_secs_f64();
                                                if elapsed > 0.0 {
                                                    let speed = bytes as f64 / elapsed;
                                                    pb.set_message(format!("{:.2} MiB/s", speed / 1024.0 / 1024.0));
                                                }
                                                *last = now;
                                            }
                                        }
                                        Err(e) => {
                                            return Err(format!("Failed to download chunk: {}", e));
                                        }
                                    }
                                }

                                return Ok(());
                            }
                            Err(e) => {
                                println!("Chunk {} request failed with error: {}", chunk_index, e);
                                retries += 1;
                                if retries >= max_retries {
                                    return Err(format!("Failed to download chunk after {} retries: {}", max_retries, e));
                                }
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                    Err(_) => {
                        println!("Downloading chunk {} timed out", chunk_index);
                        retries += 1;
                        if retries >= max_retries {
                            return Err(format!("Download timed out after {} retries", max_retries));
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
            
            if INTERRUPT_FLAG.load(Ordering::SeqCst) {
                Err("Download interrupted by user".to_string())
            } else {
                Ok(())
            }
        });

        tasks.push(task);
    }

    // 等待所有任务完成
    for task in tasks {
        task.await.map_err(|e| format!("Task failed: {}", e))??;
    }

    // 完成下载
    download_manager.finish_file(&file.rfilename).await;

    Ok(())
} 