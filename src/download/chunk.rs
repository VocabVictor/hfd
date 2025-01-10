use super::downloader::ModelDownloader;
use std::fs::{self, OpenOptions, File};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use reqwest::Client;
use indicatif::ProgressBar;
use std::io::{Write, Seek, SeekFrom, Read};
use futures::StreamExt;
use std::collections::HashSet;
use std::sync::Mutex;

// 用于记录块的下载状态
struct ChunkStatus {
    completed_chunks: HashSet<u64>,
    total_chunks: u64,
}

impl ModelDownloader {
    // 检查块是否已下载完成
    fn is_chunk_complete(
        file: &mut File,
        chunk_start: u64,
        chunk_end: u64,
    ) -> bool {
        let chunk_size = chunk_end - chunk_start + 1;
        let mut buffer = vec![0u8; chunk_size as usize];
        
        if let Ok(_) = file.seek(SeekFrom::Start(chunk_start)) {
            if let Ok(read_size) = file.read(&mut buffer) {
                // 如果读取的大小等于块大小，说明这个块可能已经下载完成
                return read_size as u64 == chunk_size;
            }
        }
        false
    }

    pub(crate) async fn download_file_with_chunks(
        client: &Client,
        url: String,
        file_path: PathBuf,
        total_size: u64,
        chunk_size: usize,
        max_retries: usize,
        auth_token: Option<String>,
        pb: ProgressBar,
        running: Arc<AtomicBool>,
    ) -> Result<(), String> {
        println!("Debug: Starting download for file: {}", file_path.display());
        println!("Debug: Total size: {} bytes", total_size);
        
        // 检查文件是否已经部分下载
        let initial_size = if let Ok(metadata) = fs::metadata(&file_path) {
            let size = metadata.len();
            if size == total_size {
                println!("Debug: File already downloaded completely");
                pb.finish_with_message(format!("{} ✓", file_path.file_name().unwrap().to_string_lossy()));
                return Ok(());  // 文件已完全下载
            }
            // 如果文件大小超过预期，删除重新下载
            if size > total_size {
                println!("Debug: File size larger than expected ({} > {}), deleting", size, total_size);
                fs::remove_file(&file_path).map_err(|e| 
                    format!("删除损坏文件失败: {}", e)
                )?;
                0
            } else {
                println!("Debug: File partially downloaded: {} bytes", size);
                size  // 使用已下载的大小
            }
        } else {
            println!("Debug: File does not exist, starting fresh download");
            0  // 文件不存在
        };

        // 预分配文件大小
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&file_path)
            .map_err(|e| format!("创建文件失败: {}", e))?;

        if initial_size == 0 {
            println!("Debug: Pre-allocating file size: {} bytes", total_size);
            file.set_len(total_size).map_err(|e| 
                format!("预分配文件大小失败: {}", e)
            )?;
        }
        
        // 使用传入的chunk_size，但确保它不会太大或太小
        let chunk_size = chunk_size.max(1 * 1024 * 1024)  // 最小1MB
            .min(64 * 1024 * 1024) as u64;  // 最大64MB
        println!("Debug: Using chunk size: {} bytes", chunk_size);

        // 计算块数
        let total_chunks = (total_size + chunk_size - 1) / chunk_size;
        println!("Debug: Total chunks: {}", total_chunks);
        
        // 创建块状态记录
        let chunk_status = Arc::new(Mutex::new(ChunkStatus {
            completed_chunks: HashSet::new(),
            total_chunks: total_chunks,
        }));

        // 检查已下载的块
        println!("Debug: Checking existing chunks");
        for chunk_idx in 0..total_chunks {
            let chunk_start = chunk_idx * chunk_size;
            let chunk_end = if chunk_idx == total_chunks - 1 {
                total_size - 1  // 最后一个块可能比较小
            } else {
                chunk_start + chunk_size - 1
            };
            
            if Self::is_chunk_complete(&mut file, chunk_start, chunk_end) {
                println!("Debug: Chunk {} is complete", chunk_idx);
                chunk_status.lock().unwrap().completed_chunks.insert(chunk_idx);
                pb.inc(chunk_end - chunk_start + 1);
            }
        }
        
        // 设置进度条的初始位置
        pb.set_length(total_size);
        let completed_size = chunk_status.lock().unwrap().completed_chunks.len() as u64 * chunk_size;
        pb.set_position(completed_size);
        println!("Debug: Initial progress: {} bytes", completed_size);
        
        // 创建信号量来限制并发数
        let semaphore = Arc::new(Semaphore::new(32));
        println!("Debug: Created semaphore with 32 permits");

        // 处理所有未完成的块
        let chunks: Vec<_> = (0..total_chunks)
            .filter(|&chunk_idx| !chunk_status.lock().unwrap().completed_chunks.contains(&chunk_idx))
            .collect();
        println!("Debug: {} chunks need to be downloaded", chunks.len());

