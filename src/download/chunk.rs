use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use std::io::SeekFrom;
use futures::StreamExt;
use std::time::Duration;
use crate::INTERRUPT_FLAG;
use crate::config::Config;
use crate::types::FileInfo;
use pyo3::exceptions::PyRuntimeError;

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
                pb.tick();
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

    if parent_pb.is_none() && result.is_ok() {
        pb.finish_with_message(format!("✓ Downloaded {}", file.rfilename));
    }

    result
}

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
    config: &Config,
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
        pb.set_position(total_size);
        pb.tick();
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
    pb.tick();

    // 创建信号量来限制并发连接数
    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.connections_per_download));

    // 创建或打开文件
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;
    let file = Arc::new(tokio::sync::Mutex::new(file));

    // 创建共享的下载速度计数器和总进度
    let bytes_downloaded = Arc::new(AtomicU64::new(0));
    let total_downloaded = Arc::new(AtomicU64::new(downloaded_size));
    let last_update = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));

    // 创建任务队列
    let mut tasks = Vec::new();
    let chunk_count = chunks.len();
    println!("Starting download with {} chunks", chunk_count);

    // 创建一个通道来接收完成的块索引
    let (tx, mut rx) = tokio::sync::mpsc::channel(chunk_count);

    while !chunks.is_empty() && running.load(Ordering::SeqCst) && !INTERRUPT_FLAG.load(Ordering::SeqCst) {
        // 获取一个信号量许可
        let permit = match semaphore.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                // 如果没有可用的许可，等待一个任务完成
                if let Some(_) = rx.recv().await {
                    continue;
                }
                break;
            }
        };

        let chunk_index = chunks.pop().unwrap();
        let start = chunk_index * chunk_size as u64;
        let end = std::cmp::min(start + chunk_size as u64, total_size);
        
        let client = client.clone();
        let url = url.clone();
        let token = token.clone();
        let pb = pb.clone();
        let file = file.clone();
        let bytes_downloaded = bytes_downloaded.clone();
        let total_downloaded = total_downloaded.clone();
        let last_update = last_update.clone();
        let tx = tx.clone();

        // 创建并立即执行异步任务
        let task = tokio::spawn(async move {
            let _permit = permit;  // 在作用域结束时自动释放许可
            
            let mut retries = 0;
            while retries < max_retries && !INTERRUPT_FLAG.load(Ordering::SeqCst) {
                let mut request = client.get(&url)
                    .header("Range", format!("bytes={}-{}", start, end - 1));

                if let Some(ref token) = token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }

                match request.send().await {
                    Ok(response) => {
                        // 检查响应状态码
                        if !response.status().is_success() {
                            let status = response.status();
                            retries += 1;
                            if retries >= max_retries {
                                return Err(format!("Server error {} after {} retries", status, max_retries));
                            }
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                            continue;
                        }

                        let mut stream = response.bytes_stream();
                        let mut current_pos = start;
                        let mut chunk_downloaded = false;

                        while let Some(chunk_result) = stream.next().await {
                            // 检查中断标志
                            if INTERRUPT_FLAG.load(Ordering::SeqCst) {
                                return Err("Download interrupted by user".to_string());
                            }

                            match chunk_result {
                                Ok(chunk) => {
                                    chunk_downloaded = true;
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

                                    // 立即更新总进度和进度条
                                    let new_total = total_downloaded.fetch_add(chunk_len, Ordering::Relaxed) + chunk_len;
                                    pb.set_position(new_total);
                                    pb.tick();

                                    // 更新下载速度统计（每100ms更新一次）
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

                        if !chunk_downloaded {
                            retries += 1;
                            if retries >= max_retries {
                                return Err(format!("No data received for chunk {} after {} retries", chunk_index, max_retries));
                            }
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                            continue;
                        }

                        // 通知一个块已完成
                        let _ = tx.send(chunk_index).await;
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