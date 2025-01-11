use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use indicatif::ProgressBar;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use std::io::SeekFrom;
use futures::StreamExt;

#[allow(dead_code)]
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
    if downloaded_size == total_size {
        return Ok(());
    }

    // 计算剩余需要下载的块
    let start_chunk = downloaded_size / chunk_size as u64;
    let num_chunks = (total_size + chunk_size as u64 - 1) / chunk_size as u64;
    let mut chunks: Vec<_> = (start_chunk..num_chunks).collect();

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

    // 创建任务队列
    let mut tasks = Vec::new();

    while !chunks.is_empty() && running.load(Ordering::SeqCst) {
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

        // 创建异步任务
        let task = tokio::spawn(async move {
            let _permit = permit;  // 在作用域结束时自动释放许可
            
            let mut retries = 0;
            while retries < max_retries {
                let mut request = client.get(&url)
                    .header("Range", format!("bytes={}-{}", start, end - 1));

                if let Some(ref token) = token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }

                match request.send().await {
                    Ok(response) => {
                        let mut stream = response.bytes_stream();
                        let mut current_pos = start;

                        while let Some(chunk_result) = stream.next().await {
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
                                    
                                    // 更新位置和进度条
                                    current_pos += chunk_len;
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
            Ok(())
        });

        tasks.push(task);
    }

    // 等待所有任务完成
    for task in tasks {
        task.await.map_err(|e| format!("Task failed: {}", e))??;
    }

    Ok(())
} 