        let mut handles = chunks.iter()
            .map(|&chunk_idx| {
                let chunk_start = chunk_idx * chunk_size;
                let chunk_end = if chunk_idx == total_chunks - 1 {
                    total_size - 1  // 最后一个块可能比较小
                } else {
                    chunk_start + chunk_size - 1
                };
                
                // 克隆需要的资源
                let url = url.clone();
                let file_path = file_path.clone();
                let client = client.clone();
                let auth_token = auth_token.clone();
                let running = running.clone();
                let semaphore = semaphore.clone();
                let pb = pb.clone();
                let chunk_status = chunk_status.clone();

                tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.map_err(|e| 
                        format!("获取信号量失败: {}", e)
                    )?;

                    let mut retry_count = 0;
                    loop {
                        if !running.load(Ordering::SeqCst) {
                            return Ok(());
                        }

                        match Self::download_chunk(
                            &client,
                            &url,
                            &file_path,
                            chunk_start,
                            chunk_end,
                            auth_token.clone(),
                            running.clone(),
                            pb.clone(),
                        ).await {
                            Ok(_) => {
                                // 标记块为已完成
                                chunk_status.lock().unwrap().completed_chunks.insert(chunk_idx);
                                break Ok(());
                            },
                            Err(e) => {
                                if !running.load(Ordering::SeqCst) {
                                    return Ok(());
                                }

                                println!("Debug: Chunk {} download failed: {}", chunk_idx, e);
                                if retry_count >= max_retries {
                                    return Err(format!("下载失败，已重试 {} 次: {}", max_retries, e));
                                }

                                let wait_time = Self::exponential_backoff(1000, retry_count, 30_000);
                                println!("Debug: Retrying chunk {} after {} ms", chunk_idx, wait_time);
                                tokio::time::sleep(tokio::time::Duration::from_millis(wait_time as u64)).await;

                                retry_count += 1;
                            }
                        }
                    }
                })
            })
            .collect::<Vec<_>>();

        // 等待所有块下载完成
        println!("Debug: Waiting for all chunks to complete");
        for handle in handles.drain(..) {
            if !running.load(Ordering::SeqCst) {
                println!("Debug: Download cancelled while waiting for chunks");
                return Ok(());
            }

            handle.await.map_err(|e| format!("任务失败: {}", e))??;
        }

        // 验证下载是否完整
        if let Ok(metadata) = fs::metadata(&file_path) {
            if metadata.len() != total_size {
                println!("Debug: Download incomplete: {} != {}", metadata.len(), total_size);
                return Err(format!("下载不完整: {} != {}", metadata.len(), total_size));
            }
        }

        // 验证所有块是否都已完成
        let status = chunk_status.lock().unwrap();
        if status.completed_chunks.len() as u64 != status.total_chunks {
            println!("Debug: Some chunks are missing: {}/{}", status.completed_chunks.len(), status.total_chunks);
            return Err(format!("部分块未下载完成: {}/{}", status.completed_chunks.len(), status.total_chunks));
        }

        println!("Debug: Download completed successfully");
        Ok(())
    }

    fn exponential_backoff(base: usize, retry: usize, max: usize) -> usize {
        use rand::{thread_rng, Rng};
        let jitter = thread_rng().gen_range(0..=500);
        (base + retry.pow(2) + jitter).min(max)
    }

    pub(crate) async fn download_chunk(
        client: &reqwest::Client,
        url: &str,
        file_path: &PathBuf,
        start: u64,
        end: u64,
        auth_token: Option<String>,
        running: Arc<AtomicBool>,
        pb: indicatif::ProgressBar,
    ) -> Result<(), String> {
        let range = format!("bytes={start}-{end}");
        println!("Debug: Downloading chunk: {} for file {}", range, file_path.display());
        
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(file_path)
            .map_err(|e| format!("创建文件失败: {}", e))?;

        file.seek(std::io::SeekFrom::Start(start))
            .map_err(|e| format!("定位文件位置失败: {}", e))?;

        let mut request = client.get(url);
        if let Some(token) = auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        request = request
            .header("Range", range.clone())
            .header("Accept-Encoding", "identity")  // 禁用压缩以准确计算大小
            .header("Connection", "keep-alive")
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .timeout(std::time::Duration::from_secs(300));

        if !running.load(Ordering::SeqCst) {
            println!("Debug: Download cancelled for chunk {}", range);
            return Ok(());
        }

        println!("Debug: Sending request for chunk {}", range);
        let response = request
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?
            .error_for_status()
            .map_err(|e| format!("HTTP错误: {}", e))?;

        println!("Debug: Got response for chunk {}, status: {}", range, response.status());

        let mut stream = response.bytes_stream();
        let mut buffer = Vec::with_capacity(8 * 1024 * 1024); // 8MB buffer
        let mut last_progress = std::time::Instant::now();
        let mut downloaded = 0u64;

        while let Some(chunk_result) = stream.next().await {
            if !running.load(Ordering::SeqCst) {
                println!("Debug: Download cancelled while streaming chunk {}", range);
                return Ok(());
            }

            let chunk = chunk_result.map_err(|e| format!("读取响应失败: {}", e))?;
            buffer.extend_from_slice(&chunk);
            downloaded += chunk.len() as u64;
            
            let now = std::time::Instant::now();
            if now.duration_since(last_progress).as_secs() > 60 {
                println!("Debug: Download too slow for chunk {}, downloaded {} bytes", range, downloaded);
                return Err("下载速度过慢，将重试".to_string());
            }
            
            // 实时更新进度条，不等缓冲区满
            pb.inc(chunk.len() as u64);
            
            if buffer.len() >= 8 * 1024 * 1024 { // 8MB
                println!("Debug: Writing buffer for chunk {}, size: {} bytes", range, buffer.len());
                file.write_all(&buffer)
                    .map_err(|e| format!("写入文件失败: {}", e))?;
                buffer.clear();
                last_progress = now;
            }
        }

        if !buffer.is_empty() && running.load(Ordering::SeqCst) {
            println!("Debug: Writing final buffer for chunk {}, size: {} bytes", range, buffer.len());
            file.write_all(&buffer)
                .map_err(|e| format!("写入文件失败: {}", e))?;
        }

        if running.load(Ordering::SeqCst) {
            file.sync_all()
                .map_err(|e| format!("同步文件失败: {}", e))?;
            println!("Debug: Completed chunk {}, total downloaded: {} bytes", range, downloaded);
        }

        Ok(())
    }
} 