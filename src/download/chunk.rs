use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use indicatif::ProgressBar;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use futures::StreamExt;
use tokio::fs::File;

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
    // 创建临时文件
    let mut temp_file = tokio::fs::File::create(&path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    // 计算块数
    let num_chunks = (total_size + chunk_size as u64 - 1) / chunk_size as u64;
    let mut chunks: Vec<_> = (0..num_chunks).collect();

    // 创建信号量来限制并发连接数
    let semaphore = Arc::new(tokio::sync::Semaphore::new(8));  // 使用配置的连接数

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
        let path = path.clone();

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
                        if let Ok(chunk_data) = response.bytes().await {
                            // 写入文件
                            let mut file = tokio::fs::OpenOptions::new()
                                .write(true)
                                .open(&path)
                                .await
                                .map_err(|e| format!("Failed to open file: {}", e))?;

                            file.seek(std::io::SeekFrom::Start(start))
                                .await
                                .map_err(|e| format!("Failed to seek: {}", e))?;

                            file.write_all(&chunk_data)
                                .await
                                .map_err(|e| format!("Failed to write chunk: {}", e))?;

                            pb.inc(chunk_data.len() as u64);
                            return Ok(());
                        }
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