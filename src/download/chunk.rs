use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use indicatif::ProgressBar;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use std::io::SeekFrom;
use futures::StreamExt;
use std::time::Duration;
use crate::INTERRUPT_FLAG;

pub async fn download_file_with_chunks(
    client: &Client,
    url: String,
    path: PathBuf,
    total_size: u64,
    chunk_size: usize,
    max_retries: usize,
    token: Option<String>,
    pb: Arc<ProgressBar>,
    running: Arc<AtomicBool>,
) -> Result<(), String> {
    // 检查chunk_size是否为0
    if chunk_size == 0 {
        return Err("Chunk size cannot be zero".to_string());
    }

    // 创建父目录
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    // 获取已下载的大小
    let downloaded_size = if let Ok(metadata) = tokio::fs::metadata(&path).await {
        metadata.len()
    } else {
        0
    };

    // 如果文件已经完全下载，直接返回
    if downloaded_size >= total_size {
        pb.inc(total_size);  // 更新父进度条，但不显示本文件的进度
        return Ok(());
    }

    // 计算剩余需要下载的块
    let start_chunk = downloaded_size / chunk_size as u64;
    let num_chunks = (total_size + chunk_size as u64 - 1) / chunk_size as u64;
    let mut chunks: Vec<_> = (start_chunk..num_chunks).collect();

    // 如果没有需要下载的块，直接返回
    if chunks.is_empty() {
        return Ok(());
    }

    // 设置进度条的初始位置为已下载的大小
    pb.set_position(downloaded_size);

    // 创建信号量来限制并发连接数
    let semaphore = Arc::new(tokio::sync::Semaphore::new(8));  // 使用配置的连接数

    // 创建或打开文件
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;
    let file = Arc::new(tokio::sync::Mutex::new(file));

    // 创建共享的下载速度计数器
    let bytes_downloaded = Arc::new(AtomicU64::new(0));
    let last_update = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));

    // 创建任务队列
    let mut tasks = Vec::new();

    while !chunks.is_empty() && running.load(Ordering::SeqCst) && !INTERRUPT_FLAG.load(Ordering::SeqCst) {
        // 获取一个信号量许可
        let permit = semaphore.clone().acquire_owned().await.unwrap();

        let chunk_index = chunks.pop().unwrap();
        let start = chunk_index * chunk_size as u64;
        let end = std::cmp::min(start + chunk_size as u64, total_size);
        
        let client = client.clone();
        let url = url.clone();
        let token = token.clone();
        let pb = pb.clone();
        let file = file.clone();
        let bytes_downloaded = bytes_downloaded.clone();
        let last_update = last_update.clone();

        // 创建异步任务
        let task = tokio::spawn(async move {
            let _permit = permit;  // 在作用域结束时自动释放许可
            
            let mut retries = 0;
            while retries < max_retries && !INTERRUPT_FLAG.load(Ordering::SeqCst) {
                let mut request = client.get(&url)
                    .header("Range", format!("bytes={}-{}", start, end - 1));

                if let Some(ref token) = token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }

                println!("Sending request for chunk {}, range: {}-{}", chunk_index, start, end - 1);
                match request.send().await {
                    Ok(response) => {
                        // 检查响应状态码
                        if !response.status().is_success() {
                            println!("Error: Server returned status code: {}", response.status());
                            let error_text = response.text().await.unwrap_or_default();
                            println!("Error response: {}", error_text);
                            retries += 1;
                            if retries >= max_retries {
                                return Err(format!("Server error {} after {} retries", response.status(), max_retries));
                            }
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                            continue;
                        }

                        let mut stream = response.bytes_stream();
                        let mut current_pos = start;

                        while let Some(chunk_result) = stream.next().await {
                            // 检查中断标志
                            if INTERRUPT_FLAG.load(Ordering::SeqCst) {
                                return Err("Download interrupted by user".to_string());
                            }

                            match chunk_result {
                                Ok(chunk) => {
                                    let chunk_len = chunk.len() as u64;
                                    let mut file = file.lock().await;
                                    
                                    // 定位到正确的写入位置
                                    file.seek(SeekFrom::Start(current_pos))
                                        .await
                                        .map_err(|e| format!("Failed to seek: {}", e))?;
                                    
                                    // 写入数据
                                    file.write_all(&chunk)
                                        .await
                                        .map_err(|e| format!("Failed to write chunk: {}", e))?;
                                    
                                    // 更新位置
                                    current_pos += chunk_len;

                                    // 更新下载速度统计
                                    bytes_downloaded.fetch_add(chunk_len, Ordering::Relaxed);
                                    let mut last = last_update.lock().unwrap();
                                    let now = std::time::Instant::now();
                                    if now.duration_since(*last) >= Duration::from_millis(100) {
                                        let bytes = bytes_downloaded.swap(0, Ordering::Relaxed);
                                        let elapsed = now.duration_since(*last).as_secs_f64();
                                        if elapsed > 0.0 {
                                            let speed = bytes as f64 / elapsed;
                                            pb.set_message(format!("{:.2} MiB/s", speed / 1024.0 / 1024.0));
                                        }
                                        *last = now;
                                    }
                                    
                                    // 更新进度条，但不显示完成消息
                                    pb.inc(chunk_len);
                                }
                                Err(e) => {
                                    return Err(format!("Failed to download chunk: {}", e));
                                }
                            }
                        }
                        return Ok(());
                    }
                    Err(_) => {
                        retries += 1;
                        if retries >= max_retries {
                            return Err(format!("Failed to download chunk after {} retries", max_retries));
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
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

    // 如果是因为中断而退出循环，返回中断错误
    if INTERRUPT_FLAG.load(Ordering::SeqCst) {
        return Err("Download interrupted by user".to_string());
    }

    // 等待所有任务完成
    for task in tasks {
        task.await.map_err(|e| format!("Task failed: {}", e))??;
    }

    Ok(())
} 